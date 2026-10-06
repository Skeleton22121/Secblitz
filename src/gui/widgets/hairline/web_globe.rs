//! Web protection: a turning globe sends traffic down to your PC (the
//! prototype's `WEB.globe`).
//!
//! Dots travel along three curved lanes from the globe to a monitor. When
//! web protection is on, a dome stands over the PC: ads, trackers and scam
//! websites stop at it, get a cross stamped on them and fall away, while
//! ordinary web pages go through. When it is off, paused or not working the
//! dome is gone, dashed or flickering, and the ads land on the screen.
//!
//! Colours keep their meaning: the turning globe is the accent (work in
//! progress), the dome and its tick are good when protection is on, ads and
//! trackers are warn, scam sites bad, web pages grey. A paused dome is grey
//! and dashed, a dome that is getting its lists ready is the accent and
//! dashed, a broken one is warn and flickers.
//!
//! The person can drag the globe to spin it (the push eases back), hover any
//! part or dot to see its name, and click a flying ad, tracker or scam site
//! to block it by hand while protection is on.
//!
//! # Frames
//!
//! The traffic is illustrative, so it does not run forever: it flows for
//! [`AWAKE_FOR`] seconds after the page shows the drawing, after the state
//! changes and after the pointer moves over it, then the last dots finish
//! their trip, the globe coasts to a stop and frames stop. Real blocks keep
//! it honest: when the page's blocked counts go up while protection is on,
//! that many ads, trackers or scam sites (a few at most) fly in and get
//! stopped, even while the rest of the drawing rests.
//!
//! Under reduced motion it is one still picture per state (one ad stopped at
//! the dome, or one sitting on the screen); hover names, clicks and dragging
//! the globe still work, without any motion of their own.
use super::motion::{lerp, phase, Spring};
use super::parts::monitor;
use super::pointer::{Area, Gesture, Hotspots, Layer, Spot};
use super::stage::{pt, stroke, tint, Ink, Plate, Stage, W_ACCENT, W_LINE, W_PART, W_THICK};
use super::svg::{PathData, Seg};
use super::{Glyph, Live};
use crate::gui::theme::{self, Palette};
use crate::gui::widgets::anim::{self, DECELERATE, STANDARD};
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path, Text};
use iced::widget::text::{LineHeight, Shaping};
use iced::{mouse, Color, Element, Length, Pixels, Point, Rectangle, Renderer, Size, Theme};
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::sync::LazyLock;
use std::time::Instant;

/// Size on the page, logical pixels (the prototype's 320 by 256 box at
/// 0.78).
pub const SIZE: Size = Size::new(250.0, 200.0);
/// The drawing's own box (the prototype's).
const UNITS: Size = Size::new(320.0, 256.0);

/// The globe.
const GC: Point = pt(86.0, 82.0);
const GR: f32 = 46.0;
/// The monitor: centre, top, panel size.
const MON_CX: f32 = 236.0;
const MON_TOP: f32 = 150.0;
const MON_W: f32 = 104.0;
const MON_H: f32 = 66.0;
/// Middle of the screen, where every lane ends.
const SCR: Point = pt(MON_CX, MON_TOP + MON_H / 2.0);
/// The dome: the top half of an ellipse standing on the desk.
const DC: Point = pt(236.0, 238.0);
const DRX: f32 = 78.0;
const DRY: f32 = 116.0;
/// The small shield on top of the dome.
const CREST: Point = pt(DC.x, DC.y - DRY - 0.2);
const CREST_SIZE: f32 = 21.6;
/// Three quadratic lanes: start (at the globe), control, end (the screen).
const LANES: [[Point; 3]; 3] = [
    [pt(120.0, 100.0), pt(180.0, 70.0), SCR],
    [pt(118.0, 70.0), pt(200.0, 30.0), SCR],
    [pt(108.0, 120.0), pt(150.0, 170.0), SCR],
];

/// Lane travelled per second (fraction of the lane).
const SPEED: f32 = 0.42;
/// The globe's resting spin, radians per second.
const SPIN: f32 = 0.5;
/// Spin added per unit dragged sideways (the prototype's 0.06).
const DRAG_SPIN: f32 = 0.06;
/// Fastest spin a drag can give.
const MAX_SPIN: f32 = 8.0;
/// Below this the coasting globe stops.
const SPIN_REST: f32 = 0.01;
/// Spin added by a click on the globe.
const CLICK_SPIN: f32 = 2.0;
/// Ambient second the still picture shows (the prototype's `still`).
const STILL: f32 = 0.4;
/// Seconds of flowing traffic after a wake.
pub const AWAKE_FOR: f32 = 12.0;
/// At most this many dots at once.
const MAX_ITEMS: usize = 8;
/// Real blocks shown per kind for one poll, and waiting in all.
const REAL_PER_KIND: u64 = 2;
const MAX_QUEUE: usize = 5;
/// When the state change's dome transition is over.
const TRANSITION_END: f32 = 0.9;
/// Seconds after a block when the cross is fully stamped; the fall starts
/// at [`FALL_AT`].
const STAMP: f32 = 0.25;
const FALL_AT: f32 = 0.35;
/// How long a dome flash lasts.
const FLASH_LIFE: f32 = 0.6;

/// What the drawing shows; the page maps its status line to this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    /// Blocking: the dome stands and stops ads, trackers and scam sites.
    On,
    /// Turned on, block lists not ready yet: a dashed accent dome, nothing
    /// stopped yet.
    Starting,
    /// Paused for an hour: a dashed grey dome, traffic slowed.
    Paused,
    /// Every switch off: no dome.
    Off,
    /// Should block but is not working: a flickering warn dome.
    Broken,
}

impl Guard {
    pub fn blocks(self) -> bool {
        self == Guard::On
    }
    /// Paused traffic drifts at under half speed.
    fn speed(self) -> f32 {
        if self == Guard::Paused {
            0.45
        } else {
            1.0
        }
    }
}

/// The hover names and the ad's mark, translated by the page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Labels {
    pub ad: String,
    pub tracker: String,
    pub scam: String,
    pub page: String,
    pub pc: String,
    pub internet: String,
    /// The dome when protection is on.
    pub on: String,
    /// The dome while the block lists get ready.
    pub starting: String,
    /// The dome when paused or not working.
    pub not_blocking: String,
    /// The short word printed on a flying ad.
    pub ad_mark: String,
}

impl Labels {
    /// Every name through `t` (the page passes `|k| ctx.t(k)`).
    pub fn new(t: impl Fn(&str) -> String) -> Labels {
        Labels {
            ad: t("Ad"),
            tracker: t("Tracker"),
            scam: t("Scam website"),
            page: t("Web page"),
            pc: t("Your PC"),
            internet: t("The internet"),
            on: t("Web protection is on"),
            starting: t("Getting block lists ready"),
            not_blocking: t("Web protection is not blocking"),
            ad_mark: t("AD"),
        }
    }
}

/// What travels down a lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Page,
    Ad,
    Tracker,
    Scam,
}

impl Kind {
    /// The blocked counts' order: ads, trackers, dangerous websites.
    const COUNTED: [Kind; 3] = [Kind::Ad, Kind::Tracker, Kind::Scam];

