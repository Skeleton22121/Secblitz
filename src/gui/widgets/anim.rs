//! Motion tokens and small animated icons drawn on a canvas.

use iced::widget::canvas::{
    self, path::Arc, Cache, Frame, Geometry, LineCap, LineJoin, Path, Stroke,
};
use iced::{mouse, Color, Element, Length, Point, Radians, Rectangle, Renderer, Theme};
use std::cell::Cell;
use std::sync::Mutex;
use std::time::{Duration, Instant};


#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

pub const fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Curve {
    Curve { x1, y1, x2, y2 }
}

pub const DECELERATE: Curve = cubic_bezier(0.0, 0.0, 0.0, 1.0);
pub const ACCELERATE: Curve = cubic_bezier(1.0, 0.0, 1.0, 1.0);
pub const POINT_TO_POINT: Curve = cubic_bezier(0.55, 0.55, 0.0, 1.0);
/// Material 3 emphasized decelerate: a stronger, more expressive entrance.
pub const EMPHASIZED: Curve = cubic_bezier(0.05, 0.7, 0.1, 1.0);
pub const STANDARD: Curve = cubic_bezier(0.2, 0.0, 0.0, 1.0);
pub const EASE_IN_OUT: Curve = cubic_bezier(0.42, 0.0, 0.58, 1.0);
#[cfg(test)]
pub const LINEAR: Curve = cubic_bezier(0.0, 0.0, 1.0, 1.0);

impl Curve {
    pub fn at(&self, t: f32) -> f32 {
        if t.is_nan() || t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        let (x1, y1, x2, y2) = (
            self.x1 as f64,
            self.y1 as f64,
            self.x2 as f64,
            self.y2 as f64,
        );
        let x = t as f64;
        let bez = |a: f64, b: f64, s: f64| {
            let u = 1.0 - s;
            3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
        };
        let dbez = |a: f64, b: f64, s: f64| {
            let u = 1.0 - s;
            3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
        };
        let mut s = x;
        for _ in 0..8 {
            let err = bez(x1, x2, s) - x;
            if err.abs() < 1e-7 {
                return bez(y1, y2, s).clamp(0.0, 1.0) as f32;
            }
            let d = dbez(x1, x2, s);
            if d.abs() < 1e-6 {
                break;
            }
            s = (s - err / d).clamp(0.0, 1.0);
        }
        // Bisection fallback: always converges because x(s) is monotonic.
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        s = x;
        for _ in 0..40 {
            let v = bez(x1, x2, s);
            if (v - x).abs() < 1e-7 {
                break;
            }
            if v < x {
                lo = s;
            } else {
                hi = s;
            }
            s = 0.5 * (lo + hi);
        }
        bez(y1, y2, s).clamp(0.0, 1.0) as f32
    }
}


pub const FASTER: Duration = Duration::from_millis(83);
pub const FAST: Duration = Duration::from_millis(150);
pub const NORMAL: Duration = Duration::from_millis(250);
pub const SLOW: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy)]
pub struct Clock {
    start: Instant,
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
    pub fn at(start: Instant) -> Self {
        Self { start }
    }
    pub fn restart(&mut self) {
        self.start = Instant::now();
    }
    pub fn start(&self) -> Instant {
        self.start
    }
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    pub fn elapsed_at(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.start)
    }
    pub fn progress_at(&self, d: Duration, now: Instant) -> f32 {
        ratio(self.elapsed_at(now), d)
    }
    pub fn done(&self, d: Duration, now: Instant) -> bool {
        reduced() || self.elapsed_at(now) >= d
    }
}

/// How far a [`SLOW`] animation that began at `at` has got by `now`.
pub fn slow_progress(at: Instant, now: Instant) -> f32 {
    Clock::at(at).progress_at(SLOW, now)
}

fn ratio(elapsed: Duration, d: Duration) -> f32 {
    if d.is_zero() {
        return 1.0;
    }
    (elapsed.as_secs_f32() / d.as_secs_f32()).clamp(0.0, 1.0)
}


