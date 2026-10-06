//! SVG path data, parsed once into absolute segments.
//!
//! [`PathData::parse`] reads the full SVG path grammar (`M m L l H h V v C c
//! S s Q q T t A a Z z`, implicit repeated commands, numbers such as `-.5`,
//! `1e-3` and `.5.5`, packed arc flags such as `a1 1 0 011 1`). Elliptical
//! arcs become cubic Beziers (endpoint to centre parametrization, then one
//! cubic per quarter turn), so a path never breaks into extra sub-paths and
//! fills stay whole.
//!
//! The parsed data lives in the drawing's own units. Move it with
//! [`PathData::placed`] (an icon from its 24-unit box) or [`PathData::map`],
//! turn it into an iced [`Path`] with [`PathData::to_path`] (usually through
//! [`super::Stage::shape`]), cut it for a line that draws itself in with
//! [`PathData::partial`], or sample points along it with
//! [`PathData::samples`].
use iced::widget::canvas::Path;
use iced::{Point, Rectangle, Vector};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// One absolute segment of a path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    /// Start a new sub-path here.
    Move(Point),
    /// Straight line to the point.
    Line(Point),
    /// Quadratic Bezier: control, end.
    Quad(Point, Point),
    /// Cubic Bezier: first control, second control, end.
    Cubic(Point, Point, Point),
    /// Close the sub-path with a straight line back to its start.
    Close,
}

/// A parsed path: absolute segments in the drawing's units.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathData {
    pub segs: Vec<Seg>,
}

/// Why a path string did not parse. For developers only; never shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    /// Byte offset where reading stopped.
    pub at: usize,
    pub what: &'static str,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "path data: {} at byte {}", self.what, self.at)
    }
}

impl std::error::Error for ParseError {}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn skip_space(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn skip_sep(&mut self) {
        self.skip_space();
        if self.i < self.s.len() && self.s[self.i] == b',' {
            self.i += 1;
            self.skip_space();
        }
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn at_number(&mut self) -> bool {
        self.skip_sep();
        matches!(self.peek(), Some(b'0'..=b'9' | b'.' | b'-' | b'+'))
    }
    fn err(&self, what: &'static str) -> ParseError {
        ParseError { at: self.i, what }
    }
    fn number(&mut self) -> Result<f64, ParseError> {
        self.skip_sep();
        let start = self.i;
        if matches!(self.peek(), Some(b'-' | b'+')) {
            self.i += 1;
        }
        let mut digits = 0;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
            digits += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            self.i = start;
            return Err(self.err("expected a number"));
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mark = self.i;
            self.i += 1;
            if matches!(self.peek(), Some(b'-' | b'+')) {
                self.i += 1;
            }
            let mut exp = 0;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
                exp += 1;
            }
            if exp == 0 {
                self.i = mark;
            }
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()
            .and_then(|t| t.parse::<f64>().ok())
            .ok_or(ParseError {
                at: start,
                what: "bad number",
            })
    }
    /// An arc flag: a single `0` or `1`, which may touch the next number.
    fn flag(&mut self) -> Result<bool, ParseError> {
        self.skip_sep();
        match self.peek() {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(self.err("expected an arc flag")),
        }
    }
}

#[derive(Clone, Copy)]
struct P64 {
    x: f64,
    y: f64,
}

impl P64 {
    fn pt(self) -> Point {
        Point::new(self.x as f32, self.y as f32)
    }
}