    fn blockable(self) -> bool {
        self != Kind::Page
    }
    /// Size against the prototype's shapes (a little larger than its 0.8
    /// and 0.7, so the ad's mark stays readable at 250 px).
    fn scale(self) -> f32 {
        if self == Kind::Page {
            0.75
        } else {
            0.9
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Travelling down its lane.
    Go,
    /// Stopped by the dome (or by hand): stamped, then falls.
    Blocked,
    /// Arrived at the PC: fades into the screen.
    In,
    /// Not protected: an ad that reached the PC sits on the screen a moment.
    Landed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Item {
    id: u32,
    kind: Kind,
    lane: usize,
    /// How far down its lane, 0..=1.
    u: f32,
    phase: Phase,
    at: Point,
    /// Seconds this item has lived (scene time).
    life: f32,
    /// `life` when the current phase began.
    since: f32,
    /// Falling speed once blocked.
    vy: f32,
    op: f32,
    /// 0..=1, how hovered it is (it grows and slows under the pointer).
    grow: f32,
    /// Which place on the screen a landed ad takes.
    slot: u8,
}

impl Item {
    fn new(id: u32, kind: Kind, lane: usize, u: f32) -> Item {
        Item {
            id,
            kind,
            lane,
            u,
            phase: Phase::Go,
            at: lane_point(lane, u),
            life: 0.0,
            since: 0.0,
            vy: 0.0,
            op: 1.0,
            grow: 0.0,
            slot: 0,
        }
    }

    /// How much of the cross is stamped on it, 0..=1.
    fn stamp(&self) -> f32 {
        if self.phase == Phase::Blocked {
            ((self.life - self.since) / STAMP).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn hit_radius(&self) -> f32 {
        12.0 * self.kind.scale() * (1.0 + 0.3 * self.grow)
    }
}

/// What a step did to an item that the scene must answer.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Outcome {
    Nothing,
    /// Stopped at the dome here.
    Blocked(Point),
    /// Reached the screen unprotected: give it a place.
    Landed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Flash {
    /// Scene clock when it began.
    at: f32,
    /// Where on the dome (ellipse angle).
    angle: f32,
}

const NO_FLASH: Flash = Flash {
    at: f32::NEG_INFINITY,
    angle: 0.0,
};

/// The parts a person can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Globe,
    Pc,
    Dome,
    Item(u32),
}

/// The canvas state: pointer and clock, plus the traffic.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    live: Live<Part>,
    guard: Option<Guard>,
    prev: Option<Guard>,
    items: Vec<Item>,
    next_id: u32,
    next_spawn: f32,
    rng: u32,
    /// Globe turn, radians, and its speed.
    spin: f32,
    vspin: f32,
    /// Scene seconds the lanes have flowed (slowed while paused, stopped
    /// while resting).
    flow: f32,
    /// Scene seconds, for flashes and the broken flicker.
    clock: f32,
    /// Seconds since the last wake.
    quiet: f32,
    /// 1 while awake, eases to 0 at rest (fades the flicker and the march).
    level: Spring,
    flashes: [Flash; 4],
    /// The page's blocked counts last seen.
    counts: Option<[u64; 3]>,
    /// Real blocks waiting to fly in.
    queue: Vec<Kind>,
    landed: u8,
    /// A drag that began on the globe is spinning it.
    spinning: bool,
    /// The items are the reduced-motion still picture.
    still: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            live: Live::default(),
            guard: None,
            prev: None,
            items: Vec::new(),
            next_id: 0,
            next_spawn: 0.3,
            rng: seed(),
            spin: STILL * SPIN,
            vspin: SPIN,
            flow: 0.0,
            clock: 0.0,
            quiet: 0.0,
            level: Spring::with(1.0, 20.0, 9.0),
            flashes: [NO_FLASH; 4],
            counts: None,
            queue: Vec::new(),
            landed: 0,
            spinning: false,
            still: false,
        }
    }
}

/// A different stream of traffic each time the drawing appears (never 0,
/// which xorshift cannot leave).
fn seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0x9E37_79B9)
        | 1
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

fn quad(l: &[Point; 3], u: f32) -> Point {
    let v = 1.0 - u;
    pt(
        v * v * l[0].x + 2.0 * v * u * l[1].x + u * u * l[2].x,
        v * v * l[0].y + 2.0 * v * u * l[1].y + u * u * l[2].y,
    )
}

fn lane_point(lane: usize, u: f32) -> Point {
    quad(&LANES[lane % LANES.len()], u.clamp(0.0, 1.0))
}

/// Inside the dome (the half ellipse over the desk).
fn in_dome(p: Point) -> bool {
    let (ex, ey) = ((p.x - DC.x) / DRX, (p.y - DC.y) / DRY);
    ex * ex + ey * ey < 1.0 && p.y <= DC.y
}

/// The ellipse angle of the dome point nearest the direction of `p`.
fn dome_angle(p: Point) -> f32 {
    ((p.y - DC.y) / DRY).atan2((p.x - DC.x) / DRX)
}

/// Where `lane` first enters the dome, as a lane fraction.
fn dome_entry(lane: usize) -> f32 {
    (0..=400)
        .map(|i| i as f32 / 400.0)
        .find(|u| in_dome(lane_point(lane, *u)))
        .unwrap_or(1.0)
}

/// Where a landed ad sits on the screen.
fn landed_spot(slot: u8) -> Point {
    let s = f32::from(slot % 3);
    pt(SCR.x + (s - 1.0) * 22.0, SCR.y + (f32::from(slot % 2) * 8.0 - 4.0))
}

/// Points along the dome's arc between two ellipse angles.
fn dome_arc(rx: f32, ry: f32, a0: f32, a1: f32, n: usize) -> Vec<Point> {
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            pt(DC.x + rx * a.cos(), DC.y + ry * a.sin())
        })
        .collect()
}

fn polyline_data(pts: &[Point]) -> PathData {
    PathData {
        segs: pts
            .iter()
            .enumerate()
            .map(|(i, p)| if i == 0 { Seg::Move(*p) } else { Seg::Line(*p) })
            .collect(),
    }
}

/// Dashes along a polyline (units): `dash` drawn, `gap` left out, the whole
/// pattern moved forward by `shift`. Built as one path by hand because the
/// two renderers read a stroke's dash offset differently.
fn dashed(s: &Stage, pts: &[Point], dash: f32, gap: f32, shift: f32) -> Path {
    let mut cum = Vec::with_capacity(pts.len());
    let mut total = 0.0;
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            total += pts[i - 1].distance(*p);
        }
        cum.push(total);
    }
    let at = |d: f32| -> Point {
        let i = cum.partition_point(|c| *c < d).clamp(1, pts.len() - 1);
        let (a, b) = (pts[i - 1], pts[i]);
        let len = cum[i] - cum[i - 1];
        let t = if len > 0.0 { (d - cum[i - 1]) / len } else { 0.0 };
        pt(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
    };
    let period = dash + gap;
    s.path(|b| {
        if pts.len() < 2 || period <= 0.0 {
            return;
        }
        let mut d0 = shift.rem_euclid(period) - period;
        while d0 < total {
            let (lo, hi) = (d0.max(0.0), (d0 + dash).min(total));
            if hi > lo {
                b.move_to(at(lo));
                for (i, c) in cum.iter().enumerate() {
                    if *c > lo && *c < hi {
                        b.line_to(pts[i]);
                    }
                }
                b.line_to(at(hi));
            }
            d0 += period;
        }
    })
}

