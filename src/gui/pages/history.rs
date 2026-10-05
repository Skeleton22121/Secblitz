//! History: score trend + timeline of checks, fixes, undos, app clean-ups;
//! Undo last fixes; Restore removed apps. OWNER: fixes agent.
//!
//! Restoring individual apps is owned by the "Clean up apps" page; the card
//! here only leads there. Everything shown is derived once, when the log
//! arrives (`Msg::Loaded`), so `view()` only builds widgets and the trend
//! chart is drawn once into a `canvas::Cache`.
use super::fixes::{banner, row_text};
use crate::app::history::{self as log, Day, Entry, Kind};
use crate::app::worker::{self, Job};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Mode, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{blocking, Ctx, Message, Page};
use iced::widget::canvas::{self, Cache, Frame, Geometry, Path, Stroke};
use iced::widget::{column, container, row};
use iced::{mouse, Alignment, Element, Length, Point, Rectangle, Renderer, Task, Theme};
use std::cell::Cell;

const TREND_POINTS: usize = 30;
const PAGE_SIZE: usize = 40;
/// Height of the trend chart.
const CHART_HEIGHT: f32 = 140.0;
/// Padding inside the chart, around the guide lines.
const CHART_INSET: f32 = theme::S2;

#[derive(Debug, Default)]
pub struct State {
    visited: bool,
    /// `None` until the log has been read.
    data: Option<Data>,
    /// `None` until the engine answered.
    engine: Option<Result<Vec<String>, String>>,
    shown: usize,
    /// Trend chart geometry; cleared when the data or the theme changes.
    chart: Cache,
    chart_mode: Cell<Option<Mode>>,
}

/// Everything the page shows, derived from the score log in `update()`.
#[derive(Debug)]
struct Data {
    points: Vec<f32>,
    /// (protected, total) of the latest check.
    latest: Option<(usize, usize)>,
    /// Apps removed so far.
    removed: usize,
    days: Vec<Day>,
}

impl Data {
    fn of(entries: &[Entry]) -> Self {
        Self {
            points: log::trend(entries, TREND_POINTS),
            latest: entries
                .iter()
                .rev()
                .find(|e| e.total > 0)
                .map(|e| (e.protected, e.total)),
            removed: entries
                .iter()
                .filter(|e| e.kind == Kind::Debloat)
                .map(|e| e.n)
                .sum(),
            days: log::timeline(entries),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Vec<Entry>),
    ShowMore,
}

fn refresh(ctx: &Ctx) -> Task<Message> {
    let load = match ctx.state_dir.clone() {
        Some(dir) => Task::perform(blocking(move || log::load(&dir)), |e| {
            Message::History(Msg::Loaded(e))
        }),
        None => Task::done(Message::History(Msg::Loaded(Vec::new()))),
    };
    let engine = Task::run(ctx.worker.run(Job::History), Message::Worker);
    Task::batch([load, engine])
}

pub fn on_enter(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    state.visited = true;
    state.shown = PAGE_SIZE;
    refresh(ctx)
}

pub fn on_worker(state: &mut State, event: &worker::Event, ctx: &mut Ctx) -> Task<Message> {
    use worker::Event as E;
    match event {
        E::History(result) => {
            state.engine = Some(result.clone());
            Task::none()
        }
        // The score log was just appended to: re-read it while this page is in use.
        E::Checked(_) | E::Applied { .. } | E::Undone { .. } if state.visited => refresh(ctx),
        _ => Task::none(),
    }
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    let _ = ctx;
    match msg {
        Msg::Loaded(entries) => {
            state.data = Some(Data::of(&entries));
            state.chart.clear();
        }
        Msg::ShowMore => state.shown += PAGE_SIZE,
    }
    Task::none()
}

/// The engine has at least one applied, not yet undone batch.
fn can_undo(state: &State) -> bool {
    matches!(&state.engine, Some(Ok(lines)) if lines.iter().any(|l| l.ends_with(" applied")))
}

/// Score trend line, drawn once per data / theme / size change.
struct Trend<'a> {
    p: Palette,
    points: &'a [f32],
    cache: &'a Cache,
}

