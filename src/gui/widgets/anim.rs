//! Motion tokens and small animated icons drawn on a canvas.
//!
//! Design notes live in docs/MOTION.md. Everything here is a pure function of
//! time: pages own the clock, this module only draws.
//!
//! # How a page drives an animation
//!
//! 1. Keep a [`Clock`] (or a plain `Instant`) in page state, set when the
//!    animation starts, e.g. `self.scan_clock = Some(Clock::new())`.
//! 2. In `subscription()` return `iced::window::frames()` ONLY while something
//!    is animating, and nothing otherwise:
//!
//!    ```ignore
//!    if self.scan_clock.is_some() && anim::animating() {
//!        iced::window::frames().map(Message::Frame)
//!    } else {
//!        Subscription::none()
//!    }
//!    ```
//!
//! 3. On `Message::Frame(now)` store `now` (`self.now = now`). One-shot
//!    animations (check/cross/warning draw-ins, value tweens) clear their
//!    clock once `Clock::done(duration, now)` is true, so the subscription
//!    switches off and the window goes back to zero redraws per second.
//! 4. In `view()` build widgets from the stored time, for example
//!    `anim::spinner(24.0, color, clock.elapsed_at(self.now))` or
//!    `anim::check_draw(32.0, color, clock.progress_at(anim::SLOW, self.now))`.
//!    Never call `Instant::now()` in `view()`; use the frame timestamp.
//!
//! When the Windows setting "Show animations in Windows" is off ([`reduced`])
//! every helper returns its final state at once and [`animating`] is false,
//! so no frame subscription is needed.
//!
//! Each icon is a tiny canvas (24 to 64 px). Static end states are cached with
//! `canvas::Cache`; only the small canvas area is redrawn while moving.

#![allow(dead_code)]

use iced::widget::canvas::{self, path::Arc, Cache, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::{mouse, Color, Element, Length, Point, Radians, Rectangle, Renderer, Theme};
use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Easing curves
// ---------------------------------------------------------------------------

/// A CSS-style cubic Bezier easing curve (P0 = 0,0 and P3 = 1,1 implied).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

/// Build a curve from the two control points, like CSS `cubic-bezier()`.
pub const fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Curve {
    Curve { x1, y1, x2, y2 }
}

/// Fluent "Fast out, slow in": things entering or settling. Feels instant.
pub const DECELERATE: Curve = cubic_bezier(0.0, 0.0, 0.0, 1.0);
/// Fluent "Slow out, fast in": things leaving.
pub const ACCELERATE: Curve = cubic_bezier(1.0, 0.0, 1.0, 1.0);
/// Fluent "point to point": an object moving between two resting places.
pub const POINT_TO_POINT: Curve = cubic_bezier(0.55, 0.55, 0.0, 1.0);
/// Material 3 emphasized decelerate: a stronger, more expressive entrance.
pub const EMPHASIZED: Curve = cubic_bezier(0.05, 0.7, 0.1, 1.0);
/// Material 3 standard: calm in-out for loops and sweeps.
pub const STANDARD: Curve = cubic_bezier(0.2, 0.0, 0.0, 1.0);
/// Symmetric ease for ping-pong loops such as the scan sweep.
pub const EASE_IN_OUT: Curve = cubic_bezier(0.42, 0.0, 0.58, 1.0);
pub const LINEAR: Curve = cubic_bezier(0.0, 0.0, 1.0, 1.0);

impl Curve {
    /// Eased value for linear progress `t`. `t` is clamped to 0..=1 and
    /// `at(0) == 0`, `at(1) == 1` exactly.
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
        // Newton-Raphson first (fast, converges in 2-4 steps for UI curves).
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

// ---------------------------------------------------------------------------
// Durations
// ---------------------------------------------------------------------------

/// Hover / press feedback (WinUI "faster").
pub const FASTER: Duration = Duration::from_millis(83);
/// Small state changes: toggles, chevrons, tooltips.
pub const FAST: Duration = Duration::from_millis(150);
/// Default for anything that moves a short distance.
pub const NORMAL: Duration = Duration::from_millis(250);
/// Completion moments: check draw-in, ring fill, count-up.
pub const SLOW: Duration = Duration::from_millis(400);

/// Wall clock for one animation. Cheap to copy; store it in page state.
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
        Self { start: Instant::now() }
    }
    pub fn at(start: Instant) -> Self {
        Self { start }
    }
    pub fn restart(&mut self) {
        self.start = Instant::now();
    }
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    /// Elapsed at a frame timestamp (preferred inside `view()`).
    pub fn elapsed_at(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.start)
    }
    /// Linear progress 0..=1 over `d`, now.
    pub fn progress(&self, d: Duration) -> f32 {
        ratio(self.elapsed(), d)
    }
    /// Linear progress 0..=1 over `d` at a frame timestamp.
    pub fn progress_at(&self, d: Duration, now: Instant) -> f32 {
        ratio(self.elapsed_at(now), d)
    }
    /// True once a one-shot animation has finished (or motion is reduced).
    pub fn done(&self, d: Duration, now: Instant) -> bool {
        reduced() || self.elapsed_at(now) >= d
    }
}

