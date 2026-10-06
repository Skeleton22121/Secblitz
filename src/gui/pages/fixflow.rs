//! Fix / undo flow drawn over any page: review sheet → working → result.
use super::fixes::row_text;
use secblitz::model::CheckStatus;
use super::history::day_title;
use crate::app::flow::{self, Summary, SummaryKind};
use crate::app::history::{self as log, Entry, Kind};
use crate::app::worker::{self, Job, Phase};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::anim::{self, Clock, Tween};
use crate::gui::widgets::handoff;
use crate::gui::widgets::controls::{scroll_style, scrollbar};
use crate::gui::widgets::hairline::{self, rewind, shield_fill, Plate, Run};
use crate::gui::widgets::{self, progress, ButtonKind};
use crate::gui::{blocking, Ctx, Message};
use iced::widget::{column, container, row, scrollable, space};
use iced::{Alignment, Background, Border, Element, Length, Subscription, Task};
use std::time::Instant;

const LIST_MAX_HEIGHT: f32 = 300.0;
const MARK: f32 = theme::ICON_ROW;
const WAIT_DOT: f32 = 6.0;

#[derive(Debug)]
pub struct State {
    stage: Stage,
    now: Instant,
    frames_seen: bool,
    work: Clock,
    bar: Option<Tween>,
    since: Instant,
    batches: Vec<Vec<String>>,
    plan: Vec<PlanRow>,
    undo_note: Option<String>,
    undo_count: usize,
    checking: bool,
    held: Option<Held>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            stage: Stage::Closed,
            now: Instant::now(),
            frames_seen: false,
            work: Clock::new(),
            bar: None,
            since: Instant::now(),
            batches: Vec::new(),
            plan: Vec::new(),
            undo_note: None,
            undo_count: 0,
            checking: false,
            held: None,
        }
    }
}

/// A finished result kept back for a moment so the progress can settle.
#[derive(Debug)]
struct Held {
    at: Instant,
    undo: bool,
    summary: Summary,
    technical: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct PlanRow {
    id: String,
    name: String,
    line: Option<String>,
    restart: bool,
}

#[derive(Debug, Clone)]
struct Done {
    id: String,
    name: String,
    status: String,
    at: Instant,
}

#[derive(Debug, Default)]
enum Stage {
    #[default]
    Closed,
    Review {
        ids: Vec<String>,
        undo: bool,
    },
    Blocked {
        ids: Vec<String>,
        undo: bool,
        reason: String,
        retry: bool,
    },
    Working {
        undo: bool,
        phase: Option<Phase>,
        items: Vec<Done>,
    },
    Result {
        undo: bool,
        summary: Summary,
        technical: Vec<String>,
        show_technical: bool,
    },
}

#[derive(Debug, Clone)]
pub enum Msg {
    Confirm,
    Cancel,
    Done,
    CheckAgain,
    Retry,
    Technical,
    Frame(Instant),
    UndoInfo(Option<(u64, usize)>),
}

impl State {
    pub fn is_open(&self) -> bool {
        !matches!(self.stage, Stage::Closed)
    }

    fn progress(&self, start: Instant) -> f32 {
        if self.frames_seen {
            Clock::at(start).progress_at(anim::SLOW, self.now)
        } else {
            1.0
        }
    }

