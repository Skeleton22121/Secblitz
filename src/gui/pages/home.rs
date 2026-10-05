//! Home: one hero region (score ring, verdict, one primary, extras in the
//! overflow menu), then flat row groups; first-run PC-check view.
//!
//! Motion (see docs/MOTION.md): `window::frames()` is subscribed only while the
//! scan view is live or the score number is counting up, never when idle.
use crate::advice::{self, Group, NextStep};
use crate::app::score::{self, Score, ToCheck, Verdict};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Tone};
use crate::gui::widgets::{self, anim, ring, scan, ButtonKind};
use crate::gui::{CheckProgress, Ctx, Message, Page};
use iced::widget::{column, row};
use iced::{Alignment, Element, Length, Subscription, Task};
use secblitz::engine::Report;
use secblitz::model::Probe;
use std::time::Instant;

/// Score number counting from the previously shown value to a new one.
#[derive(Debug, Clone, Copy)]
struct Count {
    from: i64,
    to: i64,
    clock: anim::Clock,
}

#[derive(Debug)]
pub struct State {
    /// "More details" expander of the error card.
    details_open: bool,
    /// Latest frame timestamp (views never call `Instant::now()`).
    now: Instant,
    /// Started when the scan view appears; drives the shield and spinner.
    scan: Option<anim::Clock>,
    /// Status lines of the live check, oldest first, with their start time.
    lines: Vec<(String, Instant)>,
    /// How many progress items were already turned into status lines.
    processed: usize,
    /// "Protected" list expanded.
    protected_open: bool,
    /// "Protected" list shows every row instead of the first few.
    protected_all: bool,
    /// `checked_at` of the check whose result is already on screen.
    seen_check: Option<u64>,
    /// Protected count currently shown beside the ring.
    shown: i64,
    count: Option<Count>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            details_open: false,
            now: Instant::now(),
            scan: None,
            lines: Vec::new(),
            processed: 0,
            protected_open: false,
            protected_all: false,
            seen_check: None,
            shown: 0,
            count: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Animation frame (only while something animates).
    Frame(Instant),
    /// Open / close the details on the error card.
    ToggleDetails,
    /// Expand / collapse the protected list.
    ToggleProtected,
    /// Show every protected row / only the first few.
    ToggleProtectedAll,
}

/// Free space below which the user is warned (decimal GB, as Windows shows it).
const LOW_DISK_BYTES: u64 = 5_000_000_000;
/// Status lines kept in memory for the ticker (it shows the last few).
const KEPT_LINES: usize = 8;
/// Rows of the protected list shown before "Show more".
const PROTECTED_ROWS: usize = 8;
/// Attention rows shown on Home.
const ATTENTION_ROWS: usize = 4;

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Frame(now) => frame(state, ctx, now),
        Msg::ToggleDetails => state.details_open = !state.details_open,
        Msg::ToggleProtected => state.protected_open = !state.protected_open,
        Msg::ToggleProtectedAll => state.protected_all = !state.protected_all,
    }
    Task::none()
}

/// Home has nothing to read in the background; the hook exists so the shell
/// can treat every page alike, and it drops stale view state.
#[allow(dead_code)]
pub fn preload(state: &mut State, _ctx: &mut Ctx) -> Task<Message> {
    state.protected_all = false;
    Task::none()
}

