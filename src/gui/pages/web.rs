//! "Web protection": three switches that block ads, trackers and dangerous
//! websites, a plain status line, a one-hour pause and today's counts.
use crate::app::{history, settings as app_settings};
use secblitz::explain;
use crate::gui::icons::Icon;
use crate::gui::pages::home;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::hairline::{web_globe, Plate};
use crate::gui::widgets::{self, progress, ButtonKind};
use crate::gui::{blocking, Ctx, Message};
use crate::i18n::Lang;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length, Padding, Subscription, Task};
use secblitz::filter::config::{self, Config, ErrorCode, State as ListState, Status};
use secblitz::filter::control::ServiceState;
use std::time::{Duration, Instant};

type El<'a> = Element<'a, Message>;

const SECONDS_PER_DAY: u64 = 86_400;
const PAUSE: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    Ads,
    Tracking,
    Dangerous,
}

impl Switch {
    const ALL: [Switch; 3] = [Switch::Ads, Switch::Tracking, Switch::Dangerous];

    fn id(self) -> &'static str {
        match self {
            Switch::Ads => "web.ads",
            Switch::Tracking => "web.tracking",
            Switch::Dangerous => "web.dangerous",
        }
    }

    fn get(self, c: &Config) -> bool {
        match self {
            Switch::Ads => c.ads,
            Switch::Tracking => c.tracking,
            Switch::Dangerous => c.dangerous,
        }
    }

    fn set(self, c: &mut Config, on: bool) {
        match self {
            Switch::Ads => c.ads = on,
            Switch::Tracking => c.tracking = on,
            Switch::Dangerous => c.dangerous = on,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub config: Config,
    pub status: Option<Status>,
    pub service: ServiceState,
    pub installed: bool,
    pub now: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Busy {
    Switch(Switch),
    Pause,
    Resume,
    Retry,
}

#[derive(Debug, Default)]
pub struct State {
    snapshot: Option<Snapshot>,
    busy: Option<Busy>,
    polling: bool,
    generation: u32,
    open: Vec<Switch>,
    look: Option<(web_globe::Guard, Instant)>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Seen(Box<Snapshot>),
    Polled(u32, Box<Snapshot>),
    Toggle(Switch, bool),
    Pause,
    Resume,
    Retry,
    Done(u32, Result<(), String>),
    ToggleDetail(Switch),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Off,
    On,
    Paused(u64),
    GettingReady,
    NotWorking,
}

pub fn status_line(
    config: &Config,
    status: Option<&Status>,
    service: ServiceState,
    now: u64,
) -> Line {
    if !config.any_on() {
        return Line::Off;
    }
    if let Some(until) = config.paused_until.filter(|t| *t > now) {
        return Line::Paused(until);
    }
    if service != ServiceState::Running {
        return Line::NotWorking;
    }
    let Some(status) = status.filter(|s| config::fresh(s, now)) else {
        return Line::NotWorking;
    };
    if status.last_error == Some(ErrorCode::PortInUse) {
        return Line::NotWorking;
    }
    if !status.listening {
        return Line::GettingReady;
    }
    match status.state {
        ListState::Ready => Line::On,
        ListState::Starting | ListState::NoLists => Line::GettingReady,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusAction {
    Pause,
    Resume,
    Retry,
}

pub fn status_action(line: Line) -> Option<StatusAction> {
    match line {
        Line::Off => None,
        Line::Paused(_) => Some(StatusAction::Resume),
        Line::NotWorking => Some(StatusAction::Retry),
        Line::On | Line::GettingReady => Some(StatusAction::Pause),
    }
}

fn current_line(snapshot: &Snapshot) -> Line {
    status_line(
        &snapshot.config,
        snapshot.status.as_ref(),
        snapshot.service,
        snapshot.now,
    )
}

pub fn guard_of(line: Line) -> web_globe::Guard {
    use web_globe::Guard;
    match line {
        Line::On => Guard::On,
        Line::GettingReady => Guard::Starting,
        Line::Paused(_) => Guard::Paused,
        Line::Off => Guard::Off,
        Line::NotWorking => Guard::Broken,
    }
}

fn note_look(state: &mut State, snapshot: &Snapshot, now: Instant) {
    let guard = guard_of(current_line(snapshot));
    if state.look.map(|(g, _)| g) != Some(guard) {
        state.look = Some((guard, now));
    }
}

pub fn controls_enabled(snapshot: Option<&Snapshot>, busy: bool) -> bool {
    snapshot.is_some_and(|s| s.installed) && !busy
}

pub fn suggests(snapshot: &Snapshot) -> bool {
    snapshot.installed && !snapshot.config.any_on()
}

fn blocked_today(snapshot: &Snapshot) -> Option<[u64; 3]> {
    let status = snapshot.status.as_ref()?;
    if !snapshot.config.any_on() || !config::fresh(status, snapshot.now) {
        return None;
    }
    Some(if status.day == snapshot.now / SECONDS_PER_DAY {
        status.blocked
    } else {
        [0; 3]
    })
}


fn service_state() -> ServiceState {
    #[cfg(windows)]
    {
        secblitz::filter::scm::state().unwrap_or(ServiceState::Other)
    }
    #[cfg(not(windows))]
    {
        ServiceState::NotInstalled
    }
}

fn read_snapshot() -> Snapshot {
    let config = config::config_path()
        .map(|p| config::load_config(&p))
        .unwrap_or_default();
    let status = config::status_path()
        .ok()
        .and_then(|p| config::load_status(&p));
    Snapshot {
        config,
        status,
        service: service_state(),
        installed: app_settings::installed_exe().is_some(),
        now: history::now(),
    }
}

fn apply(config: Config) -> Result<(), String> {
    #[cfg(windows)]
    {
        secblitz::filter::control::apply_switches(config).map_err(|e| format!("{e:#}"))
    }
    #[cfg(not(windows))]
    {
        let _ = config;
        Err("unavailable".into())
    }
}

fn pause() -> Result<(), String> {
    #[cfg(windows)]
    {
        secblitz::filter::control::pause_for(PAUSE).map_err(|e| format!("{e:#}"))
    }
    #[cfg(not(windows))]
    {
        let _ = PAUSE;
        Err("unavailable".into())
    }
}

fn resume() -> Result<(), String> {
    #[cfg(windows)]
    {
        secblitz::filter::control::resume().map_err(|e| format!("{e:#}"))
    }
    #[cfg(not(windows))]
    {
        Err("unavailable".into())
    }
}

fn read_task(generation: u32) -> Task<Message> {
    Task::perform(blocking(read_snapshot), move |snapshot| {
        Message::Web(Msg::Polled(generation, Box::new(snapshot)))
    })
}

fn poll(state: &mut State) -> Task<Message> {
    if state.polling || state.busy.is_some() {
        return Task::none();
    }
    state.polling = true;
    read_task(state.generation)
}

fn start(
    state: &mut State,
    busy: Busy,
    work: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Task<Message> {
    state.busy = Some(busy);
    state.generation = state.generation.wrapping_add(1);
    let generation = state.generation;
    Task::perform(blocking(work), move |result| {
        Message::Web(Msg::Done(generation, result))
    })
}

pub fn on_enter(state: &mut State, _ctx: &mut Ctx) -> Task<Message> {
    poll(state)
}

/// Whether two readings would draw the same page, whatever the clock says.
fn same_look(a: &Snapshot, b: &Snapshot) -> bool {
    let fresh = |s: &Snapshot| s.status.as_ref().is_some_and(|st| config::fresh(st, s.now));
    a.config == b.config
        && a.status == b.status
        && a.service == b.service
        && a.installed == b.installed
        && current_line(a) == current_line(b)
        && blocked_today(a) == blocked_today(b)
        && fresh(a) == fresh(b)
}

/// Rereads every two seconds on its own thread and speaks up only when the
/// page would look different, so an unchanged page never redraws.
pub fn subscription() -> Subscription<Message> {
    Subscription::run(|| {
        let (tx, rx) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            let mut last: Option<Snapshot> = None;
            loop {
                std::thread::sleep(Duration::from_secs(2));
                if tx.is_closed() {
                    break;
                }
                let now = read_snapshot();
                if last.as_ref().is_some_and(|l| same_look(l, &now)) {
                    continue;
                }
                last = Some(now.clone());
                if tx.unbounded_send(now).is_err() {
                    break;
                }
            }
        });
        rx
    })
    .map(|snapshot| Message::Web(Msg::Seen(Box::new(snapshot))))
}

fn take_snapshot(state: &mut State, snapshot: Snapshot) -> Task<Message> {
    let suggest = suggests(&snapshot);
    note_look(state, &snapshot, Instant::now());
    state.snapshot = Some(snapshot);
    Task::done(Message::Home(home::Msg::WebSuggest(suggest)))
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Seen(snapshot) => {
            if state.busy.is_some() {
                return Task::none();
            }
            take_snapshot(state, *snapshot)
        }
        Msg::Polled(generation, snapshot) => {
            state.polling = false;
            if generation != state.generation {
                return Task::none();
            }
            take_snapshot(state, *snapshot)
        }
        Msg::Toggle(switch, on) => {
            let Some(snapshot) = state.snapshot.as_ref() else {
                return Task::none();
            };
            if !controls_enabled(Some(snapshot), state.busy.is_some()) {
                return Task::none();
            }
            let mut config = snapshot.config.clone();
            switch.set(&mut config, on);
            config.paused_until = None;
            start(state, Busy::Switch(switch), move || apply(config))
        }
        Msg::Pause => {
            if !controls_enabled(state.snapshot.as_ref(), state.busy.is_some()) {
                return Task::none();
            }
            start(state, Busy::Pause, pause)
        }
        Msg::Resume => {
            if !controls_enabled(state.snapshot.as_ref(), state.busy.is_some()) {
                return Task::none();
            }
            start(state, Busy::Resume, resume)
        }
        Msg::Retry => {
            let Some(snapshot) = state.snapshot.as_ref() else {
                return Task::none();
            };
            if !controls_enabled(Some(snapshot), state.busy.is_some()) {
                return Task::none();
            }
            let config = snapshot.config.clone();
            start(state, Busy::Retry, move || apply(config))
        }
        Msg::Done(generation, result) => {
            if generation != state.generation {
                return Task::none();
            }
            state.busy = None;
            state.generation = state.generation.wrapping_add(1);
            state.polling = false;
            let reread = poll(state);
            match result {
                Ok(()) => reread,
                Err(raw) => Task::batch([
                    Task::done(Message::Toast(ctx.t(failure_text(&raw)), Tone::Warn)),
                    reread,
                ]),
            }
        }
        Msg::ToggleDetail(switch) => {
            if let Some(at) = state.open.iter().position(|s| *s == switch) {
                state.open.remove(at);
            } else {
                state.open.push(switch);
            }
            Task::none()
        }
    }
}


fn failure_text(raw: &str) -> &'static str {
    let r = raw.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| r.contains(n));
    if has(&["administrator", "access is denied", "os error 5", "elevat"]) {
        "Windows wouldn't let Secblitz change web protection. Sign in with an account that can make changes to this PC, then open Secblitz again."
    } else if has(&["unavailable", "only supported on windows", "requires windows"]) {
        "Web protection isn't available on this PC."
    } else if has(&["marked for deletion", "already exists", "1072", "1073"]) {
        "Windows is still tidying up the old web protection. Restart your PC, then try again."
    } else if has(&["untrusted", "unexpected", "writable", "owner"]) {
        "Secblitz couldn't safely set up web protection on this PC. Install the latest Secblitz and try again."
    } else {
        "We couldn't change web protection. Please try again. If it keeps happening, restart your PC."
    }
}

