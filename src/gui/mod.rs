//! The Secblitz window (iced, CPU renderer).
pub mod icons;
pub mod pages;
mod persist;
pub mod render;
mod tasks;
pub mod theme;
mod window;
pub mod widgets;

use crate::app::{self, score::Score, worker};
use crate::i18n::Lang;
use iced::widget::{button, column, container, row, scrollable, stack, text};
use iced::{keyboard, Alignment, Background, Border, Element, Length, Subscription, Task};
use icons::Icon;
use pages::{debloat, fixes, fixflow, history, home, settings, tools, web};
use secblitz::engine::Report;
use std::path::PathBuf;
use std::sync::Arc;
use theme::{Palette, Tone};

pub use persist::wait_persisted;
pub use tasks::{blocking, blocking_stream, ticks_100ms};
use persist::{forget_check, persist, Cache};
use window::window_icon;

#[derive(Debug, Clone)]
pub struct Options {
    pub lang: Lang,
    pub broker: Option<String>,
    pub start: Option<Page>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Home,
    Fixes,
    Debloat,
    Web,
    Tools,
    History,
    Settings,
}

impl Page {
    pub const ALL: [Page; 7] = [
        Page::Home,
        Page::Fixes,
        Page::Debloat,
        Page::Web,
        Page::Tools,
        Page::History,
        Page::Settings,
    ];
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "home" => Page::Home,
            "fixes" => Page::Fixes,
            "debloat" => Page::Debloat,
            "web" => Page::Web,
            "tools" => Page::Tools,
            "history" => Page::History,
            "settings" => Page::Settings,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Fixes => "Protection",
            Page::Debloat => "Clean up apps",
            Page::Web => "Web protection",
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
            Page::Web => Icon::Globe,
            Page::Tools => Icon::Toolbox,
            Page::History => Icon::History,
            Page::Settings => Icon::Settings,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CheckProgress {
    pub phase: Option<worker::Phase>,
    pub items: Vec<(String, String)>,
}

/// Whether the helper that acts with the person's normal rights is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Helper {
    Ready,
    /// Started in an unusual way; opening Secblitz from its shortcut fixes it.
    Reopen,
    /// Built-in Administrator or UAC turned off: there are no normal rights to
    /// act with, so per-account work is not possible. Pages still open.
    NotOnThisAccount,
}

pub struct Ctx {
    pub lang: Lang,
    pub palette: Palette,
    pub worker: worker::Worker,
    pub catalog: worker::Catalog,
    pub engine_error: Option<String>,
    pub report: Option<Arc<Report>>,
    pub check_error: Option<String>,
    pub checked_at: Option<u64>,
    pub checking: Option<CheckProgress>,
    pub busy: bool,
    pub broker: Option<Arc<crate::broker::Client>>,
    pub helper: Helper,
    pub state_dir: Option<PathBuf>,
    pub prefs: app::settings::Prefs,
    pub toast: Option<(String, Tone)>,
    pub explain_open: Option<String>,
}

impl Ctx {
    pub fn t(&self, key: &str) -> String {
        self.lang.t(key)
    }

    /// Something is about to change the PC: the next opening must check again
    /// instead of showing the saved check.
    pub fn forget_check(&self) {
        forget_check(self.state_dir.clone());
    }
    pub fn score(&self) -> Option<Score> {
        self.report.as_deref().map(Score::of)
    }
    /// Settings pages and web links can open: through the helper, or directly
    /// when this account has no normal rights to open them with.
    pub fn can_open_pages(&self) -> bool {
        self.helper != Helper::Reopen
    }