    fn live(&self) -> bool {
        matches!(self.stage, Stage::Working { .. })
    }
}

pub fn subscription(state: &State) -> Subscription<Message> {
    if state.live() && anim::animating() {
        iced::window::frames().map(|at| Message::Fix(Msg::Frame(at)))
    } else {
        Subscription::none()
    }
}

fn plan_row(ctx: &Ctx, id: &str, with_impact: bool) -> PlanRow {
    let impact = secblitz::advice::control_impact(id);
    let items = ctx
        .report
        .as_deref()
        .and_then(|r| r.results.iter().find(|o| o.id == id))
        .and_then(|o| super::fixes::items_line(ctx, o));
    let impact_line = (!impact.is_empty())
        .then(|| format!("{} {}", ctx.t("Protects you from:"), ctx.t(impact)));
    let consequence = secblitz::advice::is_choice_check_id(id)
        .then(|| secblitz::advice::choice_consequence(id))
        .filter(|c| !c.is_empty())
        .map(|c| ctx.t(c));
    let lines: Vec<String> = [impact_line, consequence, items].into_iter().flatten().collect();
    PlanRow {
        id: id.to_owned(),
        name: ctx.lang.control(id),
        line: (with_impact && !lines.is_empty()).then(|| lines.join("\n")),
        restart: ctx.catalog.restart.iter().any(|x| x == id),
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
    if flow::repairs_blocked(report).is_some() {
        return Task::none();
    }
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
    ctx.explain_open = None;
    state.plan = chosen.iter().map(|id| plan_row(ctx, id, true)).collect();
    state.stage = Stage::Review {
        ids: chosen,
        undo: false,
    };
    Task::none()
}

fn last_fix(entries: &[Entry]) -> Option<(u64, usize)> {
    let mut sorted: Vec<&Entry> = entries
        .iter()
        .filter(|e| matches!(e.kind, Kind::Fix | Kind::Undo))
        .collect();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.t));
    let mut undone = 0usize;
    for e in sorted {
        if e.kind == Kind::Undo {
            undone += 1;
        } else if undone > 0 {
            undone -= 1;
        } else {
            return Some((e.t, e.n));
        }
    }
    None
}

pub fn open_undo(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    if ctx.busy || state.is_open() {
        return Task::none();
    }
    ctx.explain_open = None;
    state.plan = state
        .batches
        .last()
        .map(|ids| ids.iter().map(|id| plan_row(ctx, id, false)).collect())
        .unwrap_or_default();
    state.undo_note = None;
    state.undo_count = 0;
    state.stage = Stage::Review {
        ids: Vec::new(),
        undo: true,
    };
    match ctx.state_dir.clone() {
        Some(dir) if state.plan.is_empty() => {
            Task::perform(blocking(move || last_fix(&log::load(&dir))), |found| {
                Message::Fix(Msg::UndoInfo(found))
            })
        }
        _ => Task::none(),
    }
}

pub fn escape(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let _ = ctx;
    if matches!(
        state.stage,
        Stage::Review { .. } | Stage::Blocked { .. } | Stage::Result { .. }
    ) {
        close(state);
    }
    Task::none()
}

fn close(state: &mut State) {
    state.held = None;
    state.stage = Stage::Closed;
    state.bar = None;
    state.checking = false;
}

fn planned(state: &State, undo: bool) -> usize {
    if undo && state.plan.is_empty() {
        state.undo_count
    } else {
        state.plan.len()
    }
}