fn problem_hint(snapshot: &Snapshot, line: Line) -> Option<&'static str> {
    if !matches!(
        line,
        Line::NotWorking | Line::GettingReady | Line::On
    ) {
        return None;
    }
    let status = snapshot.status.as_ref().filter(|s| config::fresh(s, snapshot.now));
    match status.and_then(|s| s.last_error) {
        Some(ErrorCode::PortInUse) => Some(
            "Another program on your PC is using what web protection needs. Close other ad blockers or VPN apps.",
        ),
        Some(ErrorCode::NoUpstream) => Some(
            "Web protection can't find your internet connection. Connect to the internet.",
        ),
        Some(ErrorCode::DownloadFailed) => Some(
            "The block lists couldn't be downloaded. Connect to the internet. Secblitz will try again by itself.",
        ),
        Some(ErrorCode::ListInvalid) => Some(
            "The block lists couldn't be used. Secblitz will try to download them again. If this stays, check for a Secblitz update.",
        ),
        None if line == Line::NotWorking => Some(
            "Press Try again. If that doesn't help, restart your PC.",
        ),
        None => None,
    }
}

fn format_clock(lang: Lang, secs: u64) -> String {
    let hour = (secs % SECONDS_PER_DAY) / 3600;
    let minute = (secs % 3600) / 60;
    if lang == Lang::En {
        let suffix = if hour < 12 { "AM" } else { "PM" };
        let h12 = match hour % 12 {
            0 => 12,
            h => h,
        };
        format!("{h12}:{minute:02} {suffix}")
    } else {
        format!("{hour}:{minute:02}")
    }
}