    pub fn broker_task(
        &self,
        request: crate::broker::Request,
        map: impl Fn(Result<crate::broker::Reply, String>) -> Message + Send + 'static,
    ) -> Task<Message> {
        let Some(client) = self.broker.clone() else {
            return match request.page() {
                Some(action) if self.helper == Helper::NotOnThisAccount => Task::perform(
                    blocking(move || {
                        secblitz::actions::run(action)
                            .map(|_| crate::broker::Reply::Done)
                            .map_err(|e| format!("{e:#}"))
                    }),
                    map,
                ),
                _ => Task::done(map(Err("unavailable".into()))),
            };
        };
        if !request.is_read_only() {
            self.forget_check();
        }
        // The page is opened by the launcher, which is not the foreground
        // process, so Windows would put it behind this window. This window
        // has the focus (the person just clicked), so it may hand that on.
        #[cfg(windows)]
        if request.opens_window() {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                AllowSetForegroundWindow, ASFW_ANY,
            };
            // SAFETY: plain Win32 call with no pointers.
            unsafe { AllowSetForegroundWindow(ASFW_ANY) };
        }
        Task::perform(
            blocking(move || client.send(request).map_err(|e| format!("{e:#}"))),
            map,
        )
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Page),
    CheckNow,
    Worker(worker::Event),
    ReviewFixes(Vec<String>),
    ReviewUndo,
    Escape,
    Noop,
    CloseRequested(iced::window::Id),
    Toast(String, Tone),
    Explain(String),
    DismissToast,
    ToastTick(std::time::Instant),
    ToastGone,
    PageFrame(std::time::Instant),
    Tab(bool),
    Home(home::Msg),
    Fixes(fixes::Msg),
    Fix(fixflow::Msg),
    Debloat(debloat::Msg),
    Web(web::Msg),
    Tools(tools::Msg),
    History(history::Msg),
    Settings(settings::Msg),
    PageOpened(crate::guide::Page, bool),
    WindowFocus(bool),
}

const RECHECK_WINDOW: std::time::Duration = std::time::Duration::from_secs(30 * 60);

#[derive(Debug, Default)]
struct Recheck {
    opened: Option<std::time::Instant>,
    left: bool,
}

impl Recheck {
    fn arm(&mut self, now: std::time::Instant, focused: bool) {
        self.opened = Some(now);
        self.left = !focused;
    }

    fn focus(&mut self, focused: bool, now: std::time::Instant, idle: bool) -> bool {
        let Some(at) = self.opened else {
            return false;
        };
        if now.saturating_duration_since(at) > RECHECK_WINDOW {
            *self = Self::default();
            return false;
        }
        if !focused {
            self.left = true;
            return false;
        }
        if !self.left || !idle {
            return false;
        }
        *self = Self::default();
        true
    }
}

