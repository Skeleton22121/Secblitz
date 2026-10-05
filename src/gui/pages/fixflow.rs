//! Fix / undo flow drawn over any page: review sheet → working → result.
//! OWNER: fixes agent.
//!
//! State machine: Closed -> Review{ids, undo} -> Working -> Result(Summary).
//! Only `Confirm` in the review sheet starts work; Esc / Cancel close it with
//! no change. The working view cannot be dismissed.
use super::fixes::{divider, link, sanitize};
use crate::app::flow::{self, Summary, SummaryKind};
use crate::app::worker::{self, Job, Phase};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{Ctx, Message};
use iced::widget::{column, container, row, scrollable, space, text};
use iced::{Alignment, Element, Length, Task};

#[derive(Debug, Default)]
pub struct State {
    stage: Stage,
}

#[derive(Debug, Default)]
enum Stage {
    #[default]
    Closed,
    Review {
        ids: Vec<String>,
        undo: bool,
    },
    Working {
        undo: bool,
        planned: Vec<String>,
        phase: Option<Phase>,
        /// (control id, engine status) as the engine reports them.
        items: Vec<(String, String)>,
    },
    Result {
        undo: bool,
        summary: Summary,
        /// Raw evidence, shown only inside "Technical details".
        technical: Vec<String>,
        show_technical: bool,
    },
}

#[derive(Debug, Clone)]
pub enum Msg {
    Confirm,
    Cancel,
    Done,
    /// Close the result and run a fresh check.
    CheckAgain,
    Technical,
}

impl State {
    /// True while the review sheet, working view or result is on screen.
    pub fn is_open(&self) -> bool {
        !matches!(self.stage, Stage::Closed)
    }
}

pub fn open_fixes(state: &mut State, ids: Vec<String>, ctx: &mut Ctx) -> Task<Message> {
    if ctx.busy || state.is_open() || ctx.check_error.is_some() || ctx.checking.is_some() {
        return Task::none();
    }
    // Only fixes the latest check still offers; stale selections are dropped.
    let Some(report) = ctx.report.as_deref() else {
        return Task::none();
    };
    let allowed = flow::candidates(report, &ctx.catalog.available);
    let mut chosen: Vec<String> = Vec::new();
    for id in ids {
        if allowed.contains(&id) && !chosen.contains(&id) {
            chosen.push(id);
        }
    }
    if chosen.is_empty() {
        return Task::none();
    }
    state.stage = Stage::Review {
        ids: chosen,
        undo: false,
    };
    Task::none()
}

pub fn open_undo(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    if ctx.busy || state.is_open() {
        return Task::none();
    }
    state.stage = Stage::Review {
        ids: Vec::new(),
        undo: true,
    };
    Task::none()
}

/// Esc: close the review sheet / result (never cancels running work).
pub fn escape(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let _ = ctx;
    if matches!(state.stage, Stage::Review { .. } | Stage::Result { .. }) {
        state.stage = Stage::Closed;
    }
    Task::none()
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Cancel | Msg::Done => {
            if matches!(state.stage, Stage::Review { .. } | Stage::Result { .. }) {
                state.stage = Stage::Closed;
            }
            Task::none()
        }
        Msg::CheckAgain => {
            if matches!(state.stage, Stage::Result { .. }) {
                state.stage = Stage::Closed;
                return Task::done(Message::CheckNow);
            }
            Task::none()
        }
        Msg::Technical => {
            if let Stage::Result { show_technical, .. } = &mut state.stage {
                *show_technical = !*show_technical;
            }
            Task::none()
        }
        Msg::Confirm => {
            let Stage::Review { ids, undo } = std::mem::take(&mut state.stage) else {
                return Task::none();
            };
            if ctx.busy {
                state.stage = Stage::Review { ids, undo };
                return Task::none();
            }
            ctx.busy = true;
            let job = if undo {
                Job::Undo
            } else {
                Job::Apply(ids.clone())
            };
            state.stage = Stage::Working {
                undo,
                planned: ids,
                phase: None,
                items: Vec::new(),
            };
            Task::run(ctx.worker.run(job), Message::Worker)
        }
    }
}