fn frame(state: &mut State, ctx: &Ctx, now: Instant) {
    state.now = now;
    if let Some(progress) = &ctx.checking {
        if state.scan.is_none() {
            state.scan = Some(anim::Clock::at(now));
            state.lines.clear();
            state.lines.push((ctx.t("Looking at your settings…"), now));
            state.processed = 0;
        }
        if state.processed > progress.items.len() {
            state.processed = 0;
        }
        for (id, _) in progress.items.iter().skip(state.processed) {
            let label = ctx.t(advice::control_label(id));
            if !state.lines.iter().any(|(l, _)| *l == label) {
                state.lines.push((label, now));
            }
        }
        if state.lines.len() > KEPT_LINES {
            let extra = state.lines.len() - KEPT_LINES;
            state.lines.drain(..extra);
        }
        state.processed = progress.items.len();
    } else {
        state.scan = None;
        state.lines.clear();
    }
    if ctx.checking.is_none() && ctx.checked_at != state.seen_check {
        state.seen_check = ctx.checked_at;
        if let Some(report) = ctx.report.as_deref() {
            let to = Score::of(report).protected as i64;
            if to != state.shown {
                state.count = Some(Count {
                    from: state.shown,
                    to,
                    clock: anim::Clock::at(now),
                });
            }
        }
    }
    if let Some(c) = &state.count {
        if c.clock.done(anim::SLOW, now) {
            state.shown = c.to;
            state.count = None;
        }
    }
}

/// Frames only while a scan is live or a number is counting up.
pub fn subscription(state: &State, ctx: &Ctx) -> Subscription<Message> {
    let live = ctx.checking.is_some()
        || state.count.is_some()
        || (ctx.checked_at != state.seen_check && ctx.report.is_some());
    if live && anim::animating() {
        iced::window::frames().map(|now| Message::Home(Msg::Frame(now)))
    } else {
        Subscription::none()
    }
}

/// Whether the page is the first check's screen, which fills the window
/// instead of scrolling.
pub fn fills_window(ctx: &Ctx) -> bool {
    ctx.engine_error.is_none() && ctx.report.is_none() && ctx.checking.is_some()
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    if let Some(error) = &ctx.engine_error {
        return error_card(state, ctx, "We couldn't start Secblitz", error);
    }
    let Some(report) = ctx.report.as_deref() else {
        if let Some(progress) = &ctx.checking {
            return scanning(state, ctx, progress);
        }
        if let Some(error) = &ctx.check_error {
            return error_card(state, ctx, "We couldn't check your PC", error);
        }
        return widgets::region(
            p,
            widgets::empty_state(
                p,
                Icon::ShieldCheck,
                ctx.t("Let's check your PC"),
                ctx.t("This takes about a minute. Nothing is changed."),
                Some(widgets::action(
                    p,
                    ButtonKind::Primary,
                    ctx.t("Check my PC"),
                    Some(Icon::Refresh),
                    Some(Message::CheckNow),
                )),
            ),
        )
        .into();
    };
    assessed(state, ctx, report)
}

// ---------------------------------------------------------------- scanning

fn scanning<'a>(state: &'a State, ctx: &'a Ctx, progress: &CheckProgress) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut seen: Vec<&str> = Vec::new();
    for (id, _) in &progress.items {
        if !seen.contains(&id.as_str()) {
            seen.push(id);
        }
    }
    let n = seen.len();
    let total = ctx.catalog.available.len().max(n + 1);
    let ratio = if ctx.catalog.available.is_empty() {
        n as f32 / (n as f32 + 6.0)
    } else {
        (n as f32 / total as f32).min(0.96)
    };
    let elapsed = state
        .scan
        .map(|c| c.elapsed_at(state.now))
        .unwrap_or_default();

    scan::checking_screen(
        p,
        ctx.t("Checking your PC"),
        ctx.t("This takes about a minute. Nothing is changed."),
        ratio,
        &state.lines,
        state.now,
        elapsed,
    )
}

// ----------------------------------------------------------------- errors

fn error_card<'a>(state: &State, ctx: &'a Ctx, title: &str, raw: &'a str) -> Element<'a, Message> {
    let p = ctx.palette;
    let content = column![
        widgets::icon(Icon::ShieldAlert, theme::ICON_ROW, p.bad_text),
        widgets::h2(p, ctx.t(title)),
        widgets::muted(
            p,
            ctx.t("Something got in the way. Trying again usually fixes it.")
        ),
        widgets::action(
            p,
            ButtonKind::Primary,
            ctx.t("Try again"),
            Some(Icon::Refresh),
            Some(Message::CheckNow)
        ),
        widgets::expander(
            p,
            ctx.t("More details"),
            state.details_open,
            Message::Home(Msg::ToggleDetails),
            widgets::small(p, raw.to_owned()),
        ),
    ]
    .spacing(theme::S4)
    .align_x(Alignment::Start);
    widgets::region(p, content).into()
}

