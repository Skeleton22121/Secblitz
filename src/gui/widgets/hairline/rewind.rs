//! Rewind: a clock running backwards, for putting things back.
//!
//! The prototype's `UNDO.rewind` with its `resultBadge`. While working the
//! hands spin backwards (minute 4.2 rad/s, hour a twelfth of that, with a
//! ghost trail) inside a back arrow that turns the other way. When the
//! work reports progress, the back arrow fills with it from its tail to
//! its head. Done: the hands ease back to ten past ten (the time before)
//! and a tick draws in on a small badge (good). Partly done: they stop
//! short and the badge shows an exclamation mark (warn). Failed: the clock
//! shakes, the hands stay where they were and the badge shows a cross (bad).
//!
//! Drag around the clock to turn the hands yourself (the angle follows the
//! pointer by how far it moved, so nothing jumps); let go and a finished
//! clock eases back to its time. A click spins it: faster while working, one
//! more turn back afterwards, a shake when it failed. Hovering names it.
use super::live::Live;
use super::motion::{lerp, phase, Spring};
use super::pointer::{Gesture, Hotspots, Layer};
use super::shield_fill::Run;
use super::stage::{pt, stroke, Ink, Plate, Stage, W_ACCENT, W_INK, W_PART, W_THICK};
use super::svg::{PathData, Seg};
use crate::gui::theme::{mix, Palette};
use crate::gui::widgets::anim::{self, DECELERATE, STANDARD};
use iced::widget::canvas::{self, Action, Event, Frame, Geometry};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::time::Instant;

/// The drawing's box in units: the clock and its back arrow.
pub const UNITS: Size = Size::new(176.0, 176.0);
/// Logical pixels per unit at the size pages show it (as the shield).
pub const SCALE: f32 = 0.75;
/// The canvas size pages give it: the drawing's height, and wide enough
/// that the hover name (up to about 330 px in German) fits beside it. The
/// drawing sits centred in it.
pub const SIZE: Size = Size::new(360.0, UNITS.height * SCALE);

/// Centre of the clock.
const C: Point = pt(88.0, 88.0);
/// Clock face radius.
const R: f32 = 54.0;
/// Back arrow radius, and where its arc starts (the head) and ends (the
/// tail), in radians (y down, so growing angles turn clockwise).
const RA: f32 = 76.0;
const A_HEAD: f32 = -1.9;
const A_TAIL: f32 = 3.3;
/// Hand lengths.
const MINUTE: f32 = 40.0;
const HOUR: f32 = 26.0;
/// Ten past ten: the time before.
const M0: f32 = 10.0 / 60.0 * TAU - FRAC_PI_2;
const H0: f32 = (10.0 + 10.0 / 60.0) / 12.0 * TAU - FRAC_PI_2;
/// Where the clock starts: about twenty to one.
const START_M: f32 = M0 + 3.0;
const START_H: f32 = H0 + TAU / 6.0 + 3.0 / 12.0;
/// Partly done stops this far short (minute hand, radians).
const SHORT: f32 = 0.9;
/// Minute hand speed while working (rad/s); the hour hand turns 12 times
/// slower. The back arrow turns the other way at 70 degrees a second.
const SPEED: f32 = 4.2;
const RING_SPEED: f32 = 70.0 * PI / 180.0;
/// Ambient second shown under reduced motion.
const STILL: f32 = 1.3;
/// Easing back to the time before, and back after a drag.
const EASE_BACK: f32 = 1.1;
const EASE_RETURN: f32 = 0.8;
const EASE_SPIN: f32 = 1.4;
/// A shake lasts this long.
const SHAKE_END: f32 = 1.6;
/// The badge has drawn in by then.
pub const BADGE_END: f32 = 1.4;
/// A click while working adds this much speed, which dies away.
const KICK: f32 = 14.0;
/// The hover area round the face.
const HOVER_R: f32 = 50.0;
/// The badge: where and how big.
const BADGE: Point = pt(C.x + 42.0, C.y + 42.0);
const BADGE_R: f32 = 13.0;

/// The parts a pointer can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Clock,
}

/// Which put-back this is, for the hover name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Undo {
    /// Undo of the last fixes.
    Fixes,
    /// Putting everything back before Secblitz is removed.
    Everything,
}

