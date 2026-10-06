//! Rewind: a clock running backwards, for putting things back.
use super::live::Live;
use super::motion::{lerp, phase, Spring};
use super::pointer::{Gesture, Hotspots, Layer};
use super::shield_fill::Run;
use super::stage::{pt, stroke, Ink, Plate, Stage, W_ACCENT, W_INK, W_PART, W_THICK};
use super::svg::{PathData, Seg};
use crate::gui::theme::{mix, Palette};
use crate::gui::widgets::anim::{self, DECELERATE, STANDARD};
use iced::widget::canvas::{self, Action, Event, Frame, Geometry};
use iced::{mouse, Color, Element, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::time::Instant;

pub const UNITS: Size = Size::new(176.0, 176.0);
pub const SCALE: f32 = 0.75;
pub const SIZE: Size = Size::new(360.0, UNITS.height * SCALE);

const C: Point = pt(88.0, 88.0);
const R: f32 = 54.0;
const RA: f32 = 76.0;
const A_HEAD: f32 = -1.9;
const A_TAIL: f32 = 3.3;
const MINUTE: f32 = 40.0;
const HOUR: f32 = 26.0;
const M0: f32 = 10.0 / 60.0 * TAU - FRAC_PI_2;
const H0: f32 = (10.0 + 10.0 / 60.0) / 12.0 * TAU - FRAC_PI_2;
const START_M: f32 = M0 + 3.0;
const START_H: f32 = H0 + TAU / 6.0 + 3.0 / 12.0;
const SHORT: f32 = 0.9;
const SPEED: f32 = 4.2;
const RING_SPEED: f32 = 70.0 * PI / 180.0;
const STILL: f32 = 1.3;
const EASE_BACK: f32 = 1.1;
const EASE_RETURN: f32 = 0.8;
const EASE_SPIN: f32 = 1.4;
const SHAKE_END: f32 = 1.6;
pub const BADGE_END: f32 = 1.4;
const KICK: f32 = 14.0;
const HOVER_R: f32 = 50.0;
const DEAD_R: f32 = 12.0;
const BADGE: Point = pt(C.x + 42.0, C.y + 42.0);
const BADGE_R: f32 = 13.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Clock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Undo {
    Fixes,
    Everything,
}

pub fn label_key(undo: Undo, run: Run) -> &'static str {
    match (undo, run) {
        (Undo::Fixes, Run::Working) => "Putting your settings back",
        (Undo::Everything, Run::Working) => "Putting everything back",
        (Undo::Fixes, Run::Done) => "Back to how it was before your fixes",
        (Undo::Fixes, Run::Partial) => "Some fixes were undone",
        (Undo::Everything, Run::Done) => "Back to how it was before Secblitz",
        (Undo::Everything, Run::Partial) => "Some things could not be put back",
        (_, Run::Failed) => "Nothing was changed",
    }
}

#[derive(Debug, Clone)]
pub struct Rewind {
    pub p: Palette,
    pub plate: Plate,
    pub run: Run,
    pub progress: Option<f32>,
    pub changed: Instant,
    pub now: Instant,
    pub label: String,
}

