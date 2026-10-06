//! Shield fills up: the picture while Secblitz makes fixes, and the result.
//!
//! The prototype's `FIXES.shield`. An empty shield fills with lines like
//! water, the level following the real share of fixes made. When every fix
//! is in, the water reaches the top, the outline draws in green with a tick
//! and a ring of short rays. Partly done: the water stops part way and an
//! exclamation mark draws in (warn). Failed: the water drains and a crack
//! draws across (bad), with one shake.
//!
//! The shield turns a little towards the pointer (a cheap 3D turn: squash
//! on x and y and a small skew, all mapped by hand so line widths stay in
//! pixels). A click makes a ripple on the water; every fix that lands makes
//! a small one too. Hovering the shield names the state.
//!
//! Water is clipped to the shield by geometry (iced has no clip paths): the
//! outline is sampled once into a polygon and the water is built column by
//! column between the wave and the shield's bottom edge.
use super::glyph::Glyph;
use super::live::Live;
use super::motion::{lerp, phase, Spring};
use super::pointer::{Hotspots, Layer};
use super::stage::{pt, stroke, Ink, Plate, Stage, W_ACCENT, W_LINE, W_THICK};
use super::svg::PathData;
use crate::gui::theme::{mix, Palette};
use crate::gui::widgets::anim::{self, DECELERATE, EASE_IN_OUT, STANDARD};
use iced::widget::canvas::{self, Action, Event, Frame, Geometry};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use std::sync::OnceLock;
use std::time::Instant;

/// The drawing's box in units: the shield and its rays, nothing more.
pub const UNITS: Size = Size::new(184.0, 176.0);
/// Logical pixels per unit at the size pages show it (the prototype sheet
/// showed its 320-unit box at 240 px).
pub const SCALE: f32 = 0.75;
/// The canvas size pages give it.
pub const SIZE: Size = Size::new(UNITS.width * SCALE, UNITS.height * SCALE);

/// Centre of the shield, in units.
const C: Point = pt(92.0, 88.0);
/// Units per icon unit: the 24-unit shield glyph drawn 6.2 times larger.
const S: f32 = 6.2;
/// Ambient second shown under reduced motion.
const STILL: f32 = 0.8;
/// Every result transition (marks, rays, shake, the wave calming) is over by
/// then.
pub const RESULT_END: f32 = 2.4;
/// The level spring: slow and nearly critically damped, like water.
const LEVEL_K: f32 = 40.0;
const LEVEL_C: f32 = 12.0;
/// A ripple has died away after this many seconds.
const RIPPLE_LIFE: f32 = 1.9;
/// The hover area around the shield, in units.
const HOVER_R: f32 = 60.0;
/// Water lines: one every 1.25 icon units from the bottom up.
const LINES: usize = 16;
/// The crack across a broken shield, in icon units.
const CRACK: &str = "M8.2 4.6l2.4 4.6-2.6 2.4 4.6 3.4-1.6 2.6 3.2 2.6";
/// The gap cut through the water behind the exclamation mark.
const EXCL_GAP: &str = "M12 7.2v6.4M12 16.5v.1";
/// Shield extent across, in icon units (the glyph is symmetric about 12).
const X0: f32 = 3.7;
const X1: f32 = 20.3;

/// Where the work is. Shared with the rewind drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    Working,
    Done,
    Partial,
    Failed,
}

/// The hover name for each state (an English catalog key; the page
/// translates it with `ctx.t`).
pub fn label_key(run: Run) -> &'static str {
    match run {
        Run::Working => "Making your fixes",
        Run::Done => "You're now more protected",
        Run::Partial => "Some fixes are done",
        Run::Failed => "Nothing was changed",
    }
}

/// Where the water heads for: 0 is empty, 1 just full (1.04 hides the
/// surface above the top). `progress` is the share of fixes made, when
/// known; without it the water rises slowly by itself.
pub fn level_target(run: Run, progress: Option<f32>, age: f32) -> f32 {
    let of = |f: f32| 0.12 + 0.76 * f.clamp(0.0, 1.0);
    match run {
        Run::Working => match progress {
            Some(f) => of(f),
            None => 0.12 + 0.62 * (1.0 - (-age.max(0.0) / 5.0).exp()),
        },
        Run::Done => 1.04,
        Run::Partial => progress.map(|f| of(f).clamp(0.3, 0.8)).unwrap_or(0.55),
        Run::Failed => 0.0,
    }
}

