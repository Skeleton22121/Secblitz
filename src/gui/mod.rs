//! The Secblitz window (iced, CPU renderer).
//!
//! OWNER: shell agent (routing below is the shared contract; page agents add
//! variants only inside their own `pages::<page>::Msg`).
//!
//! Conventions for every page module `pages::<name>`:
//! ```ignore
//! #[derive(Debug, Default)] pub struct State { .. }
//! #[derive(Debug, Clone)]   pub enum Msg { .. }
//! pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message>;
//! pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message>;
//! ```
//! Pages wrap their own messages: `Message::Tools(tools::Msg::X)`.
//! Long blocking work: use `blocking(..)` / `blocking_stream(..)` below, never
//! block in `update`/`view`.
pub mod icons;
pub mod pages;
pub mod theme;
pub mod widgets;

use crate::app::{self, score::Score, worker};
use crate::i18n::Lang;
use iced::futures::channel::{mpsc, oneshot};
use iced::futures::{Future, Stream};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{keyboard, Alignment, Background, Border, Element, Length, Subscription, Task};
use icons::Icon;
use pages::{debloat, fixes, fixflow, history, home, settings, tools};
use secblitz::engine::Report;
use std::path::PathBuf;
use std::sync::Arc;
use theme::{Palette, Tone};

#[derive(Debug, Clone)]
pub struct Options {
    pub lang: Lang,
    /// Broker pipe id from the launcher (`None` = no user-context actions).
    pub broker: Option<String>,
    /// Hidden `--self-test <page>`: open directly on this page.
    pub start: Option<Page>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Home,
    Fixes,
    Debloat,
    Tools,
    History,
    Settings,
}

impl Page {
    pub const ALL: [Page; 6] = [
        Page::Home,
        Page::Fixes,
        Page::Debloat,
        Page::Tools,
        Page::History,
        Page::Settings,
    ];
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "home" => Page::Home,
            "fixes" => Page::Fixes,
            "debloat" => Page::Debloat,
            "tools" => Page::Tools,
            "history" => Page::History,
            "settings" => Page::Settings,
            _ => return None,
        })
    }
    /// English source key for the sidebar label.
    pub fn label(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Fixes => "Protection",
            Page::Debloat => "Clean up apps",
            Page::Tools => "Tools",
            Page::History => "History",
            Page::Settings => "Settings",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Page::Home => Icon::Home,
            Page::Fixes => Icon::Shield,
            Page::Debloat => Icon::Sparkles,
            Page::Tools => Icon::Toolbox,
            Page::History => Icon::History,
            Page::Settings => Icon::Settings,
        }
    }
}

/// Live progress of the read-only check (first run, Check again, post-check).
#[derive(Debug, Clone, Default)]
pub struct CheckProgress {
    pub phase: Option<worker::Phase>,
    /// (control id, status) in arrival order.
    pub items: Vec<(String, String)>,
}

/// Shared, read-mostly application context passed to every page.
pub struct Ctx {
    pub lang: Lang,
    pub palette: Palette,
    pub worker: worker::Worker,
    pub catalog: worker::Catalog,
    /// Engine could not open (shown as a calm error card with Retry).
    pub engine_error: Option<String>,
    /// Latest full assessment.
    pub report: Option<Arc<Report>>,
    /// The latest check failed (protection unverified).
    pub check_error: Option<String>,
    /// Unix seconds of `report`.
    pub checked_at: Option<u64>,
    /// Some while a check / post-check runs.
    pub checking: Option<CheckProgress>,
    /// True while any change (fix, undo, app removal, repair…) is running.
    /// Navigation stays possible; starting another change is disabled.
    pub busy: bool,
    pub broker: Option<Arc<crate::broker::Client>>,
    pub state_dir: Option<PathBuf>,
    pub prefs: app::settings::Prefs,
    pub toast: Option<(String, Tone)>,
}