pub fn on_worker(state: &mut State, event: &worker::Event, ctx: &mut Ctx) -> Task<Message> {
    use worker::Event as E;
    let Stage::Working {
        undo,
        phase,
        items,
        ..
    } = &mut state.stage
    else {
        return Task::none();
    };
    match event {
        E::Progress {
            phase: p,
            id,
            status,
        } => {
            *phase = Some(*p);
            if matches!(p, Phase::Applying | Phase::Undoing) {
                items.push((id.clone(), status.clone()));
            }
        }
        E::Applied {
            attempted,
            result,
            verify,
        } if !*undo => {
            let summary = flow::summarize(
                Some(attempted),
                result.as_deref().map_err(String::as_str),
                verify.as_deref().map_err(String::as_str),
            );
            let technical = technical_lines(
                attempted,
                result.as_deref().map_err(String::as_str),
                verify.as_deref().map_err(String::as_str),
            );
            state.stage = Stage::Result {
                undo: false,
                summary,
                technical,
                show_technical: false,
            };
            ctx.busy = false;
        }
        E::Undone { result, verify } if *undo => {
            let summary = flow::summarize(
                None,
                result.as_deref().map_err(String::as_str),
                verify.as_deref().map_err(String::as_str),
            );
            let technical = technical_lines(
                &[],
                result.as_deref().map_err(String::as_str),
                verify.as_deref().map_err(String::as_str),
            );
            state.stage = Stage::Result {
                undo: true,
                summary,
                technical,
                show_technical: false,
            };
            ctx.busy = false;
        }
        _ => {}
    }
    Task::none()
}

/// Raw evidence lines for the "Technical details" expander.
fn technical_lines(
    attempted: &[String],
    result: Result<&secblitz::engine::Report, &str>,
    verify: Result<&secblitz::engine::Report, &str>,
) -> Vec<String> {
    let mut lines = Vec::new();
    match result {
        Ok(report) => {
            for r in &report.results {
                if attempted.is_empty() || attempted.contains(&r.id) {
                    lines.push(format!("{} · {} · {}", r.id, r.status, sanitize(&r.detail)));
                }
            }
        }
        Err(e) => lines.push(format!("result · {}", sanitize(e))),
    }
    if let Err(e) = verify {
        lines.push(format!("check · {}", sanitize(e)));
    }
    lines.truncate(60);
    lines
}

/// Wrap the window body with the active sheet / working / result view.
pub fn overlay<'a>(
    state: &'a State,
    ctx: &'a Ctx,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let p = ctx.palette;
    match &state.stage {
        Stage::Closed => body,
        Stage::Review { ids, undo } => widgets::sheet(p, body, review_view(ctx, ids, *undo)),
        Stage::Working {
            undo,
            planned,
            phase,
            items,
        } => widgets::sheet(p, body, working_view(ctx, *undo, planned, *phase, items)),
        Stage::Result {
            undo,
            summary,
            technical,
            show_technical,
        } => widgets::sheet(
            p,
            body,
            result_view(ctx, *undo, summary, technical, *show_technical),
        ),
    }
}

fn title_row<'a>(p: Palette, icon: Icon, tone: Tone, title: String) -> Element<'a, Message> {
    row![
        widgets::icon_badge(p, icon, tone),
        text(title).size(theme::H1).font(theme::BOLD).color(p.text)
    ]
    .spacing(14)
    .align_y(Alignment::Center)
    .into()
}

fn note<'a>(p: Palette, icon: Icon, s: String) -> Element<'a, Message> {
    row![
        widgets::icon(icon, 16.0, p.text_muted),
        text(s).size(theme::SMALL).font(theme::REGULAR).color(p.text_muted)
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// A bounded, scrollable list so a long selection never overflows the window.
fn bounded<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    container(scrollable(content)).max_height(300).into()
}

fn name_row<'a>(ctx: &Ctx, id: &str, trailing: Option<Element<'a, Message>>) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut r = row![
        text(ctx.lang.control(id)).size(theme::BODY).font(theme::MEDIUM).color(p.text),
        space::horizontal(),
    ]
    .align_y(Alignment::Center)
    .spacing(10);
    if ctx.catalog.restart.iter().any(|x| x == id) {
        r = r.push(widgets::pill(p, ctx.t("Needs restart"), Tone::Neutral));
    }
    if let Some(t) = trailing {
        r = r.push(t);
    }
    r.into()
}