fn bar_target(planned: usize, finished: usize, verifying: bool) -> f32 {
    let total = planned + 1;
    let done = if verifying {
        planned
    } else {
        finished.min(planned)
    };
    done as f32 / total as f32
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Frame(at) => {
            state.now = at;
            state.frames_seen = true;
            if state
                .held
                .as_ref()
                .is_some_and(|h| at.saturating_duration_since(h.at) >= handoff::sheet_hold())
            {
                if let Some(h) = state.held.take() {
                    show_result(state, h.undo, h.summary, h.technical);
                }
            }
            Task::none()
        }
        Msg::UndoInfo(found) => {
            if let (Stage::Review { undo: true, .. }, Some((t, n))) = (&state.stage, found) {
                state.undo_count = n;
                let when = day_title(ctx, log::local_day(t), log::local_day(log::now()));
                let key = if n == 1 {
                    "Your last fix changed 1 setting ({when}). We'll put it back the way it was."
                } else {
                    "Your last fix changed {n} settings ({when}). We'll put them back the way they were."
                };
                state.undo_note = Some(
                    ctx.t(key)
                        .replace("{when}", &when)
                        .replace("{n}", &n.to_string()),
                );
            }
            Task::none()
        }
        Msg::Cancel | Msg::Done => {
            if matches!(
                state.stage,
                Stage::Review { .. } | Stage::Blocked { .. } | Stage::Result { .. }
            ) {
                close(state);
            }
            Task::none()
        }
        Msg::CheckAgain => {
            if matches!(state.stage, Stage::Result { .. }) {
                close(state);
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
        Msg::Retry => {
            if let Stage::Blocked { ids, undo, .. } = std::mem::take(&mut state.stage) {
                state.stage = Stage::Review { ids, undo };
                return update(state, Msg::Confirm, ctx);
            }
            Task::none()
        }
        Msg::Confirm => {
            let Stage::Review { undo, .. } = &state.stage else {
                return Task::none();
            };
            if ctx.busy || state.checking {
                return Task::none();
            }
            let undo = *undo;
            state.checking = true;
            Task::run(ctx.worker.run(Job::Preflight { undo }), Message::Worker)
        }
    }
}

fn start(state: &mut State, ids: Vec<String>, undo: bool, ctx: &mut Ctx) -> Task<Message> {
    ctx.busy = true;
    let job = if undo {
        Job::Undo
    } else {
        Job::Apply(ids)
    };
    state.now = Instant::now();
    state.work = Clock::at(state.now);
    state.since = state.now;
    state.bar = (!undo || planned(state, true) > 0).then(|| Tween::new(0.0, 0.0, anim::NORMAL));
    state.stage = Stage::Working {
        undo,
        phase: None,
        items: Vec::new(),
    };
    Task::run(ctx.worker.run(job), Message::Worker)
}

pub fn on_worker(state: &mut State, event: &worker::Event, ctx: &mut Ctx) -> Task<Message> {
    use worker::Event as E;
    if let E::Preflight { undo: asked, result } = event {
        if !state.checking {
            return Task::none();
        }
        state.checking = false;
        let Stage::Review { ids, undo } = std::mem::take(&mut state.stage) else {
            return Task::none();
        };
        if undo != *asked {
            state.stage = Stage::Review { ids, undo };
            return Task::none();
        }
        return match result {
            Ok(()) => start(state, ids, undo, ctx),
            Err(raw) => {
                state.stage = Stage::Blocked {
                    ids,
                    undo,
                    reason: flow::plain_failure(raw).to_owned(),
                    retry: flow::can_retry(raw),
                };
                Task::none()
            }
        };
    }
    let planned = planned(state, matches!(state.stage, Stage::Working { undo: true, .. }));
    let Stage::Working {
        undo, phase, items, ..
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
                items.push(Done {
                    id: id.clone(),
                    name: ctx.lang.control(id),
                    status: status.clone(),
                    at: Instant::now(),
                });
            }
            let target = bar_target(planned, items.len(), *p == Phase::Verifying);
            if let Some(bar) = &mut state.bar {
                bar.retarget(Instant::now(), target);
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
                ctx,
                attempted,
                result.as_deref().map_err(String::as_str),
                verify.as_deref().map_err(String::as_str),
            );
            if let Ok(report) = result {
                let applied: Vec<String> = report
                    .results
                    .iter()
                    .filter(|r| r.status == CheckStatus::Applied && attempted.contains(&r.id))
                    .map(|r| r.id.clone())
                    .collect();
                state
                    .batches
                    .extend(secblitz::vbs::split_batches(&applied));
            }
            finish(state, false, summary, technical);
            ctx.busy = false;
        }
        E::Undone { result, verify } if *undo => {
            let summary = flow::summarize(
                None,
                result.as_deref().map_err(String::as_str),
                verify.as_deref().map_err(String::as_str),
            );
            let technical = technical_lines(
                ctx,
                &[],
                result.as_deref().map_err(String::as_str),
                verify.as_deref().map_err(String::as_str),
            );
            if !summary.done.is_empty() {
                state.batches.pop();
            }
            finish(state, true, summary, technical);
            ctx.busy = false;
        }
        _ => {}
    }
    Task::none()
}