/// Reads the system "animation effects" setting, re-asking at most every few seconds.
pub struct Motion {
    system: fn() -> bool,
    cache: Mutex<Option<(Instant, bool)>>,
}

impl Motion {
    const REFRESH: Duration = Duration::from_secs(3);

    pub const fn new(system: fn() -> bool) -> Self {
        Self {
            system,
            cache: Mutex::new(None),
        }
    }

    pub fn reduced_at(&self, now: Instant) -> bool {
        let Ok(mut cached) = self.cache.lock() else {
            return false;
        };
        if let Some((at, v)) = *cached {
            if now.duration_since(at) < Self::REFRESH {
                return v;
            }
        }
        let v = (self.system)();
        *cached = Some((now, v));
        v
    }
}

static MOTION: Motion = Motion::new(system_reduced);

pub fn reduced() -> bool {
    #[cfg(test)]
    if let Some(v) = forced::get() {
        return v;
    }
    MOTION.reduced_at(Instant::now())
}

pub fn animating() -> bool {
    !reduced()
}

#[cfg(windows)]
fn system_reduced() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
    };
    let mut on: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL (i32) to pvParam.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            &mut on as *mut i32 as *mut core::ffi::c_void,
            0,
        )
    };
    ok != 0 && on == 0
}

#[cfg(not(windows))]
fn system_reduced() -> bool {
    false
}

fn effective(t: f32, reduced: bool) -> f32 {
    if reduced {
        1.0
    } else {
        t.clamp(0.0, 1.0)
    }
}


pub fn ring_fill(from: f32, to: f32, t: f32) -> f32 {
    ring_fill_with(from, to, t, reduced())
}

fn ring_fill_with(from: f32, to: f32, t: f32, reduced: bool) -> f32 {
    let k = if reduced { 1.0 } else { DECELERATE.at(t) };
    from + (to - from) * k
}

pub fn count_up(from: f32, to: f32, t: f32) -> f32 {
    ring_fill(from, to, t)
}

pub fn count_up_int(from: i64, to: i64, t: f32) -> i64 {
    count_up(from as f32, to as f32, t).round() as i64
}

#[derive(Debug, Clone, Copy)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    clock: Clock,
    dur: Duration,
}

impl Tween {
    pub fn new(from: f32, to: f32, dur: Duration) -> Self {
        Self::starting(Instant::now(), from, to, dur)
    }
    pub fn starting(at: Instant, from: f32, to: f32, dur: Duration) -> Self {
        Self {
            from,
            to,
            clock: Clock::at(at),
            dur,
        }
    }
    pub fn retarget(&mut self, now: Instant, to: f32) {
        self.from = self.value(now);
        self.to = to;
        self.clock = Clock::at(now);
    }
    pub fn value(&self, now: Instant) -> f32 {
        ring_fill(self.from, self.to, self.clock.progress_at(self.dur, now))
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Spinner,
    Check,
    Cross,
    Warn,
}

#[derive(Debug, Clone, Copy)]
struct Glyph {
    kind: Kind,
    color: Color,
    t: f32,
    secs: f32,
    still: bool,
}

#[derive(Default)]
struct GlyphState {
    cache: Cache,
    key: Cell<u64>,
}

fn key_of(g: &Glyph, size: Size1) -> u64 {
    let c = g.color;
    let mut h: u64 = 0xcbf29ce484222325;
    for v in [
        g.kind as u64,
        size.0.to_bits() as u64,
        c.r.to_bits() as u64,
        c.g.to_bits() as u64,
        c.b.to_bits() as u64,
        c.a.to_bits() as u64,
    ] {
        h = (h ^ v).wrapping_mul(0x100000001b3);
    }
    h | 1
}

#[derive(Clone, Copy)]
struct Size1(f32);

impl<M> canvas::Program<M> for Glyph {
    type State = GlyphState;