impl Rewind {
    pub fn view<'a, M: 'a>(self) -> Element<'a, M> {
        super::fixed_canvas(self, SIZE)
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

pub fn spots() -> Hotspots<Part> {
    Hotspots::new().circle(Part::Clock, C, HOVER_R, Layer::Fixed)
}

pub fn wrap(a: f32) -> f32 {
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI {
        w + TAU
    } else {
        w
    }
}

pub fn back_to(a: f32, rest: f32) -> f32 {
    rest + TAU * ((a - rest) / TAU).floor()
}

pub fn nearest(a: f32, rest: f32) -> f32 {
    rest + TAU * ((a - rest) / TAU).round()
}

pub fn rest(run: Run) -> Option<(f32, f32)> {
    match run {
        Run::Done => Some((M0, H0)),
        Run::Partial => Some((M0 + SHORT, H0 + SHORT / 12.0)),
        Run::Working | Run::Failed => None,
    }
}

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
    m: f32,
    h: f32,
    ring: f32,
    spin: f32,
    ease: Option<Ease>,
    held: bool,
    grab: Option<f32>,
    fill: Spring,
    tracked: bool,
    shake: Option<f32>,
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
            held: false,
            grab: None,
            fill: Spring::with(0.0, 90.0, 16.0),
            tracked: false,
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
        if !self.held {
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

    fn turn_to(&mut self, at: Point) {
        let (dx, dy) = (at.x - C.x, at.y - C.y);
        if dx * dx + dy * dy < DEAD_R * DEAD_R {
            self.grab = None;
            return;
        }
        let a = dy.atan2(dx);
        if let Some(prev) = self.grab {
            let d = wrap(a - prev);
            self.m += d;
            self.h += d / 12.0;
        }
        self.grab = Some(a);
    }
}

fn on_clock(at: Point) -> bool {
    let d = ((at.x - C.x).powi(2) + (at.y - C.y).powi(2)).sqrt();
    d <= RA + 10.0
}

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
            if self.run == Run::Working {
                st.fill = Spring::with(0.0, 90.0, 16.0);
                st.tracked = false;
            }
            st.enter(self.run);
        }
        if self.run == Run::Working && self.progress.is_some() {
            st.tracked = true;
        }
        let stage = Stage::fit(UNITS, bounds.size());
        let step = st.live.update(event, bounds, cursor, &stage, &spots());
        let reduced = anim::reduced();
        let aim = match self.run {
            Run::Working => self.progress.unwrap_or(0.0).clamp(0.0, 1.0),
            Run::Done => 1.0,
            Run::Partial | Run::Failed => st.fill.target,
        };
        st.fill.aim(aim);
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
                st.held = true;
                st.grab = None;
                st.turn_to(at);
            }
            Some(Gesture::Drag { at, .. }) if st.held => st.turn_to(at),
            Some(Gesture::Release { at, click }) => {
                let held = std::mem::take(&mut st.held);
                st.grab = None;
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
                if click && on_clock(at) {
                    st.live.pulses.push(at, false);
                }
            }
            _ => {}
        }
        if reduced && !st.held && self.run != Run::Working {
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
        let (m, h, ring) = if anim::reduced() && !st.held {
            still_pose(run, st)
        } else {
            (st.m, st.h, st.ring)
        };
        let working = run == Run::Working;

        let result = match run {
            Run::Working => ink.accent,
            Run::Done => ink.good,
            Run::Partial => ink.warn,
            Run::Failed => ink.line,
        };
        let shift = if working {
            0.0
        } else {
            phase(age, 0.0, 0.45, STANDARD)
        };
        let face = mix(ink.accent, result, shift);
        let disc_fill = match run {
            Run::Failed => mix(ink.tint(ink.accent), ink.plate, shift),
            _ => ink.tint(face),
        };

        let arrow = arrow_shape(ring);
        let track = if working {
            self.progress.is_some()
        } else {
            st.tracked
        };
        if track {
            let fill = match (anim::reduced(), working) {
                (true, true) => self.progress.unwrap_or(0.0).clamp(0.0, 1.0),
                (true, false) => st.fill.target,
                _ => st.fill.value,
            }
            .clamp(0.0, 1.0);
            f.stroke(&stage.shape(&arrow), stroke(ink.faint, W_PART));
            if fill > 0.001 {
                let span = A_TAIL - A_HEAD;
                let a1 = A_TAIL + ring;
                f.stroke(
                    &stage.arc(C, RA, RA, a1 - span * fill, a1),
                    stroke(face, W_ACCENT),
                );
            }
            let head = phase(fill, 0.97, 1.0, STANDARD);
            if head > 0.0 {
                f.stroke(
                    &stage.shape(&head_shape(ring)),
                    stroke(face.scale_alpha(head), W_ACCENT),
                );
            }
        } else {
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

        let disc = stage.circle(C, R);
        f.fill(&disc, disc_fill);
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

        if working {
            for k in 0..3 {
                let a = m + (k + 1) as f32 * 0.16;
                f.stroke(
                    &stage.line(C, hand(a, MINUTE)),
                    stroke(ink.accent.scale_alpha(0.6 - k as f32 * 0.18), W_PART),
                );
            }
        }
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
        if !st.held {
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
        if st.held && st.live.pointer.pressed {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) && st.live.pointer.inside && on_clock(st.live.pointer.at)
        {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
}

fn hand(a: f32, len: f32) -> Point {
    pt(C.x + len * a.cos(), C.y + len * a.sin())
}

fn polar(a: f32, r: f32) -> Point {
    pt(C.x + r * a.cos(), C.y + r * a.sin())
}

fn head_shape(ring: f32) -> PathData {
    let a0 = A_HEAD + ring;
    let p0 = polar(a0, RA);
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

fn arrow_shape(ring: f32) -> PathData {
    let arc = arc_segs(A_HEAD + ring, A_TAIL + ring);
    let mut segs = arc.segs;
    segs.extend(head_shape(ring).segs);
    PathData { segs }
}

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
    use crate::gui::widgets::hairline::testing::{frame, wants_frame};
    use crate::gui::theme::LIGHT;
    use crate::gui::widgets::hairline::parallax::Parallax;
    use crate::i18n::Lang;
    use iced::widget::canvas::Program;
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
        assert!(close(back_to(M0 + 0.2, M0), M0));
        assert!(close(back_to(M0 - 0.2, M0), M0 - TAU));
        assert!(close(back_to(M0 + 5.0 * TAU + 1.0, M0), M0 + 5.0 * TAU));
        assert!(close(nearest(M0 - 0.2, M0), M0) && close(nearest(M0 + 6.0, M0), M0 + TAU));
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
        let anchor = s.anchor(Part::Clock, &tilt).unwrap();
        let stage = Stage::fit(UNITS, SIZE);
        assert!(stage.point(anchor).y >= 6.0 + 24.0, "{anchor:?}");
        assert!(on_clock(polar(1.0, RA + 8.0)) && !on_clock(pt(2.0, 2.0)));
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
        let _m = anim::forced::set(false);
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
    }

    #[test]
    fn dragging_turns_the_hands_without_a_jump_and_they_come_back() {
        let _m = anim::forced::set(false);
        let t0 = Instant::now();
        let done = prog(Run::Done, None, t0);
        let mut st = State::default();
        st.live.fresh(t0);
        (st.m, st.h, st.home) = (M0, H0, Some((M0, H0)));
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
        assert!(close(st.m, M0) && st.held && st.grab.is_some());
        assert_eq!(
            Program::<()>::mouse_interaction(&done, &st, BOUNDS, at),
            mouse::Interaction::Grabbing
        );
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
        let end = px(polar(PI + 0.5, 60.0));
        Program::<()>::update(
            &done,
            &mut st,
            &mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            BOUNDS,
            mouse::Cursor::Available(end),
        );
        assert!(!st.held && st.grab.is_none() && st.ease.is_some());
        let mut clock = t0 + Duration::from_secs(5);
        for _ in 0..80 {
            clock += Duration::from_millis(16);
            Program::<()>::update(&done, &mut st, &frame(clock), BOUNDS, mouse::Cursor::Unavailable);
        }
        assert!(st.ease.is_none() && close(wrap(st.m - M0), 0.0));
    }

    #[test]
    fn a_drag_through_the_centre_does_not_swing_the_hands() {
        let _m = anim::forced::set(false);
        let t0 = Instant::now();
        let done = prog(Run::Done, None, t0);
        let mut st = State::default();
        st.live.fresh(t0);
        (st.m, st.h, st.home) = (M0, H0, Some((M0, H0)));
        let send = |st: &mut State, e: mouse::Event, at: Point| {
            Program::<()>::update(&done, st, &mouse(e), BOUNDS, mouse::Cursor::Available(at));
        };
        let from = px(pt(C.x - 30.0, C.y));
        send(&mut st, mouse::Event::CursorMoved { position: from }, from);
        send(&mut st, mouse::Event::ButtonPressed(mouse::Button::Left), from);
        for i in 0..=60 {
            let p = px(pt(C.x - 30.0 + i as f32, C.y));
            send(&mut st, mouse::Event::CursorMoved { position: p }, p);
            assert!(close(st.m, M0), "jumped at step {i}: {}", st.m - M0);
        }
        assert!(st.held && st.grab.is_some());
    }

    #[test]
    fn clicks_beside_the_clock_make_no_ring() {
        let _m = anim::forced::set(false);
        let t0 = Instant::now();
        let working = prog(Run::Working, Some(0.2), t0);
        let mut st = State::default();
        let click = |st: &mut State, at: Point| {
            let c = mouse::Cursor::Available(at);
            for e in [
                mouse::Event::CursorMoved { position: at },
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                Program::<()>::update(&working, st, &mouse(e), BOUNDS, c);
            }
        };
        click(&mut st, Point::new(8.0, SIZE.height / 2.0));
        assert!(!st.live.pulses.alive() && st.spin == 0.0);
        click(&mut st, px(C));
        assert!(st.live.pulses.alive() && st.spin > 0.0);
    }

    #[test]
    fn a_tracked_arrow_stays_a_track_through_the_result() {
        let _m = anim::forced::set(false);
        let t0 = Instant::now();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        let mut st = State::default();
        let mut clock = t0;
        let working = prog(Run::Working, Some(0.6), t0);
        for _ in 0..60 {
            clock += Duration::from_millis(16);
            Program::<()>::update(&working, &mut st, &frame(clock), BOUNDS, off);
        }
        assert!(st.tracked && (st.fill.value - 0.6).abs() < 0.05);
        let done = prog(Run::Done, None, clock);
        let mut st_done = st.clone();
        for _ in 0..90 {
            clock += Duration::from_millis(16);
            Program::<()>::update(&done, &mut st_done, &frame(clock), BOUNDS, off);
        }
        assert!(st_done.tracked && st_done.fill.value > 0.99);
        let partial = prog(Run::Partial, None, clock);
        for _ in 0..90 {
            clock += Duration::from_millis(16);
            Program::<()>::update(&partial, &mut st, &frame(clock), BOUNDS, off);
        }
        assert!(st.tracked && (st.fill.value - 0.6).abs() < 0.05);
        let again = prog(Run::Working, None, clock);
        clock += Duration::from_millis(16);
        Program::<()>::update(&again, &mut st, &frame(clock), BOUNDS, off);
        assert!(!st.tracked && st.fill.value == 0.0);
    }

    #[test]
    fn reduced_motion_rests_at_once() {
        let _m = anim::forced::set(true);
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
    }
}