fn group_digits(lang: Lang, n: u64) -> String {
    let sep = match lang {
        Lang::En => ",",
        Lang::Fr => "\u{202f}",
        Lang::Es | Lang::De | Lang::Pt | Lang::It => ".",
    };
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(sep);
        }
        out.push(c);
    }
    out
}

fn counted(ctx: &Ctx, one: &str, many: &str, n: u64) -> String {
    ctx.t(if n == 1 { one } else { many })
        .replace("{n}", &group_digits(ctx.lang, n))
}

fn blocked_text(ctx: &Ctx, [ads, trackers, dangerous]: [u64; 3]) -> String {
    ctx.t("Blocked today: {ads}, {trackers}, {dangerous}")
        .replace("{ads}", &counted(ctx, "{n} ad", "{n} ads", ads))
        .replace(
            "{trackers}",
            &counted(ctx, "{n} tracker", "{n} trackers", trackers),
        )
        .replace(
            "{dangerous}",
            &counted(
                ctx,
                "{n} dangerous website",
                "{n} dangerous websites",
                dangerous,
            ),
        )
}

fn line_text(ctx: &Ctx, line: Line) -> String {
    match line {
        Line::Off => ctx.t("Off"),
        Line::On => ctx.t("On"),
        Line::Paused(until) => ctx.t("Paused until {time}").replace(
            "{time}",
            &format_clock(ctx.lang, history::local_seconds(until)),
        ),
        Line::GettingReady => ctx.t("Getting block lists ready"),
        Line::NotWorking => {
            ctx.t("Not working right now. Your internet still works, but nothing is being blocked.")
        }
    }
}