fn ratio(elapsed: Duration, d: Duration) -> f32 {
    if d.is_zero() {
        return 1.0;
    }
    (elapsed.as_secs_f32() / d.as_secs_f32()).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Reduced motion
// ---------------------------------------------------------------------------

// 0 = follow the system, 1 = force reduced, 2 = force full motion.
static OVERRIDE: AtomicU8 = AtomicU8::new(0);
static SYSTEM: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Force reduced motion on/off (`None` follows Windows). For an in-app
/// setting and for tests.
pub fn set_reduced_override(v: Option<bool>) {
    OVERRIDE.store(
        match v {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        },
        Ordering::Relaxed,
    );
}

/// True when the user turned off "Show animations in Windows" (or the
/// override says so). Re-read from the system at most every 3 seconds.
pub fn reduced() -> bool {
    match OVERRIDE.load(Ordering::Relaxed) {
        1 => return true,
        2 => return false,
        _ => {}
    }
    let now = Instant::now();
    if let Ok(mut g) = SYSTEM.lock() {
        if let Some((at, v)) = *g {
            if now.duration_since(at) < Duration::from_secs(3) {
                return v;
            }
        }
        let v = system_reduced();
        *g = Some((now, v));
        return v;
    }
    false
}

/// Whether pages should run the frame subscription at all.
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

/// Progress as the caller should use it: final state when motion is reduced.
fn effective(t: f32, reduced: bool) -> f32 {
    if reduced {
        1.0
    } else {
        t.clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Value helpers
// ---------------------------------------------------------------------------

/// Score / progress ring value animating from `from` to `to`, `t` = 0..1.
pub fn ring_fill(from: f32, to: f32, t: f32) -> f32 {
    ring_fill_with(from, to, t, reduced())
}

fn ring_fill_with(from: f32, to: f32, t: f32, reduced: bool) -> f32 {
    let k = if reduced { 1.0 } else { DECELERATE.at(t) };
    from + (to - from) * k
}

/// Number count-up with a decelerating finish, `t` = 0..1.
pub fn count_up(from: f32, to: f32, t: f32) -> f32 {
    ring_fill(from, to, t)
}

/// Integer count-up (rounded) for labels like "15 / 18".
pub fn count_up_int(from: i64, to: i64, t: f32) -> i64 {
    count_up(from as f32, to as f32, t).round() as i64
}

/// A value moving from `from` to `to` over [`SLOW`] (or a custom duration).
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    clock: Clock,
    dur: Duration,
}

impl Tween {
    pub fn new(from: f32, to: f32, dur: Duration) -> Self {
        Self { from, to, clock: Clock::new(), dur }
    }
    /// Retarget from the value currently shown, so changes never jump.
    pub fn retarget(&mut self, now: Instant, to: f32) {
        self.from = self.value(now);
        self.to = to;
        self.clock = Clock::at(now);
    }
    pub fn value(&self, now: Instant) -> f32 {
        ring_fill(self.from, self.to, self.clock.progress_at(self.dur, now))
    }
    pub fn done(&self, now: Instant) -> bool {
        self.clock.done(self.dur, now)
    }
}

// ---------------------------------------------------------------------------
// Canvas icons
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Spinner,
    Check,
    Cross,
    Warn,
    Shield,
    Pulse,
}

#[derive(Debug, Clone, Copy)]
struct Glyph {
    kind: Kind,
    color: Color,
    /// One-shot progress 0..=1 (draw-ins).
    t: f32,
    /// Seconds since start (loops).
    secs: f32,
    /// Final / frozen state: eligible for the geometry cache.
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

    fn mouse_interaction(&self, _: &GlyphState, _: Rectangle, _: mouse::Cursor) -> mouse::Interaction {
        mouse::Interaction::None
    }
}

fn element<'a, M: 'a>(size: f32, g: Glyph) -> Element<'a, M> {
    canvas::Canvas::new(g)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

/// Indeterminate progress ring (WinUI ProgressRing feel).
pub fn spinner<'a, M: 'a>(size: f32, color: Color, elapsed: Duration) -> Element<'a, M> {
    spinner_with(size, color, elapsed, reduced())
}

