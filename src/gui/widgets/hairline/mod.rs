//! Hairline drawings: calm line illustrations that answer the pointer.
//!
//! The visual and motion reference is the approved HTML prototype (its `HX`
//! engine, `PARTS` and one build/update function per drawing). This module is
//! the shared toolkit; each drawing is one `canvas::Program` built on it.
//!
//! # Rules every drawing keeps
//!
//! * Lines and shapes in the greys ([`Ink::line`], [`Ink::faint`],
//!   [`Ink::rule`], [`Ink::ink`]); plates (shapes that hide what is behind)
//!   filled with [`Ink::plate`], the background the drawing sits on.
//! * At most four colours, each with one [`Meaning`]: accent for work in
//!   progress, good for done or protected, warn for needs attention (and ads
//!   and trackers), bad for failed (and scam sites). A coloured part strokes
//!   in its colour and fills with [`Ink::tint`] (13 percent into the plate).
//! * Line widths are screen pixels, 1 to 2.5 ([`W_FAINT`]..=[`W_THICK`]),
//!   whatever the drawing's size. Map geometry by hand through a [`Stage`];
//!   never `Frame::scale` (the CPU renderer would scale the widths too).
//!   `Frame::translate` and `Frame::rotate` are fine.
//! * Under about 150 strokes and fills per frame: the CPU renderer runs on
//!   real customer PCs.
//! * Reduced motion ([`anim::reduced`]): loops show one chosen still frame
//!   ([`Live::ambient`]), transitions jump to their end ([`Live::age`] is
//!   [`SETTLED_AGE`]), springs jump ([`Spring::tick`]), no tilt and no
//!   pulses. Hover names and clicks still work.
//! * Frames only while something moves: [`Live::redraw`] asks for the next
//!   frame while the drawing is busy and returns `None` once it has
//!   settled. The drawing drives itself from the window's redraw events (as
//!   `appear.rs` does), so the page needs no frame subscription for it and a
//!   page that stops showing the drawing stops its frames.
//! * Light and dark: everything comes from the [`Palette`], nothing is a
//!   literal colour.
//!
//! # How a drawing is built
//!
//! The page passes plain inputs and keeps no animation state of its own:
//!
//! * the [`Palette`] and the [`Plate`] it sits on,
//! * a small state enum (for example `Idle, Working, Done, Attention`),
//! * the `Instant` that state began (`changed`),
//! * the page's `now` (its last frame time, or `changed` when it has no
//!   frame subscription),
//! * translated hover names, made with `ctx.t(...)` on the page.
//!
//! ```ignore
//! hairline::check_radar(p, Plate::Surface, Status::Working, state.changed, state.now, labels)
//! ```
//!
//! The program's `State` holds a [`Live`] (pointer, tilt, hovered part,
//! click pulses, its own frame clock) plus the drawing's own [`Spring`]s.
//! Springs are stepped in `update` on frame events, so `draw` is a pure
//! function of the inputs and the state:
//!
//! 1. `update`: call [`Live::fresh`] with `changed` to reset per-state
//!    things once, build the [`Stage`] from `bounds`, build the [`Hotspots`]
//!    from the state (one function, used again in `draw`), call
//!    [`Live::update`], step springs by `step.dt`, react to
//!    `step.gesture` / [`Step::click`], and return
//!    `live.redraw(&step, busy)` where `busy` says a loop runs, a
//!    transition is unfinished ([`Live::age`] below its end) or a spring
//!    moves (and is `false` under reduced motion).
//! 2. `draw`: one `Frame`, `age = live.age(changed, now)`,
//!    `t = live.ambient(now, STILL)`, every phase from [`phase`] with the
//!    app's curves (`DECELERATE`, `STANDARD`, `EMPHASIZED`,
//!    `EASE_IN_OUT`), each parallax layer drawn through
//!    `live.layer(&stage, Layer::Back | Mid | Front)`, then
//!    `live.pulses.draw(..)` and `live.draw_tooltip(..)` last.
//! 3. `mouse_interaction`: `live.interaction(bounds, cursor, draggable)`.
//!
//! Icons come from [`Glyph`] ([`Stage::icon`]); any other SVG path data
//! parses with [`PathData::parse`]; [`PathData::partial`] draws a line in.
//! The prototype's shared objects are in [`parts`]: [`laptop`],
//! [`monitor`], [`badge`] and [`shield_mark`].
//! The test at the bottom of this file is a complete tiny drawing.
pub mod glyph;
pub mod live;
pub mod motion;
pub mod parallax;
pub mod parts;
pub mod pointer;
pub mod rewind;
pub mod shield_fill;
pub mod stage;
pub mod svg;