fn hero_text(ctx: &Ctx, line: Line) -> (String, Option<String>) {
    match line {
        Line::On => (ctx.t("Web protection is on"), None),
        Line::GettingReady => (
            ctx.t("Getting block lists ready"),
            Some(ctx.t("Blocking starts as soon as the lists are ready.")),
        ),
        Line::Paused(_) => (
            line_text(ctx, line),
            Some(ctx.t("Nothing is being blocked for now.")),
        ),
        Line::Off => (
            ctx.t("Web protection is off"),
            Some(ctx.t("Ads, trackers and dangerous websites can load.")),
        ),
        Line::NotWorking => (ctx.t("Not working right now"), None),
    }
}

fn wrap(msg: Msg) -> Message {
    Message::Web(msg)
}

fn switch_text(ctx: &Ctx, switch: Switch) -> (Icon, String, String) {
    match switch {
        Switch::Ads => (
            Icon::Apps,
            ctx.t("Block ads"),
            ctx.t("Stops ads from loading in your browser and in apps."),
        ),
        Switch::Tracking => (
            Icon::Eye,
            ctx.t("Block tracking and telemetry"),
            ctx.t("Stops websites, apps and Windows from sending data about what you do."),
        ),
        Switch::Dangerous => (
            Icon::ShieldAlert,
            ctx.t("Block dangerous websites"),
            ctx.t("Stops your PC from opening known scam and virus websites."),
        ),
    }
}

