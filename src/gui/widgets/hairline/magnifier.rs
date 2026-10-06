//! Checking your PC: a monitor shows five settings and a magnifying glass looks them over.
use super::motion::{phase, Spring};
use super::pointer::{tooltip, Hotspots, Layer, TIP_ROOM};
use super::stage::{
    pt, stroke, tint_by, Ink, Plate, Stage, TINT, W_ACCENT, W_FAINT, W_LINE, W_MARK, W_PART,
};
use super::svg::{PathData, Seg};
use super::{Glyph, Live};
use crate::gui::theme::Palette;
use crate::gui::widgets::anim::{self, DECELERATE, EASE_IN_OUT};
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path};
use iced::{mouse, Color, Element, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::time::{Duration, Instant};


const VIEW_AT: Vector = Vector::new(36.0, 10.0);
pub const VIEW: Size = Size::new(256.0, 188.0);
pub const COMPACT: f32 = 0.625;
pub const FULL: f32 = 1.0;

const MON_CX: f32 = 160.0;
const MON_TOP: f32 = 30.0;
const MON_W: f32 = 190.0;
const MON_H: f32 = 128.0;
const SX0: f32 = MON_CX - MON_W / 2.0 + 6.0;
const SY0: f32 = MON_TOP + 6.0;
const SX1: f32 = MON_CX + MON_W / 2.0 - 6.0;
const SY1: f32 = MON_TOP + MON_H - 6.0;

const ROWS: [(Glyph, &str, f32); 5] = [
    (Glyph::Wall, "Firewall", 52.0),
    (Glyph::Shield, "Microsoft Defender", 70.0),
    (Glyph::Update, "Windows Update", 58.0),
    (Glyph::Remote, "Remote Desktop", 64.0),
    (Glyph::Wifi, "Network sharing", 48.0),
];
const N: usize = ROWS.len();
const ROW_GAP: f32 = 19.0;
const ROW_TOP: f32 = 30.0;
const TEXT_X: f32 = SX0 + 28.0;
const PILL_X: f32 = SX1 - 30.0;
const TICK_X: f32 = SX1 - 40.0;
const ROW_H: f32 = 18.0;
/// How close (units, vertically) the lens must be to read a row.
const NEAR: f32 = 9.0;

const LENS_R: f32 = 22.0;
const CLIP_R: f32 = LENS_R - 1.5;
const ZOOM: f32 = 1.55;
const HANDLE: f32 = 15.0;
const REST: Point = pt(238.0, 150.0);
const INVITE: f32 = 0.55;
const LEAD: Rectangle = Rectangle {
    x: SX0 - 10.0,
    y: SY0,
    width: SX1 - SX0 + 20.0,
    height: SY1 - SY0 + 10.0,
};


const STILL: f32 = 0.9;
const BOB_HOLD: f32 = 6.0;
const BOB_FADE: f32 = 1.5;
const LOOK_HOLD: Duration = Duration::from_millis(1600);
const RELOOK_DELAY: Duration = Duration::from_millis(450);
const TICK_STAGGER: Duration = Duration::from_millis(150);
const TICK_DRAW: f32 = 0.35;
const SWITCH_DONE: (f32, f32) = (0.45, 0.85);
const TIP_SPRING: (f32, f32) = (320.0, 34.0);


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ready,
    Checking,
    /// The check has ended: every row ticked, the glass at rest.
    Done,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Labels(pub [String; N]);

impl Labels {
    pub fn new(t: impl Fn(&str) -> String) -> Labels {
        Labels(ROWS.map(|(_, name, _)| t(name)))
    }
}

#[derive(Debug, Clone)]
pub struct Magnifier {
    pub p: Palette,
    pub plate: Plate,
    pub status: Status,
    pub progress: Option<f32>,
    pub changed: Instant,
    pub now: Instant,
    pub labels: Labels,
}

impl Magnifier {
    pub fn view<'a, M: 'a>(self, scale: f32) -> Element<'a, M> {
        super::fixed_canvas(self, Size::new(VIEW.width * scale, VIEW.height * scale))
    }
}


fn stage_for(bounds: Size) -> Stage {
    Stage::fit(VIEW, bounds).shifted(Vector::new(-VIEW_AT.x, -VIEW_AT.y))
}

fn row_y(i: usize) -> f32 {
    SY0 + ROW_TOP + i as f32 * ROW_GAP
}

pub fn rows_at(progress: f32) -> (usize, usize) {
    let done = ((progress.clamp(0.0, 1.0) * N as f32).floor() as usize).min(N);
    (done.min(N - 1), done)
}

fn spots() -> Hotspots<usize> {
    (0..N).fold(Hotspots::new(), |h, i| {
        h.rect(i, pt((SX0 + SX1) / 2.0, row_y(i)), SX1 - SX0, ROW_H, Layer::Fixed)
    })
}

fn row_reading(i: usize) -> Point {
    pt(TEXT_X + ROWS[i].2 / 2.0, row_y(i))
}

fn lead(p: Point) -> Point {
    pt(
        p.x.clamp(LEAD.x, LEAD.x + LEAD.width),
        p.y.clamp(LEAD.y, LEAD.y + LEAD.height),
    )
}

fn leads(at: Point) -> bool {
    at.x > LEAD.x && at.x < LEAD.x + LEAD.width && at.y > LEAD.y && at.y < LEAD.y + LEAD.height
}

fn reading(lens: Point, i: usize) -> f32 {
    if lens.x <= SX0 || lens.x >= SX1 {
        return 0.0;
    }
    (1.0 - ((lens.y - row_y(i)).abs() - (NEAR - 4.0)) / 4.0).clamp(0.0, 1.0)
}

fn bob_left(idle: f32) -> f32 {
    1.0 - phase(idle, BOB_HOLD, BOB_HOLD + BOB_FADE, EASE_IN_OUT)
}

fn bob(t: f32, amount: f32) -> Vector {
    Vector::new(3.0 * (t * 1.3).sin(), 4.0 * (t * 1.7).sin()) * amount
}

