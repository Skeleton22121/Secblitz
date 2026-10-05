//! Home: score ring, verdict, one primary action, attention/protected cards,
//! first-run scanning view. OWNER: page-polish agent.
//!
//! Motion (see docs/MOTION.md): `window::frames()` is subscribed only while the
//! scan view is live or the score number is counting up, never when idle.
use crate::advice::{self, Group, NextStep};
use crate::app::score::{Score, Verdict};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Tone};
use crate::gui::widgets::{self, anim, ring, ButtonKind};
use crate::gui::{CheckProgress, Ctx, Message, Page};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length, Subscription, Task};
use secblitz::engine::{Outcome, Report};
use secblitz::model::Probe;
use std::collections::HashMap;
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
    /// When each checked item finished (for its check draw-in).
    done_at: HashMap<String, Instant>,
    /// How many progress items were already stamped in `done_at`.
    processed: usize,
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
            done_at: HashMap::new(),
            processed: 0,
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
}

/// Free space below which the user is warned (decimal GB, as Windows shows it).
const LOW_DISK_BYTES: u64 = 5_000_000_000;
/// Finished items kept visible in the live checklist.
const VISIBLE_STEPS: usize = 5;

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Frame(now) => frame(state, ctx, now),
        Msg::ToggleDetails => state.details_open = !state.details_open,
    }
    Task::none()
}

fn frame(state: &mut State, ctx: &Ctx, now: Instant) {
    state.now = now;
    if let Some(progress) = &ctx.checking {
        if state.scan.is_none() {
            state.scan = Some(anim::Clock::at(now));
            state.done_at.clear();
            state.processed = 0;
        }
        if state.processed > progress.items.len() {
            state.processed = 0;
        }
        for (id, _) in progress.items.iter().skip(state.processed) {
            state.done_at.entry(id.clone()).or_insert(now);
        }
        state.processed = progress.items.len();
    } else {
        state.scan = None;
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
        return widgets::card(
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

/// One checklist line with an animated status mark.
fn step_row<'a>(
    p: theme::Palette,
    mark: Element<'a, Message>,
    label: String,
    running: bool,
) -> Element<'a, Message> {
    row![
        container(mark).center_x(theme::CHECK),
        text(label)
            .size(theme::BODY)
            .font(if running {
                theme::MEDIUM
            } else {
                theme::REGULAR
            })
            .color(if running { p.text } else { p.text_muted })
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center)
    .into()
}

fn scanning<'a>(state: &State, ctx: &'a Ctx, progress: &CheckProgress) -> Element<'a, Message> {
    let p = ctx.palette;
    // Distinct finished items, in first-seen order.
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

    let mut list = column![].spacing(theme::S3);
    for id in &seen[n.saturating_sub(VISIBLE_STEPS)..] {
        let t = state.done_at.get(*id).map_or(0.0, |at| {
            anim::Clock::at(*at).progress_at(anim::SLOW, state.now)
        });
        list = list.push(step_row(
            p,
            anim::check_draw(theme::CHECK, p.good, t),
            ctx.t(advice::control_label(id)),
            false,
        ));
    }
    list = list.push(step_row(
        p,
        anim::spinner(theme::CHECK, p.text_muted, elapsed),
        ctx.t("Looking at your settings…"),
        true,
    ));

    let body = column![
        anim::shield_scan(96.0, p.text, elapsed),
        column![
            widgets::h1(p, ctx.t("Checking your PC")),
            widgets::muted(p, ctx.t("This takes about a minute. Nothing is changed.")),
        ]
        .spacing(theme::S1)
        .align_x(Alignment::Center),
        container(widgets::bar(p, ratio, Tone::Neutral)).max_width(420),
        container(list).max_width(420),
    ]
    .spacing(theme::S6)
    .align_x(Alignment::Center)
    .width(Length::Fill);
    widgets::card(p, container(body).center_x(Length::Fill)).into()
}

// ----------------------------------------------------------------- errors

