//! Fix / undo flow drawn over any page: review sheet → working → result.
//! OWNER: fixes agent.
//!
//! State machine: Closed -> Review{ids, undo} -> Working -> Result(Summary).
//! Only `Confirm` in the review sheet starts work; Esc / Cancel close it with
//! no change. The working view cannot be dismissed.
//!
//! Motion (docs/MOTION.md): a spinner per running item, a check / cross / warn
//! draw-in as each item finishes, the overall bar easing to each new value and
//! one check draw on the result. Frames are requested by `subscription()` only
//! while one of these runs; the shell must batch it into its subscriptions.
use super::fixes::{row_text, sanitize, well};
use super::history::day_title;
use crate::app::flow::{self, Summary, SummaryKind};
use crate::app::history::{self as log, Entry, Kind};
use crate::app::worker::{self, Job, Phase};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::anim::{self, Clock, Tween};
use crate::gui::widgets::controls::{scroll_style, scrollbar};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{blocking, Ctx, Message};
use iced::widget::{column, container, row, scrollable, space};
use iced::{Alignment, Border, Element, Length, Subscription, Task};
use std::time::Instant;

/// Tallest the list inside a sheet grows before it scrolls.
const LIST_MAX_HEIGHT: f32 = 300.0;
/// Status mark size in working rows (matches the row icon size).
const MARK: f32 = 18.0;

#[derive(Debug)]
pub struct State {
    stage: Stage,
    /// Timestamp of the latest animation frame (never `Instant::now()` in view).
    now: Instant,
    /// A frame has arrived at least once. Until then nothing can be animating
    /// (frames not wired, or motion reduced), so marks are drawn finished.
    frames_seen: bool,
    /// Spinner clock for the working view.
    work: Clock,
    /// Overall progress bar value while applying.
    bar: Option<Tween>,
    /// Starts when the result appears; cleared once the draw-in is over.
    result: Option<Instant>,
    /// Fixes applied during this session, newest last (ids), so Undo can
    /// list exactly what will be put back.
    batches: Vec<Vec<String>>,
    /// What an open undo sheet will restore.
    plan: Vec<PlanRow>,
    /// Plain sentence about the last recorded fix (from the score log).
    undo_note: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            stage: Stage::Closed,
            now: Instant::now(),
            frames_seen: false,
            work: Clock::new(),
            bar: None,
            result: None,
            batches: Vec::new(),
            plan: Vec::new(),
            undo_note: None,
        }
    }
}

/// One line of "what will change", translated once when the sheet opens.
#[derive(Debug, Clone)]
struct PlanRow {
    id: String,
    name: String,
    line: Option<String>,
    restart: bool,
}

/// A finished working item.
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
    Working {
        undo: bool,
        phase: Option<Phase>,
        /// Items as the engine reports them.
        items: Vec<Done>,
    },
    Result {
        undo: bool,
        summary: Summary,
        /// Raw evidence, shown only inside "More details".
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
    /// Animation frame (only while something animates).
    Frame(Instant),
    /// The last recorded fix: (unix seconds, how many settings).
    UndoInfo(Option<(u64, usize)>),
}

impl State {
    /// True while the review sheet, working view or result is on screen.
    pub fn is_open(&self) -> bool {
        !matches!(self.stage, Stage::Closed)
    }

    /// Progress 0..=1 of a one-shot started at `start`, as of the last frame.
    fn progress(&self, start: Instant) -> f32 {
        if self.frames_seen {
            Clock::at(start).progress_at(anim::SLOW, self.now)
        } else {
            1.0
        }
    }

    /// Something on screen is moving and needs frames.
    fn live(&self) -> bool {
        match self.stage {
            Stage::Working { .. } => true,
            Stage::Result { .. } => self.result.is_some(),
            _ => false,
        }
    }
}

// Batched into the shell's subscriptions (src/gui/mod.rs); until then it is
// simply unused and every mark is drawn in its finished state.
/// Frame subscription: on only while the working spinner or a result
/// draw-in runs, and never when Windows animations are switched off.
pub fn subscription(state: &State) -> Subscription<Message> {
    if state.live() && anim::animating() {
        iced::window::frames().map(|at| Message::Fix(Msg::Frame(at)))
    } else {
        Subscription::none()
    }
}