    fn draw(
        &self,
        state: &GlyphState,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let size = bounds.size();
        let g = *self;
        if g.still {
            let key = key_of(&g, Size1(size.width));
            if state.key.get() != key {
                state.cache.clear();
                state.key.set(key);
            }
            return vec![state.cache.draw(renderer, size, |f| paint(f, &g))];
        }
        let mut frame = Frame::new(renderer, size);
        paint(&mut frame, &g);
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _: &GlyphState,
        _: Rectangle,
        _: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::None
    }
}

fn element<'a, M: 'a>(size: f32, g: Glyph) -> Element<'a, M> {
    canvas::Canvas::new(g)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

pub fn spinner<'a, M: 'a>(size: f32, color: Color, elapsed: Duration) -> Element<'a, M> {
    spinner_with(size, color, elapsed, reduced())
}

fn spinner_with<'a, M: 'a>(
    size: f32,
    color: Color,
    elapsed: Duration,
    reduced: bool,
) -> Element<'a, M> {
    element(
        size,
        Glyph {
            kind: Kind::Spinner,
            color,
            t: 1.0,
            secs: elapsed.as_secs_f32(),
            still: reduced,
        },
    )
}

pub fn check_draw<'a, M: 'a>(size: f32, color: Color, t: f32) -> Element<'a, M> {
    one_shot(Kind::Check, size, color, t)
}

pub fn cross_draw<'a, M: 'a>(size: f32, color: Color, t: f32) -> Element<'a, M> {
    one_shot(Kind::Cross, size, color, t)
}

pub fn warn_draw<'a, M: 'a>(size: f32, color: Color, t: f32) -> Element<'a, M> {
    one_shot(Kind::Warn, size, color, t)
}

fn one_shot<'a, M: 'a>(kind: Kind, size: f32, color: Color, t: f32) -> Element<'a, M> {
    let t = effective(t, reduced());
    element(
        size,
        Glyph {
            kind,
            color,
            t,
            secs: 0.0,
            still: t >= 1.0,
        },
    )
}


#[derive(Clone, Copy)]
struct Xf {
    cx: f32,
    cy: f32,
    k: f32,
}

impl Xf {
    fn new(size: iced::Size, scale: f32) -> Self {
        Self {
            cx: size.width / 2.0,
            cy: size.height / 2.0,
            k: size.width.min(size.height) / 24.0 * scale,
        }
    }
    fn p(&self, x: f32, y: f32) -> Point {
        Point::new(self.cx + (x - 12.0) * self.k, self.cy + (y - 12.0) * self.k)
    }
    fn len(&self, v: f32) -> f32 {
        v * self.k
    }
}

pub(crate) fn stroke(color: Color, w: f32) -> Stroke<'static> {
    Stroke::default()
        .with_width(w)
        .with_color(color)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
}

pub(crate) fn partial_line(pts: &[Point], frac: f32) -> Option<Path> {
    let total: f32 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    let mut left = total * frac.clamp(0.0, 1.0);
    if left <= 0.0 || pts.len() < 2 {
        return None;
    }
    Some(Path::new(|b| {
        b.move_to(pts[0]);
        for w in pts.windows(2) {
            let d = dist(w[0], w[1]);
            if left >= d {
                b.line_to(w[1]);
                left -= d;
            } else {
                let r = if d > 0.0 { left / d } else { 0.0 };
                b.line_to(Point::new(
                    w[0].x + (w[1].x - w[0].x) * r,
                    w[0].y + (w[1].y - w[0].y) * r,
                ));
                break;
            }
        }
    }))
}

fn dist(a: Point, b: Point) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

pub(crate) fn arc_path(c: Point, r: f32, start: f32, sweep: f32) -> Path {
    Path::new(|b| {
        b.arc(Arc {
            center: c,
            radius: r,
            start_angle: Radians(start),
            end_angle: Radians(start + sweep),
        })
    })
}

pub(crate) const TOP: f32 = -std::f32::consts::FRAC_PI_2;
pub(crate) const TAU: f32 = std::f32::consts::TAU;

pub(crate) fn phase(t: f32, a: f32, b: f32) -> f32 {
    ((t - a) / (b - a)).clamp(0.0, 1.0)
}