/// A seeded wobble so a broken dome flickers without timers (prototype).
fn flicker(t: f32) -> f32 {
    0.5 + 0.5 * (t * 23.0).sin() * (t * 7.3 + 1.0).sin() * (t * 3.1).sin()
}

static STAMP_PATH: LazyLock<PathData> = LazyLock::new(|| PathData::of("M-7 -7l14 14M7 -7l-14 14"));
/// Pause bars inside the crest's 24-unit shield box.
static PAUSE_PATH: LazyLock<PathData> = LazyLock::new(|| PathData::of("M10 9.2v5.4M14 9.2v5.4"));

// ---------------------------------------------------------------------------
// The scene
// ---------------------------------------------------------------------------

/// The still picture for reduced motion: a web page on its way, and an ad
/// stopped at the dome (on) or sitting on the screen (not blocking).
fn still_items(guard: Guard) -> Vec<Item> {
    let page = Item::new(0, Kind::Page, 1, 0.35);
    if guard.blocks() {
        // Lane 0 meets the dome clear of the crest and the screen.
        let mut ad = Item::new(1, Kind::Ad, 0, dome_entry(0));
        ad.phase = Phase::Blocked;
        ad.since = -0.3;
        vec![page, ad, Item::new(2, Kind::Tracker, 2, 0.3)]
    } else {
        let mut ad = Item::new(1, Kind::Ad, 0, 1.0);
        ad.phase = Phase::Landed;
        ad.since = -0.5;
        ad.at = landed_spot(0);
        vec![page, ad, Item::new(2, Kind::Tracker, 2, 0.4)]
    }
}

/// One item's step through `sdt` scene seconds.
fn step_item(it: &mut Item, sdt: f32, blocks: bool) -> Outcome {
    it.life += sdt;
    match it.phase {
        Phase::Go => {
            it.u += sdt * SPEED * (1.0 - 0.8 * it.grow);
            it.at = lane_point(it.lane, it.u);
            if blocks && it.kind.blockable() && in_dome(it.at) {
                it.phase = Phase::Blocked;
                it.since = it.life;
                return Outcome::Blocked(it.at);
            }
            if it.u >= 1.0 {
                it.since = it.life;
                if !blocks && it.kind.blockable() {
                    it.phase = Phase::Landed;
                    return Outcome::Landed;
                }
                it.phase = Phase::In;
            }
        }
        Phase::Blocked => {
            let k = it.life - it.since;
            if k > FALL_AT {
                it.vy += 380.0 * sdt;
                it.at.y += it.vy * sdt;
                it.at.x -= 12.0 * sdt;
                it.op = 1.0 - ((k - FALL_AT) / 0.6).clamp(0.0, 1.0);
            }
        }
        Phase::In => it.op = 1.0 - ((it.life - it.since) / 0.25).clamp(0.0, 1.0),
        Phase::Landed => {
            let k = it.life - it.since;
            let e = DECELERATE.at(k / 0.3);
            let to = landed_spot(it.slot);
            it.at = pt(lerp(SCR.x, to.x, e), lerp(SCR.y, to.y, e));
            it.op = 1.0 - ((k - 1.6) / 0.4).clamp(0.0, 1.0);
        }
    }
    Outcome::Nothing
}

impl State {
    fn rand(&mut self) -> f32 {
        // xorshift32: the same traffic every run, no dependency.
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn random_kind(&mut self) -> Kind {
        let r = self.rand();
        if r < 0.45 {
            Kind::Page
        } else if r < 0.73 {
            Kind::Ad
        } else if r < 0.93 {
            Kind::Tracker
        } else {
            Kind::Scam
        }
    }

    fn spawn(&mut self, kind: Kind, u: f32) {
        let lane = ((self.rand() * LANES.len() as f32) as usize).min(LANES.len() - 1);
        self.items.push(Item::new(self.next_id, kind, lane, u));
        self.next_id = self.next_id.wrapping_add(1);
    }

    fn wake(&mut self) {
        self.quiet = 0.0;
    }

    fn awake(&self) -> bool {
        self.quiet < AWAKE_FOR
    }

    fn flash(&mut self, toward: Point) {
        if anim::reduced() {
            return;
        }
        let oldest = (0..self.flashes.len())
            .min_by(|a, b| self.flashes[*a].at.total_cmp(&self.flashes[*b].at))
            .unwrap_or(0);
        self.flashes[oldest] = Flash {
            at: self.clock,
            angle: dome_angle(toward),
        };
    }

    fn flashing(&self) -> bool {
        self.flashes
            .iter()
            .any(|f| self.clock - f.at < FLASH_LIFE)
    }

    /// A new state (or a new canvas): remember the old one for the
    /// transition, wake up, and set the scene.
    fn enter(&mut self, guard: Guard, reduced: bool) {
        let first = self.guard.is_none();
        self.prev = self.guard;
        self.guard = Some(guard);
        self.wake();
        if !guard.blocks() {
            self.queue.clear();
        }
        if reduced {
            self.items = still_items(guard);
            self.still = true;
            self.spin = STILL * SPIN;
        } else if first {
            // The prototype starts with three on their way.
            for u in [0.15, 0.45, 0.7] {
                let kind = self.random_kind();
                self.spawn(kind, u);
            }
        } else if guard.blocks() && self.prev != Some(Guard::On) {
            self.live.pulses.push(CREST, true);
        }
    }

    /// Note the page's blocked counts; real blocks while protection is on
    /// queue that many (a few at most) to fly in and be stopped.
    fn take_counts(&mut self, counts: Option<[u64; 3]>, guard: Guard, reduced: bool) {
        let Some(new) = counts else {
            self.counts = None;
            return;
        };
        if let Some(old) = self.counts {
            if guard.blocks() && !reduced {
                for (i, kind) in Kind::COUNTED.iter().enumerate() {
                    let more = new[i].saturating_sub(old[i]).min(REAL_PER_KIND);
                    for _ in 0..more {
                        if self.queue.len() < MAX_QUEUE {
                            self.queue.push(*kind);
                        }
                    }
                }
            }
        }
        self.counts = Some(new);
    }

    /// One frame of `dt` seconds.
    fn advance(&mut self, dt: f32, guard: Guard, hover: Option<Part>) {
        let sdt = dt * guard.speed();
        self.clock += dt;
        self.quiet += dt;
        let awake = self.awake();
        self.level.aim(if awake { 1.0 } else { 0.0 });
        self.level.tick(dt);
        let rest = if awake { SPIN } else { 0.0 };
        self.vspin = lerp(self.vspin, rest, (dt * 1.2).min(1.0));
        if !awake && self.vspin.abs() < SPIN_REST {
            self.vspin = 0.0;
        }
        self.spin = (self.spin + self.vspin * sdt).rem_euclid(TAU);
        self.flow += sdt * self.level.value.clamp(0.0, 1.0);

        if awake || !self.queue.is_empty() {
            self.next_spawn -= sdt;
            if self.next_spawn <= 0.0 && self.items.len() < MAX_ITEMS {
                self.next_spawn = 0.45 + self.rand() * 0.35;
                let kind = if self.queue.is_empty() {
                    self.random_kind()
                } else {
                    self.queue.remove(0)
                };
                self.spawn(kind, 0.0);
            }
        }

        let grow_k = (dt * 14.0).min(1.0);
        for i in 0..self.items.len() {
            let it = &mut self.items[i];
            let hovered = hover == Some(Part::Item(it.id));
            it.grow = lerp(it.grow, if hovered { 1.0 } else { 0.0 }, grow_k);
            match step_item(it, sdt, guard.blocks()) {
                Outcome::Nothing => {}
                Outcome::Blocked(at) => self.flash(at),
                Outcome::Landed => {
                    self.items[i].slot = self.landed % 3;
                    self.landed = self.landed.wrapping_add(1);
                }
            }
        }
        self.items
            .retain(|it| it.op > 0.0 && it.at.y < UNITS.height + 40.0);
    }

    /// The reduced-motion answer to a hover: grow at once.
    fn snap_grow(&mut self, hover: Option<Part>) {
        for it in &mut self.items {
            it.grow = if hover == Some(Part::Item(it.id)) { 1.0 } else { 0.0 };
        }
    }

    fn item(&self, id: u32) -> Option<&Item> {
        self.items.iter().find(|it| it.id == id)
    }

    /// A click on `id` while protection is on blocks it by hand.
    fn block_by_hand(&mut self, id: u32, reduced: bool) -> bool {
        let Some(it) = self.items.iter_mut().find(|it| it.id == id) else {
            return false;
        };
        if !(it.kind.blockable() && it.phase == Phase::Go) {
            return false;
        }
        it.phase = Phase::Blocked;
        // Reduced motion: the cross is there at once and it stays put.
        it.since = if reduced { it.life - 0.3 } else { it.life };
        let at = it.at;
        self.flash(at);
        self.live.pulses.push(at, false);
        true
    }
}

// ---------------------------------------------------------------------------
// The program
// ---------------------------------------------------------------------------

struct WebGlobe {
    p: Palette,
    plate: Plate,
    guard: Guard,
    changed: Instant,
    now: Instant,
    blocked: Option<[u64; 3]>,
    labels: Labels,
}

/// The drawing, [`SIZE`] on the page. `changed` is when `guard` began,
/// `now` the page's last frame time (or `changed`), `blocked` today's real
/// counts (ads, trackers, dangerous websites) when the page has them.
#[allow(clippy::too_many_arguments)]
pub fn web_globe<'a, M: 'a>(
    p: Palette,
    plate: Plate,
    guard: Guard,
    changed: Instant,
    now: Instant,
    blocked: Option<[u64; 3]>,
    labels: Labels,
) -> Element<'a, M> {
    canvas::Canvas::new(WebGlobe {
        p,
        plate,
        guard,
        changed,
        now,
        blocked,
        labels,
    })
    .width(Length::Fixed(SIZE.width))
    .height(Length::Fixed(SIZE.height))
    .into()
}

