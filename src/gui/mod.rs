//! The Secblitz window (iced, CPU renderer).
//!
//! OWNER: design-system agent (routing below is the shared contract; page agents add
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
pub mod render;
pub mod theme;
pub mod widgets;

use crate::app::{self, score::Score, worker};
use crate::i18n::Lang;
use iced::futures::channel::{mpsc, oneshot};
use iced::futures::{Future, Stream};
use iced::widget::{button, column, container, row, scrollable, stack, text};
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
    CloseRequested(iced::window::Id),
    Toast(String, Tone),
    DismissToast,
    /// Slow clock used to auto-dismiss toasts.
    ToastTick(std::time::Instant),
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
    /// The toast currently shown and when it was first seen (auto-dismiss).
    toast_seen: Option<(String, std::time::Instant)>,
}

/// Run a blocking closure on a fresh thread and await its result.
pub fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = T> + Send + 'static {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    // A panicked closure drops the sender: keep the UI alive rather than
    // panicking inside the executor (the page's own error handling applies).
    async move {
        match rx.await {
            Ok(value) => value,
            Err(_) => std::future::pending().await,
        }
    }
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
            state_dir: secblitz::platform::app_dir().ok(),
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
            toast_seen: None,
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
                    Page::Settings => settings::on_enter(&mut self.settings, &mut self.ctx),
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
            Message::CloseRequested(id) => {
                // A fix, undo, removal or repair must not be cut off halfway.
                if self.ctx.busy {
                    Task::none()
                } else {
                    iced::window::close(id)
                }
            }
            Message::Escape => {
                if self.fix.is_open() {
                    return fixflow::escape(&mut self.fix, &mut self.ctx);
                }
                match self.page {
                    Page::Debloat => debloat::escape(&mut self.debloat),
                    Page::Tools => tools::escape(&mut self.tools),
                    _ => {}
                }
                Task::none()
            }
            Message::Toast(text, tone) => {
                self.ctx.toast = Some((text, tone));
                Task::none()
            }
            Message::DismissToast => {
                self.ctx.toast = None;
                self.toast_seen = None;
                Task::none()
            }
            Message::ToastTick(now) => {
                // Pages may set `ctx.toast` directly: stamp whatever is shown.
                let current = self.ctx.toast.as_ref().map(|(text, _)| text.clone());
                match (current, &self.toast_seen) {
                    (None, _) => self.toast_seen = None,
                    (Some(text), Some((seen, _))) if *seen == text => {}
                    (Some(text), _) => self.toast_seen = Some((text, now)),
                }
                if let Some((_, since)) = &self.toast_seen {
                    if now.duration_since(*since).as_secs() >= TOAST_SECONDS {
                        self.ctx.toast = None;
                        self.toast_seen = None;
                    }
                }
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
            E::Applied {
                attempted,
                result,
                verify,
            } => {
                self.ctx.checking = None;
                // Count only fixes that really took effect.
                let n = match result {
                    Ok(r) => attempted
                        .iter()
                        .filter(|id| {
                            r.results.iter().any(|o| {
                                o.id == **id && (o.status == "applied" || o.status == "unchanged")
                            })
                        })
                        .count(),
                    Err(_) => 0,
                };
                self.assessed(verify, app::history::Kind::Fix, n);
            }
            E::Undone { result, verify } => {
                self.ctx.checking = None;
                let n = match result {
                    Ok(r) => r.results.iter().filter(|o| o.status == "restored").count(),
                    Err(_) => 0,
                };
                self.assessed(verify, app::history::Kind::Undo, n);
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
    /// Fix and undo entries are logged only when something changed (`n > 0`),
    /// even if the post-check failed (then with the last known score).
    fn assessed(&mut self, outcome: &worker::Outcome, kind: app::history::Kind, n: usize) {
        let operation = kind != app::history::Kind::Check;
        let now = app::history::now();
        match outcome {
            Ok(report) => {
                self.ctx.report = Some(report.clone());
                self.ctx.check_error = None;
                self.ctx.checked_at = Some(now);
                let score = Score::of(report);
                if !operation || n > 0 {
                    self.record(now, kind, &score, n);
                }
                let _ = secblitz::status::write(&status_of(report, &score, now));
            }
            Err(e) => {
                self.ctx.check_error = Some(e.clone());
                if operation && n > 0 {
                    if let Some(report) = self.ctx.report.clone() {
                        let score = Score::of(&report);
                        self.record(now, kind, &score, n);
                    }
                }
                // A failed check must not leave the tray showing "protected".
                let _ = secblitz::status::write(&secblitz::status::summarize(&[], false, now));
            }
        }
    }

    fn record(&self, t: u64, kind: app::history::Kind, score: &Score, n: usize) {
        if let Some(dir) = &self.ctx.state_dir {
            let _ = app::history::record(
                dir,
                &app::history::Entry {
                    t,
                    kind,
                    protected: score.protected,
                    total: score.total,
                    n,
                },
            );
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
        // Content is centred with a readable maximum width.
        let column_content = container(content)
            .max_width(PAGE_MAX_WIDTH)
            .width(Length::Fill);
        let main = container(
            scrollable(
                container(column_content)
                    .center_x(Length::Fill)
                    .padding([theme::S8, theme::S10])
                    .width(Length::Fill),
            )
            .direction(widgets::controls::scrollbar())
            .style(widgets::controls::scroll_style(p)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.bg)),
            ..container::Style::default()
        });
        let body: Element<'_, Message> = row![self.sidebar(), main].into();
        // Page sheets (clean-up apps, tools) sit above the whole window.
        let modal = match self.page {
            Page::Debloat => debloat::modal(&self.debloat, &self.ctx),
            Page::Tools => tools::modal(&self.tools, &self.ctx),
            _ => None,
        };
        let body = match modal {
            Some(content) => widgets::sheet(p, body, content),
            None => body,
        };
        // The fix flow (review sheet / working / result) draws over any page.
        let base = fixflow::overlay(&self.fix, &self.ctx, body);
        match &self.ctx.toast {
            Some((message, tone)) => stack![
                base,
                container(widgets::toast(p, message.clone(), *tone))
                    .center_x(Length::Fill)
                    .align_bottom(Length::Fill)
                    .padding(theme::S6)
            ]
            .into(),
            None => base,
        }
    }

    /// Colour of the small status dot next to Home.
    fn verdict_tone(&self) -> Tone {
        match self.ctx.score().map(|s| s.verdict()) {
            Some(app::score::Verdict::Protected) => Tone::Good,
            Some(app::score::Verdict::Attention) => Tone::Warn,
            _ => Tone::Neutral,
        }
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let p = self.ctx.palette;
        let brand = row![
            widgets::brand_mark(24.0, p.text),
            text("Secblitz")
                .size(theme::H2)
                .font(theme::SEMIBOLD)
                .color(p.text)
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center)
        .padding([0.0, theme::S3]);
        let mut nav = column![].spacing(theme::S1);
        for page in Page::ALL {
            let active = self.page == page;
            let fg = if active { p.text } else { p.text_muted };
            let glyph = if active {
                widgets::icon_filled(page.icon(), 18.0, fg)
            } else {
                widgets::icon(page.icon(), 18.0, fg)
            };
            let mut item = row![
                glyph,
                text(self.ctx.t(page.label()))
                    .size(theme::BODY)
                    .line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(
                        theme::LINE_BODY
                    )))
                    .font(if active {
                        theme::SEMIBOLD
                    } else {
                        theme::MEDIUM
                    })
                    .color(fg),
                iced::widget::space::horizontal(),
            ]
            .spacing(theme::S3)
            .align_y(Alignment::Center);
            if page == Page::Home && self.ctx.checking.is_none() && self.ctx.report.is_some() {
                let dot = p.tone(self.verdict_tone());
                item = item.push(
                    container(iced::widget::space::horizontal())
                        .width(8)
                        .height(8)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(dot)),
                            border: Border {
                                radius: 4.0.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        }),
                );
            }
            nav = nav.push(widgets::arrow(
                button(container(item).center_y(Length::Fill))
                    .width(Length::Fill)
                    .height(theme::CONTROL)
                    .padding([0.0, theme::S3])
                    .on_press(Message::Navigate(page))
                    .style(move |_, status| button::Style {
                        background: match (active, status) {
                            (_, button::Status::Pressed) => Some(Background::Color(p.pressed)),
                            (true, _) => Some(Background::Color(p.selected)),
                            (false, button::Status::Hovered) => {
                                Some(Background::Color(p.hover_strong))
                            }
                            _ => None,
                        },
                        text_color: fg,
                        border: Border {
                            radius: theme::R.into(),
                            ..Border::default()
                        },
                        shadow: iced::Shadow::default(),
                        snap: true,
                    }),
            ));
        }
        let version = container(widgets::small(
            p,
            format!("{} {}", self.ctx.t("Version"), env!("CARGO_PKG_VERSION")),
        ))
        .padding([0.0, theme::S3]);
        container(column![brand, nav, iced::widget::space::vertical(), version].spacing(theme::S6))
            .padding([theme::S6, theme::S3])
            .width(232)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.sidebar)),
                border: Border {
                    width: 0.0,
                    ..Border::default()
                },
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
        let toast = if self.ctx.toast.is_some() {
            ticks_500ms().map(Message::ToastTick)
        } else {
            Subscription::none()
        };
        Subscription::batch([
            escape,
            iced::window::close_requests().map(Message::CloseRequested),
            toast,
            home::subscription(&self.home, &self.ctx),
            fixflow::subscription(&self.fix),
            debloat::subscription(&self.debloat),
            tools::subscription(&self.tools, &self.ctx),
            settings::subscription(&self.settings),
        ])
    }
}