fn plan_row(ctx: &Ctx, id: &str, with_impact: bool) -> PlanRow {
    let impact = crate::advice::control_impact(id);
    PlanRow {
        id: id.to_owned(),
        name: ctx.lang.control(id),
        line: (with_impact && !impact.is_empty())
            .then(|| format!("{} {}", ctx.t("Protects you from:"), ctx.t(impact))),
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
    state.plan = chosen.iter().map(|id| plan_row(ctx, id, true)).collect();
    state.stage = Stage::Review {
        ids: chosen,
        undo: false,
    };
    Task::none()
}

/// The newest fix in the log that no later undo has cancelled.
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
    // Exact list when this session applied the batch that will be undone.
    state.plan = state
        .batches
        .last()
        .map(|ids| ids.iter().map(|id| plan_row(ctx, id, false)).collect())
        .unwrap_or_default();
    state.undo_note = None;
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

/// Esc: close the review sheet / result (never cancels running work).
pub fn escape(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let _ = ctx;
    if matches!(state.stage, Stage::Review { .. } | Stage::Result { .. }) {
        close(state);
    }
    Task::none()
}

fn close(state: &mut State) {
    state.stage = Stage::Closed;
    state.result = None;
    state.bar = None;
}

/// Share of the overall bar: each planned fix, plus one step for the check
/// that follows.
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
                .result
                .is_some_and(|start| Clock::at(start).done(anim::SLOW, at))
            {
                state.result = None;
            }
            Task::none()
        }
        Msg::UndoInfo(found) => {
            if let (Stage::Review { undo: true, .. }, Some((t, n))) = (&state.stage, found) {
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
            if matches!(state.stage, Stage::Review { .. } | Stage::Result { .. }) {
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
            state.now = Instant::now();
            state.work = Clock::at(state.now);
            state.result = None;
            state.bar = (!undo).then(|| Tween::new(0.0, 0.0, anim::NORMAL));
            state.stage = Stage::Working {
                undo,
                phase: None,
                items: Vec::new(),
            };
            Task::run(ctx.worker.run(job), Message::Worker)
        }
    }
}

pub fn on_worker(state: &mut State, event: &worker::Event, ctx: &mut Ctx) -> Task<Message> {
    use worker::Event as E;
    let planned = state.plan.len();
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
                attempted,
                result.as_deref().map_err(String::as_str),
                verify.as_deref().map_err(String::as_str),
            );
            if let Ok(report) = result {
                let applied: Vec<String> = report
                    .results
                    .iter()
                    .filter(|r| r.status == "applied" && attempted.contains(&r.id))
                    .map(|r| r.id.clone())
                    .collect();
                if !applied.is_empty() {
                    state.batches.push(applied);
                }
            }
            show_result(state, false, summary, technical);
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
            if !summary.done.is_empty() {
                state.batches.pop();
            }
            show_result(state, true, summary, technical);
            ctx.busy = false;
        }
        _ => {}
    }
    Task::none()
}

fn show_result(state: &mut State, undo: bool, summary: Summary, technical: Vec<String>) {
    state.now = Instant::now();
    state.result = Some(state.now);
    state.bar = None;
    state.stage = Stage::Result {
        undo,
        summary,
        technical,
        show_technical: false,
    };
}

/// Raw evidence lines for the "More details" expander.
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

// -------------------------------------------------------------------- view

/// Content of the active review / working / result sheet, if any.
pub fn overlay_content<'a>(state: &'a State, ctx: &'a Ctx) -> Option<Element<'a, Message>> {
    match &state.stage {
        Stage::Closed => None,
        Stage::Review { ids, undo } => Some(review_view(state, ctx, ids, *undo)),
        Stage::Working { undo, phase, items } => {
            Some(working_view(state, ctx, *undo, *phase, items))
        }
        Stage::Result {
            undo,
            summary,
            technical,
            show_technical,
        } => Some(result_view(
            state,
            ctx,
            *undo,
            summary,
            technical,
            *show_technical,
        )),
    }
}

/// A bounded, scrollable list so a long selection never overflows the window.
fn bounded<'a>(p: Palette, content: Element<'a, Message>) -> Element<'a, Message> {
    container(
        scrollable(content)
            .direction(scrollbar())
            .style(scroll_style(p)),
    )
    .max_height(LIST_MAX_HEIGHT)
    .into()
}

/// Small muted line with a leading icon.
fn note<'a>(p: Palette, icon: Icon, s: String) -> Element<'a, Message> {
    row![
        widgets::icon(icon, 16.0, p.text_muted),
        widgets::small(p, s),
    ]
    .spacing(theme::S2)
    .align_y(Alignment::Center)
    .into()
}

/// Footer action bar: buttons right aligned, `S2` apart, `S6` above.
fn footer<'a>(buttons: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut r = row![space::horizontal()]
        .spacing(theme::S2)
        .align_y(Alignment::Center);
    for b in buttons {
        r = r.push(b);
    }
    r.into()
}