/// The parts a pointer can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Shield,
}

/// The drawing. Build it on the page and call [`ShieldFill::view`].
#[derive(Debug, Clone)]
pub struct ShieldFill {
    pub p: Palette,
    pub plate: Plate,
    pub run: Run,
    /// Share of fixes made (0..=1), when known.
    pub progress: Option<f32>,
    /// When `run` began.
    pub changed: Instant,
    /// The page's last frame time (or `changed`).
    pub now: Instant,
    /// The translated hover name ([`label_key`]).
    pub label: String,
}

impl ShieldFill {
    pub fn view<'a, M: 'a>(self) -> Element<'a, M> {
        canvas::Canvas::new(self)
            .width(Length::Fixed(SIZE.width))
            .height(Length::Fixed(SIZE.height))
            .into()
    }

    fn busy(&self, st: &State, age: f32, t: f32) -> bool {
        if anim::reduced() {
            return false;
        }
        match self.run {
            Run::Working => true,
            _ => age < RESULT_END || st.level.moving() || st.ripple_alive(t),
        }
    }

    /// The colour of the moment: accent while working, then the result's
    /// meaning, cross-faded so nothing flips.
    fn color(&self, ink: &Ink, age: f32) -> Color {
        let to = match self.run {
            Run::Working => return ink.accent,
            Run::Done => ink.good,
            Run::Partial => ink.warn,
            Run::Failed => ink.bad,
        };
        mix(ink.accent, to, phase(age, 0.0, 0.45, STANDARD))
    }
}

/// The hover areas, one function for `update` and `draw`.
pub fn spots() -> Hotspots<Part> {
    Hotspots::new().circle(Part::Shield, C, HOVER_R, Layer::Mid)
}

#[derive(Debug, Clone)]
pub struct State {
    live: Live<Part>,
    level: Spring,
    /// A ripple on the water: ambient second it began and its strength.
    ripple: Option<(f32, f32)>,
}

impl Default for State {
    fn default() -> Self {
        State {
            live: Live::default(),
            level: Spring::with(0.06, LEVEL_K, LEVEL_C),
            ripple: None,
        }
    }
}

impl State {
    fn ripple_at(&self, t: f32) -> f32 {
        match self.ripple {
            Some((t0, amp)) if t >= t0 => amp * (-(t - t0) * 2.5).exp(),
            _ => 0.0,
        }
    }
    fn ripple_alive(&self, t: f32) -> bool {
        self.ripple.is_some_and(|(t0, _)| t - t0 < RIPPLE_LIFE)
    }
}

// ---------------------------------------------------------------- geometry

/// The shield outline as a polygon in icon units, sampled once.
fn outline() -> &'static [Point] {
    static O: OnceLock<Vec<Point>> = OnceLock::new();
    O.get_or_init(|| Glyph::Shield.data().samples(180))
}

/// Where a line across the outline crosses it: min and max of the other
/// coordinate. `vertical` cuts at x = `at`, otherwise at y = `at`.
fn cut(at: f32, vertical: bool) -> Option<(f32, f32)> {
    let pts = outline();
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let (a0, a1, b0, b1) = if vertical {
            (a.x, a.y, b.x, b.y)
        } else {
            (a.y, a.x, b.y, b.x)
        };
        if (a0 - at) * (b0 - at) > 0.0 || a0 == b0 {
            continue;
        }
        let v = a1 + (at - a0) / (b0 - a0) * (b1 - a1);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    (hi > lo).then_some((lo, hi))
}

/// Top and bottom of the shield at `x` (icon units).
pub fn column(x: f32) -> Option<(f32, f32)> {
    cut(x, true)
}

/// Left and right of the shield at `y` (icon units).
pub fn span(y: f32) -> Option<(f32, f32)> {
    cut(y, false)
}