/// The parts that can be pointed at, from the state (used by both update
/// and draw).
fn spots(guard: Guard, st: &State) -> Hotspots<Part> {
    let mut h = Hotspots::new()
        .circle(Part::Globe, GC, GR, Layer::Mid)
        .circle(Part::Pc, SCR, 26.0, Layer::Mid);
    if guard != Guard::Off {
        h = h.circle(Part::Dome, CREST, 14.0, Layer::Front);
    }
    for it in &st.items {
        if it.op > 0.4 && it.phase != Phase::In {
            h.push(Spot {
                id: Part::Item(it.id),
                area: Area::Circle {
                    centre: it.at,
                    r: it.hit_radius(),
                },
                layer: Layer::Front,
            });
        }
    }
    h
}

impl WebGlobe {
    fn label(&self, st: &State, part: Part) -> String {
        let l = &self.labels;
        match part {
            Part::Globe => l.internet.clone(),
            Part::Pc => l.pc.clone(),
            Part::Dome => match self.guard {
                Guard::On => l.on.clone(),
                Guard::Starting => l.starting.clone(),
                _ => l.not_blocking.clone(),
            },
            Part::Item(id) => match st.item(id).map(|it| it.kind) {
                Some(Kind::Ad) => l.ad.clone(),
                Some(Kind::Tracker) => l.tracker.clone(),
                Some(Kind::Scam) => l.scam.clone(),
                Some(Kind::Page) => l.page.clone(),
                None => String::new(),
            },
        }
    }

    fn busy(&self, st: &State) -> bool {
        if anim::reduced() {
            return false;
        }
        st.awake()
            || !st.items.is_empty()
            || !st.queue.is_empty()
            || st.vspin != 0.0
            || st.level.moving()
            || st.flashing()
            || st.live.age(self.changed, self.now) < TRANSITION_END
    }

    fn click(&self, st: &mut State, at: Point, reduced: bool) {
        st.wake();
        match st.live.hover {
            Some(Part::Item(id)) => {
                if !(self.guard.blocks() && st.block_by_hand(id, reduced)) {
                    st.live.pulses.push(at, false);
                }
            }
            Some(Part::Globe) => {
                if reduced {
                    st.spin = (st.spin + 0.5).rem_euclid(TAU);
                } else {
                    st.vspin = (st.vspin + CLICK_SPIN).min(MAX_SPIN);
                }
            }
            Some(Part::Dome) => st.live.pulses.push(CREST, true),
            _ => st.live.pulses.push(at, false),
        }
    }

    /// Whether a click on the hovered part does something of its own.
    fn clickable(&self, st: &State) -> bool {
        match st.live.hover {
            Some(Part::Item(id)) => {
                self.guard.blocks()
                    && st
                        .item(id)
                        .is_some_and(|it| it.kind.blockable() && it.phase == Phase::Go)
            }
            _ => false,
        }
    }
}

impl<M> canvas::Program<M> for WebGlobe {
    type State = State;