fn finish(state: &mut State, undo: bool, summary: Summary, technical: Vec<String>) {
    let hold = handoff::sheet_hold();
    if hold.is_zero() || !state.frames_seen {
        show_result(state, undo, summary, technical);
        return;
    }
    let now = Instant::now();
    if let Some(bar) = &mut state.bar {
        bar.retarget(now, 1.0);
    }
    state.held = Some(Held {
        at: now,
        undo,
        summary,
        technical,
    });
}

fn show_result(state: &mut State, undo: bool, summary: Summary, technical: Vec<String>) {
    state.now = Instant::now();
    state.since = state.now;
    state.bar = None;
    state.held = None;
    state.stage = Stage::Result {
        undo,
        summary,
        technical,
        show_technical: false,
    };
}

fn technical_lines(
    ctx: &Ctx,
    attempted: &[String],
    result: Result<&secblitz::engine::Report, &str>,
    verify: Result<&secblitz::engine::Report, &str>,
) -> Vec<String> {
    let mut lines = Vec::new();
    match result {
        Ok(report) => {
            for r in &report.results {
                if attempted.is_empty() || attempted.contains(&r.id) {
                    let a = secblitz::advice::for_outcome(r);
                    let (status, next) = if r.status == CheckStatus::Error {
                        ("Not done", flow::NOT_DONE)
                    } else if r.status == CheckStatus::Skipped && r.detail.contains("readiness blocks") {
                        ("Not done", flow::REASON_DISK)
                    } else {
                        flow::plain_detail(&r.status, &a)
                    };
                    lines.push(format!(
                        "{} · {} · {}",
                        ctx.lang.control(&r.id),
                        ctx.t(status),
                        ctx.t(next)
                    ));
                }
            }
        }
        Err(e) => lines.push(ctx.t(flow::plain_failure(e))),
    }
    if let Err(e) = verify {
        let line = ctx.t(flow::plain_failure(e));
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    lines.truncate(60);
    lines
}


pub fn overlay_content<'a>(state: &'a State, ctx: &'a Ctx) -> Option<Element<'a, Message>> {
    match &state.stage {
        Stage::Closed => None,
        Stage::Review { ids, undo } => Some(review_view(state, ctx, ids, *undo)),
        Stage::Blocked {
            undo,
            reason,
            retry,
            ..
        } => Some(blocked_view(ctx, *undo, reason, *retry)),
        Stage::Working { undo, phase, items } => {
            Some(working_view(state, ctx, *undo, *phase, items))
        }
        Stage::Result {
            undo,
            summary,
            technical,
            show_technical,
        } => Some(widgets::appear::settle(
            result_view(state, ctx, *undo, summary, technical, *show_technical),
            ctx.palette.surface,
        )),
    }
}

fn bounded<'a>(p: Palette, content: Element<'a, Message>) -> Element<'a, Message> {
    container(
        scrollable(content)
            .direction(scrollbar())
            .style(scroll_style(p)),
    )
    .max_height(LIST_MAX_HEIGHT)
    .into()
}

fn below_art<'a>(p: Palette, content: Element<'a, Message>) -> Element<'a, Message> {
    container(
        scrollable(content)
            .direction(scrollbar())
            .style(scroll_style(p)),
    )
    .height(Length::Fill)
    .into()
}

fn art<'a>(
    state: &State,
    ctx: &Ctx,
    undo: bool,
    run: Run,
    progress: Option<f32>,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let (changed, now) = (state.since, state.now.max(state.since));
    let drawing = if undo {
        hairline::Rewind {
            p,
            plate: Plate::Surface,
            run,
            progress,
            changed,
            now,
            label: ctx.t(rewind::label_key(rewind::Undo::Fixes, run)),
        }
        .view()
    } else {
        hairline::ShieldFill {
            p,
            plate: Plate::Surface,
            run,
            progress,
            changed,
            now,
            label: ctx.t(shield_fill::label_key(run)),
        }
        .view()
    };
    container(drawing).center_x(Length::Fill).into()
}