/// The hover name (an English catalog key; the page translates it).
pub fn label_key(undo: Undo, run: Run) -> &'static str {
    match (undo, run) {
        (_, Run::Working) => "Putting your settings back",
        (Undo::Fixes, Run::Done) => "Back to how it was before your fixes",
        (Undo::Fixes, Run::Partial) => "Some fixes were undone",
        (Undo::Everything, Run::Done) => "Back to how it was before Secblitz",
        (Undo::Everything, Run::Partial) => "Some things could not be put back",
        (_, Run::Failed) => "Nothing was changed",
    }
}

/// The drawing. Build it on the page and call [`Rewind::view`].
#[derive(Debug, Clone)]
pub struct Rewind {
    pub p: Palette,
    pub plate: Plate,
    pub run: Run,
    /// Share put back so far (0..=1), when the work reports it.
    pub progress: Option<f32>,
    /// When `run` began.
    pub changed: Instant,
    /// The page's last frame time (or `changed`).
    pub now: Instant,
    /// The translated hover name ([`label_key`]).
    pub label: String,
}

impl Rewind {
    pub fn view<'a, M: 'a>(self) -> Element<'a, M> {
        canvas::Canvas::new(self)
            .width(Length::Fixed(SIZE.width))
            .height(Length::Fixed(SIZE.height))
            .into()
    }

    fn busy(&self, st: &State, age: f32) -> bool {
        if anim::reduced() {
            return false;
        }
        self.run == Run::Working
            || st.ease.is_some()
            || st.spin > 0.0
            || st.fill.moving()
            || st.shake.is_some()
            || age < BADGE_END
    }
}

/// The hover areas, one function for `update` and `draw`: the clock face.
/// (Grabbing works out to the back arrow, see [`on_clock`]; the name sits
/// above the face, where there is room for it.)
pub fn spots() -> Hotspots<Part> {
    Hotspots::new().circle(Part::Clock, C, HOVER_R, Layer::Fixed)
}

/// Angle a in (-pi, pi].
pub fn wrap(a: f32) -> f32 {
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI {
        w + TAU
    } else {
        w
    }
}

/// The largest `rest + k turns` at or below `a`: where a hand going
/// backwards from `a` first shows `rest`.
pub fn back_to(a: f32, rest: f32) -> f32 {
    rest + TAU * ((a - rest) / TAU).floor()
}

/// The `rest + k turns` nearest to `a`.
pub fn nearest(a: f32, rest: f32) -> f32 {
    rest + TAU * ((a - rest) / TAU).round()
}

/// Where the hands rest for a finished state (minute, hour), or `None` when
/// they stay where they were (failed).
pub fn rest(run: Run) -> Option<(f32, f32)> {
    match run {
        Run::Done => Some((M0, H0)),
        Run::Partial => Some((M0 + SHORT, H0 + SHORT / 12.0)),
        Run::Working | Run::Failed => None,
    }
}

/// The pose shown under reduced motion: minute, hour, arrow turn.
fn still_pose(run: Run, st: &State) -> (f32, f32, f32) {
    match run {
        Run::Working => (
            START_M - STILL * SPEED,
            START_H - STILL * SPEED / 12.0,
            -STILL * RING_SPEED,
        ),
        Run::Failed => (st.m, st.h, 0.0),
        _ => {
            let (m, h) = rest(run).unwrap_or((M0, H0));
            (m, h, 0.0)
        }
    }
}

/// One eased move of the hands and the arrow.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Ease {
    from: [f32; 3],
    to: [f32; 3],
    el: f32,
    dur: f32,
}

#[derive(Debug, Clone)]
pub struct State {
    live: Live<Part>,
    /// Minute and hour hand angles, unwrapped so they turn on smoothly.
    m: f32,
    h: f32,
    /// The back arrow's turn.
    ring: f32,
    /// Extra speed from clicks while working.
    spin: f32,
    ease: Option<Ease>,
    /// While dragging: the pointer's last angle around the centre.
    grab: Option<f32>,
    /// The back arrow's fill, following progress.
    fill: Spring,
    /// Seconds into a shake.
    shake: Option<f32>,
    /// Where a finished clock's hands come back to after a drag.
    home: Option<(f32, f32)>,
}

impl Default for State {
    fn default() -> Self {
        State {
            live: Live::without_tilt(),
            m: START_M,
            h: START_H,
            ring: 0.0,
            spin: 0.0,
            ease: None,
            grab: None,
            fill: Spring::with(0.0, 90.0, 16.0),
            shake: None,
            home: None,
        }
    }
}