fn error_card<'a>(state: &State, ctx: &'a Ctx, title: &str, raw: &'a str) -> Element<'a, Message> {
    let p = ctx.palette;
    let content = column![
        widgets::icon_badge(p, Icon::ShieldAlert, Tone::Bad),
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
    widgets::card(p, content).into()
}

// ---------------------------------------------------------------- assessed

/// Results the user should look at: fixable ones, and checks that need a manual choice.
fn attention_items(report: &Report) -> Vec<(&Outcome, advice::Advice)> {
    report
        .results
        .iter()
        .map(|r| (r, advice::for_outcome(r)))
        .filter(|(r, a)| {
            a.group == Group::Recommended || (a.group == Group::Choice && r.status == "attention")
        })
        .collect()
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

/// Findings that need a step in Windows Settings, shown as gentle tips.
fn tips(report: &Report) -> Vec<advice::Advice> {
    report
        .findings
        .iter()
        .map(|f| advice::for_finding(&f.title, &f.status, &f.detail))
        .filter(|a| {
            matches!(a.group, Group::Recommended | Group::Choice)
                && matches!(
                    a.step,
                    NextStep::OpenWindowsSecurity
                        | NextStep::OpenWindowsUpdate
                        | NextStep::OpenEncryption
                        | NextStep::OpenAccounts
                        | NextStep::OpenRemoteDesktop
                )
        })
        .collect()
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

/// Things that need a change the person makes themselves in Windows.
fn left_to_you(attention: usize, fixable: usize) -> usize {
    attention.saturating_sub(fixable)
}

fn assessed<'a>(state: &State, ctx: &'a Ctx, report: &'a Report) -> Element<'a, Message> {
    let p = ctx.palette;
    let score = Score::of(report);
    let items = attention_items(report);
    // After a failed check the report is stale: never claim protection from it
    // and offer no fix that cannot open.
    let stale = ctx.check_error.is_some();
    let ids = if stale {
        Vec::new()
    } else {
        crate::app::flow::recommended(report, &ctx.catalog.available)
    };
    let verdict = if stale {
        Verdict::Unknown
    } else {
        score.verdict()
    };
    let attention = score.attention.max(1);
    let fixable = ids.len().min(attention);
    let manual = left_to_you(attention, fixable);
    let mut note: Option<String> = None;
    let (tone, title, subtitle) = match verdict {
        Verdict::Protected => (
            Tone::Good,
            ctx.t("You're protected"),
            ctx.t("Everything we checked is switched on and working."),
        ),
        Verdict::Attention => {
            if fixable > 0 && manual > 0 {
                note = Some(count_text(
                    ctx,
                    "The other one needs a change you make yourself in Windows Settings.",
                    "The other {n} need a change you make yourself in Windows Settings.",
                    manual,
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
                if fixable == 0 {
                    ctx.t("These need a change in Windows Settings. We will show you where.")
                } else {
                    count_text(
                        ctx,
                        "We can fix it for you. You can undo any change later.",
                        "We can fix {n} of them for you. You can undo any change later.",
                        fixable,
                    )
                },
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

    let check_again = |kind: ButtonKind, icon: Option<Icon>| {
        widgets::action(
            p,
            kind,
            ctx.t("Check again"),
            icon,
            (!ctx.busy).then_some(Message::CheckNow),
        )
    };
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
        buttons = buttons
            .push(widgets::action(
                p,
                ButtonKind::Primary,
                count_text(ctx, "Fix it for me", "Fix {n} for me", fixable),
                Some(Icon::Wrench),
                (!ctx.busy).then(|| Message::ReviewFixes(ids.clone())),
            ))
            .push(check_again(ButtonKind::Secondary, None));
    } else if verdict == Verdict::Attention {
        buttons = buttons
            .push(widgets::action(
                p,
                ButtonKind::Primary,
                ctx.t("See what to do"),
                None,
                Some(Message::Navigate(Page::Fixes)),
            ))
            .push(check_again(ButtonKind::Secondary, None));
    } else {
        buttons = buttons.push(check_again(ButtonKind::Primary, Some(Icon::Refresh)));
    }

    let mut texts = column![widgets::h1(p, title), widgets::muted(p, subtitle)].spacing(theme::S1);
    if let Some(note) = note {
        texts = texts.push(widgets::muted(p, note));
    }
    if let Some(when) = last_checked(ctx) {
        texts = texts.push(widgets::small(p, when));
    }
    let hero = widgets::card(
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

    let mut page = column![].spacing(theme::S4);
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
        page = page.push(attention_card(ctx, &items));
    }
    let (count, labels) = protected_labels(report);
    if count > 0 {
        page = page.push(protected_card(ctx, count, &labels));
    }
    let tips = tips(report);
    if !tips.is_empty() {
        // Light and calm: plain muted lines, no boxes.
        let mut c = column![widgets::section_label(p, ctx.t("Good to know"))].spacing(theme::S3);
        for a in tips.iter().take(3) {
            c = c.push(
                row![
                    widgets::icon(Icon::Info, 16.0, p.text_muted),
                    widgets::muted(p, format!("{}. {}", ctx.t(a.label), ctx.t(a.next))),
                ]
                .spacing(theme::S3)
                .align_y(Alignment::Center),
            );
        }
        page = page.push(container(c).padding([theme::S2, theme::S1]));
    }
    page.into()
}

fn attention_card<'a>(ctx: &'a Ctx, items: &[(&Outcome, advice::Advice)]) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut c = column![row![
        widgets::h2(p, ctx.t("Needs your attention")),
        widgets::pill(p, items.len().to_string(), Tone::Warn),
    ]
    .spacing(theme::S2)
    .align_y(Alignment::Center)]
    .spacing(theme::S3);
    for (r, a) in items.iter().take(4) {
        let impact = if a.impact.is_empty() {
            String::new()
        } else {
            format!("{} {}", ctx.t(a.impact_prefix()), ctx.t(a.impact))
        };
        let line = row![
            widgets::icon_badge(p, Icon::AlertTriangle, Tone::Warn),
            column![
                text(ctx.t(advice::control_label(&r.id)))
                    .size(theme::BODY)
                    .font(theme::MEDIUM)
                    .color(p.text),
                text(impact)
                    .size(theme::SMALL)
                    .font(theme::REGULAR)
                    .color(p.text_muted),
            ]
            .spacing(theme::S1)
            .width(Length::Fill),
            widgets::icon(Icon::ChevronRight, 16.0, p.text_muted),
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center);
        c = c.push(widgets::list_button(
            p,
            line,
            Message::Navigate(Page::Fixes),
        ));
    }
    if items.len() > 4 {
        let more = count_text(ctx, "See {n} more", "See {n} more", items.len() - 4);
        c = c.push(widgets::link(p, more, Message::Navigate(Page::Fixes)));
    }
    widgets::card(p, c).into()
}

fn protected_card<'a>(ctx: &'a Ctx, count: usize, labels: &[&'static str]) -> Element<'a, Message> {
    let p = ctx.palette;
    let names: Vec<String> = labels.iter().take(4).map(|l| ctx.t(l)).collect();
    let mut summary = names.join(", ");
    if labels.len() > 4 {
        summary.push('…');
    }
    let header = row![
        widgets::icon_badge(p, Icon::ShieldCheck, Tone::Good),
        column![
            row![
                widgets::h2(p, ctx.t("Protected")),
                widgets::pill(p, count.to_string(), Tone::Good)
            ]
            .spacing(theme::S2)
            .align_y(Alignment::Center),
            widgets::small(p, summary),
        ]
        .spacing(theme::S1)
        .width(Length::Fill),
        widgets::link(p, ctx.t("See all"), Message::Navigate(Page::Fixes)),
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center);
    widgets::card(p, header).into()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(attention_items(&report).len(), 1);
        assert_eq!(protected_labels(&report).0, 1);
    }

    #[test]
    fn headline_and_button_agree() {
        assert_eq!(left_to_you(3, 2), 1);
        assert_eq!(left_to_you(2, 2), 0);
        assert_eq!(left_to_you(1, 3), 0);
    }
}