pub struct App {
    pub page: Page,
    pub ctx: Ctx,
    pub home: home::State,
    pub fixes: fixes::State,
    pub fix: fixflow::State,
    pub debloat: debloat::State,
    pub web: web::State,
    pub tools: tools::State,
    pub history: history::State,
    pub settings: settings::State,
    toast_seen: Option<(String, std::time::Instant)>,
    toast_leaving: bool,
    entered: Option<std::time::Instant>,
    enter_t: f32,
    warmed: [bool; 3],
    flight: [bool; 3],
    warm_at: [Option<std::time::Instant>; 3],
    recheck: Recheck,
    focused: bool,
    user: Option<String>,
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
        let helper = if broker.is_some() {
            Helper::Ready
        } else if crate::launcher::has_split_token() {
            Helper::Reopen
        } else {
            Helper::NotOnThisAccount
        };
        let state_dir = secblitz::platform::app_dir().ok();
        let user = crate::launcher::user_sid();
        let now = app::history::now();
        let cached = match (&state_dir, &user, app::last_check::boot_time(now)) {
            (Some(dir), Some(user), Some(boot)) => app::last_check::load(dir, user, now, boot),
            _ => None,
        };
        let checked_at = cached.as_ref().map(|(_, at)| *at);
        let report = cached.map(|(report, _)| Arc::new(report));
        let ctx = Ctx {
            lang,
            palette: Palette::of(mode),
            worker: worker.clone(),
            catalog: worker::Catalog::default(),
            engine_error: None,
            check_error: None,
            checked_at,
            checking: report.is_none().then(CheckProgress::default),
            report,
            busy: false,
            broker,
            helper,
            state_dir,
            prefs,
            toast: None,
            explain_open: None,
        };
        let mut app = App {
            page: options.start.unwrap_or_default(),
            ctx,
            home: Default::default(),
            fixes: Default::default(),
            fix: Default::default(),
            debloat: Default::default(),
            web: Default::default(),
            tools: Default::default(),
            history: Default::default(),
            settings: Default::default(),
            toast_seen: None,
            toast_leaving: false,
            entered: None,
            enter_t: 1.0,
            warmed: [false; 3],
            flight: [false; 3],
            warm_at: [None; 3],
            recheck: Recheck::default(),
            focused: true,
            user,
        };
        let opened = Task::run(worker.opened(), Message::Worker);
        let first_check = if app.ctx.checking.is_some() {
            Task::run(worker.run(worker::Job::Check), Message::Worker)
        } else {
            Task::none()
        };
        // Opening straight on a page (hidden `--self-test`) must load it too.
        let enter = app.enter_page(app.page);
        let web_state = web::on_enter(&mut app.web, &mut app.ctx);
        let pending =
            Task::perform(blocking(secblitz::debloat::offline::finish_pending), |_| ()).discard();
        (app, Task::batch([opened, first_check, enter, web_state, pending]))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let was_busy = self.ctx.busy;
        let task = self.handle(message);
        // A fix, undo, removal or repair may change what a check finds: the
        // next opening must check again (a fix or undo saves its own re-check).
        if self.ctx.busy && !was_busy {
            self.ctx.forget_check();
        }
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(page) => {
                if page == self.page {
                    return Task::none();
                }
                self.page = page;
                self.ctx.explain_open = None;
                self.begin_entrance();
                Task::batch([
                    self.enter_page(page),
                    iced::widget::operation::snap_to(
                        PAGE_SCROLL,
                        iced::widget::operation::RelativeOffset::START,
                    ),
                ])
            }
            Message::PageFrame(now) => {
                self.step_entrance(now);
                Task::none()
            }
            Message::Tab(back) => {
                if back {
                    iced::widget::operation::focus_previous()
                } else {
                    iced::widget::operation::focus_next()
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
            Message::Explain(key) => {
                self.ctx.explain_open = match self.ctx.explain_open.take() {
                    Some(open) if open == key => None,
                    _ => Some(key),
                };
                Task::none()
            }
            Message::Escape => {
                if self.fix.is_open() {
                    return fixflow::escape(&mut self.fix, &mut self.ctx);
                }
                if self.ctx.explain_open.take().is_some() {
                    return Task::none();
                }
                match self.page {
                    Page::Debloat => debloat::escape(&mut self.debloat),
                    Page::Tools => tools::escape(&mut self.tools),
                    Page::Settings => settings::escape(&mut self.settings),
                    _ => {}
                }
                Task::none()
            }
            Message::Toast(text, tone) => {
                self.ctx.toast = Some((text, tone));
                self.toast_leaving = false;
                Task::none()
            }
            Message::Noop => Task::none(),
            Message::PageOpened(page, ok) => {
                let text = if ok {
                    self.recheck.arm(std::time::Instant::now(), self.focused);
                    self.ctx.t("Opened in a new window.")
                } else {
                    crate::guide::failure_text(self.ctx.lang, page)
                };
                self.update(Message::Toast(text, if ok { Tone::Good } else { Tone::Warn }))
            }
            Message::WindowFocus(focused) => {
                self.focused = focused;
                let idle = self.ctx.checking.is_none() && !self.ctx.busy;
                if self
                    .recheck
                    .focus(focused, std::time::Instant::now(), idle)
                {
                    self.update(Message::CheckNow)
                } else {
                    Task::none()
                }
            }
            Message::DismissToast => self.begin_toast_exit(),
            Message::ToastGone => {
                if self.toast_leaving {
                    self.ctx.toast = None;
                    self.toast_seen = None;
                    self.toast_leaving = false;
                }
                Task::none()
            }
            Message::ToastTick(now) => {
                let current = self.ctx.toast.as_ref().map(|(text, _)| text.clone());
                match (current, &self.toast_seen) {
                    (None, _) => self.toast_seen = None,
                    (Some(text), Some((seen, _))) if *seen == text => {}
                    (Some(text), _) => self.toast_seen = Some((text, now)),
                }
                if self.toast_leaving {
                    return Task::none();
                }
                if let Some((_, since)) = &self.toast_seen {
                    if now.duration_since(*since).as_secs() >= TOAST_SECONDS {
                        return self.begin_toast_exit();
                    }
                }
                Task::none()
            }
            Message::Home(m) => home::update(&mut self.home, m, &mut self.ctx),
            Message::Fixes(m) => fixes::update(&mut self.fixes, m, &mut self.ctx),
            Message::Fix(m) => fixflow::update(&mut self.fix, m, &mut self.ctx),
            Message::Debloat(m) => {
                if let debloat::Msg::Scanned(g, _) = &m {
                    if debloat::is_current_scan(&self.debloat, *g) {
                        self.flight[WARM_DEBLOAT] = false;
                    }
                }
                debloat::update(&mut self.debloat, m, &mut self.ctx)
            }
            Message::Web(m) => web::update(&mut self.web, m, &mut self.ctx),
            Message::Tools(m) => tools::update(&mut self.tools, m, &mut self.ctx),
            Message::History(m) => {
                if matches!(m, history::Msg::Loaded(..)) {
                    self.flight[WARM_HISTORY] = false;
                }
                history::update(&mut self.history, m, &mut self.ctx)
            }
            Message::Settings(m) => {
                if matches!(m, settings::Msg::Loaded { .. }) {
                    self.flight[WARM_SETTINGS] = false;
                }
                settings::update(&mut self.settings, m, &mut self.ctx)
            }
        }
    }