impl State {
    fn ease_to(&mut self, m: f32, h: f32, ring: f32, dur: f32) {
        self.ease = Some(Ease {
            from: [self.m, self.h, self.ring],
            to: [m, h, ring],
            el: 0.0,
            dur,
        });
    }

    /// A new state: ease back to the time before, or freeze and shake.
    fn enter(&mut self, run: Run) {
        self.ease = None;
        self.shake = None;
        self.home = None;
        let ring = back_to(self.ring, 0.0);
        match rest(run) {
            Some((m, h)) => {
                let (m, h) = (back_to(self.m, m), back_to(self.h, h));
                self.home = Some((m, h));
                self.ease_to(m, h, ring, EASE_BACK);
            }
            None if run == Run::Failed => {
                self.home = Some((self.m, self.h));
                self.ease_to(self.m, self.h, ring, EASE_BACK);
                self.shake = Some(0.0);
            }
            None => {}
        }
    }

    fn step(&mut self, run: Run, dt: f32) {
        if self.grab.is_none() {
            if run == Run::Working {
                let w = SPEED + self.spin;
                self.m -= dt * w;
                self.h -= dt * w / 12.0;
                self.ring -= dt * RING_SPEED;
            }
            if let Some(e) = &mut self.ease {
                e.el += dt;
                let k = DECELERATE.at(e.el / e.dur);
                let [m, h, r] = [0, 1, 2].map(|i| lerp(e.from[i], e.to[i], k));
                let done = e.el >= e.dur;
                (self.m, self.h, self.ring) = (m, h, r);
                if done {
                    self.ease = None;
                }
            }
        }
        self.spin *= (-dt * 2.5).exp();
        if self.spin < 0.02 {
            self.spin = 0.0;
        }
        if let Some(s) = &mut self.shake {
            *s += dt;
            if *s >= SHAKE_END {
                self.shake = None;
            }
        }
    }

    /// Turn the hands by how far the pointer went round the centre.
    fn turn_to(&mut self, at: Point) {
        let a = (at.y - C.y).atan2(at.x - C.x);
        if let Some(prev) = self.grab {
            let d = wrap(a - prev);
            self.m += d;
            self.h += d / 12.0;
        }
        self.grab = Some(a);
    }
}

/// Close enough to the clock to grab it.
fn on_clock(at: Point) -> bool {
    let d = ((at.x - C.x).powi(2) + (at.y - C.y).powi(2)).sqrt();
    d <= RA + 10.0
}

/// The shake, sideways in units.
fn shake_dx(st: &State) -> f32 {
    match st.shake {
        Some(s) if !anim::reduced() => (s * 34.0).sin() * 3.0 * (-s * 4.0).exp(),
        _ => 0.0,
    }
}

impl<M> canvas::Program<M> for Rewind {
    type State = State;