// ---------------------------------------------------------------- assessed

/// How the things to check split up: fixes we apply (ticked by default),
/// optional choices we can apply if the person wants, and steps left to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Split {
    fixable: usize,
    choices: usize,
    manual: usize,
}

/// The recommended fix ids among `items`, and how `items` split up.
/// Also orders the list the way the headline reads it:
/// what we can fix first, then the person's choices, then their own steps.
fn split(report: &Report, available: &[String], items: &mut [ToCheck]) -> (Vec<String>, Split) {
    let recommended = crate::app::flow::recommended(report, available);
    let candidates = crate::app::flow::candidates(report, available);
    let part = |item: &ToCheck| match item {
        ToCheck::Control(r) if recommended.contains(&r.id) => 0,
        ToCheck::Control(r) if candidates.contains(&r.id) && advice::is_choice(&r.id) => 1,
        _ => 2,
    };
    items.sort_by_key(|item| part(item));
    let ids: Vec<String> = items
        .iter()
        .filter_map(|item| match item {
            ToCheck::Control(r) if part(item) == 0 => Some(r.id.clone()),
            _ => None,
        })
        .collect();
    let fixable = ids.len();
    let choices = items.iter().filter(|item| part(item) == 1).count();
    let manual = items.len() - fixable - choices;
    (
        ids,
        Split {
            fixable,
            choices,
            manual,
        },
    )
}