fn tip_at(st: &State, s: &Stage) -> Point {
    let top = if s.k > 0.0 {
        VIEW_AT.y + TIP_ROOM / s.k
    } else {
        VIEW_AT.y
    };
    pt(st.tip_x.value, st.tip_y.value.max(top))
}

fn secs(d: Duration) -> f32 {
    d.as_secs_f32()
}


#[derive(Debug, Clone, Copy, PartialEq)]
enum Paint {
    Plate,
    Fill(Color),
    Stroke(Color, f32),
}

#[derive(Debug, Clone, PartialEq)]
struct Prim {
    d: PathData,
    paint: Paint,
}

const KAPPA: f32 = 0.552_284_8;

fn line_d(a: Point, b: Point) -> PathData {
    PathData {
        segs: vec![Seg::Move(a), Seg::Line(b)],
    }
}

fn poly_d(pts: &[Point]) -> PathData {
    let mut segs: Vec<Seg> = pts
        .iter()
        .enumerate()
        .map(|(i, p)| if i == 0 { Seg::Move(*p) } else { Seg::Line(*p) })
        .collect();
    segs.push(Seg::Close);
    PathData { segs }
}

fn circle_d(c: Point, r: f32) -> PathData {
    let k = r * KAPPA;
    PathData {
        segs: vec![
            Seg::Move(pt(c.x + r, c.y)),
            Seg::Cubic(pt(c.x + r, c.y + k), pt(c.x + k, c.y + r), pt(c.x, c.y + r)),
            Seg::Cubic(pt(c.x - k, c.y + r), pt(c.x - r, c.y + k), pt(c.x - r, c.y)),
            Seg::Cubic(pt(c.x - r, c.y - k), pt(c.x - k, c.y - r), pt(c.x, c.y - r)),
            Seg::Cubic(pt(c.x + k, c.y - r), pt(c.x + r, c.y - k), pt(c.x + r, c.y)),
            Seg::Close,
        ],
    }
}

fn rr_d(x: f32, y: f32, w: f32, h: f32, r: f32) -> PathData {
    let k = r * KAPPA;
    let (x1, y1) = (x + w, y + h);
    PathData {
        segs: vec![
            Seg::Move(pt(x + r, y)),
            Seg::Line(pt(x1 - r, y)),
            Seg::Cubic(pt(x1 - r + k, y), pt(x1, y + r - k), pt(x1, y + r)),
            Seg::Line(pt(x1, y1 - r)),
            Seg::Cubic(pt(x1, y1 - r + k), pt(x1 - r + k, y1), pt(x1 - r, y1)),
            Seg::Line(pt(x + r, y1)),
            Seg::Cubic(pt(x + r - k, y1), pt(x, y1 - r + k), pt(x, y1 - r)),
            Seg::Line(pt(x, y + r)),
            Seg::Cubic(pt(x, y + r - k), pt(x + r - k, y), pt(x + r, y)),
            Seg::Close,
        ],
    }
}