impl Ctx {
    pub fn t(&self, key: &str) -> String {
        self.lang.t(key)
    }
    pub fn score(&self) -> Option<Score> {
        self.report.as_deref().map(Score::of)
    }
    /// Run a broker request off the UI thread; result arrives as `map(reply)`.
    pub fn broker_task(
        &self,
        request: crate::broker::Request,
        map: impl Fn(Result<crate::broker::Reply, String>) -> Message + Send + 'static,
    ) -> Task<Message> {
        let Some(client) = self.broker.clone() else {
            return Task::done(map(Err("unavailable".into())));
        };
        Task::perform(
            blocking(move || client.send(request).map_err(|e| format!("{e:#}"))),
            map,
        )
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Page),
    /// Start a read-only check (ignored while one is running).
    CheckNow,
    Worker(worker::Event),
    /// Open the fix review sheet with these ids pre-selected.
    ReviewFixes(Vec<String>),
    /// Open the undo review sheet.
    ReviewUndo,
    Escape,
    Toast(String, Tone),
    DismissToast,
    Home(home::Msg),
    Fixes(fixes::Msg),
    Fix(fixflow::Msg),
    Debloat(debloat::Msg),
    Tools(tools::Msg),
    History(history::Msg),
    Settings(settings::Msg),
}

pub struct App {
    pub page: Page,
    pub ctx: Ctx,
    pub home: home::State,
    pub fixes: fixes::State,
    pub fix: fixflow::State,
    pub debloat: debloat::State,
    pub tools: tools::State,
    pub history: history::State,
    pub settings: settings::State,
}

/// Run a blocking closure on a fresh thread and await its result.
pub fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = T> + Send + 'static {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    async move { rx.await.expect("background task panicked") }
}

/// Run a blocking closure that reports progress through `emit`; the stream
/// yields every emitted item and ends when the closure returns.
pub fn blocking_stream<T: Send + 'static>(
    f: impl FnOnce(&dyn Fn(T)) + Send + 'static,
) -> impl Stream<Item = T> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    std::thread::spawn(move || {
        let emit = move |item: T| {
            let _ = tx.unbounded_send(item);
        };
        f(&emit);
    });
    rx
}