fn protected_labels(report: &Report) -> (usize, Vec<&'static str>) {
    let mut count = 0;
    let mut labels: Vec<&'static str> = Vec::new();
    for r in &report.results {
        let a = advice::for_outcome(r);
        if a.group == Group::Protected {
            count += 1;
            let label = advice::control_label(&r.id);
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
    }
    (count, labels)
}

/// Plain readiness notices; only conditions that matter to the user.
fn readiness_notices(ctx: &Ctx, report: &Report) -> Vec<(Tone, String)> {
    let mut out = Vec::new();
    let Some(r) = &report.readiness else {
        return out;
    };
    if let Probe::Known(v) = &r.system_volume {
        if v.read_only {
            out.push((
                Tone::Warn,
                ctx.t("Your disk can't be written to right now, so fixes will wait."),
            ));
        } else if v.available_bytes < LOW_DISK_BYTES {
            out.push((
                Tone::Warn,
                ctx.t(
                    "Your disk is almost full. Free up some space so updates and fixes can finish.",
                ),
            ));
        }
    }
    if let Probe::Known(pw) = &r.power {
        if pw.battery_present == Some(true) && pw.ac_connected == Some(false) {
            out.push((
                Tone::Neutral,
                ctx.t("Your PC is running on battery. Plug it in before making changes."),
            ));
        }
    }
    if matches!(r.windows_update_reboot, Probe::Known(true)) {
        out.push((
            Tone::Neutral,
            ctx.t("A restart is waiting. Save your work and restart when you're ready."),
        ));
    }
    out
}

fn count_text(ctx: &Ctx, one: &str, many: &str, n: usize) -> String {
    ctx.t(if n == 1 { one } else { many })
        .replace("{n}", &n.to_string())
}

fn last_checked(ctx: &Ctx) -> Option<String> {
    let at = ctx.checked_at?;
    let secs = crate::app::history::now().saturating_sub(at);
    Some(match secs {
        0..=59 => ctx.t("Last checked just now"),
        60..=3599 => count_text(
            ctx,
            "Last checked {n} minute ago",
            "Last checked {n} minutes ago",
            (secs / 60) as usize,
        ),
        3600..=86399 => count_text(
            ctx,
            "Last checked {n} hour ago",
            "Last checked {n} hours ago",
            (secs / 3600) as usize,
        ),
        _ => count_text(
            ctx,
            "Last checked {n} day ago",
            "Last checked {n} days ago",
            (secs / 86400) as usize,
        ),
    })
}

fn assessed<'a>(state: &State, ctx: &'a Ctx, report: &'a Report) -> Element<'a, Message> {
    let p = ctx.palette;
    let score = Score::of(report);
    let mut items = score::to_check(report);
    // After a failed check the report is stale: never claim protection from it
    // and offer no fix that cannot open.
    let stale = ctx.check_error.is_some();
    let (ids, split) = if stale {
        (
            Vec::new(),
            Split {
                fixable: 0,
                choices: 0,
                manual: items.len(),
            },
        )
    } else {
        split(report, &ctx.catalog.available, &mut items)
    };
    let verdict = if stale {
        Verdict::Unknown
    } else {
        score::overall(report)
    };
    let attention = items.len().max(1);
    let fixable = split.fixable;
    let mut notes: Vec<String> = Vec::new();
    let (tone, title, subtitle) = match verdict {
        Verdict::Protected => (
            Tone::Good,
            ctx.t("You're protected"),
            ctx.t("Everything we checked is switched on and working."),
        ),
        Verdict::Attention => {
            let subtitle = if fixable == 0 && split.choices == 0 {
                ctx.t("Each one needs a step from you. We show you what to do.")
            } else if fixable == 0 && split.manual == 0 {
                ctx.t("Each one is your choice. We explain what changes before you decide.")
            } else if fixable == attention {
                if attention == 1 {
                    ctx.t("We can fix it for you. You can undo any change later.")
                } else {
                    ctx.t("We can fix all of them for you. You can undo any change later.")
                }
            } else if fixable > 0 {
                count_text(
                    ctx,
                    "We can fix 1 of them for you. You can undo any change later.",
                    "We can fix {n} of them for you. You can undo any change later.",
                    fixable,
                )
            } else {
                String::new()
            };
            // Name every part that the subtitle does not already cover.
            let mixed =
                (fixable > 0) as u8 + (split.choices > 0) as u8 + (split.manual > 0) as u8 > 1;
            if mixed && split.choices > 0 {
                notes.push(count_text(
                    ctx,
                    "1 is optional: you decide on the Protection page.",
                    "{n} are optional: you decide on the Protection page.",
                    split.choices,
                ));
            }
            if mixed && split.manual > 0 {
                notes.push(count_text(
                    ctx,
                    "1 needs a step from you, such as a restart or a Windows setting.",
                    "{n} need a step from you, such as a restart or a Windows setting.",
                    split.manual,
                ));
            }
            (
                Tone::Warn,
                count_text(
                    ctx,
                    "{n} thing needs your attention",
                    "{n} things need your attention",
                    attention,
                ),
                subtitle,
            )
        }
        Verdict::Unknown => (
            Tone::Neutral,
            ctx.t("We couldn't finish checking"),
            ctx.t("Some checks didn't finish. Try again in a moment."),
        ),
    };

    // The number counts up from the previous value after each check.
    let protected = i64::try_from(score.protected).unwrap_or(0);
    let shown = if anim::reduced() {
        protected
    } else if let Some(c) = &state.count {
        anim::count_up_int(c.from, c.to, c.clock.progress_at(anim::SLOW, state.now))
    } else if ctx.checked_at != state.seen_check {
        state.shown
    } else {
        protected
    };
    let ring_view = ring::ring(
        ring::Ring {
            p,
            ratio: score.ratio(),
            tone,
            label: shown.to_string(),
            caption: ctx
                .t("of {n} protected")
                .replace("{n}", &score.total.to_string()),
        },
        176.0,
    );

    let check_item = (
        Icon::Refresh,
        ctx.t("Check again"),
        Message::CheckNow,
        false,
    );
    let mut extras: Vec<(Icon, String, Message, bool)> = Vec::new();
    let mut buttons = row![].spacing(theme::S2).align_y(Alignment::Center);
    if ctx.checking.is_some() {
        let elapsed = state
            .scan
            .map(|c| c.elapsed_at(state.now))
            .unwrap_or_default();
        buttons = buttons
            .push(anim::spinner(theme::CHECK, p.text_muted, elapsed))
            .push(widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t("Checking…"),
                None,
                None,
            ));
    } else if verdict == Verdict::Attention && fixable > 0 {
        buttons = buttons.push(widgets::action(
            p,
            ButtonKind::Primary,
            count_text(ctx, "Fix it for me", "Fix {n} for me", fixable),
            Some(Icon::Wrench),
            (!ctx.busy).then(|| Message::ReviewFixes(ids.clone())),
        ));
        extras.push(check_item);
    } else if verdict == Verdict::Attention {
        buttons = buttons.push(widgets::action(
            p,
            ButtonKind::Primary,
            ctx.t("See what to do"),
            None,
            Some(Message::Navigate(Page::Fixes)),
        ));
        extras.push(check_item);
    } else {
        buttons = buttons.push(widgets::action(
            p,
            ButtonKind::Primary,
            ctx.t("Check again"),
            Some(Icon::Refresh),
            (!ctx.busy).then_some(Message::CheckNow),
        ));
    }
    if ctx.checking.is_none() && !ctx.busy && !extras.is_empty() {
        buttons = buttons.push(widgets::overflow_menu(p, extras));
    }

    let mut texts = column![widgets::h1(p, title)].spacing(theme::S1);
    for line in std::iter::once(subtitle)
        .chain(notes)
        .filter(|l| !l.is_empty())
    {
        texts = texts.push(widgets::muted(p, line));
    }
    if let Some(when) = last_checked(ctx) {
        texts = texts.push(widgets::small(p, when));
    }
    let hero = widgets::region(
        p,
        row![
            ring_view,
            column![texts, buttons]
                .spacing(theme::S6)
                .width(Length::Fill)
        ]
        .spacing(theme::S8)
        .align_y(Alignment::Center),
    );

    let mut page = column![].spacing(theme::S8);
    if ctx.check_error.is_some() {
        page = page.push(widgets::inline_notice(
            p,
            Tone::Warn,
            ctx.t("The last check didn't finish, so this may be out of date."),
        ));
    }
    page = page.push(hero);
    for (tone, msg) in readiness_notices(ctx, report) {
        page = page.push(widgets::inline_notice(p, tone, msg));
    }

    if !items.is_empty() {
        page = page.push(attention_group(ctx, &items));
    }
    let (count, labels) = protected_labels(report);
    if count > 0 {
        page = page.push(protected_group(state, ctx, count, &labels));
    }
    page.into()
}