    fn on_worker(&mut self, event: worker::Event) -> Task<Message> {
        use worker::Event as E;
        match &event {
            E::Opened(Ok(catalog)) => self.ctx.catalog = catalog.clone(),
            E::Opened(Err(e)) => {
                self.ctx.engine_error = Some(self.ctx.t(crate::launcher::friendly_problem(e)));
            }
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
        let warm = if matches!(&event, E::Opened(Ok(_))) {
            self.preload_all()
        } else {
            Task::none()
        };
        Task::batch([
            fixflow::on_worker(&mut self.fix, &event, &mut self.ctx),
            history::on_worker(&mut self.history, &event, &mut self.ctx),
            warm,
        ])
    }

    fn assessed(&mut self, outcome: &worker::Outcome, kind: app::history::Kind, n: usize) {
        let operation = kind != app::history::Kind::Check;
        let now = app::history::now();
        match outcome {
            Ok(report) => {
                self.ctx.report = Some(report.clone());
                self.ctx.check_error = None;
                self.ctx.checked_at = Some(now);
                let score = Score::of(report);
                let entry = (!operation || n > 0).then(|| self.entry(now, kind, &score, n));
                let cache = match &self.user {
                    Some(user) => Cache::Save(user.clone(), now, report.clone()),
                    None => Cache::Keep,
                };
                persist(
                    self.ctx.state_dir.clone(),
                    entry,
                    status_of(report, &score, now),
                    cache,
                );
            }
            Err(e) => {
                self.ctx.check_error =
                    Some(self.ctx.t(crate::launcher::friendly_check_problem(e)));
                let entry = if operation && n > 0 {
                    self.ctx
                        .report
                        .as_deref()
                        .map(|report| self.entry(now, kind, &Score::of(report), n))
                } else {
                    None
                };
                // A failed check must not leave the tray showing "protected".
                persist(
                    self.ctx.state_dir.clone(),
                    entry,
                    secblitz::status::summarize(&[], false, now),
                    Cache::Forget,
                );
            }
        }
    }

    fn entry(
        &self,
        t: u64,
        kind: app::history::Kind,
        score: &Score,
        n: usize,
    ) -> app::history::Entry {
        app::history::Entry {
            t,
            kind,
            protected: score.protected,
            total: score.total,
            n,
        }
    }

    fn on_page(&self, page: Page, sub: Subscription<Message>) -> Subscription<Message> {
        if self.page == page {
            sub
        } else {
            Subscription::none()
        }
    }

    fn enter_page(&mut self, page: Page) -> Task<Message> {
        match page {
            Page::History => self.revisit(WARM_HISTORY),
            Page::Debloat => self.revisit(WARM_DEBLOAT),
            Page::Settings => self.revisit(WARM_SETTINGS),
            Page::Web => web::on_enter(&mut self.web, &mut self.ctx),
            Page::Tools => tools::on_enter(&mut self.tools, &mut self.ctx),
            _ => Task::none(),
        }
    }

    /// Entering a page again: reload only when the last load is stale.
    fn revisit(&mut self, which: usize) -> Task<Message> {
        let fresh = self.warm_at[which].is_some_and(|t| t.elapsed() < WARM_TTL);
        if fresh {
            Task::none()
        } else {
            self.warm(which)
        }
    }

    fn warm(&mut self, which: usize) -> Task<Message> {
        if self.flight[which] {
            return Task::none();
        }
        self.flight[which] = true;
        self.warm_at[which] = Some(std::time::Instant::now());
        let first = !std::mem::replace(&mut self.warmed[which], true);
        let task = match (which, first) {
            (WARM_HISTORY, true) => history::on_enter(&mut self.history, &mut self.ctx),
            (WARM_HISTORY, false) => history::preload(&mut self.history, &mut self.ctx),
            (WARM_DEBLOAT, true) => debloat::on_enter(&mut self.debloat, &mut self.ctx),
            (WARM_DEBLOAT, false) => debloat::preload(&mut self.debloat, &mut self.ctx),
            (_, true) => settings::on_enter(&mut self.settings, &mut self.ctx),
            (_, false) => settings::preload(&mut self.settings, &mut self.ctx),
        };
        // A page that declined to load (sheet open) must not stay "in flight".
        if self.page_declined(which) {
            self.flight[which] = false;
        }
        task
    }

    fn page_declined(&self, which: usize) -> bool {
        which == WARM_DEBLOAT && self.debloat_busy()
    }

    fn debloat_busy(&self) -> bool {
        debloat::is_busy(&self.debloat)
    }

    fn preload_all(&mut self) -> Task<Message> {
        Task::batch([
            self.warm(WARM_HISTORY),
            self.warm(WARM_DEBLOAT),
            self.warm(WARM_SETTINGS),
        ])
    }

    fn begin_entrance(&mut self) {
        if widgets::anim::reduced() {
            self.finish_entrance();
            return;
        }
        self.entered = Some(std::time::Instant::now());
        self.enter_t = 0.0;
        self.apply_fade();
    }

    fn step_entrance(&mut self, now: std::time::Instant) {
        let Some(start) = self.entered else {
            return;
        };
        let t = widgets::appear::enter_progress(start, now);
        if t >= 1.0 {
            self.finish_entrance();
        } else {
            self.enter_t = t;
            self.apply_fade();
        }
    }

    fn finish_entrance(&mut self) {
        self.entered = None;
        self.enter_t = 1.0;
        self.ctx.palette = Palette::of(self.ctx.palette.mode);
    }

    fn apply_fade(&mut self) {
        let base = Palette::of(self.ctx.palette.mode);
        self.ctx.palette = widgets::appear::fade_palette(&base, base.bg, 0.3 + 0.7 * self.enter_t);
    }

    fn begin_toast_exit(&mut self) -> Task<Message> {
        if self.ctx.toast.is_none() || self.toast_leaving {
            return Task::none();
        }
        if widgets::anim::reduced() {
            self.ctx.toast = None;
            self.toast_seen = None;
            return Task::none();
        }
        self.toast_leaving = true;
        let wait = widgets::anim::FAST + std::time::Duration::from_millis(40);
        Task::perform(blocking(move || std::thread::sleep(wait)), |_| {
            Message::ToastGone
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let p = Palette::of(self.ctx.palette.mode);
        let content: Element<'_, Message> = match self.page {
            Page::Home => home::view(&self.home, &self.ctx),
            Page::Fixes => fixes::view(&self.fixes, &self.ctx),
            Page::Debloat => debloat::view(&self.debloat, &self.ctx),
            Page::Web => web::view(&self.web, &self.ctx),
            Page::Tools => tools::view(&self.tools, &self.ctx),
            Page::History => history::view(&self.history, &self.ctx),
            Page::Settings => settings::view(&self.settings, &self.ctx),
        };
        let fills = match self.page {
            Page::Home => home::fills_window(&self.ctx),
            Page::Fixes => fixes::fills_window(&self.ctx),
            _ => false,
        };
        let column_content = widgets::appear::lift(
            container(content)
                .max_width(PAGE_MAX_WIDTH)
                .width(Length::Fill)
                .height(if fills { Length::Fill } else { Length::Shrink }),
            widgets::appear::ENTER_RISE * (1.0 - self.enter_t),
        );
        let footer = match self.page {
            Page::Debloat => debloat::footer(&self.debloat, &self.ctx),
            _ => None,
        };
        let footer_bar: Element<'_, Message> = match footer {
            Some(bar) => column![
                container(iced::widget::space::horizontal())
                    .height(theme::HAIRLINE)
                    .width(Length::Fill)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(p.border)),
                        ..container::Style::default()
                    }),
                container(container(bar).max_width(PAGE_MAX_WIDTH).width(Length::Fill))
                    .center_x(Length::Fill)
                    .padding([theme::S4, theme::S10])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(p.surface)),
                        ..container::Style::default()
                    }),
            ]
            .into(),
            None => iced::widget::space().into(),
        };
        let page = container(column_content)
            .center_x(Length::Fill)
            .padding([theme::S8, theme::S10])
            .width(Length::Fill);
        let scroll: Element<'_, Message> = if fills {
            page.height(Length::Fill).into()
        } else {
            container(
                scrollable(page)
                    .id(PAGE_SCROLL)
                    .direction(widgets::controls::scrollbar())
                    .style(widgets::controls::scroll_style(p)),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };
        let main = container(column![scroll, footer_bar])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.bg)),
                ..container::Style::default()
            });
        let body: Element<'_, Message> = row![self.sidebar(), main].into();
        let none = || -> Element<'_, Message> { iced::widget::space().into() };
        let modal = match self.page {
            Page::Debloat => debloat::modal(&self.debloat, &self.ctx),
            Page::Tools => tools::modal(&self.tools, &self.ctx),
            Page::Settings => settings::modal(&self.settings, &self.ctx),
            _ => None,
        };
        let modal_layer = match modal {
            Some(content) => widgets::sheet_layer(p, content),
            None => none(),
        };
        let fix_layer = match fixflow::overlay_content(&self.fix, &self.ctx) {
            Some(content) => widgets::sheet_layer(p, content),
            None => none(),
        };
        let toast_layer = match &self.ctx.toast {
            Some((message, tone)) => container(widgets::toast(
                p,
                message.clone(),
                *tone,
                self.toast_leaving,
            ))
            .center_x(Length::Fill)
            .align_bottom(Length::Fill)
            .padding(theme::S6)
            .into(),
            None => none(),
        };
        stack![body, modal_layer, fix_layer, toast_layer].into()
    }

    fn verdict_tone(&self) -> Tone {
        match self.ctx.report.as_deref().map(app::score::overall) {
            Some(app::score::Verdict::Protected) => Tone::Good,
            Some(app::score::Verdict::Attention) => Tone::Warn,
            _ => Tone::Neutral,
        }
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let p = Palette::of(self.ctx.palette.mode);
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
        let mut nav = column![].spacing(NAV_GAP);
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
                        .width(theme::DOT)
                        .height(theme::DOT)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(dot)),
                            border: Border {
                                radius: theme::R_PILL.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        }),
                );
            }
            nav = nav.push(widgets::arrow(
                widgets::press::button(container(item).center_y(Length::Fill))
                    .width(Length::Fill)
                    .height(theme::CONTROL)
                    .padding([0.0, theme::S3])
                    .scale(false)
                    .focus_color(p.focus_ring)
                    .on_press(Message::Navigate(page))
                    .style(move |_, status| button::Style {
                        background: match status {
                            button::Status::Pressed => Some(Background::Color(p.pressed)),
                            button::Status::Hovered if !active => {
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
        let index = Page::ALL.iter().position(|q| *q == self.page).unwrap_or(0);
        let nav = stack![
            widgets::slide_marker(p, index, Page::ALL.len(), theme::CONTROL, NAV_GAP),
            nav
        ]
        .width(Length::Fill);
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
            keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Tab),
                modifiers,
                ..
            } => Some(Message::Tab(modifiers.shift())),
            _ => None,
        });
        let entrance = if self.entered.is_some() {
            iced::window::frames().map(Message::PageFrame)
        } else {
            Subscription::none()
        };
        let toast = if self.ctx.toast.is_some() && !self.toast_leaving {
            ticks_100ms().map(Message::ToastTick)
        } else {
            Subscription::none()
        };
        let focus = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(iced::window::Event::Focused) => Some(Message::WindowFocus(true)),
            iced::Event::Window(iced::window::Event::Unfocused) => {
                Some(Message::WindowFocus(false))
            }
            _ => None,
        });
        Subscription::batch([
            escape,
            focus,
            iced::window::close_requests().map(Message::CloseRequested),
            toast,
            entrance,
            // Frame clocks run only for the page on screen: a job started on
            // Tools must not keep the whole window redrawing from another page.
            // Each page catches up on its next frame when it is shown again.
            self.on_page(Page::Home, home::subscription(&self.home, &self.ctx)),
            self.on_page(Page::Fixes, fixes::subscription(&self.ctx)),
            fixflow::subscription(&self.fix),
            self.on_page(Page::Debloat, debloat::subscription(&self.debloat)),
            self.on_page(Page::Web, web::subscription()),
            self.on_page(Page::Tools, tools::subscription(&self.tools, &self.ctx)),
            self.on_page(Page::Settings, settings::subscription(&self.settings)),
        ])
    }
}