fn paint(f: &mut Frame, g: &Glyph) {
    match g.kind {
        Kind::Spinner => paint_spinner(f, g),
        Kind::Check | Kind::Cross => paint_badge(f, g),
        Kind::Warn => paint_warn(f, g),
    }
}

pub fn spinner_arc(secs: f32) -> (f32, f32) {
    const BREATH: f32 = 1.4;
    const TURN: f32 = 1.6;
    let p = (secs / BREATH).rem_euclid(1.0);
    let k = POINT_TO_POINT.at(triangle(p));
    let min = 30f32.to_radians();
    let max = 270f32.to_radians();
    let len = min + (max - min) * k;
    let rot = (secs / TURN).rem_euclid(1.0) * TAU;
    (TOP + rot - len / 2.0, len)
}

pub(crate) fn triangle(p: f32) -> f32 {
    let p = p.rem_euclid(1.0);
    if p < 0.5 {
        p * 2.0
    } else {
        2.0 - p * 2.0
    }
}

pub fn spinner_stroke(size: f32) -> f32 {
    (size * 0.085).clamp(1.75, 4.0)
}

fn paint_spinner(f: &mut Frame, g: &Glyph) {
    let size = f.size().width.min(f.size().height);
    let w = spinner_stroke(size);
    let r = (size - w) / 2.0 - 0.25;
    let c = Point::new(f.size().width / 2.0, f.size().height / 2.0);
    f.stroke(&Path::circle(c, r), stroke(g.color.scale_alpha(0.12), w));
    if g.still {
        f.stroke(&arc_path(c, r, TOP, TAU * 0.75), stroke(g.color, w));
        return;
    }
    let (start, len) = spinner_arc(g.secs);
    f.stroke(&arc_path(c, r, start, len), stroke(g.color, w));
}

fn paint_badge(f: &mut Frame, g: &Glyph) {
    let t = g.t;
    let bump = (std::f32::consts::PI * phase(t, 0.35, 1.0)).sin();
    let xf = Xf::new(f.size(), 1.0 + 0.07 * bump);
    let w = xf.len(1.7);
    let c = xf.p(12.0, 12.0);
    let ring = DECELERATE.at(phase(t, 0.0, 0.5));
    if ring > 0.0 {
        let p = if ring >= 1.0 {
            Path::circle(c, xf.len(9.75))
        } else {
            arc_path(c, xf.len(9.75), TOP, TAU * ring)
        };
        f.stroke(&p, stroke(g.color, w));
    }
    let mark = DECELERATE.at(phase(t, 0.4, 1.0));
    if g.kind == Kind::Check {
        let pts = [xf.p(7.4, 12.4), xf.p(10.6, 15.6), xf.p(16.8, 8.8)];
        if let Some(p) = partial_line(&pts, mark) {
            f.stroke(&p, stroke(g.color, w));
        }
    } else {
        let a = [xf.p(8.6, 8.6), xf.p(15.4, 15.4)];
        let b = [xf.p(15.4, 8.6), xf.p(8.6, 15.4)];
        if let Some(p) = partial_line(&a, phase(mark, 0.0, 0.6)) {
            f.stroke(&p, stroke(g.color, w));
        }
        if let Some(p) = partial_line(&b, phase(mark, 0.4, 1.0)) {
            f.stroke(&p, stroke(g.color, w));
        }
    }
}

fn paint_warn(f: &mut Frame, g: &Glyph) {
    let t = g.t;
    let bump = (std::f32::consts::PI * phase(t, 0.5, 1.0)).sin();
    let xf = Xf::new(f.size(), 1.0 + 0.06 * bump);
    let w = xf.len(1.7);
    let tri = [
        xf.p(12.0, 3.6),
        xf.p(21.0, 19.6),
        xf.p(3.0, 19.6),
        xf.p(12.0, 3.6),
    ];
    if let Some(p) = partial_line(&tri, DECELERATE.at(phase(t, 0.0, 0.55))) {
        f.stroke(&p, stroke(g.color, w));
    }
    let pop = EMPHASIZED.at(phase(t, 0.5, 1.0));
    if pop > 0.0 {
        let col = g.color.scale_alpha(pop);
        let bar = [xf.p(12.0, 9.4), xf.p(12.0, 13.6)];
        if let Some(p) = partial_line(&bar, pop) {
            f.stroke(&p, stroke(col, w));
        }
        f.fill(&Path::circle(xf.p(12.0, 16.8), xf.len(1.0) * pop), col);
    }
}