fn attention_group<'a>(ctx: &'a Ctx, items: &[ToCheck]) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut rows: Vec<Element<'a, Message>> = items
        .iter()
        .take(ATTENTION_ROWS)
        .map(|item| {
            let (id, report_only, title, line) = match item {
                ToCheck::Control(r) => {
                    let a = advice::for_outcome(r);
                    // A fix or a choice says what it protects against; any
                    // other state (restart, conflict, ...) says what to do.
                    let line = if a.step == NextStep::Repair && !a.impact.is_empty() {
                        format!("{} {}", ctx.t(a.impact_prefix()), ctx.t(a.impact))
                    } else {
                        ctx.t(a.next)
                    };
                    (
                        r.id.as_str(),
                        false,
                        ctx.t(advice::control_label(&r.id)),
                        line,
                    )
                }
                ToCheck::Finding(f) => {
                    let a = advice::for_finding(&f.title, &f.status, &f.detail);
                    (f.title.as_str(), true, ctx.t(a.label), ctx.t(a.next))
                }
            };
            let head = widgets::row_item_tinted(
                p,
                Some(Icon::AlertTriangle),
                Some(Tone::Warn),
                title,
                Some(line),
                widgets::icon(Icon::ChevronRight, 16.0, p.text_muted),
                Some(Message::Navigate(Page::Fixes)),
            );
            widgets::explain::with_disclosure(
                ctx,
                "home",
                id,
                report_only,
                widgets::explain::INDENT,
                head,
            )
        })
        .collect();
    if items.len() > ATTENTION_ROWS {
        rows.push(widgets::show_more_button(
            p,
            count_text(
                ctx,
                "See {n} more",
                "See {n} more",
                items.len() - ATTENTION_ROWS,
            ),
            Message::Navigate(Page::Fixes),
        ));
    }
    widgets::group(
        p,
        ctx.t("Needs your attention"),
        None,
        Some(widgets::pill(p, items.len().to_string(), Tone::Warn)),
        rows,
    )
}