/// Columns across the shield used for the water.
const COLS: usize = 44;

fn col_x(i: usize) -> f32 {
    let (a, b) = (X0 + 0.02, X1 - 0.02);
    a + (b - a) * i as f32 / COLS as f32
}

/// The water under `wave` as a closed polygon in icon units, inside the
/// shield. Empty when no water shows.
pub fn water(wave: impl Fn(f32) -> f32) -> Vec<Point> {
    let mut upper = Vec::with_capacity(COLS + 1);
    let mut lower = Vec::with_capacity(COLS + 1);
    for i in 0..=COLS {
        let x = col_x(i);
        if let Some((top, bottom)) = column(x) {
            let w = wave(x).max(top);
            if w < bottom {
                upper.push(pt(x, w));
                lower.push(pt(x, bottom));
            }
        }
    }
    if upper.len() < 2 {
        return Vec::new();
    }
    upper.extend(lower.into_iter().rev());
    upper
}

/// The surface line, split where it leaves the shield.
fn surface(wave: impl Fn(f32) -> f32) -> Vec<Vec<Point>> {
    let mut runs = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    for i in 0..=COLS {
        let x = col_x(i);
        let w = wave(x);
        match column(x) {
            Some((top, bottom)) if w >= top && w <= bottom => cur.push(pt(x, w)),
            _ => {
                if cur.len() > 1 {
                    runs.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() > 1 {
        runs.push(cur);
    }
    runs
}

/// The cheap 3D turn towards the pointer, plus a sideways shake: maps icon
/// units to drawing units. Affine, so curves stay exact.
#[derive(Debug, Clone, Copy)]
struct Turn {
    sx: f32,
    sy: f32,
    skew: f32,
    dx: f32,
}

impl Turn {
    fn new(tx: f32, ty: f32, dx: f32) -> Turn {
        Turn {
            sx: 1.0 - tx.abs() * 0.16,
            sy: 1.0 - ty.abs() * 0.06,
            skew: (tx * 6.0).to_radians().tan(),
            dx,
        }
    }
    fn at(&self, q: Point) -> Point {
        let rx = S * (q.x - 12.0);
        let ry = S * (q.y - 12.0) + self.skew * rx;
        pt(C.x + self.dx + rx * self.sx, C.y + ry * self.sy)
    }
    fn shape(&self, d: &PathData) -> PathData {
        d.map(|q| self.at(q))
    }
}

/// The sideways shake when it fails (drawing units).
fn shake(run: Run, age: f32) -> f32 {
    if run != Run::Failed || age <= 0.9 {
        return 0.0;
    }
    (age * 34.0).sin() * 3.0 * (-(age - 0.9) * 4.0).exp()
}

// ----------------------------------------------------------------- program

impl<M> canvas::Program<M> for ShieldFill {
    type State = State;

    fn update(
        &self,
        st: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        let first = st.live.seen_change.is_none();
        if st.live.fresh(self.changed) {
            match self.run {
                Run::Working => st.level = Spring::with(0.06, LEVEL_K, LEVEL_C),
                // Shown straight away as a result: rise from where the work
                // would have left it.
                _ if first => st.level = Spring::with(0.88, LEVEL_K, LEVEL_C),
                _ => {}
            }
        }
        let stage = Stage::fit(UNITS, bounds.size());
        let step = st.live.update(event, bounds, cursor, &stage, &spots());
        let age = st.live.age(self.changed, self.now);
        let t = st.live.ambient(self.now, STILL);
        let target = level_target(self.run, self.progress, age);
        // Each fix that lands makes a small splash.
        if self.run == Run::Working && target > st.level.target + 0.01 && !anim::reduced() {
            st.ripple = Some((t, 0.6));
        }
        st.level.aim(target);
        if let Some(dt) = step.dt {
            st.level.tick(dt);
        }
        if let Some(at) = step.click() {
            if !anim::reduced() {
                st.ripple = Some((t, 1.0));
            }
            st.live.pulses.push(at, false);
        }
        st.live.redraw(&step, self.busy(st, age, t))
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
        let ink = Ink::new(&self.p, self.plate);
        let age = st.live.age(self.changed, self.now);
        let t = st.live.ambient(self.now, STILL);
        let back = st.live.layer(&stage, Layer::Back);
        let mid = st.live.layer(&stage, Layer::Mid);
        let color = self.color(&ink, age);
        let run = self.run;
        let level = if anim::reduced() {
            level_target(run, self.progress, age)
        } else {
            st.level.value
        };

        // Rays when it is done, behind everything.
        let re = phase(age, 1.2, 1.9, DECELERATE);
        if run == Run::Done && re > 0.0 && re < 1.0 {
            let ray = stroke(color.scale_alpha(1.0 - re), W_LINE);
            for k in 0..10 {
                let a = k as f32 / 10.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let r0 = 72.0 + 16.0 * re;
                let r1 = r0 + 10.0 * (1.0 - re);
                let (c, s) = (a.cos(), a.sin() * 0.95);
                f.stroke(
                    &back.line(pt(C.x + r0 * c, C.y + r0 * s), pt(C.x + r1 * c, C.y + r1 * s)),
                    ray,
                );
            }
        }

        let tilt = &st.live.tilt;
        let turn = Turn::new(tilt.x.value, tilt.y.value, shake(run, age));
        let shield = Glyph::Shield.data();
        f.fill(&mid.shape(&turn.shape(shield)), ink.plate);

        // The water: a wavy top (two sines, plus any ripple) over a tinted
        // pool with lines through it.
        let top = 21.2 - level * 19.0;
        let rp = st.ripple_at(t);
        let calm = match run {
            Run::Working => 0.45,
            _ => lerp(0.45, 0.0, phase(age, 0.0, 2.0, EASE_IN_OUT)),
        };
        let amp = calm + 1.6 * rp;
        let since = st.ripple.map_or(0.0, |(t0, _)| t - t0);
        let wave = |x: f32| {
            top + amp * (x * 0.7 + t * 3.0).sin()
                + amp * 0.6 * (x * 1.3 - t * 2.1).sin()
                + if rp > 0.01 {
                    1.2 * rp * (x * 1.6 - since * 12.0).sin()
                } else {
                    0.0
                }
        };
        if level > 0.02 {
            let pool: Vec<Point> = water(wave).into_iter().map(|q| turn.at(q)).collect();
            if pool.len() > 2 {
                f.fill(&mid.polyline(&pool, true), ink.tint(color));
            }
            let line = stroke(color.scale_alpha(0.35), W_LINE);
            for k in 0..LINES {
                let y = 21.0 - k as f32 * 1.25;
                if y <= top + 0.8 {
                    break;
                }
                if let Some((l, r)) = span(y) {
                    f.stroke(&mid.line(turn.at(pt(l, y)), turn.at(pt(r, y))), line);
                }
            }
            if level < 1.02 {
                let s = stroke(color, W_ACCENT);
                for run_pts in surface(wave) {
                    let pts: Vec<Point> = run_pts.into_iter().map(|q| turn.at(q)).collect();
                    f.stroke(&mid.polyline(&pts, false), s);
                }
            }
            if run == Run::Working {
                let s = stroke(color, W_LINE);
                for k in 0..7 {
                    let ph = k as f32 / 7.0;
                    let p = (t * 0.4 + ph).rem_euclid(1.0);
                    let y = lerp(21.5, top + 0.6, p);
                    let x = 6.0 + ((k * 37) % 12) as f32 + (p * 9.0 + ph * 7.0).sin() * 0.6;
                    let inside = column(x).is_some_and(|(a, b)| y > a + 0.5 && y < b - 0.5);
                    if y > top && inside {
                        f.stroke(
                            &mid.circle(turn.at(pt(x, y)), 0.45 * S),
                            s.with_color(color.scale_alpha(1.0 - p * 0.6)),
                        );
                    }
                }
            }
        }
        f.stroke(&mid.shape(&turn.shape(shield)), ink.ln());

        // The result marks, drawn in.
        let drawn = |f: &mut Frame, d: &PathData, frac: f32, width: f32| {
            if frac > 0.001 {
                f.stroke(&mid.shape(&turn.shape(&d.partial(frac))), stroke(color, width));
            }
        };
        let outline_in = match run {
            Run::Working => 0.0,
            Run::Done => phase(age, 0.5, 1.1, STANDARD),
            Run::Partial => phase(age, 0.6, 1.0, STANDARD) * 0.55,
            Run::Failed => phase(age, 0.4, 0.9, STANDARD),
        };
        drawn(&mut f, shield, outline_in, W_ACCENT);
        match run {
            Run::Done => drawn(
                &mut f,
                Glyph::Tick.data(),
                phase(age, 1.0, 1.35, DECELERATE),
                W_THICK,
            ),
            Run::Partial => {
                let gap = phase(age, 0.8, 1.0, STANDARD);
                if gap > 0.0 {
                    let knock = ink.knock(stage.len(9.0)).with_color(ink.plate.scale_alpha(gap));
                    f.stroke(&mid.shape(&turn.shape(&excl_gap())), knock);
                }
                drawn(
                    &mut f,
                    Glyph::Excl.data(),
                    phase(age, 0.9, 1.2, DECELERATE),
                    W_THICK,
                );
            }
            Run::Failed => drawn(
                &mut f,
                crack(),
                phase(age, 0.9, 1.4, DECELERATE),
                W_THICK,
            ),
            Run::Working => {}
        }

        st.live.pulses.draw(&mut f, &stage, color);
        st.live
            .draw_tooltip(&mut f, &self.p, &stage, &spots(), |_| self.label.clone());
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

fn crack() -> &'static PathData {
    static P: OnceLock<PathData> = OnceLock::new();
    P.get_or_init(|| PathData::of(CRACK))
}

fn excl_gap() -> &'static PathData {
    static P: OnceLock<PathData> = OnceLock::new();
    P.get_or_init(|| PathData::of(EXCL_GAP))
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

    fn prog(run: Run, progress: Option<f32>, at: Instant) -> ShieldFill {
        ShieldFill {
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

    #[test]
    fn constants_parse() {
        assert!(!crack().segs.is_empty());
        assert!(!excl_gap().segs.is_empty());
    }

    #[test]
    fn water_follows_real_progress() {
        // Rises with each fix made and never reaches the top before the end.
        let w = |f| level_target(Run::Working, Some(f), 0.0);
        assert!((w(0.0) - 0.12).abs() < 1e-6);
        assert!(w(0.5) > w(0.25) && w(1.0) > w(0.5));
        assert!(w(1.0) < 1.0 && w(7.0) == w(1.0) && w(-1.0) == w(0.0));
        // Unknown progress creeps up by itself but stays below full.
        let slow = |age| level_target(Run::Working, None, age);
        assert!(slow(10.0) > slow(1.0) && slow(1000.0) < 0.75);
        // Results: full, part way, empty.
        assert!(level_target(Run::Done, None, 0.0) > 1.0);
        assert_eq!(level_target(Run::Failed, Some(1.0), 0.0), 0.0);
        let part = level_target(Run::Partial, Some(0.5), 0.0);
        assert!(part > 0.3 && part < 0.8);
        assert_eq!(level_target(Run::Partial, Some(0.0), 0.0), 0.3);
        assert_eq!(level_target(Run::Partial, Some(1.0), 0.0), 0.8);
        assert_eq!(level_target(Run::Partial, None, 0.0), 0.55);
    }

    #[test]
    fn the_outline_bounds_the_water() {
        // The glyph is symmetric about x = 12, from 3.7 to 20.3 across.
        let (l, r) = span(10.0).unwrap();
        assert!((l - 3.7).abs() < 0.05 && (r - 20.3).abs() < 0.05);
        let (l, r) = span(18.0).unwrap();
        assert!(((12.0 - l) - (r - 12.0)).abs() < 0.05 && r - l < 12.0);
        assert!(span(1.0).is_none() && span(22.0).is_none());
        let (top, bottom) = column(12.0).unwrap();
        // Sampled, so the two points of the outline are cut a little.
        assert!((top - 2.6).abs() < 0.15 && (bottom - 21.2).abs() < 0.15, "{top} {bottom}");
        // Water at a flat level stays inside the shield, between the level
        // and the bottom edge.
        let pool = water(|_| 14.0);
        assert!(pool.len() > 10);
        for q in &pool {
            let (a, b) = column(q.x.clamp(X0 + 0.02, X1 - 0.02)).unwrap();
            assert!(q.y >= 14.0 - 1e-4 && q.y >= a - 1e-3 && q.y <= b + 1e-3, "{q:?}");
        }
        // Above the top it fills the whole shield; below the bottom, nothing.
        let full = water(|_| 0.0);
        assert!(full.iter().any(|q| q.y < 3.0));
        assert!(water(|_| 22.0).is_empty());
        // The surface is cut where it leaves the shield.
        assert!(surface(|_| 0.0).is_empty());
        assert_eq!(surface(|_| 14.0).len(), 1);
    }

    #[test]
    fn hover_names_the_shield_only() {
        let tilt = Parallax::off();
        let s = spots();
        assert_eq!(s.hit(C, &tilt), Some(Part::Shield));
        assert_eq!(s.hit(pt(C.x + 50.0, C.y + 20.0), &tilt), Some(Part::Shield));
        assert_eq!(s.hit(pt(4.0, 4.0), &tilt), None);
        // The whole drawing fits its box, rays included.
        assert!(C.x - 88.0 >= 0.0 && C.x + 88.0 <= UNITS.width);
        assert!(C.y - 84.0 >= 0.0 && C.y + 84.0 <= UNITS.height);
    }

    #[test]
    fn labels_go_through_translation() {
        for run in [Run::Working, Run::Done, Run::Partial, Run::Failed] {
            let key = label_key(run);
            for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                assert_ne!(lang.t(key), key, "{key} in {lang:?}");
            }
        }
    }

    #[test]
    fn fills_while_working_then_settles_and_goes_quiet() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        let mut st = State::default();
        let mut clock = t0;
        let tick = |st: &mut State, p: &ShieldFill, clock: &mut Instant| {
            *clock += Duration::from_millis(16);
            Program::update(p, st, &frame(*clock), BOUNDS, off)
        };
        let working = prog(Run::Working, Some(0.5), t0);
        for _ in 0..200 {
            assert!(wants_frame(tick(&mut st, &working, &mut clock)));
        }
        assert!((st.level.value - 0.5).abs() < 0.02, "{}", st.level.value);
        // A fix lands: the water rises and splashes.
        st.ripple = None;
        let more = prog(Run::Working, Some(1.0), t0);
        tick(&mut st, &more, &mut clock);
        assert!(st.ripple.is_some() && st.level.target > 0.85);
        // Done: frames until everything has drawn in and calmed, then none.
        let done = ShieldFill {
            run: Run::Done,
            changed: clock,
            now: clock,
            ..more
        };
        let mut frames = 0;
        while wants_frame(tick(&mut st, &done, &mut clock)) {
            frames += 1;
            assert!(frames < 600, "never settled");
        }
        assert!(frames as f32 * 0.016 >= RESULT_END - 0.05);
        assert_eq!(st.level.value, 1.04);
        anim::set_reduced_override(None);
    }

    #[test]
    fn reduced_motion_shows_the_end_at_once() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(true));
        let t0 = Instant::now();
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        let working = prog(Run::Working, Some(0.5), t0);
        assert!(!wants_frame(Program::update(
            &working,
            &mut st,
            &frame(t0),
            BOUNDS,
            off
        )));
        assert_eq!(st.level.value, level_target(Run::Working, Some(0.5), 0.0));
        let failed = prog(Run::Failed, None, t0 + Duration::from_secs(1));
        assert!(!wants_frame(Program::update(
            &failed,
            &mut st,
            &frame(t0 + Duration::from_secs(1)),
            BOUNDS,
            off
        )));
        assert_eq!(st.level.value, 0.0);
        assert!(st.ripple.is_none());
        anim::set_reduced_override(None);
    }
}
