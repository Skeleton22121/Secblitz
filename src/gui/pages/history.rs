//! History: score trend + timeline of checks, fixes, undos, app clean-ups;
//! Undo last fixes; Restore removed apps. OWNER: fixes agent.
//!
//! Restoring individual apps is owned by the "Clean up apps" page; the card
//! here only leads there.
use crate::app::history::{self as log, Day, Entry, Kind};
use crate::app::worker::{self, Job};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{blocking, Ctx, Message, Page};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{column, row, space, text};
use iced::{mouse, Alignment, Element, Length, Point, Rectangle, Renderer, Task, Theme};

const TREND_POINTS: usize = 30;
const PAGE_SIZE: usize = 40;

#[derive(Debug, Default)]
pub struct State {
    visited: bool,
    /// `None` until the log has been read.
    entries: Option<Vec<Entry>>,
    /// `None` until the engine answered.
    engine: Option<Result<Vec<String>, String>>,
    shown: usize,
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
        Msg::Loaded(entries) => state.entries = Some(entries),
        Msg::ShowMore => state.shown += PAGE_SIZE,
    }
    Task::none()
}

/// The engine has at least one applied, not yet undone batch.
fn can_undo(state: &State) -> bool {
    matches!(&state.engine, Some(Ok(lines)) if lines.iter().any(|l| l.ends_with(" applied")))
}

struct Trend {
    p: Palette,
    points: Vec<f32>,
}

impl canvas::Program<Message> for Trend {
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
        let (w, h) = (bounds.width, bounds.height);
        let pad = 8.0;
        let (top, bottom) = (pad, h - pad);
        let y_of = |ratio: f32| bottom - (bottom - top) * ratio.clamp(0.0, 1.0);
        for guide in [0.0, 0.5, 1.0] {
            let y = y_of(guide);
            frame.stroke(
                &Path::line(Point::new(pad, y), Point::new(w - pad, y)),
                Stroke::default().with_width(1.0).with_color(self.p.border),
            );
        }
        let n = self.points.len();
        if n == 0 {
            return vec![frame.into_geometry()];
        }
        let x_of = |i: usize| {
            if n == 1 {
                w / 2.0
            } else {
                pad + (w - 2.0 * pad) * i as f32 / (n - 1) as f32
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
            frame.fill(&area, self.p.tint(Tone::Good));
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
                    .with_color(self.p.good)
                    .with_line_join(canvas::LineJoin::Round),
            );
        }
        let last = Point::new(x_of(n - 1), y_of(self.points[n - 1]));
        frame.fill(&Path::circle(last, 5.0), self.p.good);
        frame.fill(&Path::circle(last, 2.5), self.p.surface);
        vec![frame.into_geometry()]
    }
}