fn monitor_prims(ink: &Ink, out: &mut Vec<Prim>) {
    let x0 = MON_CX - MON_W / 2.0;
    let y1 = MON_TOP + MON_H;
    let line = Paint::Stroke(ink.line, W_LINE);
    let shapes = [
        poly_d(&[
            pt(MON_CX - 10.0, y1),
            pt(MON_CX + 10.0, y1),
            pt(MON_CX + 14.0, y1 + 22.0),
            pt(MON_CX - 14.0, y1 + 22.0),
        ]),
        rr_d(MON_CX - 40.0, y1 + 21.0, 80.0, 7.0, 3.5),
        rr_d(x0, MON_TOP, MON_W, MON_H, 7.0),
    ];
    for d in shapes {
        out.push(Prim {
            d: d.clone(),
            paint: Paint::Plate,
        });
        out.push(Prim { d, paint: line });
    }
    out.push(Prim {
        d: rr_d(SX0, SY0, SX1 - SX0, SY1 - SY0, 3.0),
        paint: Paint::Stroke(ink.rule, W_FAINT),
    });
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RowLook {
    read: f32,
    knob: f32,
    tick: f32,
    done: f32,
}

fn screen_prims(ink: &Ink, rows: &[RowLook; N], out: &mut Vec<Prim>) {
    for i in 0..3 {
        out.push(Prim {
            d: circle_d(pt(SX0 + 9.0 + i as f32 * 6.0, SY0 + 8.0), 1.5),
            paint: Paint::Fill(ink.line),
        });
    }
    out.push(Prim {
        d: line_d(pt(SX0, SY0 + 16.0), pt(SX1, SY0 + 16.0)),
        paint: Paint::Stroke(ink.rule, W_FAINT),
    });
    for (i, look) in rows.iter().enumerate() {
        let (glyph, _, len) = ROWS[i];
        let y = row_y(i);
        let icon = if look.read > 0.5 { ink.accent } else { ink.line };
        out.push(Prim {
            d: glyph.data().placed(pt(SX0 + 16.0, y), 12.0),
            paint: Paint::Stroke(icon, W_PART),
        });
        let text = line_d(pt(TEXT_X, y - 2.0), pt(TEXT_X + len, y - 2.0));
        out.push(Prim {
            d: text.clone(),
            paint: Paint::Stroke(ink.faint, W_FAINT),
        });
        out.push(Prim {
            d: line_d(pt(TEXT_X, y + 3.0), pt(TEXT_X + len * 0.6, y + 3.0)),
            paint: Paint::Stroke(ink.rule, W_FAINT),
        });
        if look.read > 0.004 {
            out.push(Prim {
                d: text,
                paint: Paint::Stroke(ink.accent.scale_alpha(look.read), W_ACCENT),
            });
        }
        let pill = rr_d(PILL_X, y - 5.0, 20.0, 10.0, 5.0);
        let on = look.knob.clamp(0.0, 1.0);
        let col = tint_by(ink.good, ink.accent, look.done.clamp(0.0, 1.0));
        if on > 0.004 {
            out.push(Prim {
                d: pill.clone(),
                paint: Paint::Fill(tint_by(col, ink.plate, TINT * on)),
            });
        }
        out.push(Prim {
            d: pill.clone(),
            paint: Paint::Stroke(ink.faint, W_FAINT),
        });
        if on > 0.004 {
            out.push(Prim {
                d: pill,
                paint: Paint::Stroke(col.scale_alpha(on), W_PART),
            });
        }
        let knob = circle_d(pt(PILL_X + 5.0 + 10.0 * look.knob, y), 3.0);
        if on < 0.996 {
            out.push(Prim {
                d: knob.clone(),
                paint: Paint::Stroke(ink.faint.scale_alpha(1.0 - on), W_FAINT),
            });
        }
        if on > 0.004 {
            out.push(Prim {
                d: knob,
                paint: Paint::Fill(col.scale_alpha(on)),
            });
        }
        if look.tick > 0.001 {
            let tick = PathData {
                segs: vec![
                    Seg::Move(pt(TICK_X, y)),
                    Seg::Line(pt(TICK_X + 2.2, y + 2.2)),
                    Seg::Line(pt(TICK_X + 6.4, y - 2.4)),
                ],
            };
            out.push(Prim {
                d: tick.partial(look.tick),
                paint: Paint::Stroke(ink.good, W_MARK),
            });
        }
    }
}

fn draw_prim(f: &mut Frame, s: &Stage, prim: &Prim, plate: Color) {
    let path = s.shape(&prim.d);
    match prim.paint {
        Paint::Plate => f.fill(&path, plate),
        Paint::Fill(c) => f.fill(&path, c),
        Paint::Stroke(c, w) => f.stroke(&path, stroke(c, w)),
    }
}


fn flatten(d: &PathData) -> Vec<(Vec<Point>, bool)> {
    let mut out: Vec<(Vec<Point>, bool)> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    let flush = |cur: &mut Vec<Point>, closed: bool, out: &mut Vec<(Vec<Point>, bool)>| {
        if cur.len() > 1 {
            out.push((std::mem::take(cur), closed));
        } else {
            cur.clear();
        }
    };
    let mut pen = Point::ORIGIN;
    let mut start = Point::ORIGIN;
    for s in &d.segs {
        match *s {
            Seg::Move(p) => {
                flush(&mut cur, false, &mut out);
                cur.push(p);
                pen = p;
                start = p;
            }
            Seg::Line(p) => {
                if cur.is_empty() {
                    cur.push(pen);
                }
                cur.push(p);
                pen = p;
            }
            Seg::Quad(c, p) => {
                if cur.is_empty() {
                    cur.push(pen);
                }
                for k in 1..=6 {
                    let t = k as f32 / 6.0;
                    let u = 1.0 - t;
                    cur.push(pt(
                        u * u * pen.x + 2.0 * u * t * c.x + t * t * p.x,
                        u * u * pen.y + 2.0 * u * t * c.y + t * t * p.y,
                    ));
                }
                pen = p;
            }
            Seg::Cubic(a, b, p) => {
                if cur.is_empty() {
                    cur.push(pen);
                }
                for k in 1..=8 {
                    let t = k as f32 / 8.0;
                    let u = 1.0 - t;
                    let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    cur.push(pt(
                        w0 * pen.x + w1 * a.x + w2 * b.x + w3 * p.x,
                        w0 * pen.y + w1 * a.y + w2 * b.y + w3 * p.y,
                    ));
                }
                pen = p;
            }
            Seg::Close => {
                if cur.last() != Some(&start) {
                    cur.push(start);
                }
                flush(&mut cur, true, &mut out);
                pen = start;
            }
        }
    }
    flush(&mut cur, false, &mut out);
    out
}

fn inside_span(a: Point, b: Point, c: Point, r: f32) -> Option<(f32, f32)> {
    let d = b - a;
    let f = a - c;
    let qa = d.x * d.x + d.y * d.y;
    let qc = f.x * f.x + f.y * f.y - r * r;
    if qa < 1e-9 {
        return (qc < 0.0).then_some((0.0, 1.0));
    }
    let qb = 2.0 * (f.x * d.x + f.y * d.y);
    let disc = qb * qb - 4.0 * qa * qc;
    if disc <= 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let t0 = ((-qb - s) / (2.0 * qa)).max(0.0);
    let t1 = ((-qb + s) / (2.0 * qa)).min(1.0);
    (t0 < t1).then_some((t0, t1))
}

fn along(a: Point, b: Point, t: f32) -> Point {
    pt(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn clip_line(pts: &[Point], c: Point, r: f32) -> Vec<Vec<Point>> {
    let mut out = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        match inside_span(a, b, c, r) {
            Some((t0, t1)) => {
                if cur.is_empty() || t0 > 1e-5 {
                    if cur.len() > 1 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur = vec![along(a, b, t0)];
                }
                cur.push(along(a, b, t1));
                if t1 < 1.0 - 1e-5 {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

fn clip_fill(poly: &[Point], c: Point, r: f32) -> Vec<Point> {
    const SIDES: usize = 36;
    let rr = r / (std::f32::consts::PI / SIDES as f32).cos();
    let corner = |k: usize| {
        let a = k as f32 * std::f32::consts::TAU / SIDES as f32;
        pt(c.x + rr * a.cos(), c.y + rr * a.sin())
    };
    let mut out: Vec<Point> = poly.to_vec();
    for k in 0..SIDES {
        if out.len() < 3 {
            return Vec::new();
        }
        let (e0, e1) = (corner(k), corner(k + 1));
        let side = |p: Point| (e1.x - e0.x) * (p.y - e0.y) - (e1.y - e0.y) * (p.x - e0.x);
        let input = std::mem::take(&mut out);
        for (i, &q) in input.iter().enumerate() {
            let prev = input[(i + input.len() - 1) % input.len()];
            let (sq, sp) = (side(q), side(prev));
            if sq >= 0.0 {
                if sp < 0.0 {
                    out.push(along(prev, q, sp / (sp - sq)));
                }
                out.push(q);
            } else if sp >= 0.0 {
                out.push(along(prev, q, sp / (sp - sq)));
            }
        }
    }
    out
}

fn draw_zoomed(f: &mut Frame, s: &Stage, prims: &[Prim], c: Point) {
    let reach = CLIP_R / ZOOM + 1.0;
    let zoom = |p: Point| pt(c.x + (p.x - c.x) * ZOOM, c.y + (p.y - c.y) * ZOOM);
    for prim in prims {
        if prim.paint == Paint::Plate {
            continue;
        }
        let Some(b) = prim.d.bounds() else { continue };
        if b.x > c.x + reach
            || b.x + b.width < c.x - reach
            || b.y > c.y + reach
            || b.y + b.height < c.y - reach
        {
            continue;
        }
        for (pts, closed) in flatten(&prim.d) {
            let pts: Vec<Point> = pts.into_iter().map(zoom).collect();
            match prim.paint {
                Paint::Fill(col) if closed => {
                    let cut = clip_fill(&pts, c, CLIP_R);
                    if cut.len() > 2 {
                        let path = Path::new(|b| {
                            b.move_to(s.point(cut[0]));
                            for q in &cut[1..] {
                                b.line_to(s.point(*q));
                            }
                            b.close();
                        });
                        f.fill(&path, col);
                    }
                }
                Paint::Stroke(col, w) => {
                    let pieces = clip_line(&pts, c, CLIP_R);
                    if pieces.is_empty() {
                        continue;
                    }
                    let path = Path::new(|b| {
                        for piece in &pieces {
                            b.move_to(s.point(piece[0]));
                            for q in &piece[1..] {
                                b.line_to(s.point(*q));
                            }
                        }
                    });
                    f.stroke(&path, stroke(col, w));
                }
                _ => {}
            }
        }
    }
}


#[derive(Debug, Clone, PartialEq)]
struct Row {
    seen: Option<Instant>,
    knob: Spring,
}

impl Default for Row {
    fn default() -> Self {
        Row {
            seen: None,
            knob: Spring::with(0.0, 160.0, 20.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct State {
    live: Live<usize>,
    lens_x: Spring,
    lens_y: Spring,
    rows: [Row; N],
    look: Option<(usize, Instant)>,
    active: Option<Instant>,
    tip_x: Spring,
    tip_y: Spring,
    tip_on: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            live: Live::without_tilt(),
            lens_x: Spring::with(REST.x, 70.0, 13.0),
            lens_y: Spring::with(REST.y, 70.0, 13.0),
            rows: Default::default(),
            look: None,
            active: None,
            tip_x: Spring::with(0.0, TIP_SPRING.0, TIP_SPRING.1),
            tip_y: Spring::with(0.0, TIP_SPRING.0, TIP_SPRING.1),
            tip_on: false,
        }
    }
}

impl State {
    fn lens(&self) -> Point {
        pt(self.lens_x.value, self.lens_y.value)
    }
}

impl Magnifier {
    fn working(&self) -> bool {
        self.status == Status::Checking
    }

    fn bob_amount(&self, st: &State, now: Instant) -> f32 {
        if self.working() || self.status == Status::Done || anim::reduced() {
            return 0.0;
        }
        match st.active.or(st.live.born) {
            Some(at) => bob_left(secs(now.saturating_duration_since(at))),
            None => 1.0,
        }
    }

    fn target(&self, st: &State, now: Instant, t: f32) -> Point {
        if let Some((i, _)) = st.look {
            return row_reading(i);
        }
        let ptr = &st.live.pointer;
        let over = ptr.inside && leads(ptr.at);
        match (self.status, self.progress) {
            (Status::Ready | Status::Done, _) => {
                let rest = REST + bob(t, self.bob_amount(st, now));
                if over {
                    let to = lead(ptr.at);
                    pt(rest.x + (to.x - rest.x) * INVITE, rest.y + (to.y - rest.y) * INVITE)
                } else {
                    rest
                }
            }
            (Status::Checking, _) if over => lead(ptr.at),
            (Status::Checking, Some(p)) => {
                let (focus, _) = rows_at(p);
                pt(155.0 + 50.0 * (t * 0.9).sin(), row_y(focus) + 2.5 * (t * 1.7).sin())
            }
            (Status::Checking, None) => pt(
                138.0 + 46.0 * (t * 0.9).sin(),
                row_y(0) + (row_y(N - 1) - row_y(0)) * (0.5 + 0.5 * (t * 1.5 + 0.6).sin()),
            ),
        }
    }

    fn sync_rows(&self, st: &mut State, now: Instant) {
        let progress = if self.status == Status::Done {
            Some(1.0)
        } else {
            self.progress
        };
        match (self.status, progress) {
            (Status::Ready, _) => {
                for r in &mut st.rows {
                    r.seen = None;
                }
            }
            (Status::Checking | Status::Done, Some(p)) => {
                let (_, done) = rows_at(p);
                let mut fresh = 0u32;
                let stagger = if anim::reduced() {
                    Duration::ZERO
                } else {
                    TICK_STAGGER
                };
                for (i, r) in st.rows.iter_mut().enumerate() {
                    if i < done {
                        if r.seen.is_none() {
                            r.seen = Some(now + stagger * fresh);
                            fresh += 1;
                        }
                    } else {
                        r.seen = None;
                    }
                }
            }
            (Status::Checking | Status::Done, None) => {
                let age = st.live.age(self.changed, self.now);
                let lens = st.lens();
                for (i, r) in st.rows.iter_mut().enumerate() {
                    if r.seen.is_none() && reading(lens, i) > 0.5 && age > 0.6 + i as f32 * 1.3 {
                        r.seen = Some(now);
                    }
                }
            }
        }
    }

    fn look_again(&self, st: &mut State, i: usize, now: Instant) {
        st.look = Some((i, now));
        if let Some(seen) = &mut st.rows[i].seen {
            *seen = if anim::reduced() { now } else { now + RELOOK_DELAY };
        }
    }

    fn row_looks(&self, st: &State, now: Instant) -> [RowLook; N] {
        let lens = st.lens();
        let reads = self.working() || st.look.is_some();
        std::array::from_fn(|i| {
            let (tick, done) = match st.rows[i].seen {
                Some(_) if anim::reduced() => (1.0, 1.0),
                Some(at) if now >= at => {
                    let age = secs(now - at);
                    (
                        phase(age, 0.0, TICK_DRAW, DECELERATE),
                        phase(age, SWITCH_DONE.0, SWITCH_DONE.1, EASE_IN_OUT),
                    )
                }
                _ => (0.0, 0.0),
            };
            RowLook {
                read: if reads { reading(lens, i) } else { 0.0 },
                knob: st.rows[i].knob.value,
                tick,
                done,
            }
        })
    }

    fn busy(&self, st: &State, now: Instant) -> bool {
        if anim::reduced() {
            return false;
        }
        let ticking = st.rows.iter().any(|r| {
            r.knob.moving()
                || r.seen.is_some_and(|at| {
                    now < at + Duration::from_secs_f32(TICK_DRAW.max(SWITCH_DONE.1))
                })
        });
        let tip = st.tip_on && (st.tip_x.moving() || st.tip_y.moving());
        self.working()
            || self.bob_amount(st, now) > 0.0
            || st.lens_x.moving()
            || st.lens_y.moving()
            || st.look.is_some()
            || ticking
            || tip
    }

    fn tip_anchor(&self, st: &State, i: usize) -> Point {
        let ptr = &st.live.pointer;
        let on_lens = match st.look {
            Some((j, _)) => j == i,
            None => self.working() && ptr.inside && leads(ptr.at),
        };
        if on_lens {
            let c = st.lens();
            pt(c.x, c.y - LENS_R - 1.0)
        } else {
            pt((SX0 + SX1) / 2.0, row_y(i) - ROW_H / 2.0 * 0.7)
        }
    }
}

impl<M> canvas::Program<M> for Magnifier {
    type State = State;

    fn update(
        &self,
        st: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        let stage = stage_for(bounds.size());
        let fresh = st.live.fresh(self.changed);
        let step = st.live.update(event, bounds, cursor, &stage, &spots());
        let now = st.live.clock(self.now);
        let t = st.live.ambient(self.now, STILL);
        if fresh {
            st.look = None;
            st.active = Some(now);
            for r in &mut st.rows {
                *r = Row::default();
            }
            let at = self.target(st, now, t);
            st.lens_x = Spring::with(at.x, 70.0, 13.0);
            st.lens_y = Spring::with(at.y, 70.0, 13.0);
        }
        // Only a real pointer event over the drawing keeps the Ready bob
        // going: a cursor resting on it must not keep frames coming.
        if step.gesture.is_some() && st.live.pointer.inside {
            st.active = Some(now);
        }
        if let Some(at) = step.click() {
            match st.live.hover {
                Some(i) => self.look_again(st, i, now),
                None => st.live.pulses.push(at, false),
            }
        }
        if st.look.is_some_and(|(_, since)| now >= since + LOOK_HOLD) {
            st.look = None;
        }
        if let Some(dt) = step.dt {
            self.sync_rows(st, now);
            let to = self.target(st, now, t);
            st.lens_x.target = to.x;
            st.lens_y.target = to.y;
            st.lens_x.tick(dt);
            st.lens_y.tick(dt);
            for r in &mut st.rows {
                r.knob.target = if r.seen.is_some_and(|at| now >= at) { 1.0 } else { 0.0 };
                r.knob.tick(dt);
            }
        }
        match st.live.hover {
            Some(i) => {
                let to = self.tip_anchor(st, i);
                if !st.tip_on || anim::reduced() {
                    st.tip_x = Spring::with(to.x, TIP_SPRING.0, TIP_SPRING.1);
                    st.tip_y = Spring::with(to.y, TIP_SPRING.0, TIP_SPRING.1);
                } else {
                    st.tip_x.target = to.x;
                    st.tip_y.target = to.y;
                    if let Some(dt) = step.dt {
                        st.tip_x.tick(dt);
                        st.tip_y.tick(dt);
                    }
                }
                st.tip_on = true;
            }
            None => st.tip_on = false,
        }
        let busy = self.busy(st, now) || step.gesture.is_some();
        match st.live.redraw(&step, busy) {
            Some(a) => Some(a),
            None => st
                .look
                .map(|(_, since)| Action::request_redraw_at(since + LOOK_HOLD)),
        }
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
        let s = stage_for(bounds.size());
        let ink = Ink::new(&self.p, self.plate);
        let now = st.live.clock(self.now);
        let rows = self.row_looks(st, now);

        let mut prims = Vec::with_capacity(64);
        monitor_prims(&ink, &mut prims);
        screen_prims(&ink, &rows, &mut prims);
        for prim in &prims {
            draw_prim(&mut f, &s, prim, ink.plate);
        }

        let c = st.lens();
        let glass = if self.working() {
            tint_by(ink.accent, ink.plate, 0.06)
        } else {
            ink.plate
        };
        f.fill(&s.circle(c, LENS_R), glass);
        draw_zoomed(&mut f, &s, &prims, c);
        let from = pt(c.x + LENS_R * 0.72, c.y + LENS_R * 0.72);
        f.stroke(
            &s.line(from, pt(from.x + HANDLE, from.y + HANDLE)),
            ink.thick(ink.ink),
        );
        f.stroke(&s.circle(c, LENS_R), ink.strong());
        if self.working() {
            f.stroke(
                &s.circle(c, LENS_R),
                stroke(ink.accent.scale_alpha(0.55), W_ACCENT),
            );
        }
        f.stroke(&s.arc(c, LENS_R - 5.0, LENS_R - 5.0, -2.6, -1.7), ink.ln2());

        st.live.pulses.draw(&mut f, &s, ink.accent);
        if let (Some(i), true) = (st.live.hover, st.tip_on) {
            tooltip(&mut f, &self.p, &s, tip_at(st, &s), &self.labels.0[i]);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::theme::{DARK, LIGHT};
    use crate::i18n::Lang;
    use iced::window;

    #[test]
    fn progress_moves_the_lens_down_the_rows() {
        assert_eq!(rows_at(0.0), (0, 0));
        assert_eq!(rows_at(0.19), (0, 0));
        assert_eq!(rows_at(0.2), (1, 1));
        assert_eq!(rows_at(0.5), (2, 2));
        assert_eq!(rows_at(0.96), (4, 4));
        assert_eq!(rows_at(1.0), (4, 5));
        assert_eq!(rows_at(-1.0), (0, 0));
        assert_eq!(rows_at(7.0), (4, 5));
        let mut last = 0;
        for k in 0..=100 {
            let (_, done) = rows_at(k as f32 / 100.0);
            assert!(done >= last);
            last = done;
        }
    }

    #[test]
    fn rows_are_hit_where_they_are_drawn() {
        let h = spots();
        let tilt = crate::gui::widgets::hairline::parallax::Parallax::off();
        for i in 0..N {
            assert_eq!(h.hit(pt(120.0, row_y(i)), &tilt), Some(i));
            assert_eq!(h.hit(pt(SX1 - 2.0, row_y(i) + 8.0), &tilt), Some(i));
        }
        assert_eq!(h.hit(pt(150.0, row_y(1) + 6.0), &tilt), Some(1));
        assert_eq!(h.hit(pt(150.0, row_y(1) + 13.0), &tilt), Some(2));
        assert_eq!(h.hit(pt(150.0, SY0 + 8.0), &tilt), None);
        assert_eq!(h.hit(pt(SX0 - 3.0, row_y(2)), &tilt), None);
        assert_eq!(h.hit(pt(MON_CX, SY1 + 12.0), &tilt), None);
        assert_eq!(h.hit(pt(SX1 + 8.0, row_y(1)), &tilt), None);
    }

    #[test]
    fn labels_go_through_translation() {
        let marked = Labels::new(|s| format!("<{s}>"));
        for (i, (_, name, _)) in ROWS.iter().enumerate() {
            assert_eq!(marked.0[i], format!("<{name}>"));
        }
        let en = Labels::new(|s| Lang::En.t(s));
        assert_eq!(en.0[0], "Firewall");
        assert_eq!(en.0[3], "Remote Desktop");
        assert_eq!(Labels::new(|s| Lang::Fr.t(s)).0[0], "Pare-feu");
        assert_eq!(Labels::new(|s| Lang::De.t(s)).0[4], "Netzwerkfreigabe");
        assert_eq!(Labels::new(|s| Lang::Es.t(s)).0[4], "Uso compartido de red");
        assert_eq!(Labels::new(|s| Lang::Pt.t(s)).0[3], "Área de Trabalho Remota");
        assert_eq!(Labels::new(|s| Lang::It.t(s)).0[3], "Desktop remoto");
    }

    #[test]
    fn the_glass_never_leaves_the_drawing() {
        let reach = |c: Point| {
            let r = LENS_R + 1.0;
            let tip = c.x + LENS_R * 0.72 + HANDLE + 1.5;
            let low = c.y + LENS_R * 0.72 + HANDLE + 1.5;
            c.x - r >= VIEW_AT.x
                && c.y - r >= VIEW_AT.y
                && tip.max(c.x + r) <= VIEW_AT.x + VIEW.width
                && low.max(c.y + r) <= VIEW_AT.y + VIEW.height
        };
        for c in [
            lead(pt(-100.0, -100.0)),
            lead(pt(1000.0, 1000.0)),
            lead(pt(-100.0, 1000.0)),
            lead(pt(1000.0, -100.0)),
            REST + bob(0.3, 1.0),
            pt(REST.x + 3.0, REST.y + 4.0),
        ] {
            assert!(reach(c), "{c:?}");
        }
        const { assert!(MON_CX - MON_W / 2.0 - 1.0 >= VIEW_AT.x) };
        const { assert!(MON_TOP + MON_H + 28.0 + 1.0 <= VIEW_AT.y + VIEW.height) };
        for i in 0..N {
            assert!(reach(row_reading(i)));
        }
    }

    #[test]
    fn reading_fades_between_rows() {
        let y = row_y(2);
        assert_eq!(reading(pt(150.0, y), 2), 1.0);
        assert_eq!(reading(pt(150.0, y + 4.0), 2), 1.0);
        assert!(reading(pt(150.0, y + 7.0), 2) > 0.0 && reading(pt(150.0, y + 7.0), 2) < 1.0);
        assert_eq!(reading(pt(150.0, y + 9.5), 2), 0.0);
        assert_eq!(reading(pt(SX0 - 1.0, y), 2), 0.0);
    }

    #[test]
    fn the_bob_settles_after_a_while() {
        assert_eq!(bob_left(0.0), 1.0);
        assert_eq!(bob_left(BOB_HOLD), 1.0);
        let mid = bob_left(BOB_HOLD + BOB_FADE / 2.0);
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(bob_left(BOB_HOLD + BOB_FADE), 0.0);
        assert_eq!(bob(1.234, 0.0), Vector::ZERO);
    }

    #[test]
    fn lens_cuts_lines_and_fills_to_its_circle() {
        let c = pt(0.0, 0.0);
        let pieces = clip_line(&[pt(-50.0, 0.0), pt(50.0, 0.0)], c, 10.0);
        assert_eq!(pieces.len(), 1);
        assert!((pieces[0][0].x + 10.0).abs() < 1e-3 && (pieces[0][1].x - 10.0).abs() < 1e-3);
        let zig = [pt(-20.0, 0.0), pt(0.0, 0.0), pt(0.0, 30.0), pt(0.0, -5.0)];
        assert_eq!(clip_line(&zig, c, 10.0).len(), 2);
        assert!(clip_line(&[pt(20.0, 20.0), pt(30.0, 20.0)], c, 10.0).is_empty());
        let inside = clip_line(&[pt(-2.0, 1.0), pt(3.0, 1.0)], c, 10.0);
        assert_eq!(inside, vec![vec![pt(-2.0, 1.0), pt(3.0, 1.0)]]);
        let sq = [pt(-50.0, -50.0), pt(50.0, -50.0), pt(50.0, 50.0), pt(-50.0, 50.0)];
        let cut = clip_fill(&sq, c, 10.0);
        assert!(cut.len() >= 30);
        assert!(cut.iter().all(|p| (p.x.hypot(p.y) - 10.0).abs() < 0.2));
        let small = [pt(1.0, 1.0), pt(3.0, 1.0), pt(3.0, 3.0)];
        assert_eq!(clip_fill(&small, c, 10.0).len(), 3);
        assert!(clip_fill(&[pt(40.0, 40.0), pt(45.0, 40.0), pt(45.0, 45.0)], c, 10.0).is_empty());
    }

    #[test]
    fn shapes_flatten_into_closed_rings() {
        let ring = flatten(&circle_d(pt(5.0, 5.0), 3.0));
        assert_eq!(ring.len(), 1);
        assert!(ring[0].1);
        assert!(ring[0].0.iter().all(|p| ((p.x - 5.0).hypot(p.y - 5.0) - 3.0).abs() < 0.01));
        let wall = flatten(&Glyph::Wall.data().placed(pt(0.0, 0.0), 12.0));
        assert!(wall.len() > 5);
    }

    fn sample() -> Magnifier {
        let t0 = Instant::now();
        Magnifier {
            p: LIGHT,
            plate: Plate::Bg,
            status: Status::Checking,
            progress: Some(0.0),
            changed: t0,
            now: t0,
            labels: Labels::new(|s| s.to_owned()),
        }
    }

    const BOUNDS: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 256.0,
        height: 188.0,
    };

    fn frame(st: &mut State, m: &Magnifier, clock: &mut Instant, cursor: mouse::Cursor) -> bool {
        *clock += Duration::from_millis(16);
        let a = canvas::Program::<()>::update(
            m,
            st,
            &Event::Window(window::Event::RedrawRequested(*clock)),
            BOUNDS,
            cursor,
        );
        a.map(|a| a.into_inner().1 == window::RedrawRequest::NextFrame)
            .unwrap_or(false)
    }

    #[test]
    fn progress_ticks_rows_and_the_ready_glass_goes_quiet() {
        let _m = anim::forced::set(false);
        let mut m = sample();
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        let mut clock = m.changed;
        for _ in 0..20 {
            assert!(frame(&mut st, &m, &mut clock, off));
        }
        assert!(st.rows.iter().all(|r| r.seen.is_none()));
        assert!((st.lens_y.value - row_y(0)).abs() < 4.0);
        m.progress = Some(0.45);
        for _ in 0..120 {
            frame(&mut st, &m, &mut clock, off);
        }
        assert!(st.rows[0].seen.is_some() && st.rows[1].seen.is_some());
        assert!(st.rows[0].seen < st.rows[1].seen);
        assert!(st.rows[2].seen.is_none());
        assert!(st.rows[0].knob.value > 0.9);
        assert!((st.lens_y.value - row_y(2)).abs() < 4.0);
        let looks = m.row_looks(&st, clock);
        assert_eq!(looks[0].tick, 1.0);
        assert!(looks[2].read > 0.5 && looks[0].read == 0.0);

        let at = Point::new(120.0 - VIEW_AT.x, row_y(0) - VIEW_AT.y);
        let on_row = mouse::Cursor::Available(at);
        let moved = Event::Mouse(mouse::Event::CursorMoved { position: at });
        canvas::Program::<()>::update(&m, &mut st, &moved, BOUNDS, on_row);
        assert_eq!(st.live.hover, Some(0));
        let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        canvas::Program::<()>::update(&m, &mut st, &down, BOUNDS, on_row);
        canvas::Program::<()>::update(&m, &mut st, &up, BOUNDS, on_row);
        assert_eq!(st.look.map(|l| l.0), Some(0));
        assert_eq!(m.row_looks(&st, clock)[0].tick, 0.0);
        for _ in 0..60 {
            frame(&mut st, &m, &mut clock, off);
        }
        assert!((st.lens_y.value - row_y(0)).abs() < 2.0);
        assert_eq!(m.row_looks(&st, clock)[0].tick, 1.0);
        let left = Event::Mouse(mouse::Event::CursorLeft);
        canvas::Program::<()>::update(&m, &mut st, &left, BOUNDS, off);
        for _ in 0..120 {
            frame(&mut st, &m, &mut clock, off);
        }
        assert!(st.look.is_none());
        assert!((st.lens_y.value - row_y(2)).abs() < 4.0);

        let ready = Magnifier {
            status: Status::Ready,
            changed: clock,
            now: clock,
            ..m.clone()
        };
        let mut frames = 0;
        while frame(&mut st, &ready, &mut clock, off) {
            frames += 1;
            assert!(frames < 1000, "never settled");
        }
        assert!(st.rows.iter().all(|r| r.seen.is_none() && r.knob.value == 0.0));
        let secs = frames as f32 * 0.016;
        assert!(secs > BOB_HOLD && secs < BOB_HOLD + BOB_FADE + 2.0, "{secs}");
        assert!((st.lens().x - REST.x).abs() < 0.01 && (st.lens().y - REST.y).abs() < 0.01);
    }

    #[test]
    fn reduced_motion_is_still_but_answers() {
        let _m = anim::forced::set(true);
        let m = Magnifier {
            progress: Some(0.45),
            ..sample()
        };
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        let mut clock = m.changed;
        assert!(!frame(&mut st, &m, &mut clock, off));
        assert!(!frame(&mut st, &m, &mut clock, off));
        let looks = m.row_looks(&st, clock);
        assert_eq!(looks[0].tick, 1.0);
        assert_eq!(st.rows[0].knob.value, 1.0);
        let at = Point::new(150.0 - VIEW_AT.x, row_y(3) - VIEW_AT.y);
        let moved = Event::Mouse(mouse::Event::CursorMoved { position: at });
        let over = mouse::Cursor::Available(at);
        assert!(canvas::Program::<()>::update(&m, &mut st, &moved, BOUNDS, over).is_some());
        assert_eq!(st.live.hover, Some(3));
        assert!(!frame(&mut st, &m, &mut clock, mouse::Cursor::Available(at)));
        assert!((st.lens_y.value - (at.y + VIEW_AT.y)).abs() < 0.01);
    }

    #[test]
    fn a_frame_stays_within_the_stroke_budget() {
        for p in [LIGHT, DARK] {
            let ink = Ink::new(&p, Plate::Surface);
            let rows = [RowLook {
                read: 1.0,
                knob: 0.7,
                tick: 1.0,
                done: 0.5,
            }; N];
            let mut prims = Vec::new();
            monitor_prims(&ink, &mut prims);
            screen_prims(&ink, &rows, &mut prims);
            assert!(prims.len() * 2 + 8 < 150, "{}", prims.len());
        }
    }

    fn switch_fills(ink: &Ink, knob: f32, done: f32) -> Vec<Color> {
        let rows = [RowLook {
            read: 0.0,
            knob,
            tick: 0.0,
            done,
        }; N];
        let mut prims = Vec::new();
        screen_prims(ink, &rows, &mut prims);
        prims
            .iter()
            .filter(|p| {
                p.d.bounds()
                    .is_some_and(|b| b.x >= PILL_X - 0.5 && b.y > row_y(0) - 6.0 && b.y < row_y(0))
            })
            .filter_map(|p| match p.paint {
                Paint::Fill(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn switches_follow_the_colour_rules() {
        for p in [LIGHT, DARK] {
            for plate in [Plate::Bg, Plate::Surface] {
                let ink = Ink::new(&p, plate);
                assert!(switch_fills(&ink, 0.0, 0.0).is_empty());
                assert_eq!(switch_fills(&ink, 1.0, 0.0), vec![ink.tint(ink.accent), ink.accent]);
                let done = switch_fills(&ink, 1.0, 1.0);
                assert_eq!(done.len(), 2);
                let close = |a: Color, b: Color| {
                    (a.r - b.r).abs() < 1e-4 && (a.g - b.g).abs() < 1e-4 && (a.b - b.b).abs() < 1e-4
                };
                assert!(close(done[0], ink.tint(ink.good)), "{:?}", done[0]);
                assert!(close(done[1], ink.good));
            }
        }
    }

    #[test]
    fn a_resting_pointer_lets_the_ready_glass_go_quiet() {
        let _m = anim::forced::set(false);
        let m = Magnifier {
            status: Status::Ready,
            progress: None,
            ..sample()
        };
        let mut st = State::default();
        let mut clock = m.changed;
        let at = Point::new(120.0 - VIEW_AT.x, row_y(0) - VIEW_AT.y);
        let on_row = mouse::Cursor::Available(at);
        frame(&mut st, &m, &mut clock, on_row);
        let moved = Event::Mouse(mouse::Event::CursorMoved { position: at });
        canvas::Program::<()>::update(&m, &mut st, &moved, BOUNDS, on_row);
        assert_eq!(st.live.hover, Some(0));
        let mut frames = 0;
        while frame(&mut st, &m, &mut clock, on_row) {
            frames += 1;
            assert!(frames < 1000, "never settled");
            assert!(m.row_looks(&st, clock).iter().all(|r| r.read == 0.0));
        }
        let secs = frames as f32 * 0.016;
        assert!(secs > BOB_HOLD && secs < BOB_HOLD + BOB_FADE + 2.0, "{secs}");
        assert_eq!(st.live.hover, Some(0));
        assert!(st.tip_on);
        assert!((st.tip_y.value - (row_y(0) - ROW_H / 2.0 * 0.7)).abs() < 0.01);
    }

    #[test]
    fn the_label_glides_and_never_covers_its_row() {
        let _m = anim::forced::set(false);
        let m = sample();
        let mut st = State::default();
        let mut clock = m.changed;
        let row = |i: usize| Point::new(150.0 - VIEW_AT.x, row_y(i) - VIEW_AT.y);
        frame(&mut st, &m, &mut clock, mouse::Cursor::Available(row(1)));
        let moved = Event::Mouse(mouse::Event::CursorMoved { position: row(1) });
        canvas::Program::<()>::update(&m, &mut st, &moved, BOUNDS, mouse::Cursor::Available(row(1)));
        for _ in 0..60 {
            frame(&mut st, &m, &mut clock, mouse::Cursor::Available(row(1)));
        }
        assert!((st.tip_y.value - (st.lens().y - LENS_R - 1.0)).abs() < 0.5);
        let moved = Event::Mouse(mouse::Event::CursorMoved { position: row(2) });
        let over = mouse::Cursor::Available(row(2));
        canvas::Program::<()>::update(&m, &mut st, &moved, BOUNDS, over);
        assert_eq!(st.live.hover, Some(2));
        let mut last = pt(st.tip_x.value, st.tip_y.value);
        for _ in 0..60 {
            frame(&mut st, &m, &mut clock, over);
            let now = pt(st.tip_x.value, st.tip_y.value);
            assert!(now.distance(last) < 4.0, "{last:?} -> {now:?}");
            last = now;
        }
        for k in [COMPACT, 0.8, FULL] {
            let size = Size::new(VIEW.width * k, VIEW.height * k);
            let s = stage_for(size);
            for i in 0..N {
                for y in [row_y(i) - LENS_R - 1.0, row_y(i) - ROW_H / 2.0 * 0.7] {
                    st.tip_y = Spring::new(y);
                    let anchor = s.point(tip_at(&st, &s));
                    let r = super::super::pointer::tooltip_rect(anchor, 90.0, size);
                    assert!(r.y + r.height <= anchor.y + 0.5, "k {k} row {i}: {r:?} {anchor:?}");
                }
            }
        }
    }

    #[test]
    fn reduced_motion_rests_the_ready_glass_in_place() {
        let _m = anim::forced::set(true);
        let m = Magnifier {
            status: Status::Ready,
            progress: None,
            ..sample()
        };
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        let mut clock = m.changed;
        assert!(!frame(&mut st, &m, &mut clock, off));
        assert_eq!(st.lens(), REST);
        clock += Duration::from_secs(20);
        assert!(!frame(&mut st, &m, &mut clock, off));
        assert_eq!(st.lens(), REST);
    }
}