fn plan_list<'a>(p: Palette, plan: &[PlanRow], restart_label: &str) -> Element<'a, Message> {
    let mut list = column![].spacing(theme::S3);
    for r in plan {
        let mut line = row![row_text(p, r.name.clone(), r.line.clone())]
            .spacing(theme::S3)
            .align_y(Alignment::Center);
        if r.restart {
            line = line.push(widgets::pill(p, restart_label.to_owned(), Tone::Neutral));
        }
        list = list.push(line);
    }
    well(p, bounded(p, list.into())).into()
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
            c = c.push(plan_list(p, &state.plan, &restart_label));
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
        c = c.push(plan_list(p, &state.plan, &restart_label));
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
        ctx.t(if undo { "Undo" } else { "Fix now" }),
        None,
        Some(Message::Fix(Msg::Confirm)),
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

/// Waiting dot for items that have not started.
fn waiting_mark<'a>(p: Palette) -> Element<'a, Message> {
    container(space::horizontal())
        .width(MARK)
        .height(MARK)
        .style(move |_| container::Style {
            border: Border {
                radius: (MARK / 2.0).into(),
                width: 1.5,
                color: p.border_strong,
            },
            ..container::Style::default()
        })
        .into()
}

/// Animated mark for one finished item.
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
    let mut c = column![
        row![spin(20.0), widgets::h2(p, title)]
            .spacing(theme::S3)
            .align_y(Alignment::Center),
        widgets::muted(
            p,
            ctx.t("Please keep this window open. This can take a minute.")
        ),
    ]
    .spacing(theme::S3);
    if let Some(bar) = &state.bar {
        c = c.push(widgets::bar(p, bar.value(state.now), Tone::Brand));
    }

    let verifying = phase == Some(Phase::Verifying);
    let mut list = column![].spacing(theme::S3);
    let finished = |d: &Done| done_mark(state, p, &d.status, d.at);
    if undo {
        // The engine names each setting as it puts it back.
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
    list = list.push(step(
        p,
        if verifying {
            spin(MARK)
        } else {
            waiting_mark(p)
        },
        ctx.t("Checking the result"),
        verifying,
    ));
    c.push(well(p, bounded(p, list.into()))).into()
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

fn group<'a>(p: Palette, label: String, lines: Vec<Element<'a, Message>>) -> Element<'a, Message> {
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
    let (tone, title) = match (undo, s.kind) {
        (false, SummaryKind::Success) => (Tone::Good, "You're now more protected"),
        (false, SummaryKind::Partial) => (Tone::Warn, "Some fixes are done"),
        (false, SummaryKind::Failed) => (Tone::Bad, "We couldn't make these fixes"),
        (true, SummaryKind::Success) => (Tone::Good, "Your fixes were undone"),
        (true, SummaryKind::Partial) => (Tone::Warn, "Some fixes were undone"),
        (true, SummaryKind::Failed) => (Tone::Bad, "We couldn't undo your fixes"),
    };
    // One draw-in, then a cached static mark (no more frames).
    let t = state.result.map_or(1.0, |at| state.progress(at));
    let hero_size = theme::CONTROL + theme::S6;
    let hero = match s.kind {
        SummaryKind::Success => anim::check_draw(hero_size, p.tone(tone), t),
        SummaryKind::Partial => anim::warn_draw(hero_size, p.tone(tone), t),
        SummaryKind::Failed => anim::cross_draw(hero_size, p.tone(tone), t),
    };

    let mut body = column![].spacing(theme::S4);
    if !s.protected_now.is_empty() {
        body = body.push(group(
            p,
            ctx.t("You're now protected from:"),
            s.protected_now
                .iter()
                .map(|k| bullet(p, Tone::Good, ctx.t(k)))
                .collect(),
        ));
    }
    if !s.after_restart.is_empty() {
        body = body.push(group(
            p,
            ctx.t("After you restart, you'll also be protected from:"),
            s.after_restart
                .iter()
                .map(|k| bullet(p, Tone::Neutral, ctx.t(k)))
                .collect(),
        ));
    }
    if undo && !s.done.is_empty() {
        body = body.push(group(
            p,
            ctx.t("Put back:"),
            s.done
                .iter()
                .map(|id| bullet(p, Tone::Good, ctx.lang.control(id)))
                .collect(),
        ));
    }
    if !s.not_done.is_empty() {
        body = body.push(group(
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
        container(hero).center_x(Length::Fill),
        container(widgets::h1(p, ctx.t(title))).center_x(Length::Fill),
        bounded(p, body.into()),
        space::vertical().height(theme::S3),
        footer(buttons),
    ]
    .spacing(theme::S3)
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
    fn bar_moves_forward_and_never_completes_before_the_check() {
        assert_eq!(bar_target(3, 0, false), 0.0);
        assert!((bar_target(3, 3, false) - 0.75).abs() < 1e-6);
        assert!((bar_target(3, 9, false) - 0.75).abs() < 1e-6);
        assert!((bar_target(3, 0, true) - 0.75).abs() < 1e-6);
        assert!(bar_target(0, 0, true) <= 0.0 + f32::EPSILON);
    }
}