fn detail_line<'a>(p: Palette, label: String, text: String) -> El<'a> {
    column![widgets::small(p, label), widgets::body(p, text)]
        .spacing(2)
        .into()
}

fn switch_row<'a>(state: &'a State, ctx: &'a Ctx, switch: Switch, snapshot: &Snapshot) -> El<'a> {
    let p = ctx.palette;
    let (icon, title, sentence) = switch_text(ctx, switch);
    let enabled = controls_enabled(Some(snapshot), state.busy.is_some());
    let working = state.busy == Some(Busy::Switch(switch));
    let on = switch.get(&snapshot.config);
    let control = widgets::switch(
        p,
        on,
        enabled.then_some(move |now_on: bool| wrap(Msg::Toggle(switch, now_on))),
    );
    let sub = if working {
        ctx.t("Changing…")
    } else {
        sentence
    };
    let head = widgets::row_item(p, Some(icon), title, Some(sub), control, None);
    let mut rows = vec![head];
    if working {
        rows.push(widgets::under_row(vec![progress::indeterminate(p, Tone::Brand)]));
    }
    if let Some(e) = explain::for_check(switch.id()) {
        rows.push(widgets::under_row(vec![widgets::expander(
            p,
            ctx.t("More details"),
            state.open.contains(&switch),
            wrap(Msg::ToggleDetail(switch)),
            column![
                detail_line(p, ctx.t("What it is"), ctx.t(e.what)),
                detail_line(p, ctx.t("If it's off"), ctx.t(e.risk)),
                detail_line(p, ctx.t("If you turn it on"), ctx.t(e.change)),
            ]
            .spacing(theme::S2),
        )]));
    }
    column(rows).width(Length::Fill).into()
}

fn status_button<'a>(
    state: &'a State,
    ctx: &'a Ctx,
    snapshot: &Snapshot,
    line: Line,
) -> Option<El<'a>> {
    let p = ctx.palette;
    let (label, msg) = match status_action(line)? {
        StatusAction::Resume => ("Resume now", Msg::Resume),
        StatusAction::Retry => ("Try again", Msg::Retry),
        StatusAction::Pause => ("Pause for 1 hour", Msg::Pause),
    };
    let enabled = controls_enabled(Some(snapshot), state.busy.is_some());
    let button = widgets::action(
        p,
        ButtonKind::Secondary,
        ctx.t(label),
        None,
        enabled.then_some(wrap(msg)),
    );
    let working = matches!(state.busy, Some(Busy::Pause | Busy::Resume | Busy::Retry));
    let mut col = column![button].spacing(theme::S2);
    if working {
        col = col.push(progress::indeterminate(p, Tone::Brand));
    }
    Some(container(col).padding(Padding::default().top(theme::S2)).into())
}