fn review_view<'a>(ctx: &'a Ctx, ids: &'a [String], undo: bool) -> Element<'a, Message> {
    let p = ctx.palette;
    let n = ids.len();
    let title = if undo {
        ctx.t("Undo your last fixes?")
    } else if n == 1 {
        ctx.t("Fix 1 problem?")
    } else {
        ctx.t("Fix {n} problems?").replace("{n}", &n.to_string())
    };
    let mut c = column![title_row(
        p,
        if undo { Icon::Undo } else { Icon::Wrench },
        Tone::Neutral,
        title
    )]
    .spacing(theme::GAP);
    if undo {
        c = c.push(widgets::body(
            p,
            ctx.t("We'll put your settings back the way they were before your most recent fixes."),
        ));
        c = c.push(widgets::muted(
            p,
            ctx.t("This turns off the protection those fixes added. Anything you changed yourself since then is left as it is."),
        ));
    } else {
        c = c.push(widgets::muted(p, ctx.t("Here's what we'll change:")));
        let mut list = column![].spacing(10);
        for (i, id) in ids.iter().enumerate() {
            if i > 0 {
                list = list.push(divider(p));
            }
            list = list.push(name_row(ctx, id, None));
        }
        c = c.push(
            container(bounded(list.into()))
                .padding(14)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(p.surface_alt)),
                    border: iced::Border { radius: theme::RADIUS_SMALL.into(), ..Default::default() },
                    ..container::Style::default()
                }),
        );
        if ids.iter().any(|id| ctx.catalog.restart.contains(id)) {
            c = c.push(note(
                p,
                Icon::Restart,
                ctx.t("Some fixes need a restart. We'll never restart without asking."),
            ));
        }
    }
    c = c.push(note(
        p,
        Icon::History,
        if undo {
            ctx.t("Nothing else on your PC is touched.")
        } else {
            ctx.t("You can undo this later from History.")
        },
    ));
    c = c.push(
        row![
            widgets::action(p, ButtonKind::Secondary, ctx.t("Cancel"), None, Some(Message::Fix(Msg::Cancel))),
            space::horizontal(),
            widgets::action(
                p,
                ButtonKind::Primary,
                ctx.t(if undo { "Undo" } else { "Fix now" }),
                None,
                Some(Message::Fix(Msg::Confirm)),
            ),
        ]
        .align_y(Alignment::Center),
    );
    c.into()
}

fn status_icon<'a>(p: Palette, status: &str) -> Element<'a, Message> {
    match status {
        "applied" | "restored" | "unchanged" | "compliant" | "ok" => {
            widgets::icon(Icon::CheckCircle, 18.0, p.good)
        }
        "error" | "conflict" | "skipped" | "attention" => {
            widgets::icon(Icon::AlertTriangle, 18.0, p.warn)
        }
        _ => widgets::icon(Icon::Refresh, 18.0, p.text_muted),
    }
}

fn working_view<'a>(
    ctx: &'a Ctx,
    undo: bool,
    planned: &'a [String],
    phase: Option<Phase>,
    items: &'a [(String, String)],
) -> Element<'a, Message> {
    let p = ctx.palette;
    let title = if undo {
        ctx.t("Undoing your fixes…")
    } else {
        ctx.t("Fixing your PC…")
    };
    let c = column![
        title_row(p, Icon::Refresh, Tone::Neutral, title),
        widgets::muted(p, ctx.t("Please keep this window open. This can take a minute.")),
    ]
    .spacing(theme::GAP);

    let mut list = column![].spacing(10);
    // Planned fixes first (apply); undo lists items as the engine reports them.
    let mut shown: Vec<(String, String)> = Vec::new();
    for id in planned {
        let status = items
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, s)| s.clone())
            .unwrap_or_default();
        shown.push((id.clone(), status));
    }
    if planned.is_empty() {
        shown = items.to_vec();
    }
    for (id, status) in &shown {
        list = list.push(
            row![status_icon(p, status), name_row(ctx, id, None)]
                .spacing(12)
                .align_y(Alignment::Center),
        );
    }
    if shown.is_empty() {
        list = list.push(
            row![
                widgets::icon(Icon::Refresh, 18.0, p.text_muted),
                widgets::muted(p, ctx.t(if undo { "Putting your settings back" } else { "Getting ready" }))
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );
    }
    let verifying = phase == Some(Phase::Verifying);
    list = list.push(divider(p));
    list = list.push(
        row![
            if verifying {
                widgets::icon(Icon::Refresh, 18.0, p.text)
            } else {
                widgets::icon(Icon::ShieldCheck, 18.0, p.text_muted)
            },
            text(ctx.t("Checking the result"))
                .size(theme::BODY)
                .font(theme::MEDIUM)
                .color(if verifying { p.text } else { p.text_muted }),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    );
    c.push(bounded(list.into())).into()
}

fn bullets<'a>(ctx: &Ctx, heading: String, keys: &[String], tone: Tone) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut c = column![widgets::h2(p, heading)].spacing(8);
    for k in keys {
        c = c.push(
            row![
                widgets::icon(Icon::Check, 16.0, p.tone(tone)),
                text(ctx.t(k)).size(theme::BODY).font(theme::REGULAR).color(p.text).width(Length::Fill)
            ]
            .spacing(10),
        );
    }
    c.into()
}