fn work_share(planned: usize, finished: usize, verifying: bool) -> Option<f32> {
    (planned > 0).then(|| {
        if verifying {
            1.0
        } else {
            finished.min(planned) as f32 / planned as f32
        }
    })
}

fn result_art(s: &Summary) -> (Run, Option<f32>) {
    let run = match s.kind {
        SummaryKind::Success => Run::Done,
        SummaryKind::Partial => Run::Partial,
        SummaryKind::Failed => Run::Failed,
    };
    let total = s.done.len() + s.not_done.len();
    (run, (total > 0).then(|| s.done.len() as f32 / total as f32))
}

fn note<'a>(p: Palette, icon: Icon, s: String) -> Element<'a, Message> {
    row![
        widgets::icon(icon, 16.0, p.text_muted),
        widgets::small(p, s),
    ]
    .spacing(theme::S2)
    .align_y(Alignment::Center)
    .into()
}

fn footer<'a>(buttons: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut r = row![space::horizontal()]
        .spacing(theme::S2)
        .align_y(Alignment::Center);
    for b in buttons {
        r = r.push(b);
    }
    r.into()
}

fn plan_list<'a>(ctx: &Ctx, plan: &[PlanRow], restart_label: &str) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut list = column![].spacing(theme::S3);
    for r in plan {
        let mut line = row![row_text(p, r.name.clone(), r.line.clone())]
            .spacing(theme::S3)
            .align_y(Alignment::Center);
        if r.restart {
            line = line.push(widgets::tag(
                p,
                Some(Icon::Restart),
                restart_label.to_owned(),
            ));
        }
        if let Some(info) = widgets::explain::toggle(ctx, "plan", &r.id) {
            line = line.push(info);
        }
        let mut item = column![line].spacing(theme::S2);
        if let Some(inset) = widgets::explain::panel(ctx, "plan", &r.id, false, 0.0) {
            item = item.push(inset);
        }
        list = list.push(item);
    }
    bounded(p, list.into())
}

fn review_view<'a>(
    state: &'a State,
    ctx: &'a Ctx,
    ids: &'a [String],
    undo: bool,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let n = ids.len();
    let title = if undo {
        ctx.t("Undo your last fixes?")
    } else if n == 1 {
        ctx.t("Fix 1 problem?")
    } else {
        ctx.t("Fix {n} problems?").replace("{n}", &n.to_string())
    };
    let mut c = column![widgets::h2(p, title)].spacing(theme::S3);
    let restart_label = ctx.t("Needs restart");
    if undo {
        if !state.plan.is_empty() {
            c = c.push(widgets::muted(
                p,
                ctx.t("We'll put these settings back the way they were:"),
            ));
            c = c.push(plan_list(ctx, &state.plan, &restart_label));
        } else if let Some(note) = &state.undo_note {
            c = c.push(widgets::muted(p, note.clone()));
        } else {
            c = c.push(widgets::muted(
                p,
                ctx.t("We'll put your settings back the way they were before your last fix."),
            ));
        }
        c = c.push(widgets::small(
            p,
            ctx.t("Anything you changed yourself since then is left as it is."),
        ));
    } else {
        c = c.push(widgets::muted(p, ctx.t("Here's what we'll change:")));
        c = c.push(plan_list(ctx, &state.plan, &restart_label));
        if state.plan.iter().any(|r| r.restart) {
            c = c.push(note(
                p,
                Icon::Restart,
                ctx.t("Some fixes need a restart. We'll never restart without asking."),
            ));
        }
        c = c.push(note(
            p,
            Icon::History,
            ctx.t("You can undo this later from History."),
        ));
    }
    let confirm = widgets::action(
        p,
        if undo {
            ButtonKind::Danger
        } else {
            ButtonKind::Primary
        },
        ctx.t(if state.checking {
            "Checking…"
        } else if undo {
            "Undo fixes"
        } else {
            "Fix now"
        }),
        None,
        (!state.checking).then_some(Message::Fix(Msg::Confirm)),
    );
    c.push(space::vertical().height(theme::S3))
        .push(footer(vec![
            widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t("Cancel"),
                None,
                Some(Message::Fix(Msg::Cancel)),
            ),
            confirm,
        ]))
        .into()
}