impl canvas::Program<Message> for Trend<'_> {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self
            .cache
            .draw(renderer, bounds.size(), |frame: &mut Frame| {
                let p = self.p;
                let (w, h) = (frame.width(), frame.height());
                let (top, bottom) = (CHART_INSET, h - CHART_INSET);
                let y_of = |ratio: f32| bottom - (bottom - top) * ratio.clamp(0.0, 1.0);
                for guide in [0.0, 0.5, 1.0] {
                    let y = y_of(guide);
                    frame.stroke(
                        &Path::line(Point::new(CHART_INSET, y), Point::new(w - CHART_INSET, y)),
                        Stroke::default().with_width(1.0).with_color(p.border),
                    );
                }
                let n = self.points.len();
                if n == 0 {
                    return;
                }
                let x_of = |i: usize| {
                    if n == 1 {
                        w / 2.0
                    } else {
                        CHART_INSET + (w - 2.0 * CHART_INSET) * i as f32 / (n - 1) as f32
                    }
                };
                if n > 1 {
                    let area = Path::new(|b| {
                        b.move_to(Point::new(x_of(0), bottom));
                        for (i, r) in self.points.iter().enumerate() {
                            b.line_to(Point::new(x_of(i), y_of(*r)));
                        }
                        b.line_to(Point::new(x_of(n - 1), bottom));
                        b.close();
                    });
                    frame.fill(&area, p.tint(Tone::Good));
                    let line = Path::new(|b| {
                        b.move_to(Point::new(x_of(0), y_of(self.points[0])));
                        for (i, r) in self.points.iter().enumerate().skip(1) {
                            b.line_to(Point::new(x_of(i), y_of(*r)));
                        }
                    });
                    frame.stroke(
                        &line,
                        Stroke::default()
                            .with_width(2.5)
                            .with_color(p.good)
                            .with_line_join(canvas::LineJoin::Round),
                    );
                }
                let last = Point::new(x_of(n - 1), y_of(self.points[n - 1]));
                frame.fill(&Path::circle(last, 5.0), p.good);
                frame.fill(&Path::circle(last, 2.5), p.surface);
            });
        vec![geometry]
    }
}

fn kind_icon(kind: Kind) -> (Icon, Tone) {
    match kind {
        Kind::Check => (Icon::Scan, Tone::Neutral),
        Kind::Fix => (Icon::Wrench, Tone::Good),
        Kind::Undo => (Icon::Undo, Tone::Warn),
        Kind::Debloat => (Icon::Package, Tone::Neutral),
        Kind::Restore => (Icon::Refresh, Tone::Good),
    }
}

pub(super) fn day_title(ctx: &Ctx, day: u64, today: u64) -> String {
    if day == today {
        return ctx.t("Today");
    }
    if day + 1 == today {
        return ctx.t("Yesterday");
    }
    let (y, m, d) = log::civil(day);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{} {} {}", d, ctx.t(MONTHS[(m as usize - 1) % 12]), y)
}

/// Sentence for the removed-apps card ("1 app" / "{n} apps").
fn removed_text(ctx: &Ctx, removed: usize) -> String {
    match removed {
        0 => ctx.t("You haven't removed any apps yet."),
        1 => ctx.t("You've removed 1 app so far. You can bring it back at any time."),
        n => ctx
            .t("You've removed {n} apps so far. You can bring one back at any time.")
            .replace("{n}", &n.to_string()),
    }
}

