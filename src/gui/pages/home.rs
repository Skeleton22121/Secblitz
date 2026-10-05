//! Home: score ring, verdict, one primary action, attention/protected cards,
//! first-run scanning view. OWNER: shell agent.
use crate::advice::{self, Group, NextStep};
use crate::app::score::{Score, Verdict};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, ring, ButtonKind, StepState};
use crate::gui::{CheckProgress, Ctx, Message, Page};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{column, container, row, stack, text};
use iced::{
    mouse, Alignment, Element, Length, Point, Rectangle, Renderer, Subscription, Task, Theme,
};
use secblitz::engine::{Outcome, Report};
use secblitz::model::Probe;

#[derive(Debug, Default)]
pub struct State {
    /// Seconds of animation while scanning (drives the pulse).
    phase: f32,
    /// "Technical details" expander of the error card.
    details_open: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Animation tick while scanning.
    Tick,
    /// Open / close the technical details on the error card.
    ToggleDetails,
}

const TICK_SECONDS: f32 = 0.033;
/// Free space below which the user is warned (decimal GB, as Windows shows it).
const LOW_DISK_BYTES: u64 = 5_000_000_000;

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    let _ = ctx;
    match msg {
        Msg::Tick => state.phase = (state.phase + TICK_SECONDS) % 3600.0,
        Msg::ToggleDetails => state.details_open = !state.details_open,
    }
    Task::none()
}

/// A ~30 fps tick, only while a check is running.
pub fn subscription(state: &State, ctx: &Ctx) -> Subscription<Message> {
    let _ = state;
    if ctx.checking.is_some() {
        crate::gui::ticks_30().map(|_| Message::Home(Msg::Tick))
    } else {
        Subscription::none()
    }
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    if let Some(progress) = &ctx.checking {
        return scanning(state, ctx, progress);
    }
    if let Some(error) = &ctx.engine_error {
        return error_card(state, ctx, "We couldn't start Secblitz", error);
    }
    let Some(report) = ctx.report.as_deref() else {
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

/// Concentric rings that expand and fade behind the shield.
struct Pulse {
    p: Palette,
    phase: f32,
}

impl canvas::Program<Message> for Pulse {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let max = bounds.width.min(bounds.height) / 2.0 - 2.0;
        let min = 46.0;
        // Three rings, evenly staggered, each cycle lasting 2.4 s.
        for k in 0..3 {
            let t = ((self.phase / 2.4) + k as f32 / 3.0).fract();
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            let radius = min + (max - min) * eased;
            let alpha = (1.0 - t) * 0.5;
            frame.stroke(
                &Path::circle(center, radius),
                Stroke::default().with_width(2.0).with_color(iced::Color {
                    a: alpha,
                    ..self.p.text_muted
                }),
            );
        }
        frame.fill(&Path::circle(center, min + 8.0), self.p.surface_alt);
        vec![frame.into_geometry()]
    }
}

fn scanning<'a>(state: &State, ctx: &'a Ctx, progress: &CheckProgress) -> Element<'a, Message> {
    let p = ctx.palette;
    // Latest status per item, in first-seen order.
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

    let breathe = 1.0 + 0.06 * (state.phase * std::f32::consts::TAU / 2.4).sin();
    let hero = stack![
        canvas::Canvas::new(Pulse {
            p,
            phase: state.phase
        })
        .width(220)
        .height(220),
        container(widgets::icon(Icon::Shield, 64.0 * breathe, p.text)).center(Length::Fill),
    ]
    .width(220)
    .height(220);

    let mut list = column![].spacing(10);
    let start = n.saturating_sub(5);
    for id in &seen[start..] {
        list = list.push(widgets::progress_row(
            p,
            ctx.t(advice::control_label(id)),
            StepState::Done,
        ));
    }
    list = list.push(widgets::progress_row(
        p,
        ctx.t("Looking at your settings…"),
        StepState::Running,
    ));

    let body = column![
        container(hero).center_x(Length::Fill),
        column![
            text(ctx.t("Checking your PC"))
                .size(theme::H1)
                .font(theme::BOLD)
                .color(p.text),
            widgets::muted(p, ctx.t("This takes about a minute. Nothing is changed.")),
        ]
        .spacing(6)
        .align_x(Alignment::Center)
        .width(Length::Fill),
        container(widgets::bar(p, ratio, Tone::Good)).max_width(420),
        container(list).max_width(420).padding([4, 0]),
    ]
    .spacing(theme::GAP + 8.0)
    .align_x(Alignment::Center)
    .width(Length::Fill);
    widgets::card(p, container(body).center_x(Length::Fill).padding([16, 0])).into()
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
            ctx.t("Technical details"),
            state.details_open,
            Message::Home(Msg::ToggleDetails),
            widgets::small(p, raw.to_owned()),
        ),
    ]
    .spacing(theme::GAP)
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