fn blocked_view<'a>(ctx: &'a Ctx, undo: bool, reason: &str, retry: bool) -> Element<'a, Message> {
    let p = ctx.palette;
    let title = if undo {
        "We can't undo your fixes right now"
    } else {
        "We can't make these fixes right now"
    };
    let mut buttons = vec![widgets::action(
        p,
        ButtonKind::Secondary,
        ctx.t("Close"),
        None,
        Some(Message::Fix(Msg::Cancel)),
    )];
    if retry {
        buttons.push(widgets::action(
            p,
            ButtonKind::Primary,
            ctx.t("Try again"),
            Some(Icon::Refresh),
            Some(Message::Fix(Msg::Retry)),
        ));
    }
    column![
        widgets::h2(p, ctx.t(title)),
        widgets::muted(p, ctx.t(reason)),
        space::vertical().height(theme::S3),
        footer(buttons),
    ]
    .spacing(theme::S3)
    .into()
}

fn waiting_mark<'a>(p: Palette) -> Element<'a, Message> {
    container(
        container(space::horizontal())
            .width(WAIT_DOT)
            .height(WAIT_DOT)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.disabled_fg)),
                border: Border {
                    radius: (WAIT_DOT / 2.0).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
    )
    .center(MARK)
    .into()
}

fn done_mark<'a>(state: &State, p: Palette, status: &str, at: Instant) -> Element<'a, Message> {
    let t = state.progress(at);
    match status {
        "error" | "conflict" => anim::cross_draw(MARK, p.bad, t),
        "skipped" | "attention" | "pending" => anim::warn_draw(MARK, p.warn, t),
        _ => anim::check_draw(MARK, p.good, t),
    }
}

fn step<'a>(
    p: Palette,
    mark: Element<'a, Message>,
    label: String,
    active: bool,
) -> Element<'a, Message> {
    row![
        mark,
        iced::widget::text(label)
            .size(theme::BODY)
            .font(if active {
                theme::MEDIUM
            } else {
                theme::REGULAR
            })
            .color(if active { p.text } else { p.text_muted }),
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center)
    .into()
}

fn working_view<'a>(
    state: &'a State,
    ctx: &'a Ctx,
    undo: bool,
    phase: Option<Phase>,
    items: &'a [Done],
) -> Element<'a, Message> {
    let p = ctx.palette;
    let spin = |size: f32| anim::spinner(size, p.text, state.work.elapsed_at(state.now));
    let title = if undo {
        ctx.t("Putting your settings back…")
    } else {
        ctx.t("Fixing your PC…")
    };
    let settled = state.held.is_some();
    let verifying = phase == Some(Phase::Verifying) || settled;
    let share = work_share(planned(state, undo), items.len(), verifying);
    let mut c = column![
        art(state, ctx, undo, Run::Working, share),
        widgets::h2_centred(p, title),
        widgets::muted_centred(
            p,
            ctx.t("Please keep this window open. This can take a minute.")
        ),
    ]
    .spacing(theme::S3)
    .height(Length::Fixed(widgets::SHEET_FIT_HEIGHT));
    if let Some(bar) = &state.bar {
        c = c.push(progress::bar(p, bar, Tone::Brand, state.now));
    } else if undo {
        c = c.push(progress::indeterminate(p, Tone::Brand));
    }

    let mut list = column![].spacing(theme::S3);
    let finished = |d: &Done| done_mark(state, p, &d.status, d.at);
    if undo {
        for d in items {
            list = list.push(step(p, finished(d), d.name.clone(), false));
        }
        if items.is_empty() {
            list = list.push(step(
                p,
                spin(MARK),
                ctx.t("Putting your settings back"),
                true,
            ));
        }
    } else {
        let mut running_shown = false;
        for r in &state.plan {
            let (mark, active) = match items.iter().find(|d| d.id == r.id) {
                Some(d) => (finished(d), false),
                None if !running_shown && !verifying => {
                    running_shown = true;
                    (spin(MARK), true)
                }
                None => (waiting_mark(p), false),
            };
            list = list.push(step(p, mark, r.name.clone(), active));
        }
    }
    let last = match &state.held {
        Some(h) => anim::check_draw(MARK, p.good, state.progress(h.at)),
        None if verifying => spin(MARK),
        None => waiting_mark(p),
    };
    list = list.push(step(
        p,
        last,
        ctx.t("Checking the result"),
        verifying && !settled,
    ));
    c.push(below_art(p, list.into())).into()
}