fn timeline_card<'a>(state: &'a State, ctx: &'a Ctx, days: &'a [Day]) -> Element<'a, Message> {
    let p = ctx.palette;
    let today = log::local_day(log::now());
    let mut c = column![widgets::h2(p, ctx.t("What happened"))].spacing(theme::S3);
    let mut budget = state.shown.max(PAGE_SIZE);
    let mut truncated = false;
    for day in days {
        if budget == 0 {
            truncated = true;
            break;
        }
        let mut rows = column![widgets::section_label(
            p,
            day_title(ctx, day.day, today)
        )]
        .spacing(theme::S1);
        for item in &day.items {
            if budget == 0 {
                truncated = true;
                break;
            }
            budget -= 1;
            let (icon, tone) = kind_icon(item.kind);
            let label = ctx
                .t(log::label(item.kind, item.n))
                .replace("{n}", &item.n.to_string());
            let mut detail = Vec::new();
            if item.total > 0 {
                detail.push(
                    ctx.t("{a} of {b} protected")
                        .replace("{a}", &item.protected.to_string())
                        .replace("{b}", &item.total.to_string()),
                );
            }
            if item.repeats > 1 {
                detail.push(
                    ctx.t("{n} checks")
                        .replace("{n}", &item.repeats.to_string()),
                );
            }
            let line = (!detail.is_empty()).then(|| detail.join(" · "));
            rows = rows.push(
                container(
                    row![widgets::icon_badge(p, icon, tone), row_text(p, label, line)]
                        .spacing(theme::S3)
                        .align_y(Alignment::Center),
                )
                .padding([theme::S3, theme::S4])
                .width(Length::Fill),
            );
        }
        c = c.push(rows);
        if truncated {
            break;
        }
    }
    if truncated {
        c = c.push(
            container(widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t("Show more"),
                None,
                Some(Message::History(Msg::ShowMore)),
            ))
            .center_x(Length::Fill),
        );
    }
    widgets::card(p, c).into()
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let header = widgets::page_header(
        p,
        ctx.t("History"),
        Some(ctx.t("How your protection has changed over time.")),
    );
    let spacer = iced::widget::space::vertical().height(theme::S6);
    let mut page = column![].spacing(theme::S4);

    let Some(data) = &state.data else {
        return column![
            header,
            spacer,
            widgets::card(
                p,
                column![
                    widgets::h2(p, ctx.t("Loading your history")),
                    widgets::muted(p, ctx.t("This only takes a moment.")),
                ]
                .spacing(theme::S1),
            )
        ]
        .into();
    };

    // Trend.
    if state.chart_mode.get() != Some(p.mode) {
        state.chart.clear();
        state.chart_mode.set(Some(p.mode));
    }
    let trend_body: Element<'a, Message> = if data.points.len() < 2 {
        widgets::muted(
            p,
            ctx.t("Check your PC a few times and we'll draw how your protection changes."),
        )
    } else {
        let summary = data
            .latest
            .map(|(a, b)| {
                ctx.t("{a} of {b} protected")
                    .replace("{a}", &a.to_string())
                    .replace("{b}", &b.to_string())
            })
            .unwrap_or_default();
        column![
            widgets::muted(p, summary),
            canvas::Canvas::new(Trend {
                p,
                points: &data.points,
                cache: &state.chart,
            })
            .width(Length::Fill)
            .height(Length::Fixed(CHART_HEIGHT)),
        ]
        .spacing(theme::S3)
        .into()
    };
    page = page.push(widgets::card(
        p,
        column![
            widgets::h2(p, ctx.t("Your protection over time")),
            trend_body
        ]
        .spacing(theme::S3),
    ));

    // Undo.
    let (undo_body, undo_enabled) = match &state.engine {
        None => (ctx.t("Checking what can be undone…"), false),
        Some(Err(_)) => (
            ctx.t("We couldn't read your list of fixes. Try again in a moment."),
            false,
        ),
        Some(Ok(_)) if can_undo(state) => (
            ctx.t("Put your settings back the way they were before your last fixes."),
            true,
        ),
        Some(Ok(_)) => (ctx.t("There's nothing to undo yet."), false),
    };
    page = page.push(banner(
        p,
        Icon::Undo,
        Tone::Neutral,
        ctx.t("Undo your last fixes"),
        undo_body,
        Some(widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Undo…"),
            None,
            (undo_enabled && !ctx.busy).then_some(Message::ReviewUndo),
        )),
    ));

    // Removed apps (restoring happens on the Clean up apps page).
    page = page.push(banner(
        p,
        Icon::Package,
        Tone::Neutral,
        ctx.t("Removed apps"),
        removed_text(ctx, data.removed),
        Some(widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Manage removed apps"),
            None,
            Some(Message::Navigate(Page::Debloat)),
        )),
    ));

    // Timeline.
    if data.days.is_empty() {
        page = page.push(widgets::card(
            p,
            widgets::empty_state(
                p,
                Icon::History,
                ctx.t("Nothing here yet"),
                ctx.t("When you check your PC or fix something, it will show up here."),
                None,
            ),
        ));
    } else {
        page = page.push(timeline_card(state, ctx, &data.days));
    }
    column![header, spacer, page].into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_needs_an_applied_batch() {
        let mut s = State::default();
        assert!(!can_undo(&s));
        s.engine = Some(Ok(vec!["b reverted".into(), "a pending".into()]));
        assert!(!can_undo(&s));
        s.engine = Some(Ok(vec!["c applied".into(), "b reverted".into()]));
        assert!(can_undo(&s));
        s.engine = Some(Err("x".into()));
        assert!(!can_undo(&s));
    }

    #[test]
    fn data_is_derived_once_from_the_log() {
        let e = |t, kind, protected, total, n| Entry {
            t,
            kind,
            protected,
            total,
            n,
        };
        let d = Data::of(&[
            e(1, Kind::Check, 3, 5, 0),
            e(2, Kind::Debloat, 0, 0, 2),
            e(3, Kind::Debloat, 0, 0, 1),
            e(4, Kind::Check, 4, 5, 0),
        ]);
        assert_eq!(d.removed, 3);
        assert_eq!(d.latest, Some((4, 5)));
        assert_eq!(d.points.len(), 2);
    }
}