fn spinner_with<'a, M: 'a>(size: f32, color: Color, elapsed: Duration, reduced: bool) -> Element<'a, M> {
    element(
        size,
        Glyph { kind: Kind::Spinner, color, t: 1.0, secs: elapsed.as_secs_f32(), still: reduced },
    )
}

/// Circle strokes in, then the check draws, with a small overshoot settle.
pub fn check_draw<'a, M: 'a>(size: f32, color: Color, t: f32) -> Element<'a, M> {
    one_shot(Kind::Check, size, color, t)
}

/// Failure: circle then a cross.
pub fn cross_draw<'a, M: 'a>(size: f32, color: Color, t: f32) -> Element<'a, M> {
    one_shot(Kind::Cross, size, color, t)
}

/// Attention: triangle outlines, then the exclamation mark pops in.
pub fn warn_draw<'a, M: 'a>(size: f32, color: Color, t: f32) -> Element<'a, M> {
    one_shot(Kind::Warn, size, color, t)
}

fn one_shot<'a, M: 'a>(kind: Kind, size: f32, color: Color, t: f32) -> Element<'a, M> {
    let t = effective(t, reduced());
    element(size, Glyph { kind, color, t, secs: 0.0, still: t >= 1.0 })
}

/// Shield outline with a thin scan line sweeping over it.
pub fn shield_scan<'a, M: 'a>(size: f32, color: Color, elapsed: Duration) -> Element<'a, M> {
    element(
        size,
        Glyph { kind: Kind::Shield, color, t: 1.0, secs: elapsed.as_secs_f32(), still: reduced() },
    )
}

/// Small status dot with a slow, subtle halo.
pub fn pulse_dot<'a, M: 'a>(size: f32, color: Color, elapsed: Duration) -> Element<'a, M> {
    element(
        size,
        Glyph { kind: Kind::Pulse, color, t: 1.0, secs: elapsed.as_secs_f32(), still: reduced() },
    )
}

// --- geometry --------------------------------------------------------------

/// Maps the 24x24 Fluent grid onto the canvas (optionally scaled around the centre).
#[derive(Clone, Copy)]
struct Xf {
    cx: f32,
    cy: f32,
    k: f32,
}

impl Xf {
    fn new(size: iced::Size, scale: f32) -> Self {
        Self { cx: size.width / 2.0, cy: size.height / 2.0, k: size.width.min(size.height) / 24.0 * scale }
    }
    fn p(&self, x: f32, y: f32) -> Point {
        Point::new(self.cx + (x - 12.0) * self.k, self.cy + (y - 12.0) * self.k)
    }
    fn len(&self, v: f32) -> f32 {
        v * self.k
    }
}