impl PathData {
    /// Parse SVG path data. Anything the grammar does not allow is an error,
    /// so a typo in a constant fails its unit test instead of drawing junk.
    pub fn parse(d: &str) -> Result<PathData, ParseError> {
        let mut r = Reader {
            s: d.as_bytes(),
            i: 0,
        };
        let mut segs = Vec::new();
        let mut cur = P64 { x: 0.0, y: 0.0 };
        let mut start = cur;
        // Reflection points for S and T.
        let mut last_cubic: Option<P64> = None;
        let mut last_quad: Option<P64> = None;
        let mut cmd: Option<u8> = None;
        loop {
            r.skip_sep();
            let Some(c) = r.peek() else { break };
            if c.is_ascii_alphabetic() {
                r.i += 1;
                cmd = Some(c);
            } else if cmd.is_none() {
                return Err(r.err("path must start with a command"));
            } else if matches!(cmd, Some(b'z' | b'Z')) {
                return Err(r.err("number after close"));
            }
            let c = cmd.unwrap_or(b'M');
            let rel = c.is_ascii_lowercase();
            let base = if rel { cur } else { P64 { x: 0.0, y: 0.0 } };
            let pair = |r: &mut Reader| -> Result<P64, ParseError> {
                let x = r.number()?;
                let y = r.number()?;
                Ok(P64 {
                    x: base.x + x,
                    y: base.y + y,
                })
            };
            let mut next_cubic = None;
            let mut next_quad = None;
            match c.to_ascii_uppercase() {
                b'M' => {
                    let p = pair(&mut r)?;
                    segs.push(Seg::Move(p.pt()));
                    cur = p;
                    start = p;
                    // Further pairs are implicit line-tos.
                    cmd = Some(if rel { b'l' } else { b'L' });
                }
                b'L' => {
                    let p = pair(&mut r)?;
                    segs.push(Seg::Line(p.pt()));
                    cur = p;
                }
                b'H' => {
                    let x = r.number()?;
                    cur = P64 {
                        x: if rel { cur.x + x } else { x },
                        y: cur.y,
                    };
                    segs.push(Seg::Line(cur.pt()));
                }
                b'V' => {
                    let y = r.number()?;
                    cur = P64 {
                        x: cur.x,
                        y: if rel { cur.y + y } else { y },
                    };
                    segs.push(Seg::Line(cur.pt()));
                }
                b'C' => {
                    let c1 = pair(&mut r)?;
                    let c2 = pair(&mut r)?;
                    let p = pair(&mut r)?;
                    segs.push(Seg::Cubic(c1.pt(), c2.pt(), p.pt()));
                    next_cubic = Some(c2);
                    cur = p;
                }
                b'S' => {
                    let c1 = reflect(last_cubic, cur);
                    let c2 = pair(&mut r)?;
                    let p = pair(&mut r)?;
                    segs.push(Seg::Cubic(c1.pt(), c2.pt(), p.pt()));
                    next_cubic = Some(c2);
                    cur = p;
                }
                b'Q' => {
                    let q = pair(&mut r)?;
                    let p = pair(&mut r)?;
                    segs.push(Seg::Quad(q.pt(), p.pt()));
                    next_quad = Some(q);
                    cur = p;
                }
                b'T' => {
                    let q = reflect(last_quad, cur);
                    let p = pair(&mut r)?;
                    segs.push(Seg::Quad(q.pt(), p.pt()));
                    next_quad = Some(q);
                    cur = p;
                }
                b'A' => {
                    let rx = r.number()?;
                    let ry = r.number()?;
                    let rot = r.number()?;
                    let large = r.flag()?;
                    let sweep = r.flag()?;
                    let p = pair(&mut r)?;
                    svg_arc(&mut segs, cur, p, rx, ry, rot, large, sweep);
                    cur = p;
                }
                b'Z' => {
                    segs.push(Seg::Close);
                    cur = start;
                }
                _ => return Err(r.err("unknown command")),
            }
            last_cubic = next_cubic;
            last_quad = next_quad;
            // A command letter must be followed by its numbers; after the
            // first set, more numbers repeat it.
            if !matches!(c, b'z' | b'Z') && r.at_number() {
                continue;
            }
        }
        Ok(PathData { segs })
    }

    /// Parse data the program itself defines (the icon set and drawing
    /// constants). Panics on a typo, which the unit tests catch.
    pub fn of(d: &str) -> PathData {
        match PathData::parse(d) {
            Ok(p) => p,
            Err(e) => panic!("{e}: {d}"),
        }
    }