fn bullet<'a>(p: Palette, tone: Tone, s: String) -> Element<'a, Message> {
    row![
        widgets::icon(Icon::Check, 16.0, p.tone(tone)),
        widgets::body(p, s)
    ]
    .spacing(theme::S2)
    .align_y(Alignment::Center)
    .into()
}

fn block<'a>(p: Palette, label: String, lines: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut c = column![widgets::section_label(p, label)].spacing(theme::S2);
    for l in lines {
        c = c.push(l);
    }
    c.into()
}

fn result_view<'a>(
    state: &'a State,
    ctx: &'a Ctx,
    undo: bool,
    s: &'a Summary,
    technical: &'a [String],
    show_technical: bool,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let title = match (undo, s.kind) {
        (false, SummaryKind::Success) => "You're now more protected",
        (false, SummaryKind::Partial) => "Some fixes are done",
        (false, SummaryKind::Failed) => "We couldn't make these fixes",
        (true, SummaryKind::Success) => "Your fixes were undone",
        (true, SummaryKind::Partial) => "Some fixes were undone",
        (true, SummaryKind::Failed) => "We couldn't undo your fixes",
    };
    let (run, share) = result_art(s);

    let mut body = column![].spacing(theme::S4);
    if !s.protected_now.is_empty() {
        body = body.push(block(
            p,
            ctx.t("You're now protected from:"),
            s.protected_now
                .iter()
                .map(|k| bullet(p, Tone::Good, ctx.t(k)))
                .collect(),
        ));
    }
    if !s.after_restart.is_empty() {
        body = body.push(block(
            p,
            ctx.t("After you restart, you'll also be protected from:"),
            s.after_restart
                .iter()
                .map(|k| bullet(p, Tone::Neutral, ctx.t(k)))
                .collect(),
        ));
    }
    if undo && !s.done.is_empty() {
        body = body.push(block(
            p,
            ctx.t("Put back:"),
            s.done
                .iter()
                .map(|id| bullet(p, Tone::Good, ctx.lang.control(id)))
                .collect(),
        ));
    }
    if !s.not_done.is_empty() {
        body = body.push(block(
            p,
            ctx.t(if undo {
                "Couldn't undo"
            } else {
                "Couldn't fix"
            }),
            s.not_done
                .iter()
                .map(|(id, reason)| row_text(p, ctx.lang.control(id), Some(ctx.t(reason))))
                .collect(),
        ));
    } else if let Some(why) = &s.failure {
        body = body.push(widgets::muted(p, ctx.t(why)));
    } else if s.kind == SummaryKind::Failed && s.done.is_empty() {
        body = body.push(widgets::muted(
            p,
            ctx.t("Nothing was changed that we could confirm. You can try again in a moment."),
        ));
    }
    if s.restart {
        body = body.push(widgets::inline_notice(
            p,
            Tone::Neutral,
            ctx.t("Restart your PC when you're ready to finish. We'll never restart it for you."),
        ));
    }
    if s.unverified {
        body = body.push(widgets::inline_notice(
            p,
            Tone::Warn,
            ctx.t("We couldn't confirm the result. Check again to be sure."),
        ));
    }
    if !technical.is_empty() {
        let mut lines = column![].spacing(theme::S1);
        for line in technical {
            lines = lines.push(widgets::small(p, line.clone()));
        }
        body = body.push(widgets::expander(
            p,
            ctx.t("More details"),
            show_technical,
            Message::Fix(Msg::Technical),
            lines,
        ));
    }

    let mut buttons = Vec::new();
    if s.unverified {
        buttons.push(widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Check again"),
            Some(Icon::Refresh),
            Some(Message::Fix(Msg::CheckAgain)),
        ));
    }
    buttons.push(widgets::action(
        p,
        ButtonKind::Primary,
        ctx.t("Done"),
        None,
        Some(Message::Fix(Msg::Done)),
    ));
    column![
        art(state, ctx, undo, run, share),
        container(widgets::h1(p, ctx.t(title))).center_x(Length::Fill),
        below_art(p, body.into()),
        space::vertical().height(theme::S3),
        footer(buttons),
    ]
    .spacing(theme::S3)
    .height(Length::Fixed(widgets::SHEET_FIT_HEIGHT))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(t: u64, kind: Kind, n: usize) -> Entry {
        Entry {
            t,
            kind,
            protected: 0,
            total: 0,
            n,
        }
    }

    #[test]
    fn undo_of_an_earlier_fix_counts_from_the_log() {
        let mut state = State {
            undo_count: 3,
            ..State::default()
        };
        assert_eq!(planned(&state, true), 3);
        assert_eq!(planned(&state, false), 0);
        state.plan = vec![PlanRow::default()];
        assert_eq!(planned(&state, true), 1);
    }

    #[test]
    fn last_fix_skips_fixes_already_undone() {
        assert_eq!(last_fix(&[]), None);
        let log = [e(1, Kind::Fix, 2), e(2, Kind::Check, 0), e(3, Kind::Fix, 4)];
        assert_eq!(last_fix(&log), Some((3, 4)));
        let log = [e(1, Kind::Fix, 2), e(2, Kind::Fix, 4), e(3, Kind::Undo, 0)];
        assert_eq!(last_fix(&log), Some((1, 2)));
        let log = [e(1, Kind::Fix, 2), e(3, Kind::Undo, 0)];
        assert_eq!(last_fix(&log), None);
    }

    #[test]
    fn drawing_follows_the_real_work() {
        assert_eq!(work_share(0, 0, false), None);
        assert_eq!(work_share(4, 1, false), Some(0.25));
        assert_eq!(work_share(4, 9, false), Some(1.0));
        assert_eq!(work_share(4, 0, true), Some(1.0));
        let mut s = Summary::default();
        assert_eq!(result_art(&s), (Run::Done, None));
        s.kind = SummaryKind::Partial;
        s.done = vec!["a".into(), "b".into(), "c".into()];
        s.not_done = vec![("d".into(), "why".into())];
        assert_eq!(result_art(&s), (Run::Partial, Some(0.75)));
        s.kind = SummaryKind::Failed;
        assert_eq!(result_art(&s).0, Run::Failed);
    }

    #[test]
    fn bar_moves_forward_and_never_completes_before_the_check() {
        assert_eq!(bar_target(3, 0, false), 0.0);
        assert!((bar_target(3, 3, false) - 0.75).abs() < 1e-6);
        assert!((bar_target(3, 9, false) - 0.75).abs() < 1e-6);
        assert!((bar_target(3, 0, true) - 0.75).abs() < 1e-6);
        assert!(bar_target(0, 0, true) <= 0.0 + f32::EPSILON);
    }
}