/// Everything that is fine, folded away with a one-line summary.
fn protected_group<'a>(
    state: &State,
    ctx: &'a Ctx,
    count: usize,
    labels: &[&'static str],
) -> Element<'a, Message> {
    let p = ctx.palette;
    let names: Vec<String> = labels.iter().take(3).map(|l| ctx.t(l)).collect();
    let mut summary = names.join(", ");
    if labels.len() > 3 {
        summary.push('…');
    }
    let shown = widgets::limited(labels, PROTECTED_ROWS, state.protected_all);
    let mut body = column![].spacing(theme::S1);
    for l in shown {
        body = body.push(widgets::row_item_tinted(
            p,
            Some(Icon::ShieldCheck),
            Some(Tone::Good),
            ctx.t(l),
            None,
            iced::widget::space::horizontal(),
            None,
        ));
    }
    if labels.len() > PROTECTED_ROWS {
        body = body.push(widgets::show_more_button(
            p,
            if state.protected_all {
                ctx.t("Show less")
            } else {
                count_text(
                    ctx,
                    "Show {n} more",
                    "Show {n} more",
                    labels.len() - PROTECTED_ROWS,
                )
            },
            Message::Home(Msg::ToggleProtectedAll),
        ));
    }
    widgets::collapsible(
        p,
        ctx.t("Protected"),
        Some(format!("{count} · {summary}")),
        state.protected_open,
        Message::Home(Msg::ToggleProtected),
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use secblitz::engine::Outcome;

    fn outcome(id: &str, status: &str) -> Outcome {
        Outcome {
            id: id.into(),
            status: status.into(),
            ..Outcome::default()
        }
    }

    #[test]
    fn attention_and_protected_are_split_by_advice_group() {
        let report = Report {
            results: vec![
                outcome("uac.enabled", "compliant"),
                outcome("uac.consent", "attention"),
            ],
            ..Report::default()
        };
        assert_eq!(score::to_check(&report).len(), 1);
        assert_eq!(protected_labels(&report).0, 1);
    }

    #[test]
    fn headline_parts_add_up() {
        let report = Report {
            results: vec![
                outcome("uac.consent", "attention"),
                outcome("autorun.disabled", "attention"),
                Outcome {
                    detail: "Preference applied; restart required".into(),
                    ..outcome("uac.enabled", "applied")
                },
            ],
            ..Report::default()
        };
        let available: Vec<String> = ["uac.consent", "autorun.disabled"]
            .map(String::from)
            .to_vec();
        let mut items = score::to_check(&report);
        let (ids, split) = split(&report, &available, &mut items);
        assert_eq!(split.fixable + split.choices + split.manual, items.len());
        assert_eq!(ids.len(), split.fixable);
        assert!(ids.iter().all(|id| !advice::is_choice(id)));
        // The list leads with what the button fixes, in the headline's order.
        let lead: Vec<&str> = items
            .iter()
            .take(ids.len())
            .filter_map(|item| match item {
                ToCheck::Control(r) => Some(r.id.as_str()),
                ToCheck::Finding(_) => None,
            })
            .collect();
        assert_eq!(lead, ids.iter().map(String::as_str).collect::<Vec<_>>());
    }
}