pub use glyph::Glyph;
pub use live::{Live, Step, SETTLED_AGE};
pub use motion::{lerp, phase, Pulses, Spring, MAX_DT, PULSE_BIG, PULSE_LIFE, PULSE_SMALL};
pub use parallax::{Parallax, DEPTHS};
pub use parts::{badge, laptop, monitor, shield_mark, BadgeLook, Mark, ShieldLook};
pub use pointer::{
    interaction, label_width, tooltip, tooltip_rect, Area, Gesture, Hotspots, Layer, Pointer,
    Spot, CLICK_SLOP, TIP_SIZE,
};
pub use rewind::Rewind;
pub use shield_fill::{Run, ShieldFill};
pub use stage::{
    pt, stroke, tint, tint_by, Ink, Meaning, Plate, Sketch, Stage, TINT, W_ACCENT, W_FAINT,
    W_INK, W_LINE, W_MARK, W_PART, W_THICK,
};
pub use svg::{ParseError, PathData, Seg};

#[allow(unused_imports)]
use crate::gui::theme::Palette;
#[allow(unused_imports)]
use crate::gui::widgets::anim;

#[cfg(test)]
mod example {
    //! A complete tiny drawing: a badge with a shield that pulses while
    //! working and draws a tick in when done. Never shown; it proves the
    //! pieces fit and pins how `update` asks for frames.
    use super::*;
    use crate::gui::theme::LIGHT;
    use crate::gui::widgets::anim::{self, DECELERATE, MOTION_LOCK};
    use iced::widget::canvas::{self, Action, Event, Frame, Geometry};
    use iced::{mouse, window, Point, Rectangle, Renderer, Size, Theme};
    use std::time::{Duration, Instant};