fn stroke(color: Color, w: f32) -> Stroke<'static> {
    Stroke::default()
        .with_width(w)
        .with_color(color)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
}

/// Polyline drawn up to `frac` (0..=1) of its total length.
fn partial_line(pts: &[Point], frac: f32) -> Option<Path> {
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
                b.line_to(Point::new(w[0].x + (w[1].x - w[0].x) * r, w[0].y + (w[1].y - w[0].y) * r));
                break;
            }
        }
    }))
}

fn dist(a: Point, b: Point) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

fn arc_path(c: Point, r: f32, start: f32, sweep: f32) -> Path {
    Path::new(|b| {
        b.arc(Arc { center: c, radius: r, start_angle: Radians(start), end_angle: Radians(start + sweep) })
    })
}

const TOP: f32 = -std::f32::consts::FRAC_PI_2;
const TAU: f32 = std::f32::consts::TAU;

/// Remap `t` so the phase [a, b] runs 0..1 (clamped).
fn phase(t: f32, a: f32, b: f32) -> f32 {
    ((t - a) / (b - a)).clamp(0.0, 1.0)
}

fn paint(f: &mut Frame, g: &Glyph) {
    match g.kind {
        Kind::Spinner => paint_spinner(f, g),
        Kind::Check | Kind::Cross => paint_badge(f, g),
        Kind::Warn => paint_warn(f, g),
        Kind::Shield => paint_shield(f, g),
        Kind::Pulse => paint_pulse(f, g),
    }
}

fn paint_spinner(f: &mut Frame, g: &Glyph) {
    let xf = Xf::new(f.size(), 1.0);
    let w = xf.len(2.4);
    let r = xf.len(10.0);
    let c = xf.p(12.0, 12.0);
    // Faint track so the ring reads as a ring even on the short arc.
    f.stroke(&Path::circle(c, r), stroke(g.color.scale_alpha(0.16), w));
    if g.still {
        // Reduced motion: a fixed three-quarter arc.
        f.stroke(&arc_path(c, r, TOP, TAU * 0.75), stroke(g.color, w));
        return;
    }
    // Head races ahead, tail catches up (point-to-point), the whole ring turns.
    const CYCLE: f32 = 1.5;
    const SPIN: f32 = 2.2;
    let p = (g.secs / CYCLE).fract();
    let head = POINT_TO_POINT.at(phase(p, 0.0, 0.72));
    let tail = POINT_TO_POINT.at(phase(p, 0.28, 1.0));
    let len = (head - tail).max(0.03) * TAU;
    let rot = (g.secs / SPIN).fract() * TAU;
    f.stroke(&arc_path(c, r, TOP + rot + tail * TAU, len), stroke(g.color, w));
}