impl App {
    fn new(options: Options) -> (Self, Task<Message>) {
        let prefs = app::settings::load();
        let lang = prefs
            .lang
            .as_deref()
            .and_then(Lang::parse)
            .unwrap_or(options.lang);
        let mode = match prefs.theme {
            app::settings::ThemeChoice::Dark => theme::Mode::Dark,
            app::settings::ThemeChoice::Light => theme::Mode::Light,
        };
        let worker = worker::Worker::spawn(|| {
            let engine = secblitz::engine::Engine::open(
                secblitz::platform::state_dir()?,
                secblitz::permissions::with_permissions(secblitz::platform::backend()?),
            )?;
            Ok(Box::new(engine) as Box<dyn worker::Session>)
        });
        let broker = options
            .broker
            .as_deref()
            .and_then(|id| crate::broker::Client::connect(id).ok())
            .map(Arc::new);
        let ctx = Ctx {
            lang,
            palette: Palette::of(mode),
            worker: worker.clone(),
            catalog: worker::Catalog::default(),
            engine_error: None,
            report: None,
            check_error: None,
            checked_at: None,
            checking: Some(CheckProgress::default()),
            busy: false,
            broker,
            state_dir: secblitz::platform::state_dir().ok(),
            prefs,
            toast: None,
        };
        let app = App {
            page: options.start.unwrap_or_default(),
            ctx,
            home: Default::default(),
            fixes: Default::default(),
            fix: Default::default(),
            debloat: Default::default(),
            tools: Default::default(),
            history: Default::default(),
            settings: Default::default(),
        };
        let opened = Task::run(worker.opened(), Message::Worker);
        let first_check = Task::run(worker.run(worker::Job::Check), Message::Worker);
        (app, Task::batch([opened, first_check]))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(page) => {
                self.page = page;
                match page {
                    Page::History => history::on_enter(&mut self.history, &mut self.ctx),
                    Page::Debloat => debloat::on_enter(&mut self.debloat, &mut self.ctx),
                    _ => Task::none(),
                }
            }
            Message::CheckNow => {
                if self.ctx.checking.is_some() || self.ctx.busy {
                    return Task::none();
                }
                self.ctx.checking = Some(CheckProgress::default());
                Task::run(self.ctx.worker.run(worker::Job::Check), Message::Worker)
            }
            Message::Worker(event) => self.on_worker(event),
            Message::ReviewFixes(ids) => fixflow::open_fixes(&mut self.fix, ids, &mut self.ctx),
            Message::ReviewUndo => fixflow::open_undo(&mut self.fix, &mut self.ctx),
            Message::Escape => fixflow::escape(&mut self.fix, &mut self.ctx),
            Message::Toast(text, tone) => {
                self.ctx.toast = Some((text, tone));
                Task::none()
            }
            Message::DismissToast => {
                self.ctx.toast = None;
                Task::none()
            }
            Message::Home(m) => home::update(&mut self.home, m, &mut self.ctx),
            Message::Fixes(m) => fixes::update(&mut self.fixes, m, &mut self.ctx),
            Message::Fix(m) => fixflow::update(&mut self.fix, m, &mut self.ctx),
            Message::Debloat(m) => debloat::update(&mut self.debloat, m, &mut self.ctx),
            Message::Tools(m) => tools::update(&mut self.tools, m, &mut self.ctx),
            Message::History(m) => history::update(&mut self.history, m, &mut self.ctx),
            Message::Settings(m) => settings::update(&mut self.settings, m, &mut self.ctx),
        }
    }

    fn on_worker(&mut self, event: worker::Event) -> Task<Message> {
        use worker::Event as E;
        match &event {
            E::Opened(Ok(catalog)) => self.ctx.catalog = catalog.clone(),
            E::Opened(Err(e)) => self.ctx.engine_error = Some(e.clone()),
            E::Progress { phase, id, status } => {
                if matches!(phase, worker::Phase::Checking | worker::Phase::Verifying) {
                    let p = self.ctx.checking.get_or_insert_with(Default::default);
                    p.phase = Some(*phase);
                    p.items.push((id.clone(), status.clone()));
                }
            }
            E::Checked(outcome) => {
                self.ctx.checking = None;
                self.assessed(outcome, app::history::Kind::Check, 0);
            }
            E::Applied { attempted, verify, .. } => {
                self.ctx.checking = None;
                self.assessed(verify, app::history::Kind::Fix, attempted.len());
            }
            E::Undone { verify, .. } => {
                self.ctx.checking = None;
                self.assessed(verify, app::history::Kind::Undo, 0);
            }
            E::History(_) => {}
        }
        // Let the flows react (fix result card, history list).
        Task::batch([
            fixflow::on_worker(&mut self.fix, &event, &mut self.ctx),
            history::on_worker(&mut self.history, &event, &mut self.ctx),
        ])
    }

    /// Store a fresh assessment, log the score and refresh the tray status.
    fn assessed(&mut self, outcome: &worker::Outcome, kind: app::history::Kind, n: usize) {
        match outcome {
            Ok(report) => {
                self.ctx.report = Some(report.clone());
                self.ctx.check_error = None;
                let now = app::history::now();
                self.ctx.checked_at = Some(now);
                let score = Score::of(report);
                if let Some(dir) = &self.ctx.state_dir {
                    let _ = app::history::record(
                        dir,
                        &app::history::Entry {
                            t: now,
                            kind,
                            protected: score.protected,
                            total: score.total,
                            n,
                        },
                    );
                }
                let _ = secblitz::status::write(&status_of(report, &score, now));
            }
            Err(e) => self.ctx.check_error = Some(e.clone()),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let p = self.ctx.palette;
        let content: Element<'_, Message> = match self.page {
            Page::Home => home::view(&self.home, &self.ctx),
            Page::Fixes => fixes::view(&self.fixes, &self.ctx),
            Page::Debloat => debloat::view(&self.debloat, &self.ctx),
            Page::Tools => tools::view(&self.tools, &self.ctx),
            Page::History => history::view(&self.history, &self.ctx),
            Page::Settings => settings::view(&self.settings, &self.ctx),
        };
        let main = container(scrollable(container(content).padding(32).width(Length::Fill)))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.bg)),
                ..container::Style::default()
            });
        let body = row![self.sidebar(), main];
        // The fix flow (review sheet / working / result) draws over any page.
        fixflow::overlay(&self.fix, &self.ctx, body.into())
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let p = self.ctx.palette;
        let brand = row![
            widgets::icon(Icon::ShieldCheck, 26.0, p.brand),
            text("Secblitz").size(20).font(theme::BOLD).color(p.text)
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let mut nav = column![].spacing(4);
        for page in Page::ALL {
            let active = self.page == page;
            let fg = if active { p.text } else { p.text_muted };
            let item = row![
                widgets::icon(page.icon(), 18.0, if active { p.brand } else { p.text_muted }),
                text(self.ctx.t(page.label())).size(theme::BODY).font(theme::MEDIUM).color(fg)
            ]
            .spacing(12)
            .align_y(Alignment::Center);
            nav = nav.push(
                button(item)
                    .width(Length::Fill)
                    .padding([10, 14])
                    .on_press(Message::Navigate(page))
                    .style(move |_, status| button::Style {
                        background: Some(Background::Color(if active {
                            p.surface_alt
                        } else if status == button::Status::Hovered {
                            p.surface
                        } else {
                            iced::Color::TRANSPARENT
                        })),
                        text_color: fg,
                        border: Border { radius: theme::RADIUS_SMALL.into(), ..Border::default() },
                        ..button::Style::default()
                    }),
            );
        }
        let version = widgets::small(p, format!("{} {}", self.ctx.t("Version"), env!("CARGO_PKG_VERSION")));
        container(column![brand, nav, iced::widget::space::vertical(), version].spacing(28))
            .padding(20)
            .width(232)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.sidebar)),
                border: Border { width: 0.0, ..Border::default() },
                ..container::Style::default()
            })
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let escape = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            } => Some(Message::Escape),
            _ => None,
        });
        Subscription::batch([escape, home::subscription(&self.home, &self.ctx)])
    }
}