    const UNITS: Size = Size::new(120.0, 96.0);
    const CENTRE: Point = pt(60.0, 48.0);
    const R: f32 = 18.0;
    /// Ambient second shown under reduced motion.
    const STILL: f32 = 0.6;
    /// When the done transition has finished.
    const DONE_END: f32 = 1.05;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Status {
        Idle,
        Working,
        Done,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Part {
        Badge,
    }

    struct Beacon {
        p: Palette,
        status: Status,
        changed: Instant,
        now: Instant,
        label: String,
    }

    #[derive(Default)]
    struct State {
        live: Live<Part>,
        pop: Spring,
    }

    impl Beacon {
        fn spots(&self) -> Hotspots<Part> {
            Hotspots::new().circle(Part::Badge, CENTRE, R + 2.0, Layer::Front)
        }
        fn busy(&self, st: &State) -> bool {
            if anim::reduced() {
                return false;
            }
            let transition = match self.status {
                Status::Idle => false,
                Status::Working => true,
                Status::Done => st.live.age(self.changed, self.now) < DONE_END,
            };
            transition || st.pop.moving()
        }
    }

    impl canvas::Program<()> for Beacon {
        type State = State;

        fn update(
            &self,
            st: &mut State,
            event: &Event,
            bounds: Rectangle,
            cursor: mouse::Cursor,
        ) -> Option<Action<()>> {
            let stage = Stage::fit(UNITS, bounds.size());
            let step = st.live.update(event, bounds, cursor, &stage, &self.spots());
            if let Some(dt) = step.dt {
                st.pop.tick(dt);
            }
            if let Some(at) = step.click() {
                if st.live.hover == Some(Part::Badge) {
                    st.pop.kick(8.0);
                } else {
                    st.live.pulses.push(at, false);
                }
            }
            st.live.redraw(&step, self.busy(st))
        }

        fn draw(
            &self,
            st: &State,
            renderer: &Renderer,
            _: &Theme,
            bounds: Rectangle,
            _: mouse::Cursor,
        ) -> Vec<Geometry> {
            let mut f = Frame::new(renderer, bounds.size());
            let stage = Stage::fit(UNITS, bounds.size());
            let ink = Ink::new(&self.p, Plate::Surface);
            let age = st.live.age(self.changed, self.now);
            let t = st.live.ambient(self.now, STILL);
            let back = st.live.layer(&stage, Layer::Back);
            let front = st.live.layer(&stage, Layer::Front);

            f.stroke(&back.ellipse(CENTRE, 40.0, 32.0), ink.lo());
            if self.status == Status::Working {
                let e = DECELERATE.at((t / 2.0).fract());
                f.stroke(
                    &back.circle(CENTRE, R + 24.0 * e),
                    ink.accent_line(ink.accent).with_color(ink.accent.scale_alpha(1.0 - e)),
                );
            }
            let color = match self.status {
                Status::Done => ink.of(Meaning::Done),
                Status::Working => ink.of(Meaning::Working),
                Status::Idle => ink.line,
            };
            let scale = 1.0 + 0.06 * st.pop.value;
            let look = BadgeLook {
                color,
                scale,
                glow: if st.live.hover.is_some() { 0.9 } else { 0.0 },
                ..BadgeLook::new(&ink)
            };
            badge(&mut f, &front, &ink, Glyph::Shield, CENTRE, R, &look);
            let r = R * scale;
            if self.status == Status::Done {
                let drawn = Glyph::Tick
                    .data()
                    .placed(CENTRE, r * 1.15)
                    .partial(phase(age, 0.25, 0.85, DECELERATE));
                f.stroke(&front.shape(&drawn), ink.thick(color));
            }
            st.live.pulses.draw(&mut f, &stage, ink.accent);
            st.live
                .draw_tooltip(&mut f, &self.p, &stage, &self.spots(), |_| self.label.clone());
            vec![f.into_geometry()]
        }

        fn mouse_interaction(
            &self,
            st: &State,
            bounds: Rectangle,
            cursor: mouse::Cursor,
        ) -> mouse::Interaction {
            st.live.interaction(bounds, cursor, false)
        }
    }

    const BOUNDS: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 240.0,
        height: 192.0,
    };

    fn frame(at: Instant) -> Event {
        Event::Window(window::Event::RedrawRequested(at))
    }

    /// One 16 ms frame through the program.
    fn tick(
        st: &mut State,
        prog: &Beacon,
        cursor: mouse::Cursor,
        clock: &mut Instant,
    ) -> Option<Action<()>> {
        *clock += Duration::from_millis(16);
        canvas::Program::update(prog, st, &frame(*clock), BOUNDS, cursor)
    }

    fn wants_frame(a: Option<Action<()>>) -> bool {
        a.map(|a| a.into_inner().1 == window::RedrawRequest::NextFrame)
            .unwrap_or(false)
    }