    fn update(
        &self,
        st: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        let reduced = anim::reduced();
        if st.live.fresh(self.changed) {
            st.enter(self.guard, reduced);
        }
        if reduced && !st.still {
            st.items = still_items(self.guard);
            st.still = true;
        } else if !reduced && st.still {
            st.still = false;
        }
        st.take_counts(self.blocked, self.guard, reduced);
        let stage = Stage::fit(UNITS, bounds.size());
        let step = st.live.update(event, bounds, cursor, &stage, &spots(self.guard, st));
        let mut dirty = false;
        if let Some(dt) = step.dt {
            if !reduced {
                st.advance(dt, self.guard, st.live.hover);
            }
        }
        if reduced {
            let before: Vec<f32> = st.items.iter().map(|it| it.grow).collect();
            st.snap_grow(st.live.hover);
            dirty |= st.items.iter().map(|it| it.grow).ne(before);
        }
        match step.gesture {
            Some(Gesture::Hover) if st.live.pointer.inside => st.wake(),
            Some(Gesture::Press(_)) => {
                st.spinning = st.live.hover == Some(Part::Globe);
                st.wake();
            }
            Some(Gesture::Drag { delta, .. }) if st.spinning => {
                if reduced {
                    // Direct: the globe's face follows the pointer.
                    st.spin = (st.spin + delta.x / GR).rem_euclid(TAU);
                } else {
                    st.vspin = (st.vspin + delta.x * DRAG_SPIN).clamp(-MAX_SPIN, MAX_SPIN);
                }
                st.wake();
                dirty = true;
            }
            _ => {}
        }
        if let Some(at) = step.click() {
            self.click(st, at, reduced);
            dirty = true;
        }
        if matches!(step.gesture, Some(Gesture::Release { .. })) {
            st.spinning = false;
        }
        st.live.redraw(&step, dirty || self.busy(st))
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
        let back = st.live.layer(&stage, Layer::Back);
        let mid = st.live.layer(&stage, Layer::Mid);
        let front = st.live.layer(&stage, Layer::Front);

        let domes = dome_states(self.guard, st.prev, age);

        // Lanes: dashes that flow towards the PC.
        for l in &LANES {
            let pts: Vec<Point> = (0..=24).map(|i| quad(l, i as f32 / 24.0)).collect();
            f.stroke(&dashed(&back, &pts, 2.0, 5.0, st.flow * 10.0), ink.lo());
        }

        // The dome's tint sits behind the PC, so the PC stays plain.
        for d in &domes {
            if d.guard == Guard::On && d.fill > 0.0 {
                let mut pts = dome_arc(DRX - 6.0, DRY - 6.0, PI, TAU, 40);
                pts.push(pts[0]);
                f.fill(
                    &front.polyline(&pts, true),
                    tint(ink.good, ink.plate).scale_alpha(d.fill * d.weight),
                );
            }
        }

        draw_globe(&mut f, &mid, &ink, st.spin);
        monitor(&mut f, &mid, &ink, MON_CX, MON_TOP, MON_W, MON_H, ink.plate, 1.0);

        for d in &domes {
            draw_dome(&mut f, &front, &ink, d, st);
        }
        for fl in &st.flashes {
            let k = (st.clock - fl.at) / FLASH_LIFE;
            if (0.0..1.0).contains(&k) {
                let pts = dome_arc(DRX, DRY, fl.angle - 0.2, fl.angle + 0.2, 10);
                f.stroke(
                    &front.polyline(&pts, false),
                    stroke(ink.good.scale_alpha(1.0 - k), W_THICK),
                );
            }
        }

        for it in &st.items {
            draw_item(&mut f, &front, &ink, it, &self.labels.ad_mark);
        }

        // Pulses take the dome's colour (grey with no dome).
        let pulse = dome_look(self.guard, &ink).map_or(ink.line, |l| l.0);
        st.live.pulses.draw(&mut f, &stage, pulse);
        st.live.draw_tooltip(&mut f, &self.p, &stage, &spots(self.guard, st), |id| {
            self.label(st, id)
        });
        vec![f.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        st: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if st.spinning && st.live.pointer.pressed {
            return mouse::Interaction::Grabbing;
        }
        if !cursor.is_over(bounds) {
            return mouse::Interaction::None;
        }
        if st.live.hover == Some(Part::Globe) {
            mouse::Interaction::Grab
        } else if self.clickable(st) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/// What sits on the crest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrestMark {
    Tick,
    Pause,
    Excl,
}

/// One dome to draw this frame: during a change the old one fades out
/// while the new one comes in.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DomeDraw {
    guard: Guard,
    /// Fade weight, 0..=1.
    weight: f32,
    /// How much of the outline has drawn in.
    outline: f32,
    /// The tint inside (on only).
    fill: f32,
    /// The crest's mark drawn in.
    mark: f32,
}

/// The domes for `guard`, `age` seconds after it replaced `prev`. Turning
/// on draws the dome in from the left and the tick after it; anything else
/// cross-fades over 0.4 s.
fn dome_states(guard: Guard, prev: Option<Guard>, age: f32) -> Vec<DomeDraw> {
    let p = phase(age, 0.0, 0.4, STANDARD);
    let mut out = Vec::with_capacity(2);
    if let Some(old) = prev.filter(|g| *g != guard && *g != Guard::Off) {
        if p < 1.0 {
            out.push(DomeDraw {
                guard: old,
                weight: 1.0 - p,
                outline: 1.0,
                fill: 1.0,
                mark: 1.0,
            });
        }
    }
    match guard {
        Guard::Off => {}
        Guard::On => out.push(DomeDraw {
            guard,
            weight: phase(age, 0.0, 0.15, STANDARD),
            outline: phase(age, 0.0, 0.7, DECELERATE),
            fill: phase(age, 0.15, 0.7, STANDARD),
            mark: phase(age, 0.45, 0.85, DECELERATE),
        }),
        _ => out.push(DomeDraw {
            guard,
            weight: p,
            outline: 1.0,
            fill: 0.0,
            mark: 1.0,
        }),
    }
    out
}

/// The dome's colour, dashes and mark for a state; `None` when off.
fn dome_look(guard: Guard, ink: &Ink) -> Option<(Color, bool, CrestMark)> {
    match guard {
        Guard::On => Some((ink.good, false, CrestMark::Tick)),
        Guard::Starting => Some((ink.accent, true, CrestMark::Tick)),
        Guard::Paused => Some((ink.line, true, CrestMark::Pause)),
        Guard::Broken => Some((ink.warn, false, CrestMark::Excl)),
        Guard::Off => None,
    }
}

fn draw_dome(f: &mut Frame, s: &Stage, ink: &Ink, d: &DomeDraw, st: &State) {
    let Some((color, dashes, mark)) = dome_look(d.guard, ink) else {
        return;
    };
    let mut alpha = d.weight;
    if d.guard == Guard::Broken {
        // Flickers while awake, rests at a steady dim when quiet.
        let calm = if anim::reduced() { 0.0 } else { st.level.value.clamp(0.0, 1.0) };
        alpha *= lerp(0.7, 0.2 + 0.8 * flicker(st.clock), calm);
    }
    if alpha <= 0.004 {
        return;
    }
    let c = color.scale_alpha(alpha);
    let pts = dome_arc(DRX, DRY, PI, TAU, 48);
    if dashes {
        // Getting ready marches along; paused stands still.
        let shift = if d.guard == Guard::Starting { st.flow * 8.0 } else { 0.0 };
        f.stroke(&dashed(s, &pts, 4.0, 6.0, shift), stroke(c, W_ACCENT));
    } else if d.outline > 0.001 {
        f.stroke(&s.shape(&polyline_data(&pts).partial(d.outline)), stroke(c, W_ACCENT));
    }

    // The crest: a small shield on top, with what the dome is doing.
    let crest_a = alpha * if d.guard == Guard::On { (d.outline * 2.5).min(1.0) } else { 1.0 };
    if crest_a <= 0.004 {
        return;
    }
    let shield = Glyph::Shield.data().placed(CREST, CREST_SIZE);
    let path = s.shape(&shield);
    f.fill(&path, tint(color, ink.plate).scale_alpha(crest_a));
    f.stroke(&path, stroke(color.scale_alpha(crest_a), W_LINE));
    if d.guard == Guard::Starting {
        // Not ready yet: no mark until it is.
        return;
    }
    let (data, amount) = match mark {
        CrestMark::Tick => (Glyph::Tick.data().placed(CREST, CREST_SIZE), d.mark),
        CrestMark::Pause => (PAUSE_PATH.placed(CREST, CREST_SIZE), 1.0),
        CrestMark::Excl => (Glyph::Excl.data().placed(CREST, CREST_SIZE), 1.0),
    };
    if amount > 0.001 {
        f.stroke(
            &s.shape(&data.partial(amount)),
            stroke(color.scale_alpha(crest_a), W_ACCENT),
        );
    }
}

/// The globe: a disc, three latitudes and six great circles whose facing
/// halves turn with `spin` (one path for all the grid lines).
fn draw_globe(f: &mut Frame, s: &Stage, ink: &Ink, spin: f32) {
    let c = ink.accent;
    let disc = s.circle(GC, GR);
    f.fill(&disc, tint(c, ink.plate));
    f.stroke(&disc, stroke(c, W_PART));
    let grid = s.path(|b| {
        for lat in [-0.5f32, 0.0, 0.5] {
            let k = (1.0 - lat * lat).sqrt();
            let centre = pt(GC.x, GC.y + GR * lat);
            b.move_to(pt(centre.x + GR * k, centre.y));
            b.arc(centre, GR * k, 7.0 * k, 0.0, TAU);
        }
        for m in 0..6 {
            let phi = spin + m as f32 * PI / 6.0;
            let (sn, cs) = phi.sin_cos();
            let rx = (sn.abs() * GR).max(0.01);
            if sn * cs > 0.0 {
                b.move_to(pt(GC.x, GC.y - GR));
                b.arc(GC, rx, GR, -FRAC_PI_2, FRAC_PI_2);
            } else {
                b.move_to(pt(GC.x, GC.y + GR));
                b.arc(GC, rx, GR, FRAC_PI_2, PI + FRAC_PI_2);
            }
        }
    });
    f.stroke(&grid, stroke(c.scale_alpha(0.55), W_PART));
}

fn kind_color(kind: Kind, ink: &Ink) -> Color {
    match kind {
        Kind::Ad | Kind::Tracker => ink.warn,
        Kind::Scam => ink.bad,
        Kind::Page => ink.line,
    }
}

/// One travelling thing (the prototype's `thing()`), centred on its point.
fn draw_item(f: &mut Frame, front: &Stage, ink: &Ink, it: &Item, ad_mark: &str) {
    if it.op <= 0.004 {
        return;
    }
    let scale = it.kind.scale() * (1.0 + 0.3 * it.grow);
    let l = front.local(it.at, scale);
    let a = it.op;
    let c = kind_color(it.kind, ink);
    let line = stroke(c.scale_alpha(a), W_PART);
    let fill = tint(c, ink.plate).scale_alpha(a);
    let body = match it.kind {
        Kind::Ad => l.rounded_rect(-13.0, -9.0, 26.0, 18.0, 3.5),
        Kind::Tracker => l.circle(pt(0.0, 0.0), 9.0),
        Kind::Scam => l.rounded_rect(-10.0, -12.0, 20.0, 24.0, 3.0),
        Kind::Page => l.rounded_rect(-10.0, -10.0, 20.0, 20.0, 5.0),
    };
    f.fill(&body, fill);
    f.stroke(&body, line);
    match it.kind {
        Kind::Ad => f.fill_text(Text {
            content: ad_mark.to_string(),
            position: l.px(0.0, 0.4),
            color: c.scale_alpha(a),
            size: Pixels(l.len(9.5)),
            line_height: LineHeight::Relative(1.0),
            font: theme::SEMIBOLD,
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            shaping: Shaping::Advanced,
            ..Text::default()
        }),
        Kind::Tracker => f.stroke(&l.icon(Glyph::Eye, pt(0.0, 0.0), 13.0), line),
        Kind::Scam => f.stroke(&l.icon(Glyph::Warn, pt(0.0, 1.0), 13.0), line),
        Kind::Page => f.stroke(&l.icon(Glyph::Doc, pt(0.0, 0.0), 13.0), line),
    }
    let stamp = it.stamp();
    if stamp > 0.001 {
        f.stroke(
            &l.shape(&STAMP_PATH.partial(stamp)),
            stroke(ink.ink.scale_alpha(a), W_THICK),
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::theme::LIGHT;
    use crate::gui::widgets::anim::MOTION_LOCK;
    use crate::gui::widgets::hairline::Parallax;
    use iced::{window, Vector};
    use std::time::Duration;

    const BOUNDS: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: SIZE.width,
        height: SIZE.height,
    };

    fn prog(guard: Guard, changed: Instant) -> WebGlobe {
        WebGlobe {
            p: LIGHT,
            plate: Plate::Surface,
            guard,
            changed,
            now: changed,
            blocked: None,
            labels: Labels::new(|k| format!("<{k}>")),
        }
    }

    fn frame(at: Instant) -> Event {
        Event::Window(window::Event::RedrawRequested(at))
    }

    fn wants_frame(a: Option<Action<()>>) -> bool {
        a.map(|a| a.into_inner().1 == window::RedrawRequest::NextFrame)
            .unwrap_or(false)
    }

    fn run(p: &WebGlobe, st: &mut State, e: &Event, cursor: mouse::Cursor) -> Option<Action<()>> {
        canvas::Program::<()>::update(p, st, e, BOUNDS, cursor)
    }

    /// Unit point to the cursor over it.
    fn at(u: Point) -> mouse::Cursor {
        let s = Stage::fit(UNITS, SIZE);
        mouse::Cursor::Available(s.point(u))
    }

    fn moved(u: Point) -> Event {
        let s = Stage::fit(UNITS, SIZE);
        Event::Mouse(mouse::Event::CursorMoved {
            position: s.point(u),
        })
    }

    #[test]
    fn constants_parse_and_lanes_end_inside_the_dome() {
        assert_eq!(STAMP_PATH.segs.len(), 4);
        assert_eq!(PAUSE_PATH.segs.len(), 4);
        for lane in 0..LANES.len() {
            assert!(!in_dome(lane_point(lane, 0.0)), "lane {lane} starts outside");
            assert!(in_dome(lane_point(lane, 1.0)), "lane {lane} ends under the dome");
            let u = dome_entry(lane);
            assert!(u > 0.2 && u < 1.0, "{u}");
            assert!(in_dome(lane_point(lane, u)) && !in_dome(lane_point(lane, u - 0.01)));
        }
        // The globe and the dome never overlap.
        const { assert!(GC.x + GR < DC.x - DRX) };
    }

    #[test]
    fn traffic_is_stopped_only_when_protection_is_on() {
        let mut ad = Item::new(1, Kind::Ad, 1, 0.0);
        let mut page = Item::new(2, Kind::Page, 1, 0.0);
        let mut blocked_at = None;
        for _ in 0..400 {
            if let Outcome::Blocked(p) = step_item(&mut ad, 1.0 / 60.0, true) {
                blocked_at = Some(p);
            }
            step_item(&mut page, 1.0 / 60.0, true);
        }
        let p = blocked_at.expect("the dome stops the ad");
        assert!(in_dome(p));
        assert!(ad.op < 1.0 && ad.stamp() == 1.0, "stamped and falling");
        assert_eq!(page.phase, Phase::In, "a web page goes through");

        // Not protected: the ad reaches the screen and sits there.
        let mut ad = Item::new(3, Kind::Ad, 0, 0.0);
        let mut landed = false;
        for _ in 0..200 {
            landed |= step_item(&mut ad, 1.0 / 60.0, false) == Outcome::Landed;
        }
        assert!(landed && ad.phase == Phase::Landed && ad.stamp() == 0.0);
    }

    #[test]
    fn still_pictures_say_what_each_state_does() {
        let on = still_items(Guard::On);
        let ad = on.iter().find(|i| i.kind == Kind::Ad).unwrap();
        assert_eq!(ad.phase, Phase::Blocked);
        assert_eq!(ad.stamp(), 1.0);
        assert!(in_dome(ad.at) && ad.op == 1.0);
        for g in [Guard::Off, Guard::Paused, Guard::Broken, Guard::Starting] {
            let items = still_items(g);
            let ad = items.iter().find(|i| i.kind == Kind::Ad).unwrap();
            assert_eq!(ad.phase, Phase::Landed, "{g:?}");
            assert_eq!(ad.at, landed_spot(0));
            assert!(items.iter().all(|i| i.stamp() == 0.0));
        }
    }

    #[test]
    fn dome_looks_and_transitions() {
        let ink = Ink::new(&LIGHT, Plate::Surface);
        assert_eq!(dome_look(Guard::On, &ink).unwrap().0, LIGHT.good);
        assert_eq!(dome_look(Guard::Starting, &ink).unwrap().0, LIGHT.accent);
        assert_eq!(dome_look(Guard::Broken, &ink).unwrap().0, LIGHT.warn);
        assert_eq!(dome_look(Guard::Paused, &ink).unwrap(), (LIGHT.text_muted, true, CrestMark::Pause));
        assert!(dome_look(Guard::Off, &ink).is_none());

        // Turning on draws the dome in, then the tick.
        let start = dome_states(Guard::On, Some(Guard::Off), 0.0);
        assert_eq!(start.len(), 1);
        assert_eq!((start[0].outline, start[0].mark), (0.0, 0.0));
        let mid = dome_states(Guard::On, Some(Guard::Off), 0.4);
        assert!(mid[0].outline > 0.5 && mid[0].mark == 0.0);
        let end = dome_states(Guard::On, Some(Guard::Off), TRANSITION_END);
        assert_eq!((end[0].outline, end[0].fill, end[0].mark, end[0].weight), (1.0, 1.0, 1.0, 1.0));
        // Pausing fades the green dome out while the grey one comes in.
        let fade = dome_states(Guard::Paused, Some(Guard::On), 0.2);
        assert_eq!(fade.len(), 2);
        assert_eq!(fade[0].guard, Guard::On);
        assert!((fade[0].weight + fade[1].weight - 1.0).abs() < 1e-4);
        assert_eq!(dome_states(Guard::Paused, Some(Guard::On), 1.0).len(), 1);
        // Turning off leaves nothing once faded.
        assert!(dome_states(Guard::Off, Some(Guard::On), 1.0).is_empty());
        // Settled (reduced motion's age) is the end state.
        let settled = dome_states(Guard::On, None, super::super::SETTLED_AGE);
        assert_eq!(settled[0].outline, 1.0);
    }

    #[test]
    fn hotspots_name_the_parts() {
        let tilt = Parallax::off();
        let st = State {
            items: still_items(Guard::On),
            ..State::default()
        };
        let h = spots(Guard::On, &st);
        assert_eq!(h.hit(GC, &tilt), Some(Part::Globe));
        assert_eq!(h.hit(pt(GC.x + GR - 2.0, GC.y), &tilt), Some(Part::Globe));
        assert_eq!(h.hit(SCR, &tilt), Some(Part::Pc));
        assert_eq!(h.hit(CREST, &tilt), Some(Part::Dome));
        let ad = st.items.iter().find(|i| i.kind == Kind::Ad).unwrap();
        assert_eq!(h.hit(ad.at, &tilt), Some(Part::Item(ad.id)));
        assert_eq!(h.hit(pt(10.0, 250.0), &tilt), None);
        // No dome to name when protection is off.
        let off = spots(Guard::Off, &State::default());
        assert_ne!(off.hit(CREST, &tilt), Some(Part::Dome));

        let p = prog(Guard::On, Instant::now());
        assert_eq!(p.label(&st, Part::Globe), "<The internet>");
        assert_eq!(p.label(&st, Part::Pc), "<Your PC>");
        assert_eq!(p.label(&st, Part::Dome), "<Web protection is on>");
        assert_eq!(p.label(&st, Part::Item(ad.id)), "<Ad>");
        let tracker = st.items.iter().find(|i| i.kind == Kind::Tracker).unwrap();
        assert_eq!(p.label(&st, Part::Item(tracker.id)), "<Tracker>");
        let paused = prog(Guard::Paused, Instant::now());
        assert_eq!(paused.label(&st, Part::Dome), "<Web protection is not blocking>");
    }

    #[test]
    fn labels_go_through_translation() {
        let en = Labels::new(|k| k.to_string());
        let fr = Labels::new(|k| crate::i18n::Lang::Fr.t(k));
        let pairs = [
            (&en.ad, &fr.ad),
            (&en.tracker, &fr.tracker),
            (&en.scam, &fr.scam),
            (&en.page, &fr.page),
            (&en.pc, &fr.pc),
            (&en.internet, &fr.internet),
            (&en.on, &fr.on),
            (&en.starting, &fr.starting),
            (&en.not_blocking, &fr.not_blocking),
            (&en.ad_mark, &fr.ad_mark),
        ];
        for (e, f) in pairs {
            assert_ne!(e, f, "{e} has no French row");
        }
    }

    #[test]
    fn real_blocks_queue_and_others_do_not() {
        let mut st = State::default();
        st.take_counts(Some([10, 5, 0]), Guard::On, false);
        assert!(st.queue.is_empty(), "the first counts are only noted");
        st.take_counts(Some([11, 9, 1]), Guard::On, false);
        assert_eq!(st.queue, vec![Kind::Ad, Kind::Tracker, Kind::Tracker, Kind::Scam]);
        // A new day starts from zero: nothing to show.
        st.queue.clear();
        st.take_counts(Some([0, 0, 0]), Guard::On, false);
        assert!(st.queue.is_empty());
        // Paused: counts noted, nothing flies.
        st.take_counts(Some([3, 0, 0]), Guard::Paused, false);
        assert!(st.queue.is_empty());
    }

    #[test]
    fn drag_on_the_globe_spins_it_and_clicks_block_by_hand() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let p = prog(Guard::On, t0);
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        run(&p, &mut st, &frame(t0), off);
        assert!(st.guard == Some(Guard::On) && st.items.len() == 3);

        // Drag the globe to the right: faster spin.
        let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        st.items.clear();
        run(&p, &mut st, &moved(GC), at(GC));
        assert_eq!(st.live.hover, Some(Part::Globe));
        assert_eq!(
            canvas::Program::<()>::mouse_interaction(&p, &st, BOUNDS, at(GC)),
            mouse::Interaction::Grab
        );
        run(&p, &mut st, &down, at(GC));
        let before = st.vspin;
        let to = pt(GC.x + 30.0, GC.y);
        run(&p, &mut st, &moved(to), at(to));
        assert!(st.vspin > before + 1.0, "{} -> {}", before, st.vspin);
        assert_eq!(
            canvas::Program::<()>::mouse_interaction(&p, &st, BOUNDS, at(to)),
            mouse::Interaction::Grabbing
        );
        run(&p, &mut st, &up, at(to));
        assert!(!st.spinning);

        // A drag that starts off the globe does not spin it.
        let empty = pt(300.0, 20.0);
        run(&p, &mut st, &moved(empty), at(empty));
        run(&p, &mut st, &down, at(empty));
        let before = st.vspin;
        let to = pt(310.0, 20.0);
        run(&p, &mut st, &moved(to), at(to));
        assert_eq!(st.vspin, before);
        run(&p, &mut st, &up, at(to));

        // Click a flying ad: blocked by hand, with a dome flash.
        st.items = vec![Item::new(40, Kind::Ad, 0, 0.2)];
        let ad = st.items[0].at;
        run(&p, &mut st, &moved(ad), at(ad));
        assert_eq!(st.live.hover, Some(Part::Item(40)));
        assert_eq!(
            canvas::Program::<()>::mouse_interaction(&p, &st, BOUNDS, at(ad)),
            mouse::Interaction::Pointer
        );
        run(&p, &mut st, &down, at(ad));
        run(&p, &mut st, &up, at(ad));
        assert_eq!(st.items[0].phase, Phase::Blocked);
        assert!(st.flashing());

        // A web page cannot be blocked, and nothing is blocked when off.
        st.items = vec![Item::new(41, Kind::Page, 0, 0.2)];
        let page = st.items[0].at;
        run(&p, &mut st, &moved(page), at(page));
        run(&p, &mut st, &down, at(page));
        run(&p, &mut st, &up, at(page));
        assert_eq!(st.items[0].phase, Phase::Go);
        let off_prog = prog(Guard::Off, t0);
        st.items = vec![Item::new(42, Kind::Ad, 0, 0.2)];
        let ad = st.items[0].at;
        run(&off_prog, &mut st, &moved(ad), at(ad));
        assert_eq!(
            canvas::Program::<()>::mouse_interaction(&off_prog, &st, BOUNDS, at(ad)),
            mouse::Interaction::None
        );
        run(&off_prog, &mut st, &down, at(ad));
        run(&off_prog, &mut st, &up, at(ad));
        assert_eq!(st.items[0].phase, Phase::Go);
        anim::set_reduced_override(None);
    }

    #[test]
    fn traffic_rests_after_a_while_and_wakes_for_the_pointer() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let p = prog(Guard::On, t0);
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        let mut clock = t0;
        let mut frames = 0;
        while wants_frame(run(&p, &mut st, &frame(clock), off)) {
            clock += Duration::from_millis(16);
            frames += 1;
            assert!(frames < 60 * 40, "never rested");
        }
        let secs = frames as f32 * 0.016;
        assert!(secs > AWAKE_FOR, "{secs}");
        assert!(st.items.is_empty() && st.vspin == 0.0);
        let rested_spin = st.spin;
        // Still at rest: a stray frame asks for nothing and moves nothing.
        clock += Duration::from_millis(16);
        assert!(!wants_frame(run(&p, &mut st, &frame(clock), off)));
        assert_eq!(st.spin, rested_spin);

        // The pointer comes over it: traffic flows again.
        run(&p, &mut st, &moved(pt(160.0, 40.0)), at(pt(160.0, 40.0)));
        assert!(st.awake());
        clock += Duration::from_millis(16);
        assert!(wants_frame(run(&p, &mut st, &frame(clock), off)));

        // Real blocks fly in even while resting, without waking the rest.
        let counted = |n: u64| WebGlobe {
            blocked: Some([n, 0, 0]),
            ..prog(Guard::On, t0)
        };
        st.quiet = AWAKE_FOR + 1.0;
        st.items.clear();
        run(&counted(5), &mut st, &moved(pt(-40.0, -40.0)), off);
        st.quiet = AWAKE_FOR + 1.0;
        let mut frames = 0;
        let mut stopped = false;
        clock += Duration::from_millis(16);
        while wants_frame(run(&counted(6), &mut st, &frame(clock), off)) {
            clock += Duration::from_millis(16);
            stopped |= st.items.iter().any(|i| i.kind == Kind::Ad && i.phase == Phase::Blocked);
            assert!(st.items.iter().all(|i| i.kind == Kind::Ad), "only the real block flies");
            frames += 1;
            assert!(frames < 60 * 10, "never rested");
        }
        assert!(stopped && frames > 30);
        assert!(!st.awake());
        anim::set_reduced_override(None);
    }