const NAV_GAP: f32 = theme::S1;
const PAGE_SCROLL: &str = "page-scroll";
const WARM_TTL: std::time::Duration = std::time::Duration::from_secs(60);

const WARM_HISTORY: usize = 0;
const WARM_DEBLOAT: usize = 1;
const WARM_SETTINGS: usize = 2;

const PAGE_MAX_WIDTH: f32 = 960.0;
const TOAST_SECONDS: u64 = 4;

pub fn status_of(report: &Report, score: &Score, now: u64) -> secblitz::status::Status {
    let attention: Vec<String> = app::score::to_check_ids(report)
        .into_iter()
        .take(64)
        .collect();
    secblitz::status::Status {
        schema: secblitz::status::SCHEMA,
        t: now,
        protected: score.protected as u32,
        total: score.total as u32,
        state: match app::score::overall(report) {
            app::score::Verdict::Protected => secblitz::status::State::Ok,
            app::score::Verdict::Attention => secblitz::status::State::Attention,
            app::score::Verdict::Unknown => secblitz::status::State::Unknown,
        },
        attention,
    }
}

pub fn run(options: Options) -> anyhow::Result<()> {
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
            min_size: Some(iced::Size::new(
                theme::WINDOW_MIN_WIDTH,
                theme::WINDOW_MIN_HEIGHT,
            )),
            exit_on_close_request: false,
            icon: window_icon(),
            ..Default::default()
        })
        .run()
        .map_err(|e| anyhow::anyhow!("{e}"))
}
#[cfg(test)]
mod recheck_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn returning_to_the_window_rechecks_once() {
        let t0 = Instant::now();
        let mut r = Recheck::default();
        assert!(!r.focus(true, t0, true), "nothing opened yet");
        r.arm(t0, true);
        assert!(!r.focus(true, t0, true), "focus before leaving does nothing");
        assert!(!r.focus(false, t0, true));
        assert!(r.focus(true, t0 + Duration::from_secs(60), true));
        assert!(
            !r.focus(false, t0 + Duration::from_secs(70), true)
                && !r.focus(true, t0 + Duration::from_secs(80), true),
            "at most once per open"
        );
    }

    #[test]
    fn unfocused_before_armed_still_counts_as_left() {
        let t0 = Instant::now();
        let mut r = Recheck::default();
        r.arm(t0, false);
        assert!(r.focus(true, t0 + Duration::from_secs(20), true));
    }

    #[test]
    fn busy_waits_and_late_returns_are_ignored() {
        let t0 = Instant::now();
        let mut r = Recheck::default();
        r.arm(t0, true);
        assert!(!r.focus(false, t0, true));
        assert!(!r.focus(true, t0 + Duration::from_secs(5), false), "busy");
        assert!(r.focus(true, t0 + Duration::from_secs(9), true), "still armed");
        r.arm(t0, true);
        assert!(!r.focus(false, t0, true));
        assert!(!r.focus(true, t0 + RECHECK_WINDOW + Duration::from_secs(1), true));
        assert!(!r.focus(true, t0 + Duration::from_secs(5), true), "disarmed");
    }
}