/// Widest the page content grows on large windows.
const PAGE_MAX_WIDTH: f32 = 960.0;
/// How long a toast stays on screen.
const TOAST_SECONDS: u64 = 4;

fn ticker(period: std::time::Duration) -> impl Stream<Item = std::time::Instant> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    std::thread::spawn(move || loop {
        std::thread::sleep(period);
        // The receiver is dropped when the subscription ends: stop then.
        if tx.unbounded_send(std::time::Instant::now()).is_err() {
            break;
        }
    });
    rx
}

/// Slow tick used for time-outs such as toast dismissal.
pub fn ticks_500ms() -> Subscription<std::time::Instant> {
    Subscription::run(|| ticker(std::time::Duration::from_millis(500)))
}

/// Draw the application icon (white shield with a check on a dark tile) as RGBA.
pub fn window_icon_rgba(size: u32) -> Vec<u8> {
    // Shield outline in a 24x24 design grid.
    const SHIELD: [(f32, f32); 10] = [
        (12.0, 2.5),
        (19.5, 5.5),
        (19.5, 12.0),
        (18.0, 15.8),
        (15.0, 19.0),
        (12.0, 21.5),
        (9.0, 19.0),
        (6.0, 15.8),
        (4.5, 12.0),
        (4.5, 5.5),
    ];
    fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
        let mut hit = false;
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let (xi, yi) = poly[i];
            let (xj, yj) = poly[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                hit = !hit;
            }
            j = i;
        }
        hit
    }
    fn near_segment(a: (f32, f32), b: (f32, f32), x: f32, y: f32, r: f32) -> bool {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let (px, py) = (a.0 + t * dx, a.1 + t * dy);
        (x - px).powi(2) + (y - py).powi(2) <= r * r
    }
    const SS: u32 = 3; // supersampling per axis
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    let scale = 24.0 / size as f32;
    let tile_radius = 5.0f32;
    for py in 0..size {
        for px in 0..size {
            let (mut tile, mut shield) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) * scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) * scale;
                    // Rounded tile covering the whole canvas.
                    let cx = x.clamp(tile_radius, 24.0 - tile_radius);
                    let cy = y.clamp(tile_radius, 24.0 - tile_radius);
                    if (x - cx).powi(2) + (y - cy).powi(2) <= tile_radius * tile_radius {
                        tile += 1;
                        let tick = near_segment((8.8, 12.2), (11.0, 14.4), x, y, 0.9)
                            || near_segment((11.0, 14.4), (15.4, 9.8), x, y, 0.9);
                        if inside(&SHIELD, x, y) && !tick {
                            shield += 1;
                        }
                    }
                }
            }
            let n = SS * SS;
            // Colours: tile #18181B, shield white.
            let mix = |bg: u32, fg: u32| (bg * (tile - shield) + fg * shield) / tile.max(1);
            out.extend_from_slice(&[
                mix(0x18, 0xFF) as u8,
                mix(0x18, 0xFF) as u8,
                mix(0x1B, 0xFF) as u8,
                (tile * 255 / n) as u8,
            ]);
        }
    }
    out
}