    #[test]
    fn reduced_motion_is_one_still_picture_that_still_answers() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(true));
        let t0 = Instant::now();
        let p = prog(Guard::On, t0);
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(-50.0, -50.0));
        assert!(!wants_frame(run(&p, &mut st, &frame(t0), off)));
        assert_eq!(st.items, still_items(Guard::On));
        let spin = st.spin;
        assert!(!wants_frame(run(&p, &mut st, &frame(t0 + Duration::from_millis(16)), off)));
        assert_eq!(st.spin, spin, "no turning");
        assert_eq!(st.live.tilt.layers(), [Vector::ZERO; 3]);

        // Hover names a dot (one redraw), a click on a flying tracker stamps
        // it at once and it stays put.
        let tracker = st.items.iter().find(|i| i.kind == Kind::Tracker).unwrap();
        let (id, spot) = (tracker.id, tracker.at);
        assert!(wants_frame(run(&p, &mut st, &moved(spot), at(spot))));
        assert_eq!(st.live.hover, Some(Part::Item(id)));
        let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        run(&p, &mut st, &down, at(spot));
        assert!(wants_frame(run(&p, &mut st, &up, at(spot))));
        let t = st.item(id).unwrap();
        assert_eq!((t.phase, t.stamp(), t.at), (Phase::Blocked, 1.0, spot));
        assert!(!st.flashing() && !st.live.pulses.alive());
        assert!(!wants_frame(run(&p, &mut st, &frame(t0 + Duration::from_millis(32)), at(spot))));

        // Dragging the globe turns it directly, with no coasting.
        run(&p, &mut st, &moved(GC), at(GC));
        run(&p, &mut st, &down, at(GC));
        let to = pt(GC.x + GR * 0.5, GC.y);
        run(&p, &mut st, &moved(to), at(to));
        assert!((st.spin - (spin + 0.5)).abs() < 1e-3);
        run(&p, &mut st, &up, at(to));
        assert!(!wants_frame(run(&p, &mut st, &frame(t0 + Duration::from_millis(48)), at(to))));

        // A new state is a new still picture.
        let paused = prog(Guard::Paused, t0 + Duration::from_secs(1));
        run(&paused, &mut st, &frame(t0 + Duration::from_secs(1)), off);
        assert_eq!(st.items, still_items(Guard::Paused));
        anim::set_reduced_override(None);
    }
}