    #[test]
    fn example_drawing_hovers_clicks_and_goes_quiet() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let prog = Beacon {
            p: LIGHT,
            status: Status::Idle,
            changed: t0,
            now: t0,
            label: "Shield".into(),
        };
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(500.0, 500.0));
        let mut clock = t0;

        // Idle and untouched: one frame, then nothing more is asked for.
        assert!(!wants_frame(tick(&mut st, &prog, off, &mut clock)));

        // Pointer over the badge (unit 60,48 is pixel 120,96): named, and a
        // frame is asked for because the hover changed.
        let over = mouse::Cursor::Available(Point::new(120.0, 96.0));
        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(120.0, 96.0),
        });
        assert!(wants_frame(canvas::Program::update(
            &prog, &mut st, &moved, BOUNDS, over
        )));
        assert_eq!(st.live.hover, Some(Part::Badge));
        assert_eq!(
            canvas::Program::mouse_interaction(&prog, &st, BOUNDS, over),
            mouse::Interaction::Pointer
        );

        // Click the badge: it pops (a spring moves), no pulse.
        let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        canvas::Program::update(&prog, &mut st, &down, BOUNDS, over);
        assert!(wants_frame(canvas::Program::update(
            &prog, &mut st, &up, BOUNDS, over
        )));
        assert!(st.pop.moving() && !st.live.pulses.alive());

        // Frames keep coming while it settles, then stop.
        let mut frames = 0;
        while wants_frame(tick(&mut st, &prog, over, &mut clock)) {
            frames += 1;
            assert!(frames < 400, "never settled");
        }
        assert!(frames > 5);
        assert!(!st.pop.moving() && !st.live.moving());

        // Click on empty space: a pulse, which also ends by itself.
        let empty = mouse::Cursor::Available(Point::new(10.0, 10.0));
        canvas::Program::update(
            &prog,
            &mut st,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(10.0, 10.0),
            }),
            BOUNDS,
            empty,
        );
        assert_eq!(st.live.hover, None);
        canvas::Program::update(&prog, &mut st, &down, BOUNDS, empty);
        canvas::Program::update(&prog, &mut st, &up, BOUNDS, empty);
        assert!(st.live.pulses.alive());
        let mut frames = 0;
        while wants_frame(tick(&mut st, &prog, empty, &mut clock)) {
            frames += 1;
            assert!(frames < 400, "never settled");
        }
        assert!(!st.live.pulses.alive());

        // Working loops: every frame asks for the next.
        let working = Beacon {
            status: Status::Working,
            changed: clock,
            now: clock,
            ..prog
        };
        for _ in 0..30 {
            assert!(wants_frame(tick(&mut st, &working, empty, &mut clock)));
        }

        // Done: frames until the tick has drawn in, then quiet.
        let done = Beacon {
            status: Status::Done,
            changed: clock,
            now: clock,
            ..working
        };
        let mut frames = 0;
        while wants_frame(tick(&mut st, &done, empty, &mut clock)) {
            frames += 1;
            assert!(frames < 400, "never settled");
        }
        let secs = frames as f32 * 0.016;
        assert!((secs - DONE_END).abs() < 0.1, "{secs}");
        anim::set_reduced_override(None);
    }

    #[test]
    fn example_drawing_is_still_under_reduced_motion() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(true));
        let t0 = Instant::now();
        let prog = Beacon {
            p: LIGHT,
            status: Status::Working,
            changed: t0,
            now: t0,
            label: "Shield".into(),
        };
        let mut st = State::default();
        let over = mouse::Cursor::Available(Point::new(120.0, 96.0));
        // A looping state still asks for nothing.
        assert!(!wants_frame(canvas::Program::update(
            &prog,
            &mut st,
            &frame(t0),
            BOUNDS,
            over
        )));
        assert_eq!(st.live.ambient(t0, STILL), STILL);
        assert_eq!(st.live.age(t0, t0), SETTLED_AGE);
        // Hover still names the part (one redraw for the change)...
        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(120.0, 96.0),
        });
        assert!(wants_frame(canvas::Program::update(
            &prog, &mut st, &moved, BOUNDS, over
        )));
        assert_eq!(st.live.hover, Some(Part::Badge));
        // ...with no tilt, and the next frame is the last.
        assert_eq!(st.live.tilt.layers(), [iced::Vector::ZERO; 3]);
        assert!(!wants_frame(canvas::Program::update(
            &prog,
            &mut st,
            &frame(t0 + Duration::from_millis(16)),
            BOUNDS,
            over
        )));
        // A click still lands (the spring jumps, no pulse).
        let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        canvas::Program::update(&prog, &mut st, &down, BOUNDS, over);
        canvas::Program::update(&prog, &mut st, &up, BOUNDS, over);
        assert!(st.live.pointer.was_click);
        assert!(!st.pop.moving() && !st.live.pulses.alive());
        anim::set_reduced_override(None);
    }
}