fn window_icon() -> Option<iced::window::Icon> {
    iced::window::icon::from_rgba(window_icon_rgba(64), 64, 64).ok()
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
    // GPU when a real adapter exists, tiny-skia otherwise (decided before iced starts).
    let renderer = render::select();
    let mut application =
        iced::application(move || App::new(options.clone()), App::update, App::view)
            .title(|app: &App| app.ctx.t("Secblitz"))
            .theme(|app: &App| app.ctx.palette.theme())
            .subscription(App::subscription)
            .window_size((1100.0, 720.0))
            .default_font(theme::REGULAR)
            .antialiasing(render::use_msaa(renderer));
    for font in theme::FONT_FILES {
        application = application.font(font);
    }
    application
        .window(iced::window::Settings {
            size: iced::Size::new(1100.0, 720.0),
            min_size: Some(iced::Size::new(880.0, 600.0)),
            exit_on_close_request: false,
            icon: window_icon(),
            ..Default::default()
        })
        .run()
        .map_err(|e| anyhow::anyhow!("{e}"))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_expected_size_and_shape() {
        let px = window_icon_rgba(32);
        assert_eq!(px.len(), 32 * 32 * 4);
        let at = |x: usize, y: usize| &px[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(at(0, 0)[3], 0, "corner is transparent");
        assert_eq!(at(16, 9)[..3], [255, 255, 255], "shield body is white");
        assert!(window_icon().is_some());
    }
}