    /// Every point moved through `f`. `f` must be affine (move, scale,
    /// rotate) for the curves to stay exact.
    pub fn map(&self, f: impl Fn(Point) -> Point) -> PathData {
        PathData {
            segs: self
                .segs
                .iter()
                .map(|s| match *s {
                    Seg::Move(p) => Seg::Move(f(p)),
                    Seg::Line(p) => Seg::Line(f(p)),
                    Seg::Quad(a, p) => Seg::Quad(f(a), f(p)),
                    Seg::Cubic(a, b, p) => Seg::Cubic(f(a), f(b), f(p)),
                    Seg::Close => Seg::Close,
                })
                .collect(),
        }
    }

    /// An icon drawn in a 24-unit box, placed centred on `centre` at `size`
    /// units square (the prototype's `icon(parent, name, x, y, size)`).
    pub fn placed(&self, centre: Point, size: f32) -> PathData {
        let k = size / 24.0;
        let o = Vector::new(centre.x - size / 2.0, centre.y - size / 2.0);
        self.map(|p| Point::new(o.x + p.x * k, o.y + p.y * k))
    }

    /// The iced path, every point first moved through `f` (for example
    /// units to pixels).
    pub fn to_path(&self, f: impl Fn(Point) -> Point) -> Path {
        Path::new(|b| {
            for s in &self.segs {
                match *s {
                    Seg::Move(p) => b.move_to(f(p)),
                    Seg::Line(p) => b.line_to(f(p)),
                    Seg::Quad(a, p) => b.quadratic_curve_to(f(a), f(p)),
                    Seg::Cubic(a, c, p) => b.bezier_curve_to(f(a), f(c), f(p)),
                    Seg::Close => b.close(),
                }
            }
        })
    }