fn assessed<'a>(state: &State, ctx: &'a Ctx, report: &'a Report) -> Element<'a, Message> {
    let p = ctx.palette;
    let score = Score::of(report);
    let items = attention_items(report);
    let ids = crate::app::flow::recommended(report, &ctx.catalog.available);
    let (tone, title, subtitle) = match score.verdict() {
        Verdict::Protected => (
            Tone::Good,
            ctx.t("You're protected"),
            ctx.t("Everything we checked is switched on and working."),
        ),
        Verdict::Attention => {
            let n = score.attention.max(1);
            (
                Tone::Warn,
                count_text(
                    ctx,
                    "{n} thing needs your attention",
                    "{n} things need your attention",
                    n,
                ),
                if ids.is_empty() {
                    ctx.t("The steps below take just a moment in Windows Settings.")
                } else {
                    ctx.t("We can fix these for you in one step. You can undo any change later.")
                },
            )
        }
        Verdict::Unknown => (
            Tone::Neutral,
            ctx.t("We couldn't finish checking"),
            ctx.t("Some checks didn't finish. Try again in a moment."),
        ),
    };

    let ring_view = ring::ring(
        ring::Ring {
            p,
            ratio: score.ratio(),
            tone,
            label: format!("{}/{}", score.protected, score.total),
            caption: ctx.t("protected"),
        },
        176.0,
    );

    let mut buttons = row![].spacing(10).align_y(Alignment::Center);
    if ids.is_empty() {
        buttons = buttons.push(widgets::action(
            p,
            ButtonKind::Primary,
            ctx.t("Check again"),
            Some(Icon::Refresh),
            (!ctx.busy).then_some(Message::CheckNow),
        ));
    } else {
        let label = count_text(ctx, "Fix {n} problem", "Fix {n} problems", ids.len());
        buttons = buttons
            .push(widgets::action(
                p,
                ButtonKind::Primary,
                label,
                Some(Icon::Wrench),
                (!ctx.busy).then(|| Message::ReviewFixes(ids.clone())),
            ))
            .push(widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t("Check again"),
                None,
                (!ctx.busy).then_some(Message::CheckNow),
            ));
    }

    let mut texts = column![
        text(title)
            .size(theme::DISPLAY - 4.0)
            .font(theme::BOLD)
            .color(p.text),
        widgets::muted(p, subtitle),
    ]
    .spacing(6);
    if let Some(when) = last_checked(ctx) {
        texts = texts.push(widgets::small(p, when));
    }
    let hero = widgets::card(
        p,
        row![
            ring_view,
            column![texts, buttons]
                .spacing(theme::GAP + 6.0)
                .width(Length::Fill)
        ]
        .spacing(32)
        .align_y(Alignment::Center),
    );

    let mut page = column![].spacing(theme::GAP);
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
        let mut c = column![widgets::section_label(p, ctx.t("Good to know"))].spacing(10);
        for a in tips.iter().take(3) {
            c = c.push(widgets::inline_notice(
                p,
                Tone::Neutral,
                format!("{}. {}", ctx.t(a.label), ctx.t(a.next)),
            ));
        }
        page = page.push(c);
    }
    let _ = state;
    page.into()
}

fn attention_card<'a>(ctx: &'a Ctx, items: &[(&Outcome, advice::Advice)]) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut c = column![row![
        widgets::h2(p, ctx.t("Needs your attention")),
        widgets::pill(p, items.len().to_string(), Tone::Warn),
    ]
    .spacing(10)
    .align_y(Alignment::Center)]
    .spacing(6);
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
            .spacing(2)
            .width(Length::Fill),
            widgets::icon(Icon::ChevronRight, 16.0, p.text_muted),
        ]
        .spacing(14)
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
            .spacing(10)
            .align_y(Alignment::Center),
            widgets::small(p, summary),
        ]
        .spacing(2)
        .width(Length::Fill),
        widgets::link(p, ctx.t("See all"), Message::Navigate(Page::Fixes)),
    ]
    .spacing(14)
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
    fn tick_advances_and_wraps_phase() {
        let mut state = State::default();
        state.phase = 3599.99;
        state.phase = (state.phase + TICK_SECONDS) % 3600.0;
        assert!(state.phase < 1.0);
    }
}