/// Per-thread reduced-motion override for tests, so they never share state.
#[cfg(test)]
pub(crate) mod forced {
    use std::cell::Cell;

    thread_local! {
        static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
    }

    pub fn get() -> Option<bool> {
        FORCED.with(Cell::get)
    }

    pub struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            FORCED.with(|f| f.set(None));
        }
    }

    pub fn set(reduced: bool) -> Guard {
        FORCED.with(|f| f.set(Some(reduced)));
        Guard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Curve; 7] = [
        DECELERATE,
        ACCELERATE,
        POINT_TO_POINT,
        EMPHASIZED,
        STANDARD,
        EASE_IN_OUT,
        LINEAR,
    ];

    #[test]
    fn endpoints_are_exact() {
        for c in ALL {
            assert_eq!(c.at(0.0), 0.0);
            assert_eq!(c.at(1.0), 1.0);
        }
    }

    #[test]
    fn clamps_out_of_range_and_nan() {
        for c in ALL {
            assert_eq!(c.at(-3.0), 0.0);
            assert_eq!(c.at(7.0), 1.0);
            assert_eq!(c.at(f32::NAN), 0.0);
        }
    }

    #[test]
    fn monotonic_and_in_range() {
        for c in ALL {
            let mut prev = 0.0;
            for i in 0..=2000 {
                let v = c.at(i as f32 / 2000.0);
                assert!((0.0..=1.0).contains(&v));
                assert!(v + 1e-5 >= prev, "{c:?} dropped at {i}: {v} < {prev}");
                prev = v;
            }
        }
    }

    #[test]
    fn known_midpoints() {
        assert!((LINEAR.at(0.5) - 0.5).abs() < 1e-5);
        assert!((EASE_IN_OUT.at(0.5) - 0.5).abs() < 1e-5);
        let s = 0.5f64.powf(1.0 / 3.0);
        let want = (3.0 * s * s - 2.0 * s * s * s) as f32;
        assert!(
            (DECELERATE.at(0.5) - want).abs() < 1e-4,
            "{}",
            DECELERATE.at(0.5)
        );
        // Decelerate is front-loaded, accelerate back-loaded.
        assert!(DECELERATE.at(0.25) > 0.5);
        assert!(ACCELERATE.at(0.75) < 0.5);
    }

    #[test]
    fn solver_matches_brute_force() {
        let c = cubic_bezier(0.25, 0.1, 0.25, 1.0);
        for i in 1..20 {
            let x = i as f64 / 20.0;
            let (mut lo, mut hi) = (0.0f64, 1.0f64);
            for _ in 0..60 {
                let s = 0.5 * (lo + hi);
                let u = 1.0 - s;
                let bx = 3.0 * u * u * s * 0.25 + 3.0 * u * s * s * 0.25 + s * s * s;
                if bx < x {
                    lo = s
                } else {
                    hi = s
                }
            }
            let s = 0.5 * (lo + hi);
            let u = 1.0 - s;
            let by = 3.0 * u * u * s * 0.1 + 3.0 * u * s * s * 1.0 + s * s * s;
            assert!((c.at(x as f32) as f64 - by).abs() < 1e-4);
        }
    }

    #[test]
    fn reduced_motion_short_circuits() {
        assert_eq!(effective(0.2, true), 1.0);
        assert_eq!(effective(0.2, false), 0.2);
        assert_eq!(effective(5.0, false), 1.0);
        assert_eq!(ring_fill_with(10.0, 90.0, 0.0, true), 90.0);
        assert_eq!(ring_fill_with(10.0, 90.0, 0.0, false), 10.0);
        assert!((ring_fill_with(10.0, 90.0, 1.0, false) - 90.0).abs() < 1e-4);
    }

    #[test]
    fn override_controls_reduced_and_clock() {
        let _m = forced::set(true);
        assert!(reduced() && !animating());
        let c = Clock::new();
        assert!(c.done(SLOW, Instant::now()));
        assert_eq!(count_up_int(0, 18, 0.0), 18);
        let _m = forced::set(false);
        assert!(!reduced() && animating());
        assert_eq!(count_up_int(0, 18, 0.0), 0);
        assert_eq!(count_up_int(0, 18, 1.0), 18);
    }

    #[test]
    fn motion_caches_the_system_answer_for_a_few_seconds() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static ANSWER: AtomicBool = AtomicBool::new(true);
        let motion = Motion::new(|| ANSWER.load(Ordering::SeqCst));
        let t0 = Instant::now();
        assert!(motion.reduced_at(t0));
        ANSWER.store(false, Ordering::SeqCst);
        assert!(motion.reduced_at(t0 + Duration::from_secs(2)));
        assert!(!motion.reduced_at(t0 + Duration::from_secs(4)));
    }

    #[test]
    fn clock_progress() {
        let start = Instant::now();
        let c = Clock::at(start);
        assert_eq!(c.progress_at(SLOW, start), 0.0);
        assert_eq!(c.progress_at(SLOW, start + SLOW * 3), 1.0);
        assert!((c.progress_at(SLOW, start + SLOW / 2) - 0.5).abs() < 1e-3);
        assert_eq!(c.progress_at(Duration::ZERO, start), 1.0);
    }

    #[test]
    fn spinner_arc_breathes_between_30_and_270() {
        let (mut lo, mut hi) = (f32::MAX, 0f32);
        for i in 0..2800 {
            let (_, len) = spinner_arc(i as f32 / 1000.0);
            lo = lo.min(len);
            hi = hi.max(len);
        }
        assert!((lo.to_degrees() - 30.0).abs() < 0.5, "{}", lo.to_degrees());
        assert!((hi.to_degrees() - 270.0).abs() < 0.5, "{}", hi.to_degrees());
    }

    #[test]
    fn spinner_centre_turns_at_constant_speed() {
        let mid = |s: f32| {
            let (a, l) = spinner_arc(s);
            a + l / 2.0
        };
        let d1 = mid(0.30) - mid(0.20);
        let d2 = mid(1.00) - mid(0.90);
        assert!((d1 - d2).abs() < 1e-3);
        assert!(d1 > 0.0);
    }

    #[test]
    fn spinner_stroke_scales_and_clamps() {
        assert_eq!(spinner_stroke(16.0), 1.75);
        assert!(spinner_stroke(32.0) > spinner_stroke(20.0));
        assert_eq!(spinner_stroke(400.0), 4.0);
    }

    #[test]
    fn tween_math() {
        let _m = forced::set(false);
        let t0 = Instant::now();
        let mut tw = Tween::starting(t0, 10.0, 20.0, SLOW);
        assert_eq!(tw.value(t0), 10.0);
        assert!((tw.value(t0 + SLOW) - 20.0).abs() < 1e-4);
        // Decelerate: more than half way at half time.
        assert!(tw.value(t0 + SLOW / 2) > 15.0);
        let mid = t0 + SLOW / 4;
        let shown = tw.value(mid);
        tw.retarget(mid, 0.0);
        assert!((tw.value(mid) - shown).abs() < 1e-4);
        assert_eq!(tw.to, 0.0);
        assert!((tw.value(mid + SLOW)).abs() < 1e-4);
    }

    #[test]
    fn partial_line_lengths() {
        let pts = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        assert!(partial_line(&pts, 0.0).is_none());
        assert!(partial_line(&pts, 0.5).is_some());
    }
}