    /// Box around every point, control points included. `None` when empty.
    pub fn bounds(&self) -> Option<Rectangle> {
        let mut pts = self.segs.iter().flat_map(|s| match *s {
            Seg::Move(p) | Seg::Line(p) => vec![p],
            Seg::Quad(a, p) => vec![a, p],
            Seg::Cubic(a, b, p) => vec![a, b, p],
            Seg::Close => vec![],
        });
        let first = pts.next()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for p in pts {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Some(Rectangle::new(
            Point::new(x0, y0),
            iced::Size::new(x1 - x0, y1 - y0),
        ))
    }

    /// The last point the pen reaches. `None` when empty.
    pub fn end(&self) -> Option<Point> {
        let mut start = None;
        let mut cur = None;
        for s in &self.segs {
            match *s {
                Seg::Move(p) => {
                    start = Some(p);
                    cur = Some(p);
                }
                Seg::Line(p) | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => cur = Some(p),
                Seg::Close => cur = start,
            }
        }
        cur
    }

    /// Each drawn piece with its start point and length. Moves are not
    /// pieces: the pen lifts between sub-paths.
    fn pieces(&self) -> Vec<(Point, Seg, f32)> {
        let mut out = Vec::with_capacity(self.segs.len());
        let mut cur = Point::ORIGIN;
        let mut start = cur;
        for s in &self.segs {
            match *s {
                Seg::Move(p) => {
                    cur = p;
                    start = p;
                }
                Seg::Close => {
                    out.push((cur, Seg::Line(start), dist(cur, start)));
                    out.push((start, Seg::Close, 0.0));
                    cur = start;
                }
                seg => {
                    out.push((cur, seg, seg_len(cur, seg)));
                    cur = seg_end(seg);
                }
            }
        }
        out
    }

    /// Total drawn length (sub-paths added, jumps between them not).
    pub fn length(&self) -> f32 {
        self.pieces().iter().map(|p| p.2).sum()
    }

    /// The first `frac` (0..=1) of the path by length, curves cut exactly
    /// where they should be: a line that draws itself in, like the
    /// prototype's `drawn()` with `stroke-dashoffset`. Sub-paths draw one
    /// after the other.
    pub fn partial(&self, frac: f32) -> PathData {
        if frac >= 1.0 {
            return self.clone();
        }
        let mut left = self.length() * frac.max(0.0);
        let mut segs = Vec::new();
        if left <= 0.0 {
            return PathData { segs };
        }
        let mut pen: Option<Point> = None;
        let mut start = Point::ORIGIN;
        let mut cur = Point::ORIGIN;
        for s in &self.segs {
            let (seg, len) = match *s {
                Seg::Move(p) => {
                    start = p;
                    cur = p;
                    continue;
                }
                Seg::Close => (Seg::Line(start), dist(cur, start)),
                seg => (seg, seg_len(cur, seg)),
            };
            if pen != Some(cur) {
                segs.push(Seg::Move(cur));
            }
            if left >= len {
                segs.push(if matches!(s, Seg::Close) { Seg::Close } else { seg });
                left -= len;
                cur = seg_end(seg);
                pen = Some(cur);
                if left <= 0.0 {
                    break;
                }
            } else {
                let t = t_at_length(cur, seg, left);
                segs.push(split(cur, seg, t));
                break;
            }
        }
        PathData { segs }
    }

    /// The point `frac` (0..=1) of the way along the path by length.
    pub fn point_at(&self, frac: f32) -> Option<Point> {
        let pieces = self.pieces();
        let total: f32 = pieces.iter().map(|p| p.2).sum();
        let mut left = total * frac.clamp(0.0, 1.0);
        let mut last = None;
        for (from, seg, len) in pieces {
            if matches!(seg, Seg::Close) {
                continue;
            }
            if left <= len && len > 0.0 {
                return Some(eval(from, seg, t_at_length(from, seg, left)));
            }
            left -= len;
            last = Some(seg_end(seg));
        }
        last.or_else(|| self.end())
    }

    /// `n` points spread evenly along the path by length, both ends
    /// included (for drawings that break a shape into dots).
    pub fn samples(&self, n: usize) -> Vec<Point> {
        match n {
            0 => Vec::new(),
            1 => self.point_at(0.0).into_iter().collect(),
            _ => (0..n)
                .filter_map(|i| self.point_at(i as f32 / (n - 1) as f32))
                .collect(),
        }
    }
}

fn reflect(ctrl: Option<P64>, cur: P64) -> P64 {
    match ctrl {
        Some(c) => P64 {
            x: 2.0 * cur.x - c.x,
            y: 2.0 * cur.y - c.y,
        },
        None => cur,
    }
}

/// SVG arc from `from` to `to` as cubics (SVG 1.1 appendix F.6.5).
#[allow(clippy::too_many_arguments)]
fn svg_arc(
    segs: &mut Vec<Seg>,
    from: P64,
    to: P64,
    rx: f64,
    ry: f64,
    rot_deg: f64,
    large: bool,
    sweep: bool,
) {
    if (from.x - to.x).abs() < 1e-9 && (from.y - to.y).abs() < 1e-9 {
        return;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-9 || ry < 1e-9 {
        segs.push(Seg::Line(to.pt()));
        return;
    }
    let phi = rot_deg.to_radians();
    let (sin, cos) = phi.sin_cos();
    let dx2 = (from.x - to.x) / 2.0;
    let dy2 = (from.y - to.y) / 2.0;
    let x1p = cos * dx2 + sin * dy2;
    let y1p = -sin * dx2 + cos * dy2;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = if den > 0.0 {
        (num / den).max(0.0).sqrt()
    } else {
        0.0
    };
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;
    let cx = cos * cxp - sin * cyp + (from.x + to.x) / 2.0;
    let cy = sin * cxp + cos * cyp + (from.y + to.y) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut dtheta = angle(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += TAU;
    }
    ellipse_cubics(segs, (cx, cy), (rx, ry), phi, theta1, dtheta);
    // Land exactly on the end point the data asked for.
    if let Some(Seg::Cubic(_, _, p)) = segs.last_mut() {
        *p = to.pt();
    }
}

/// Cubics for the elliptical arc around `c` with radii `r` and rotation
/// `phi`, from angle `t1` turning by `dt` (positive is clockwise on screen).
/// The pen must already be at the arc's start.
pub(super) fn ellipse_cubics(
    segs: &mut Vec<Seg>,
    c: (f64, f64),
    r: (f64, f64),
    phi: f64,
    t1: f64,
    dt: f64,
) {
    if dt.abs() < 1e-12 {
        return;
    }
    let n = (dt.abs() / FRAC_PI_2 - 1e-9).ceil().max(1.0) as usize;
    let d = dt / n as f64;
    let alpha = 4.0 / 3.0 * (d / 4.0).tan();
    let (sin, cos) = phi.sin_cos();
    let at = |x: f64, y: f64| {
        let (x, y) = (x * r.0, y * r.1);
        Point::new(
            (c.0 + cos * x - sin * y) as f32,
            (c.1 + sin * x + cos * y) as f32,
        )
    };
    let mut a = t1;
    for _ in 0..n {
        let b = a + d;
        let (sa, ca) = a.sin_cos();
        let (sb, cb) = b.sin_cos();
        segs.push(Seg::Cubic(
            at(ca - alpha * sa, sa + alpha * ca),
            at(cb + alpha * sb, sb - alpha * cb),
            at(cb, sb),
        ));
        a = b;
    }
}

/// Start point of the arc [`ellipse_cubics`] would draw.
pub(super) fn ellipse_point(c: (f64, f64), r: (f64, f64), phi: f64, t: f64) -> Point {
    let (sin, cos) = phi.sin_cos();
    let (x, y) = (t.cos() * r.0, t.sin() * r.1);
    Point::new(
        (c.0 + cos * x - sin * y) as f32,
        (c.1 + sin * x + cos * y) as f32,
    )
}

// ---------------------------------------------------------------------------
// Geometry of one segment
// ---------------------------------------------------------------------------

/// Samples per curve when measuring length.
const STEPS: usize = 16;

fn dist(a: Point, b: Point) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

fn lerp(a: Point, b: Point, t: f32) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn seg_end(s: Seg) -> Point {
    match s {
        Seg::Move(p) | Seg::Line(p) | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => p,
        Seg::Close => Point::ORIGIN,
    }
}

fn eval(from: Point, s: Seg, t: f32) -> Point {
    match s {
        Seg::Line(p) => lerp(from, p, t),
        Seg::Quad(a, p) => lerp(lerp(from, a, t), lerp(a, p, t), t),
        Seg::Cubic(a, b, p) => {
            let ab = lerp(lerp(from, a, t), lerp(a, b, t), t);
            let bc = lerp(lerp(a, b, t), lerp(b, p, t), t);
            lerp(ab, bc, t)
        }
        Seg::Move(p) => p,
        Seg::Close => from,
    }
}

fn seg_len(from: Point, s: Seg) -> f32 {
    match s {
        Seg::Line(p) => dist(from, p),
        Seg::Quad(..) | Seg::Cubic(..) => {
            let mut prev = from;
            let mut sum = 0.0;
            for i in 1..=STEPS {
                let q = eval(from, s, i as f32 / STEPS as f32);
                sum += dist(prev, q);
                prev = q;
            }
            sum
        }
        Seg::Move(_) | Seg::Close => 0.0,
    }
}

/// Curve parameter at which the segment has run `len` along its length.
fn t_at_length(from: Point, s: Seg, len: f32) -> f32 {
    match s {
        Seg::Line(p) => {
            let d = dist(from, p);
            if d > 0.0 {
                (len / d).clamp(0.0, 1.0)
            } else {
                1.0
            }
        }
        Seg::Quad(..) | Seg::Cubic(..) => {
            let mut prev = from;
            let mut run = 0.0;
            for i in 1..=STEPS {
                let q = eval(from, s, i as f32 / STEPS as f32);
                let d = dist(prev, q);
                if run + d >= len {
                    let f = if d > 0.0 { (len - run) / d } else { 0.0 };
                    return (i as f32 - 1.0 + f) / STEPS as f32;
                }
                run += d;
                prev = q;
            }
            1.0
        }
        Seg::Move(_) | Seg::Close => 1.0,
    }
}

/// The part of the segment from its start to parameter `t` (de Casteljau).
fn split(from: Point, s: Seg, t: f32) -> Seg {
    match s {
        Seg::Line(p) => Seg::Line(lerp(from, p, t)),
        Seg::Quad(a, p) => {
            let a1 = lerp(from, a, t);
            let mid = lerp(a1, lerp(a, p, t), t);
            Seg::Quad(a1, mid)
        }
        Seg::Cubic(a, b, p) => {
            let p01 = lerp(from, a, t);
            let p12 = lerp(a, b, t);
            let p23 = lerp(b, p, t);
            let p012 = lerp(p01, p12, t);
            let p123 = lerp(p12, p23, t);
            Seg::Cubic(p01, p012, lerp(p012, p123, t))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    #[test]
    fn square_absolute_and_relative() {
        let a = PathData::of("M0 0L10 0L10 10L0 10Z");
        let b = PathData::of("m0 0h10v10h-10z");
        let c = PathData::of("M0,0 10,0 10,10 0,10 z");
        for d in [&a, &b, &c] {
            assert_eq!(d.segs.len(), 5);
            assert_eq!(d.segs[0], Seg::Move(Point::new(0.0, 0.0)));
            assert_eq!(d.segs[2], Seg::Line(Point::new(10.0, 10.0)));
            assert_eq!(d.segs[4], Seg::Close);
            assert!((d.length() - 40.0).abs() < 1e-4);
        }
    }

    #[test]
    fn relative_moves_chain_and_repeat() {
        // A relative move after a close starts from the sub-path start, and
        // pairs after `m` are relative line-tos.
        let d = PathData::of("m2 3 4 0 0 4zm1 1l1 1 1 1");
        assert_eq!(
            d.segs,
            vec![
                Seg::Move(Point::new(2.0, 3.0)),
                Seg::Line(Point::new(6.0, 3.0)),
                Seg::Line(Point::new(6.0, 7.0)),
                Seg::Close,
                Seg::Move(Point::new(3.0, 4.0)),
                Seg::Line(Point::new(4.0, 5.0)),
                Seg::Line(Point::new(5.0, 6.0)),
            ]
        );
    }

    #[test]
    fn numbers_in_every_spelling() {
        let d = PathData::of("M-.5.5L1e-3-2E1l.5.5");
        assert_eq!(d.segs[0], Seg::Move(Point::new(-0.5, 0.5)));
        assert_eq!(d.segs[1], Seg::Line(Point::new(0.001, -20.0)));
        assert_eq!(d.segs[2], Seg::Line(Point::new(0.501, -19.5)));
        assert!(PathData::parse("10 10").is_err());
        assert!(PathData::parse("M1").is_err());
        assert!(PathData::parse("M1 1X").is_err());
        assert!(PathData::parse("M1 1z 3").is_err());
        assert_eq!(PathData::parse("").unwrap().segs.len(), 0);
    }

    #[test]
    fn smooth_curves_reflect() {
        let d = PathData::of("M0 0C0 10 10 10 10 0S20 -10 20 0");
        assert_eq!(
            d.segs[2],
            Seg::Cubic(
                Point::new(10.0, -10.0),
                Point::new(20.0, -10.0),
                Point::new(20.0, 0.0)
            )
        );
        let q = PathData::of("M0 0Q5 10 10 0T20 0");
        assert_eq!(q.segs[2], Seg::Quad(Point::new(15.0, -10.0), Point::new(20.0, 0.0)));
        // S with no cubic before it uses the current point.
        let s = PathData::of("M0 0L5 0S10 5 10 0");
        assert_eq!(
            s.segs[2],
            Seg::Cubic(Point::new(5.0, 0.0), Point::new(10.0, 5.0), Point::new(10.0, 0.0))
        );
    }

    #[test]
    fn arcs_end_where_asked_and_stay_on_the_circle() {
        // Half circle of radius 5 from (0,0) to (10,0). Sweep 1 turns through
        // growing angles, which on a y-down screen passes over the top.
        let d = PathData::of("M0 0A5 5 0 0 1 10 0");
        assert!(near(d.end().unwrap(), Point::new(10.0, 0.0)));
        assert_eq!(d.segs.len(), 3, "half turn = two quarter cubics");
        let mid = d.point_at(0.5).unwrap();
        assert!(near(mid, Point::new(5.0, -5.0)), "{mid:?}");
        assert!((d.length() - std::f32::consts::PI * 5.0).abs() < 0.05);
        // Same arc, other sweep: goes underneath.
        let u = PathData::of("M0 0a5 5 0 0 0 10 0");
        assert!(near(u.point_at(0.5).unwrap(), Point::new(5.0, 5.0)));
        // Too small radii are scaled up to just reach.
        let t = PathData::of("M0 0A1 1 0 0 1 10 0");
        assert!(near(t.end().unwrap(), Point::new(10.0, 0.0)));
        // Large arc flag: the long way round a radius-10 circle.
        let l = PathData::of("M0 0A10 10 0 1 1 10 0");
        assert!(l.length() > std::f32::consts::PI * 10.0);
        assert!(near(l.end().unwrap(), Point::new(10.0, 0.0)));
        // Packed flags, as minifiers write them.
        let p = PathData::of("M0 0a5 5 0 0110 0");
        assert!(near(p.end().unwrap(), Point::new(10.0, 0.0)));
        // Every arc point lies on the circle.
        for i in 0..=20 {
            let q = d.point_at(i as f32 / 20.0).unwrap();
            let r = ((q.x - 5.0).powi(2) + q.y.powi(2)).sqrt();
            assert!((r - 5.0).abs() < 0.01, "{q:?} r={r}");
        }
        // A full circle written as two arcs closes on itself.
        let c = PathData::of("M12 11.8a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z");
        assert!(near(c.end().unwrap(), Point::new(12.0, 11.8)));
        assert!((c.length() - std::f32::consts::TAU * 3.5).abs() < 0.05);
    }

    #[test]
    fn partial_draws_in_by_length() {
        let d = PathData::of("M0 0h10v10");
        assert!(d.partial(0.0).segs.is_empty());
        assert_eq!(d.partial(1.0), d);
        let h = d.partial(0.25);
        assert!(near(h.end().unwrap(), Point::new(5.0, 0.0)));
        let h = d.partial(0.75);
        assert!(near(h.end().unwrap(), Point::new(10.0, 5.0)));
        assert!((h.length() - 15.0).abs() < 1e-3);
        // Curves are cut on the curve.
        let a = PathData::of("M0 0A5 5 0 0 1 10 0");
        let half = a.partial(0.5);
        assert!(near(half.end().unwrap(), Point::new(5.0, -5.0)));
        // Two sub-paths draw one after the other.
        let two = PathData::of("M0 0h10M0 5h10");
        let p = two.partial(0.75);
        assert_eq!(p.segs.len(), 4);
        assert!(near(p.end().unwrap(), Point::new(5.0, 5.0)));
        // A closed shape draws its closing edge last.
        let sq = PathData::of("M0 0h10v10h-10z");
        let p = sq.partial(0.9);
        assert!(near(p.end().unwrap(), Point::new(0.0, 4.0)));
        assert_eq!(sq.partial(1.0).segs.last(), Some(&Seg::Close));
    }

    #[test]
    fn placing_an_icon_scales_its_box() {
        let d = PathData::of("M0 0L24 24");
        let p = d.placed(Point::new(100.0, 50.0), 48.0);
        assert_eq!(p.segs[0], Seg::Move(Point::new(76.0, 26.0)));
        assert_eq!(p.segs[1], Seg::Line(Point::new(124.0, 74.0)));
        let s = PathData::of("M0 0h10v10").samples(3);
        assert_eq!(s.len(), 3);
        assert!(near(s[1], Point::new(10.0, 0.0)));
        assert!(near(s[2], Point::new(10.0, 10.0)));
    }
}