fn result_view<'a>(
    ctx: &'a Ctx,
    undo: bool,
    s: &'a Summary,
    technical: &'a [String],
    show_technical: bool,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let (icon, tone, title) = match (undo, s.kind) {
        (false, SummaryKind::Success) => (Icon::ShieldCheck, Tone::Good, "You're now more protected"),
        (false, SummaryKind::Partial) => (Icon::ShieldAlert, Tone::Warn, "Some fixes are done"),
        (false, SummaryKind::Failed) => (Icon::ShieldX, Tone::Bad, "We couldn't make these fixes"),
        (true, SummaryKind::Success) => (Icon::CheckCircle, Tone::Good, "Your fixes were undone"),
        (true, SummaryKind::Partial) => (Icon::AlertTriangle, Tone::Warn, "Some fixes were undone"),
        (true, SummaryKind::Failed) => (Icon::ShieldX, Tone::Bad, "We couldn't undo your fixes"),
    };
    let hero = container(widgets::icon(icon, 56.0, p.tone(tone)))
        .center_x(Length::Fill)
        .padding([4, 0]);
    let mut body = column![].spacing(theme::GAP);
    if !s.protected_now.is_empty() {
        body = body.push(bullets(ctx, ctx.t("You're now protected from:"), &s.protected_now, Tone::Good));
    }
    if !s.after_restart.is_empty() {
        body = body.push(bullets(
            ctx,
            ctx.t("After you restart, you'll also be protected from:"),
            &s.after_restart,
            Tone::Neutral,
        ));
    }
    if undo && !s.done.is_empty() {
        let mut names = column![widgets::h2(p, ctx.t("Put back:"))].spacing(8);
        for id in &s.done {
            names = names.push(name_row(ctx, id, None));
        }
        body = body.push(names);
    }
    if !s.not_done.is_empty() {
        let mut names = column![widgets::h2(p, ctx.t("Couldn't fix"))].spacing(10);
        if undo {
            names = column![widgets::h2(p, ctx.t("Couldn't undo"))].spacing(10);
        }
        for (id, reason) in &s.not_done {
            names = names.push(
                column![
                    text(ctx.lang.control(id)).size(theme::BODY).font(theme::MEDIUM).color(p.text),
                    widgets::small(p, ctx.t(reason)),
                ]
                .spacing(2),
            );
        }
        body = body.push(names);
    } else if s.kind == SummaryKind::Failed && s.done.is_empty() {
        body = body.push(widgets::muted(
            p,
            ctx.t("Nothing was changed that we could confirm. You can try again in a moment."),
        ));
    }
    if s.restart {
        body = body.push(note(
            p,
            Icon::Restart,
            ctx.t("Restart your PC when you're ready to finish. We'll never restart it for you."),
        ));
    }
    if s.unverified {
        body = body.push(
            container(
                row![
                    widgets::icon(Icon::AlertTriangle, 18.0, p.warn),
                    text(ctx.t("We couldn't confirm the result — check again"))
                        .size(theme::BODY)
                        .font(theme::MEDIUM)
                        .color(p.text)
                        .width(Length::Fill),
                    widgets::action(
                        p,
                        ButtonKind::Secondary,
                        ctx.t("Check again"),
                        Some(Icon::Refresh),
                        Some(Message::Fix(Msg::CheckAgain)),
                    ),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding(14)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(p.tint(Tone::Warn))),
                border: iced::Border { radius: theme::RADIUS_SMALL.into(), ..Default::default() },
                ..container::Style::default()
            }),
        );
    }
    if !technical.is_empty() {
        let mut t = column![link(
            p,
            ctx.t("Technical details"),
            show_technical,
            Message::Fix(Msg::Technical)
        )]
        .spacing(6);
        if show_technical {
            for line in technical {
                t = t.push(widgets::small(p, line.clone()));
            }
        }
        body = body.push(t);
    }
    column![
        hero,
        container(text(ctx.t(title)).size(theme::H1).font(theme::BOLD).color(p.text))
            .center_x(Length::Fill),
        bounded(body.into()),
        row![
            space::horizontal(),
            widgets::action(p, ButtonKind::Primary, ctx.t("Done"), None, Some(Message::Fix(Msg::Done)))
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(theme::GAP)
    .into()
}