/// Build the tray summary from a fresh report.
pub fn status_of(report: &Report, score: &Score, now: u64) -> secblitz::status::Status {
    use crate::advice::{self, Group};
    let attention: Vec<String> = report
        .results
        .iter()
        .filter(|r| advice::for_outcome(r).group == Group::Recommended)
        .map(|r| r.id.clone())
        .take(64)
        .collect();
    secblitz::status::Status {
        schema: secblitz::status::SCHEMA,
        t: now,
        protected: score.protected as u32,
        total: score.total as u32,
        state: match score.verdict() {
            app::score::Verdict::Protected => secblitz::status::State::Ok,
            app::score::Verdict::Attention => secblitz::status::State::Attention,
            app::score::Verdict::Unknown => secblitz::status::State::Unknown,
        },
        attention,
    }
}

pub fn run(options: Options) -> anyhow::Result<()> {
    let mut application = iced::application(move || App::new(options.clone()), App::update, App::view)
        .title(|app: &App| app.ctx.t("Secblitz"))
        .theme(|app: &App| app.ctx.palette.theme())
        .subscription(App::subscription)
        .window_size((1100.0, 720.0))
        .default_font(theme::REGULAR)
        .antialiasing(true);
    for font in theme::FONT_FILES {
        application = application.font(font);
    }
    application.window(iced::window::Settings {
        size: iced::Size::new(1100.0, 720.0),
        min_size: Some(iced::Size::new(880.0, 600.0)),
        ..Default::default()
    })
    .run()
    .map_err(|e| anyhow::anyhow!("{e}"))
}