    fn update(
        &self,
        st: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        if st.live.fresh(self.changed) {
            st.enter(self.run);
        }
        let stage = Stage::fit(UNITS, bounds.size());
        let step = st.live.update(event, bounds, cursor, &stage, &spots());
        let reduced = anim::reduced();
        st.fill.aim(match self.run {
            Run::Working => self.progress.unwrap_or(0.0).clamp(0.0, 1.0),
            _ => 1.0,
        });
        if let Some(dt) = step.dt {
            st.fill.tick(dt);
            if reduced {
                st.ease = None;
                st.shake = None;
                st.spin = 0.0;
            } else {
                st.step(self.run, dt);
            }
        }
        match step.gesture {
            Some(Gesture::Press(at)) if on_clock(at) => {
                st.ease = None;
                st.grab = None;
                st.turn_to(at);
            }
            Some(Gesture::Drag { at, .. }) if st.grab.is_some() => st.turn_to(at),
            Some(Gesture::Release { at, click }) => {
                let held = st.grab.take().is_some();
                if click && on_clock(at) {
                    match self.run {
                        Run::Working => st.spin += KICK,
                        Run::Failed => st.shake = Some(0.0),
                        _ => {
                            if let Some((m, h)) = rest(self.run) {
                                let to = (back_to(st.m, m) - TAU, back_to(st.h, h) - TAU);
                                st.home = Some(to);
                                st.ease_to(to.0, to.1, st.ring, EASE_SPIN);
                            }
                        }
                    }
                } else if held && self.run != Run::Working {
                    if let Some((m, h)) = st.home {
                        let to = (nearest(st.m, m), nearest(st.h, h));
                        st.home = Some(to);
                        st.ease_to(to.0, to.1, st.ring, EASE_RETURN);
                    }
                }
                if click {
                    st.live.pulses.push(at, false);
                }
            }
            _ => {}
        }
        if reduced && st.grab.is_none() && self.run != Run::Working {
            // Transitions jump to their end.
            if let Some((m, h)) = st.home {
                (st.m, st.h) = (m, h);
            }
            st.ease = None;
        }
        st.live.redraw(&step, self.busy(st, st.live.age(self.changed, self.now)))
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
        let base = Stage::fit(UNITS, bounds.size());
        let stage = base.shifted(Vector::new(shake_dx(st), 0.0));
        let ink = Ink::new(&self.p, self.plate);
        let age = st.live.age(self.changed, self.now);
        let run = self.run;
        let (m, h, ring) = if anim::reduced() && st.grab.is_none() {
            still_pose(run, st)
        } else {
            (st.m, st.h, st.ring)
        };
        let working = run == Run::Working;

        // The colour of the moment: accent while working, then the result,
        // cross-faded. A failed clock goes grey; its badge says why.
        let result = match run {
            Run::Working => ink.accent,
            Run::Done => ink.good,
            Run::Partial => ink.warn,
            Run::Failed => ink.line,
        };
        let face = if working {
            ink.accent
        } else {
            mix(ink.accent, result, phase(age, 0.0, 0.45, STANDARD))
        };

        // The back arrow, turned by `ring`.
        let arrow = arrow_shape(ring);
        match (working, self.progress) {
            (true, Some(_)) => {
                let fill = if anim::reduced() {
                    self.progress.unwrap_or(0.0).clamp(0.0, 1.0)
                } else {
                    st.fill.value.clamp(0.0, 1.0)
                };
                f.stroke(&stage.shape(&arrow), stroke(ink.faint, W_PART));
                if fill > 0.001 {
                    let span = A_TAIL - A_HEAD;
                    let a1 = A_TAIL + ring;
                    f.stroke(
                        &stage.arc(C, RA, RA, a1 - span * fill, a1),
                        stroke(ink.accent, W_ACCENT),
                    );
                }
                let head = phase(fill, 0.97, 1.0, STANDARD);
                if head > 0.0 {
                    f.stroke(
                        &stage.shape(&head_shape(ring)),
                        stroke(ink.accent.scale_alpha(head), W_ACCENT),
                    );
                }
            }
            _ => {
                f.stroke(&stage.shape(&arrow), stroke(face.scale_alpha(0.75), W_PART));
                let done_in = if run == Run::Done {
                    phase(age, 0.9, 1.2, STANDARD)
                } else {
                    0.0
                };
                if done_in > 0.0 {
                    f.stroke(
                        &stage.shape(&arrow),
                        stroke(ink.good.scale_alpha(done_in), W_ACCENT),
                    );
                }
            }
        }

        // The face: tinted plate, inner ring and the twelve marks.
        let disc = stage.circle(C, R);
        f.fill(&disc, ink.tint(face));
        f.stroke(&disc, stroke(face, W_PART));
        f.stroke(&stage.circle(C, R - 6.0), ink.lo());
        let marks = |major: bool| {
            stage.path(|s| {
                for k in (0..12).filter(|k| (k % 3 == 0) == major) {
                    let a = k as f32 / 12.0 * TAU;
                    let l = if major { 7.0 } else { 4.0 };
                    let (c, sn) = (a.cos(), a.sin());
                    s.move_to(pt(C.x + (R - 10.0) * c, C.y + (R - 10.0) * sn));
                    s.line_to(pt(C.x + (R - 10.0 - l) * c, C.y + (R - 10.0 - l) * sn));
                }
            })
        };
        f.stroke(&marks(false), ink.ln2());
        f.stroke(&marks(true), ink.ln());

        // A ghost trail behind the minute hand while it spins back.
        if working {
            for k in 0..3 {
                let a = m + (k + 1) as f32 * 0.16;
                f.stroke(
                    &stage.line(C, hand(a, MINUTE)),
                    stroke(ink.accent.scale_alpha(0.6 - k as f32 * 0.18), W_PART),
                );
            }
        }
        // A failed clock's hands tremble once.
        let tremble = match (run, st.shake) {
            (Run::Failed, Some(s)) if !anim::reduced() => 0.12 * (s * 26.0).sin() * (-s * 2.5).exp(),
            _ => 0.0,
        };
        f.stroke(
            &stage.line(C, hand(h + tremble / 12.0, HOUR)),
            stroke(ink.ink, W_THICK),
        );
        f.stroke(&stage.line(C, hand(m + tremble, MINUTE)), stroke(ink.ink, W_INK));
        f.fill(&stage.circle(C, 2.6), ink.ink);

        // The result badge.
        if !working {
            let (show, mark_at) = match run {
                Run::Failed => (phase(age, 0.5, 0.7, STANDARD), (0.6, 0.9)),
                _ => (phase(age, 0.9, 1.1, STANDARD), (1.0, 1.3)),
            };
            if show > 0.0 {
                let bc = match run {
                    Run::Done => ink.good,
                    Run::Partial => ink.warn,
                    _ => ink.bad,
                };
                let plate = stage.circle(BADGE, BADGE_R);
                f.fill(&plate, ink.tint(bc).scale_alpha(show));
                f.stroke(&plate, stroke(bc.scale_alpha(show), W_ACCENT));
                let drawn = phase(age, mark_at.0, mark_at.1, DECELERATE);
                if drawn > 0.001 {
                    let d = badge_mark(run).partial(drawn);
                    f.stroke(&stage.shape(&d), stroke(bc, W_THICK));
                }
            }
        }

        let pulse: Color = if working { ink.accent } else { face };
        st.live.pulses.draw(&mut f, &base, pulse);
        if st.grab.is_none() {
            st.live
                .draw_tooltip(&mut f, &self.p, &base, &spots(), |_| self.label.clone());
        }
        vec![f.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        st: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if st.grab.is_some() && st.live.pointer.pressed {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) && st.live.pointer.inside && on_clock(st.live.pointer.at)
        {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
}

/// The end of a hand at angle `a`.
fn hand(a: f32, len: f32) -> Point {
    pt(C.x + len * a.cos(), C.y + len * a.sin())
}

fn polar(a: f32, r: f32) -> Point {
    pt(C.x + r * a.cos(), C.y + r * a.sin())
}

/// The back arrow's head (the stub past the arc and the two barbs),
/// turned by `ring`.
fn head_shape(ring: f32) -> PathData {
    let a0 = A_HEAD + ring;
    let p0 = polar(a0, RA);
    // Going backwards round the circle (anticlockwise on screen).
    let dv = Vector::new(a0.sin(), -a0.cos());
    let nv = Vector::new(a0.cos(), a0.sin());
    let tip = p0 + dv * 4.0;
    PathData {
        segs: vec![
            Seg::Move(p0),
            Seg::Line(tip),
            Seg::Move(tip - dv * 9.0 + nv * 6.0),
            Seg::Line(tip),
            Seg::Line(tip - dv * 9.0 - nv * 6.0),
        ],
    }
}

/// The whole back arrow: the arc from head to tail plus the head.
fn arrow_shape(ring: f32) -> PathData {
    let arc = arc_segs(A_HEAD + ring, A_TAIL + ring);
    let mut segs = arc.segs;
    segs.extend(head_shape(ring).segs);
    PathData { segs }
}

/// An arc of the back arrow's circle as path data (an SVG arc, which the
/// parser turns into cubics).
fn arc_segs(a0: f32, a1: f32) -> PathData {
    let p0 = polar(a0, RA);
    let p1 = polar(a1, RA);
    let large = u8::from((a1 - a0).abs() > PI);
    let sweep = u8::from(a1 > a0);
    PathData::of(&format!(
        "M{} {}A{RA} {RA} 0 {large} {sweep} {} {}",
        p0.x, p0.y, p1.x, p1.y
    ))
}

/// The tick, exclamation mark or cross inside the badge.
fn badge_mark(run: Run) -> PathData {
    let (x, y, r) = (BADGE.x, BADGE.y, BADGE_R);
    let segs = match run {
        Run::Done => vec![
            Seg::Move(pt(x - r * 0.42, y + 0.5)),
            Seg::Line(pt(x - r * 0.12, y + 0.5 + r * 0.3)),
            Seg::Line(pt(x + r * 0.43, y + 0.5 - r * 0.3)),
        ],
        Run::Partial => vec![
            Seg::Move(pt(x, y - r * 0.45)),
            Seg::Line(pt(x, y + r * 0.05)),
            Seg::Move(pt(x, y + r * 0.42)),
            Seg::Line(pt(x, y + r * 0.42 + 0.1)),
        ],
        _ => vec![
            Seg::Move(pt(x - r * 0.35, y - r * 0.35)),
            Seg::Line(pt(x + r * 0.35, y + r * 0.35)),
            Seg::Move(pt(x + r * 0.35, y - r * 0.35)),
            Seg::Line(pt(x - r * 0.35, y + r * 0.35)),
        ],
    };
    PathData { segs }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::theme::LIGHT;
    use crate::gui::widgets::anim::MOTION_LOCK;
    use crate::gui::widgets::hairline::Parallax;
    use crate::i18n::Lang;
    use iced::widget::canvas::Program;
    use iced::window;
    use std::time::Duration;

    const BOUNDS: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: SIZE.width,
        height: SIZE.height,
    };

    fn prog(run: Run, progress: Option<f32>, at: Instant) -> Rewind {
        Rewind {
            p: LIGHT,
            plate: Plate::Surface,
            run,
            progress,
            changed: at,
            now: at,
            label: String::new(),
        }
    }

    fn frame(at: Instant) -> Event {
        Event::Window(window::Event::RedrawRequested(at))
    }

    fn wants_frame(a: Option<Action<()>>) -> bool {
        a.map(|a| a.into_inner().1 == window::RedrawRequest::NextFrame)
            .unwrap_or(false)
    }

    /// Unit point to the window position the canvas sees.
    fn px(p: Point) -> Point {
        Stage::fit(UNITS, BOUNDS.size()).point(p)
    }

    fn mouse(e: mouse::Event) -> Event {
        Event::Mouse(e)
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn angles_unwrap_and_find_the_time_before() {
        assert!(close(wrap(TAU + 0.5), 0.5) && close(wrap(-TAU - 0.5), -0.5));
        assert!(close(wrap(PI), PI) && close(wrap(-PI), PI));
        // Going back from just past ten past ten reaches it at once; from
        // just before, a whole turn back.
        assert!(close(back_to(M0 + 0.2, M0), M0));
        assert!(close(back_to(M0 - 0.2, M0), M0 - TAU));
        assert!(close(back_to(M0 + 5.0 * TAU + 1.0, M0), M0 + 5.0 * TAU));
        assert!(close(nearest(M0 - 0.2, M0), M0) && close(nearest(M0 + 6.0, M0), M0 + TAU));
        // Done rests at ten past ten, partly done short of it, failed stays.
        assert_eq!(rest(Run::Done), Some((M0, H0)));
        let (m, h) = rest(Run::Partial).unwrap();
        assert!(m > M0 && h > H0);
        assert_eq!(rest(Run::Failed), None);
        assert_eq!(rest(Run::Working), None);
    }

    #[test]
    fn hover_names_the_face_and_grabbing_reaches_the_arrow() {
        let tilt = Parallax::off();
        let s = spots();
        assert_eq!(s.hit(C, &tilt), Some(Part::Clock));
        assert_eq!(s.hit(polar(0.7, R - 8.0), &tilt), Some(Part::Clock));
        assert_eq!(s.hit(polar(A_HEAD, RA), &tilt), None);
        assert_eq!(s.hit(pt(2.0, 2.0), &tilt), None);
        assert!(on_clock(polar(A_HEAD, RA)));
        // The name fits above the face at the size pages use.
        let anchor = s.anchor(Part::Clock, &tilt).unwrap();
        let stage = Stage::fit(UNITS, SIZE);
        assert!(stage.point(anchor).y >= 6.0 + 24.0, "{anchor:?}");
        assert!(on_clock(polar(1.0, RA + 8.0)) && !on_clock(pt(2.0, 2.0)));
        // Everything fits the box: the arrow head and the badge.
        for a in [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            let b = arrow_shape(a).bounds().unwrap();
            assert!(b.x >= 0.0 && b.y >= 0.0, "{b:?}");
            assert!(b.x + b.width <= UNITS.width && b.y + b.height <= UNITS.height);
        }
        const { assert!(BADGE.x + BADGE_R + 4.0 <= UNITS.width) };
    }

    #[test]
    fn labels_go_through_translation() {
        for undo in [Undo::Fixes, Undo::Everything] {
            for run in [Run::Working, Run::Done, Run::Partial, Run::Failed] {
                let key = label_key(undo, run);
                for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                    assert_ne!(lang.t(key), key, "{key} in {lang:?}");
                }
            }
        }
    }

    #[test]
    fn spins_back_then_rests_at_ten_past_ten_and_goes_quiet() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        let mut st = State::default();
        let mut clock = t0;
        let tick = |st: &mut State, p: &Rewind, clock: &mut Instant| {
            *clock += Duration::from_millis(16);
            Program::update(p, st, &frame(*clock), BOUNDS, off)
        };
        let working = prog(Run::Working, Some(0.5), t0);
        let m_start = st.m;
        for _ in 0..60 {
            assert!(wants_frame(tick(&mut st, &working, &mut clock)));
        }
        // About a second of turning backwards.
        assert!(st.m < m_start - 3.5 && st.ring < 0.0);
        assert!((st.fill.value - 0.5).abs() < 0.05);
        let done = Rewind {
            run: Run::Done,
            changed: clock,
            now: clock,
            ..working.clone()
        };
        let before = st.m;
        let mut frames = 0;
        while wants_frame(tick(&mut st, &done, &mut clock)) {
            frames += 1;
            assert!(frames < 400, "never settled");
        }
        assert!(st.m <= before && close(wrap(st.m - M0), 0.0) && close(wrap(st.h - H0), 0.0));
        assert!(close(wrap(st.ring), 0.0));
        assert!(frames as f32 * 0.016 >= BADGE_END - 0.05);
        anim::set_reduced_override(None);
    }

    #[test]
    fn dragging_turns_the_hands_without_a_jump_and_they_come_back() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let done = prog(Run::Done, None, t0);
        let mut st = State::default();
        st.live.fresh(t0);
        (st.m, st.h, st.home) = (M0, H0, Some((M0, H0)));
        // Press on the right edge of the clock: nothing moves yet.
        let start = px(pt(C.x + 60.0, C.y));
        let at = mouse::Cursor::Available(start);
        Program::<()>::update(
            &done,
            &mut st,
            &mouse(mouse::Event::CursorMoved { position: start }),
            BOUNDS,
            at,
        );
        Program::<()>::update(
            &done,
            &mut st,
            &mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            BOUNDS,
            at,
        );
        assert!(close(st.m, M0) && st.grab.is_some());
        assert_eq!(
            Program::<()>::mouse_interaction(&done, &st, BOUNDS, at),
            mouse::Interaction::Grabbing
        );
        // A quarter turn clockwise, in small steps, then right round past
        // the seam at pi: the hand follows by the same amount.
        for i in 1..=30 {
            let a = i as f32 / 30.0 * (PI + 0.5);
            let p = px(polar(a, 60.0));
            Program::<()>::update(
                &done,
                &mut st,
                &mouse(mouse::Event::CursorMoved { position: p }),
                BOUNDS,
                mouse::Cursor::Available(p),
            );
        }
        assert!((st.m - (M0 + PI + 0.5)).abs() < 0.01, "{}", st.m - M0);
        assert!((st.h - (H0 + (PI + 0.5) / 12.0)).abs() < 0.01);
        // Let go: it eases back to ten past ten (the nearest one).
        let end = px(polar(PI + 0.5, 60.0));
        Program::<()>::update(
            &done,
            &mut st,
            &mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            BOUNDS,
            mouse::Cursor::Available(end),
        );
        assert!(st.grab.is_none() && st.ease.is_some());
        let mut clock = t0 + Duration::from_secs(5);
        for _ in 0..80 {
            clock += Duration::from_millis(16);
            Program::<()>::update(&done, &mut st, &frame(clock), BOUNDS, mouse::Cursor::Unavailable);
        }
        assert!(st.ease.is_none() && close(wrap(st.m - M0), 0.0));
        anim::set_reduced_override(None);
    }

    #[test]
    fn reduced_motion_rests_at_once() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(true));
        let t0 = Instant::now();
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        let working = prog(Run::Working, Some(0.3), t0);
        assert!(!wants_frame(Program::update(
            &working,
            &mut st,
            &frame(t0),
            BOUNDS,
            off
        )));
        let done = prog(Run::Done, None, t0 + Duration::from_secs(1));
        assert!(!wants_frame(Program::update(
            &done,
            &mut st,
            &frame(t0 + Duration::from_secs(1)),
            BOUNDS,
            off
        )));
        assert!(close(wrap(st.m - M0), 0.0) && st.ease.is_none());
        assert_eq!(still_pose(Run::Done, &st), (M0, H0, 0.0));
        anim::set_reduced_override(None);
    }
}