fn hero<'a>(state: &'a State, ctx: &'a Ctx, snapshot: &Snapshot) -> El<'a> {
    let p = ctx.palette;
    let line = current_line(snapshot);
    let (guard, since) = state
        .look
        .unwrap_or_else(|| (guard_of(line), Instant::now()));
    let counts = blocked_today(snapshot);
    let picture = web_globe::WebGlobe {
        p,
        plate: Plate::Surface,
        guard,
        changed: since,
        now: since,
        blocked: counts,
        labels: web_globe::Labels::new(|k| ctx.t(k)),
    }
    .view();
    let (title, sub) = hero_text(ctx, line);
    let mut words = column![widgets::h2(p, title)].spacing(theme::S1);
    if let Some(sub) = sub {
        words = words.push(widgets::muted(p, sub));
    }
    if snapshot.installed {
        if let Some(hint) = problem_hint(snapshot, line) {
            words = words.push(widgets::muted(p, ctx.t(hint)));
        }
    }
    if let Some(counts) = counts {
        words = words.push(widgets::small(p, blocked_text(ctx, counts)));
    }
    if snapshot.installed {
        if let Some(button) = status_button(state, ctx, snapshot, line) {
            words = words.push(button);
        }
    }
    widgets::region(
        p,
        row![picture, words.width(Length::Fill)]
            .spacing(theme::S6)
            .align_y(Alignment::Center),
    )
    .into()
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let mut page = column![widgets::page_header(
        p,
        ctx.t("Web protection"),
        Some(ctx.t("Stops ads, trackers and dangerous websites before they load.")),
    )]
    .spacing(theme::S8);
    let Some(snapshot) = state.snapshot.as_ref() else {
        return page.push(widgets::muted(p, ctx.t("Checking…"))).into();
    };
    page = page.push(hero(state, ctx, snapshot));
    if !snapshot.installed {
        page = page.push(widgets::inline_notice(
            p,
            Tone::Neutral,
            ctx.t("Web protection needs Secblitz to be installed."),
        ));
    }
    let switches: Vec<El<'a>> = Switch::ALL
        .iter()
        .map(|s| switch_row(state, ctx, *s, snapshot))
        .collect();
    page = page.push(widgets::group(
        p,
        ctx.t("What to block"),
        None,
        None,
        switches,
    ));
    page = page.push(widgets::small(
        p,
        ctx.t(
            "Some ads, like the ones inside YouTube videos, come from the same place as the video and can't be blocked this way.",
        ),
    ));
    page.push(widgets::small(
        p,
        ctx.t("Block lists by AdGuard, EasyList and HaGeZi."),
    ))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::Page;

    const NOW: u64 = 1_000_000;

    fn config(ads: bool) -> Config {
        Config {
            ads,
            ..Config::default()
        }
    }

    fn healthy() -> Status {
        Status {
            listening: true,
            state: ListState::Ready,
            written_at: NOW - 5,
            day: NOW / SECONDS_PER_DAY,
            blocked: [1204, 388, 0],
            ..Status::default()
        }
    }

    fn snapshot(config: Config, status: Option<Status>, installed: bool) -> Snapshot {
        Snapshot {
            config,
            status,
            service: ServiceState::Running,
            installed,
            now: NOW,
        }
    }

    #[test]
    fn a_reading_that_only_moved_the_clock_looks_the_same() {
        let a = snapshot(config(true), Some(healthy()), true);
        let later = Snapshot {
            now: NOW + 2,
            ..a.clone()
        };
        assert!(same_look(&a, &later));
        let stale = Snapshot {
            now: NOW + 3600,
            ..a.clone()
        };
        assert!(!same_look(&a, &stale));
        let mut more = healthy();
        more.blocked[0] += 1;
        assert!(!same_look(&a, &snapshot(config(true), Some(more), true)));
    }

    #[test]
    fn page_order_has_web_after_debloat() {
        let at = |page| Page::ALL.iter().position(|p| *p == page).unwrap();
        assert_eq!(at(Page::Web), at(Page::Debloat) + 1);
        assert_eq!(Page::parse("web"), Some(Page::Web));
        assert_eq!(Page::Web.label(), "Web protection");
    }

    #[test]
    fn status_button_fits_each_state() {
        assert_eq!(status_action(Line::Off), None);
        assert_eq!(status_action(Line::On), Some(StatusAction::Pause));
        assert_eq!(status_action(Line::GettingReady), Some(StatusAction::Pause));
        assert_eq!(status_action(Line::Paused(NOW)), Some(StatusAction::Resume));
        assert_eq!(status_action(Line::NotWorking), Some(StatusAction::Retry));
    }

    #[test]
    fn status_line_for_each_state() {
        let on = config(true);
        let ok = healthy();
        let running = ServiceState::Running;
        assert_eq!(status_line(&config(false), None, running, NOW), Line::Off);
        assert_eq!(status_line(&on, Some(&ok), running, NOW), Line::On);
        let waiting = Status {
            state: ListState::NoLists,
            ..healthy()
        };
        assert_eq!(
            status_line(&on, Some(&waiting), running, NOW),
            Line::GettingReady
        );
        let starting = Status {
            listening: false,
            state: ListState::Starting,
            ..healthy()
        };
        assert_eq!(
            status_line(&on, Some(&starting), running, NOW),
            Line::GettingReady
        );
        for service in [
            ServiceState::Stopped,
            ServiceState::NotInstalled,
            ServiceState::Other,
        ] {
            assert_eq!(status_line(&on, Some(&ok), service, NOW), Line::NotWorking);
        }
        let port = Status {
            listening: false,
            last_error: Some(ErrorCode::PortInUse),
            ..healthy()
        };
        assert_eq!(
            status_line(&on, Some(&port), running, NOW),
            Line::NotWorking
        );
        let late = Status {
            last_error: Some(ErrorCode::DownloadFailed),
            ..healthy()
        };
        assert_eq!(status_line(&on, Some(&late), running, NOW), Line::On);
    }

    #[test]
    fn paused_shows_until_time() {
        let paused = Config {
            paused_until: Some(NOW + 600),
            ..config(true)
        };
        assert_eq!(
            status_line(&paused, Some(&healthy()), ServiceState::Running, NOW),
            Line::Paused(NOW + 600)
        );
        let over = Config {
            paused_until: Some(NOW - 1),
            ..config(true)
        };
        assert_eq!(
            status_line(&over, Some(&healthy()), ServiceState::Running, NOW),
            Line::On
        );
        let idle = Config {
            paused_until: Some(NOW + 600),
            ..config(false)
        };
        assert_eq!(
            status_line(&idle, None, ServiceState::Running, NOW),
            Line::Off
        );
        assert_eq!(format_clock(Lang::En, 15 * 3600 + 15 * 60), "3:15 PM");
        assert_eq!(format_clock(Lang::En, 5), "12:00 AM");
        assert_eq!(format_clock(Lang::En, 12 * 3600 + 5 * 60), "12:05 PM");
        assert_eq!(format_clock(Lang::De, 9 * 3600 + 7 * 60), "9:07");
    }

    #[test]
    fn stale_status_shows_not_working() {
        let old = Status {
            written_at: NOW - 121,
            ..healthy()
        };
        assert_eq!(
            status_line(&config(true), Some(&old), ServiceState::Running, NOW),
            Line::NotWorking
        );
        assert_eq!(
            status_line(&config(true), None, ServiceState::Running, NOW),
            Line::NotWorking
        );
        assert!(blocked_today(&snapshot(config(true), Some(old), true)).is_none());
    }

    #[test]
    fn portable_disables_switches() {
        let portable = snapshot(config(false), Some(healthy()), false);
        assert!(!controls_enabled(Some(&portable), false));
        let installed = snapshot(config(false), None, true);
        assert!(controls_enabled(Some(&installed), false));
        assert!(!controls_enabled(Some(&installed), true));
        assert!(!controls_enabled(None, false));
        assert!(!suggests(&portable));
        assert!(suggests(&installed));
        assert!(!suggests(&snapshot(config(true), None, true)));
    }

    #[test]
    fn counts_come_from_today_only() {
        let s = snapshot(config(true), Some(healthy()), true);
        assert_eq!(blocked_today(&s), Some([1204, 388, 0]));
        let yesterday = Status {
            day: NOW / SECONDS_PER_DAY - 1,
            ..healthy()
        };
        assert_eq!(
            blocked_today(&snapshot(config(true), Some(yesterday), true)),
            Some([0; 3])
        );
        assert!(blocked_today(&snapshot(config(false), Some(healthy()), true)).is_none());
    }

    #[test]
    fn numbers_are_grouped() {
        assert_eq!(group_digits(Lang::En, 0), "0");
        assert_eq!(group_digits(Lang::En, 999), "999");
        assert_eq!(group_digits(Lang::En, 1204), "1,204");
        assert_eq!(group_digits(Lang::De, 1_234_567), "1.234.567");
    }

    #[test]
    fn changing_a_switch_keeps_the_others() {
        let mut c = Config {
            tracking: true,
            paused_until: Some(5),
            ..config(false)
        };
        Switch::Ads.set(&mut c, true);
        assert!(Switch::Ads.get(&c) && Switch::Tracking.get(&c) && !Switch::Dangerous.get(&c));
    }

    #[test]
    fn picture_follows_the_status_and_keeps_its_clock() {
        use web_globe::Guard;
        assert_eq!(guard_of(Line::On), Guard::On);
        assert_eq!(guard_of(Line::Off), Guard::Off);
        assert_eq!(guard_of(Line::Paused(NOW)), Guard::Paused);
        assert_eq!(guard_of(Line::GettingReady), Guard::Starting);
        assert_eq!(guard_of(Line::NotWorking), Guard::Broken);
        assert!(Guard::On.blocks() && !Guard::Starting.blocks());

        let mut state = State::default();
        let t0 = Instant::now();
        let on = snapshot(config(true), Some(healthy()), true);
        note_look(&mut state, &on, t0);
        assert_eq!(state.look, Some((Guard::On, t0)));
        note_look(&mut state, &on, t0 + Duration::from_secs(2));
        assert_eq!(state.look, Some((Guard::On, t0)));
        let t1 = t0 + Duration::from_secs(4);
        note_look(&mut state, &snapshot(config(false), None, true), t1);
        assert_eq!(state.look, Some((Guard::Off, t1)));
    }

    #[test]
    fn every_switch_has_an_explainer() {
        for s in Switch::ALL {
            assert!(explain::for_check(s.id()).is_some());
        }
    }

    #[test]
    fn known_failures_get_a_fix_and_unknown_ones_never_echo_raw_text() {
        let admin = failure_text("Changing web protection needs administrator rights");
        assert!(admin.contains("Sign in with an account"));
        assert!(failure_text("The service is marked for deletion (1072)").contains("Restart your PC"));
        let raw = "os error 87: weird HRESULT 0x80070057 in scm.rs";
        let general = failure_text(raw);
        assert!(general.starts_with("We couldn't change web protection."));
        assert!(!general.contains("0x8") && !general.contains("scm"));
        for text in [admin, general] {
            assert!(!text.contains('\u{2014}'));
        }
    }

    #[test]
    fn problems_explain_themselves_with_a_next_step() {
        let on = config(true);
        let port = Status {
            listening: false,
            last_error: Some(ErrorCode::PortInUse),
            ..healthy()
        };
        let s = snapshot(on.clone(), Some(port), true);
        let line = status_line(&s.config, s.status.as_ref(), s.service, s.now);
        assert!(!problem_hint(&s, line).unwrap().contains("Try again"));
        let down = Snapshot {
            service: ServiceState::Stopped,
            ..snapshot(on.clone(), None, true)
        };
        assert!(problem_hint(&down, Line::NotWorking).unwrap().contains("restart your PC"));
        let ok = snapshot(on, Some(healthy()), true);
        assert!(problem_hint(&ok, Line::On).is_none());
    }
}
