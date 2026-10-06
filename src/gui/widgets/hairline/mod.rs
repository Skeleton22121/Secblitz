//! Hairline drawings: calm line illustrations that answer the pointer.
pub mod glyph;
pub mod live;
pub mod magnifier;
pub mod motion;
pub mod parallax;
pub mod parts;
pub mod pointer;
pub mod rewind;
pub mod shield_fill;
pub mod stage;
pub mod start_menu;
pub mod svg;
pub mod web_globe;

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
    const STILL: f32 = 0.6;
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

        let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        canvas::Program::update(&prog, &mut st, &down, BOUNDS, over);
        assert!(wants_frame(canvas::Program::update(
            &prog, &mut st, &up, BOUNDS, over
        )));
        assert!(st.pop.moving() && !st.live.pulses.alive());

        let mut frames = 0;
        while wants_frame(tick(&mut st, &prog, over, &mut clock)) {
            frames += 1;
            assert!(frames < 400, "never settled");
        }
        assert!(frames > 5);
        assert!(!st.pop.moving() && !st.live.moving());

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

        let working = Beacon {
            status: Status::Working,
            changed: clock,
            now: clock,
            ..prog
        };
        for _ in 0..30 {
            assert!(wants_frame(tick(&mut st, &working, empty, &mut clock)));
        }

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
        assert!(!wants_frame(canvas::Program::update(
            &prog,
            &mut st,
            &frame(t0),
            BOUNDS,
            over
        )));
        assert_eq!(st.live.ambient(t0, STILL), STILL);
        assert_eq!(st.live.age(t0, t0), SETTLED_AGE);
        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(120.0, 96.0),
        });
        assert!(wants_frame(canvas::Program::update(
            &prog, &mut st, &moved, BOUNDS, over
        )));
        assert_eq!(st.live.hover, Some(Part::Badge));
        assert_eq!(st.live.tilt.layers(), [iced::Vector::ZERO; 3]);
        assert!(!wants_frame(canvas::Program::update(
            &prog,
            &mut st,
            &frame(t0 + Duration::from_millis(16)),
            BOUNDS,
            over
        )));
        let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        canvas::Program::update(&prog, &mut st, &down, BOUNDS, over);
        canvas::Program::update(&prog, &mut st, &up, BOUNDS, over);
        assert!(st.live.pointer.was_click);
        assert!(!st.pop.moving() && !st.live.pulses.alive());
        anim::set_reduced_override(None);
    }
}