fn paint_badge(f: &mut Frame, g: &Glyph) {
    let t = g.t;
    // Overshoot settle on the whole badge: 1 -> 1.07 -> 1.
    let bump = (std::f32::consts::PI * phase(t, 0.35, 1.0)).sin();
    let xf = Xf::new(f.size(), 1.0 + 0.07 * bump);
    let w = xf.len(1.7);
    let c = xf.p(12.0, 12.0);
    let ring = DECELERATE.at(phase(t, 0.0, 0.5));
    if ring > 0.0 {
        let p = if ring >= 1.0 { Path::circle(c, xf.len(9.75)) } else { arc_path(c, xf.len(9.75), TOP, TAU * ring) };
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

fn shield_path(xf: &Xf) -> Path {
    Path::new(|b| {
        b.move_to(xf.p(12.0, 2.6));
        b.bezier_curve_to(xf.p(14.5, 4.4), xf.p(17.2, 5.4), xf.p(20.3, 5.6));
        b.line_to(xf.p(20.3, 11.0));
        b.bezier_curve_to(xf.p(20.3, 15.7), xf.p(17.4, 19.0), xf.p(12.0, 21.2));
        b.bezier_curve_to(xf.p(6.6, 19.0), xf.p(3.7, 15.7), xf.p(3.7, 11.0));
        b.line_to(xf.p(3.7, 5.6));
        b.bezier_curve_to(xf.p(6.8, 5.4), xf.p(9.5, 4.4), xf.p(12.0, 2.6));
        b.close();
    })
}

fn paint_shield(f: &mut Frame, g: &Glyph) {
    let xf = Xf::new(f.size(), 1.0);
    let shield = shield_path(&xf);
    f.stroke(&shield, stroke(g.color, xf.len(1.6)));
    if g.still {
        return;
    }
    // Ping-pong sweep, 2.4 s per full trip, eased so it lingers at the ends.
    const PERIOD: f32 = 2.4;
    let p = (g.secs / PERIOD).fract() * 2.0;
    let leg = if p < 1.0 { p } else { 2.0 - p };
    let y = 2.6 + (21.2 - 2.6) * EASE_IN_OUT.at(leg);
    let size = f.size();
    let full = |y0: f32, y1: f32| {
        let top = xf.p(0.0, y0.max(0.0)).y.max(0.0);
        let bot = xf.p(0.0, y1).y.min(size.height);
        Rectangle::new(Point::new(0.0, top), iced::Size::new(size.width, (bot - top).max(0.0)))
    };
    // Faint trailing band and the bright line, both clipped to the shield by
    // filling the shield shape through a thin clip rectangle.
    let trail = if p < 1.0 { y - 3.2 } else { y };
    let trail_end = if p < 1.0 { y } else { y + 3.2 };
    let band = full(trail, trail_end);
    if band.height > 0.5 {
        f.with_clip(band, |c| c.fill(&shield, g.color.scale_alpha(0.14)));
    }
    let line = full(y - 0.45, y + 0.45);
    if line.height > 0.0 {
        f.with_clip(line, |c| c.fill(&shield, g.color.scale_alpha(0.9)));
    }
}

fn paint_pulse(f: &mut Frame, g: &Glyph) {
    let xf = Xf::new(f.size(), 1.0);
    let c = xf.p(12.0, 12.0);
    f.fill(&Path::circle(c, xf.len(5.0)), g.color);
    if g.still {
        return;
    }
    // Halo expands and fades once every 2.4 s, then rests.
    const PERIOD: f32 = 2.4;
    let p = phase((g.secs / PERIOD).fract(), 0.0, 0.7);
    if p > 0.0 && p < 1.0 {
        let e = DECELERATE.at(p);
        f.fill(
            &Path::circle(c, xf.len(5.0 + 6.5 * e)),
            g.color.scale_alpha(0.28 * (1.0 - e)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Curve; 7] = [DECELERATE, ACCELERATE, POINT_TO_POINT, EMPHASIZED, STANDARD, EASE_IN_OUT, LINEAR];

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
        // Linear and symmetric curves hit 0.5 at 0.5.
        assert!((LINEAR.at(0.5) - 0.5).abs() < 1e-5);
        assert!((EASE_IN_OUT.at(0.5) - 0.5).abs() < 1e-5);
        // (0,0,0,1): x = s^3, y = 3s^2 - 2s^3 -> closed form.
        let s = 0.5f64.powf(1.0 / 3.0);
        let want = (3.0 * s * s - 2.0 * s * s * s) as f32;
        assert!((DECELERATE.at(0.5) - want).abs() < 1e-4, "{}", DECELERATE.at(0.5));
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
                if bx < x { lo = s } else { hi = s }
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
        set_reduced_override(Some(true));
        assert!(reduced() && !animating());
        let c = Clock::new();
        assert!(c.done(SLOW, Instant::now()));
        assert_eq!(count_up_int(0, 18, 0.0), 18);
        set_reduced_override(Some(false));
        assert!(!reduced() && animating());
        assert_eq!(count_up_int(0, 18, 0.0), 0);
        assert_eq!(count_up_int(0, 18, 1.0), 18);
        set_reduced_override(None);
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
    fn partial_line_lengths() {
        let pts = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        assert!(partial_line(&pts, 0.0).is_none());
        assert!(partial_line(&pts, 0.5).is_some());
    }
}