fn card_with_action<'a>(
    p: Palette,
    icon: Icon,
    tone: Tone,
    title: String,
    body: String,
    button: Element<'a, Message>,
) -> Element<'a, Message> {
    widgets::card(
        p,
        row![
            widgets::icon_badge(p, icon, tone),
            column![widgets::h2(p, title), widgets::muted(p, body)]
                .spacing(4)
                .width(Length::Fill),
            button,
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .into()
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

fn day_title(ctx: &Ctx, day: u64, today: u64) -> String {
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

fn timeline_card<'a>(state: &'a State, ctx: &'a Ctx, days: &[Day]) -> Element<'a, Message> {
    let p = ctx.palette;
    let today = log::local_day(log::now());
    let mut c = column![widgets::h2(p, ctx.t("What happened"))].spacing(theme::GAP);
    let mut budget = state.shown.max(PAGE_SIZE);
    let mut truncated = false;
    'days: for day in days {
        if budget == 0 {
            truncated = true;
            break;
        }
        c = c.push(widgets::small(p, day_title(ctx, day.day, today)));
        for item in &day.items {
            if budget == 0 {
                truncated = true;
                break 'days;
            }
            budget -= 1;
            c = c.push(super::fixes::divider(p));
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
            let mut texts = column![text(label)
                .size(theme::BODY)
                .font(theme::MEDIUM)
                .color(p.text)]
            .spacing(2)
            .width(Length::Fill);
            if !detail.is_empty() {
                texts = texts.push(widgets::small(p, detail.join(" · ")));
            }
            c = c.push(
                row![widgets::icon_badge(p, icon, tone), texts]
                    .spacing(12)
                    .align_y(Alignment::Center),
            );
        }
    }
    if truncated {
        c = c.push(widgets::action(
            p,
            ButtonKind::Ghost,
            ctx.t("Show more"),
            None,
            Some(Message::History(Msg::ShowMore)),
        ));
    }
    widgets::card(p, c).into()
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut page = column![widgets::page_header(
        p,
        ctx.t("History"),
        Some(ctx.t("How your protection has changed over time.")),
    )]
    .spacing(theme::GAP);

    let Some(entries) = &state.entries else {
        return page
            .push(widgets::card(
                p,
                column![
                    widgets::h2(p, ctx.t("Loading your history")),
                    widgets::muted(p, ctx.t("This only takes a moment.")),
                ]
                .spacing(4),
            ))
            .into();
    };

    // Trend.
    let points = log::trend(entries, TREND_POINTS);
    let trend_body: Element<'a, Message> = if points.len() < 2 {
        widgets::muted(
            p,
            ctx.t("Check your PC a few times and we'll draw how your protection changes."),
        )
    } else {
        let latest = entries.iter().rev().find(|e| e.total > 0);
        let summary = latest
            .map(|e| {
                ctx.t("{a} of {b} protected")
                    .replace("{a}", &e.protected.to_string())
                    .replace("{b}", &e.total.to_string())
            })
            .unwrap_or_default();
        column![
            widgets::muted(p, summary),
            canvas::Canvas::new(Trend { p, points })
                .width(Length::Fill)
                .height(Length::Fixed(140.0)),
        ]
        .spacing(8)
        .into()
    };
    page = page.push(widgets::card(
        p,
        column![
            widgets::h2(p, ctx.t("Your protection over time")),
            trend_body
        ]
        .spacing(10),
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
    page = page.push(card_with_action(
        p,
        Icon::Undo,
        Tone::Neutral,
        ctx.t("Undo your last fixes"),
        undo_body,
        widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Undo…"),
            None,
            (undo_enabled && !ctx.busy).then_some(Message::ReviewUndo),
        ),
    ));

    // Removed apps (restoring happens on the Clean up apps page).
    let removed: usize = entries
        .iter()
        .filter(|e| e.kind == Kind::Debloat)
        .map(|e| e.n)
        .sum();
    let removed_body = if removed == 0 {
        ctx.t("You haven't removed any apps yet.")
    } else {
        ctx.t("You've removed {n} apps so far. You can bring one back at any time.")
            .replace("{n}", &removed.to_string())
    };
    page = page.push(card_with_action(
        p,
        Icon::Package,
        Tone::Neutral,
        ctx.t("Removed apps"),
        removed_body,
        widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Manage removed apps"),
            None,
            Some(Message::Navigate(Page::Debloat)),
        ),
    ));

    // Timeline.
    let days = log::timeline(entries);
    if days.is_empty() {
        page = page.push(widgets::card(
            p,
            row![
                widgets::icon_badge(p, Icon::History, Tone::Neutral),
                column![
                    widgets::h2(p, ctx.t("Nothing here yet")),
                    widgets::muted(
                        p,
                        ctx.t("When you check your PC or fix something, it will show up here."),
                    ),
                ]
                .spacing(4),
                space::horizontal(),
            ]
            .spacing(14)
            .align_y(Alignment::Center),
        ));
    } else {
        page = page.push(timeline_card(state, ctx, &days));
    }
    page.into()
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
}
