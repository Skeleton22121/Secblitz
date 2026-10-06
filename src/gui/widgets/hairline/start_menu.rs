//! The Start menu drawing on the Clean up apps sheet.
use super::glyph::Glyph;
use super::live::Live;
use super::motion::{phase, Spring};
use super::parts::Mark;
use super::pointer::{Hotspots, Layer};
use super::stage::{pt, stroke, Ink, Meaning, Plate, Stage, W_ACCENT, W_FAINT, W_PART};
use super::svg::PathData;
use super::SETTLED_AGE;
use crate::gui::theme::{self, mix, Palette};
use crate::gui::widgets::anim::{
    self, ACCELERATE, DECELERATE, EASE_IN_OUT, EMPHASIZED, STANDARD,
};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Text};
use iced::widget::text::{LineHeight, Shaping};
use iced::{
    mouse, window, Color, Element, Length, Pixels, Point, Rectangle, Renderer, Size, Theme, Vector,
};
use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;
use std::time::Instant;

pub const UNITS: Size = Size::new(320.0, 230.0);
pub const HEIGHT: f32 = 173.0;
pub const SLOTS: usize = 9;
pub const MAX_LIT: usize = 6;
const SPREAD: [usize; SLOTS] = [1, 5, 6, 3, 8, 0, 4, 2, 7];
const TILE: f32 = 26.0;
const STILL: f32 = 0.3;
pub const REFLOW: f32 = 0.85;
const VANISH_END: f32 = 1.05;
const REFUSE_END: f32 = 2.4;
const STAY_END: f32 = 0.8;
const ABSENT_END: f32 = 0.7;
const BUMP_END: f32 = 0.8;
const MARK_C: Point = pt(256.0, 30.0);
const MARK_R: f32 = 14.0;
const MORE_C: Point = pt(218.0, 74.0);
const MORE_X: f32 = 212.0;
const TILE_HIT: f32 = 16.0;
const COUNT_SIZE: f32 = theme::SMALL;
const RULE_X: (f32, f32) = (218.0, 240.0);
const RULE_Y: f32 = 74.0;
const SWEEP_LEN: f32 = 8.0;
const SWEEP_PERIOD: f32 = 1.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    Waiting,
    Saving,
    Busy,
    Removed(Instant),
    Refused(Instant),
    /// Left installed on purpose (no copy could be saved): moves like
    /// `Refused`, named differently.
    Kept(Instant),
    Stays(Instant),
    Absent(Instant),
}

impl Fate {
    fn at(self) -> Option<Instant> {
        match self {
            Fate::Removed(t)
            | Fate::Refused(t)
            | Fate::Kept(t)
            | Fate::Stays(t)
            | Fate::Absent(t) => Some(t),
            _ => None,
        }
    }
    pub fn lit(self) -> bool {
        matches!(self, Fate::Waiting | Fate::Saving | Fate::Busy)
    }
    fn active(self) -> bool {
        matches!(self, Fate::Saving | Fate::Busy)
    }
    fn leaves(self) -> bool {
        matches!(self, Fate::Removed(_) | Fate::Absent(_))
    }
    fn end(self) -> f32 {
        match self {
            Fate::Removed(_) => VANISH_END,
            Fate::Refused(_) | Fate::Kept(_) => REFUSE_END,
            Fate::Stays(_) => STAY_END,
            Fate::Absent(_) => ABSENT_END,
            _ => 0.0,
        }
    }
    fn settled(self) -> f32 {
        match self {
            Fate::Removed(_) | Fate::Absent(_) => REFLOW,
            Fate::Refused(_) | Fate::Kept(_) => 1.0,
            Fate::Stays(_) => STAY_END,
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Working,
    Removed,
    Unchanged,
    Partly,
    Failed,
}

impl Outcome {
    pub fn mark(self) -> Option<(Meaning, Mark)> {
        match self {
            Outcome::Removed => Some((Meaning::Done, Mark::Tick)),
            Outcome::Partly => Some((Meaning::Attention, Mark::Excl)),
            Outcome::Failed => Some((Meaning::Failed, Mark::Cross)),
            Outcome::Working | Outcome::Unchanged => None,
        }
    }
    pub fn flag(self) -> Meaning {
        match self {
            Outcome::Failed => Meaning::Failed,
            _ => Meaning::Attention,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuApp {
    pub glyph: Glyph,
    pub name: String,
    pub fate: Fate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Filler {
    pub glyph: Glyph,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Labels {
    pub waiting: String,
    pub saving: String,
    pub removing: String,
    pub refused: String,
    pub kept: String,
    pub protected: String,
    pub more_one: String,
    pub more_many: String,
    pub result: String,
}

#[derive(Debug, Clone)]
pub struct StartMenu {
    pub palette: Palette,
    pub plate: Plate,
    pub outcome: Outcome,
    pub changed: Instant,
    pub now: Instant,
    pub apps: Vec<MenuApp>,
    pub fillers: Vec<Filler>,
    pub labels: Labels,
}

pub fn start_menu<'a, M: 'a>(menu: StartMenu) -> Element<'a, M> {
    canvas::Canvas::new(menu)
        .width(Length::Fill)
        .height(HEIGHT)
        .into()
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    App(usize),
    Filler(usize),
}

pub fn sequence(apps: usize, fillers: usize) -> Vec<Who> {
    let lit = apps.min(MAX_LIT);
    let chosen = &SPREAD[..lit];
    let mut out = Vec::with_capacity(apps + fillers.min(SLOTS));
    let (mut a, mut f) = (0, 0);
    for s in 0..SLOTS {
        if chosen.contains(&s) {
            out.push(Who::App(a));
            a += 1;
        } else if f < fillers {
            out.push(Who::Filler(f));
            f += 1;
        }
    }
    out.extend((a..apps).map(Who::App));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Slot(usize),
    Hidden,
    Gone,
}

pub fn places(gone: &[bool]) -> (Vec<Place>, usize) {
    let mut k = 0;
    let mut hidden = 0;
    let out = gone
        .iter()
        .map(|g| {
            if *g {
                return Place::Gone;
            }
            let p = if k < SLOTS {
                Place::Slot(k)
            } else {
                hidden += 1;
                Place::Hidden
            };
            k += 1;
            p
        })
        .collect();
    (out, hidden)
}

pub fn slot_centre(k: usize) -> Point {
    pt(104.0 + (k % 3) as f32 * 56.0, 100.0 + (k / 3) as f32 * 34.0)
}

fn since(at: Instant, clock: Instant) -> f32 {
    if anim::reduced() {
        SETTLED_AGE
    } else {
        clock.saturating_duration_since(at).as_secs_f32()
    }
}

fn rel(to: Instant, from: Instant) -> f32 {
    if to >= from {
        to.duration_since(from).as_secs_f32()
    } else {
        -from.duration_since(to).as_secs_f32()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub lift: f32,
    pub rot: f32,
    pub scale: f32,
    pub ring: f32,
    pub dashed: bool,
    pub tone: f32,
    pub flag: f32,
    pub spark: Option<f32>,
}

impl Pose {
    const REST: Pose = Pose {
        lift: 0.0,
        rot: 0.0,
        scale: 1.0,
        ring: 0.0,
        dashed: false,
        tone: 0.0,
        flag: 0.0,
        spark: None,
    };
}

pub fn pose(fate: Option<Fate>, age: f32, t: f32, i: usize, wobble: f32) -> Pose {
    let i = i as f32;
    let busy = |amp: f32| 3.5 * amp * (t * 13.0 + i).sin();
    match fate {
        None => Pose::REST,
        Some(Fate::Waiting) => Pose {
            rot: 2.0 * wobble * (t * 9.0 + i * 1.3).sin(),
            ring: 0.55,
            dashed: true,
            ..Pose::REST
        },
        Some(Fate::Saving | Fate::Busy) => Pose {
            lift: -2.0,
            rot: busy(wobble),
            ring: 1.0,
            dashed: true,
            ..Pose::REST
        },
        Some(Fate::Removed(_)) => {
            let calm = 1.0 - phase(age, 0.0, 0.2, STANDARD);
            let e = phase(age, 0.6, VANISH_END, DECELERATE);
            Pose {
                lift: -2.0 - 10.0 * phase(age, 0.1, 0.5, DECELERATE),
                rot: busy(1.0) * calm,
                scale: 1.0 - phase(age, 0.35, 0.7, ACCELERATE),
                ring: 1.0 - phase(age, 0.0, 0.3, STANDARD),
                dashed: true,
                spark: (age > 0.6 && e < 1.0).then_some(e),
                ..Pose::REST
            }
        }
        Some(Fate::Refused(_) | Fate::Kept(_)) => {
            let shake = if age > 0.8 {
                (-(age - 0.8) * 3.5).exp()
            } else {
                0.0
            };
            let solid = phase(age, 0.75, 0.9, STANDARD);
            Pose {
                lift: -10.0 * (PI * phase(age, 0.1, 0.8, EASE_IN_OUT)).sin(),
                rot: 9.0 * (age * 32.0).sin() * shake,
                ring: (1.0 - phase(age, 0.0, 0.2, STANDARD)).max(solid),
                dashed: age < 0.5,
                tone: solid,
                flag: phase(age, 0.8, 1.0, STANDARD),
                ..Pose::REST
            }
        }
        Some(Fate::Stays(_)) => Pose {
            lift: -6.0 * (PI * phase(age, 0.1, 0.7, EASE_IN_OUT)).sin(),
            ring: 1.0 - phase(age, 0.0, 0.3, STANDARD),
            dashed: true,
            tone: phase(age, 0.5, 0.8, STANDARD),
            ..Pose::REST
        },
        Some(Fate::Absent(_)) => Pose {
            scale: 1.0 - phase(age, 0.25, ABSENT_END, ACCELERATE),
            ring: 0.55 * (1.0 - phase(age, 0.0, 0.25, STANDARD)),
            dashed: true,
            tone: phase(age, 0.0, 0.3, STANDARD),
            ..Pose::REST
        },
    }
}

pub fn glyph_for(family: &str) -> Glyph {
    let f = family.to_ascii_lowercase();
    const TABLE: &[(&[&str], Glyph)] = &[
        (&["xbox", "gamingapp", "roblox", "king.com"], Glyph::Game),
        (&["solitaire"], Glyph::Cards),
        (
            &["clipchamp", "zunevideo", "disney", "netflix", "primevideo", "tiktok"],
            Glyph::Play,
        ),
        (&["spotify", "zunemusic", "soundrecorder"], Glyph::Music),
        (&["windowscamera", "photos", "adobeexpress"], Glyph::Camera),
        (&["bingnews", "twitter", "webexperience"], Glyph::News),
        (&["skypeapp", "messaging", "outlook"], Glyph::Mail),
        (&["people", "feedbackhub", "facebook", "teams"], Glyph::Person),
        (
            &["bingsearch", "gethelp", "549981c3f5f10", "copilot", "maps"],
            Glyph::Search,
        ),
        (&["wallet"], Glyph::Cart),
        (&["yourphone", "crossdevice", "quickassist"], Glyph::Remote),
        (&["powerautomate", "devhome"], Glyph::Gear),
        (&["alarms"], Glyph::Update),
    ];
    TABLE
        .iter()
        .find(|(keys, _)| keys.iter().any(|k| f.contains(k)))
        .map(|(_, g)| *g)
        .unwrap_or(Glyph::Doc)
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Tile(usize),
    More,
    Mark,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TileSprings {
    x: Spring,
    y: Spring,
    sc: Spring,
    enter: Spring,
    bump: Option<f32>,
}

impl TileSprings {
    fn at(p: Point, shown: bool) -> Self {
        let e = if shown { 1.0 } else { 0.0 };
        TileSprings {
            x: Spring::with(p.x, 120.0, 16.0),
            y: Spring::with(p.y, 120.0, 16.0),
            sc: Spring::with(1.0, 220.0, 18.0),
            enter: Spring::with(e, 180.0, 15.0),
            bump: None,
        }
    }
    fn moving(&self) -> bool {
        self.x.moving() || self.y.moving() || self.sc.moving() || self.enter.moving()
    }
}

#[derive(Debug, Default)]
pub struct State {
    live: Live<Part>,
    tiles: Vec<TileSprings>,
    mark_sc: Spring,
    pop: Spring,
}

pub(crate) struct Scene {
    pub(crate) seq: Vec<Who>,
    pub(crate) places: Vec<Place>,
    pub(crate) hidden: usize,
    pub(crate) active_hidden: Option<usize>,
    clock: Instant,
}

impl StartMenu {
    fn stage(bounds: Size) -> Stage {
        Stage::fit(UNITS, bounds)
    }

    fn fate(&self, who: Who) -> Option<Fate> {
        match who {
            Who::App(i) => self.apps.get(i).map(|a| a.fate),
            Who::Filler(_) => None,
        }
    }

    pub(crate) fn scene(&self, clock: Instant) -> Scene {
        let seq = sequence(self.apps.len(), self.fillers.len());
        let gone: Vec<bool> = seq
            .iter()
            .map(|w| {
                self.fate(*w).is_some_and(|f| {
                    f.leaves() && f.at().is_some_and(|at| since(at, clock) > REFLOW)
                })
            })
            .collect();
        let (places, hidden) = places(&gone);
        let active_hidden = seq.iter().zip(&places).find_map(|(w, p)| match (w, p) {
            (Who::App(j), Place::Hidden) if self.apps[*j].fate.active() => Some(*j),
            _ => None,
        });
        Scene {
            seq,
            places,
            hidden,
            active_hidden,
            clock,
        }
    }

    fn mark_delay(&self) -> f32 {
        self.apps
            .iter()
            .filter_map(|a| a.fate.at().map(|at| rel(at, self.changed) + a.fate.settled()))
            .fold(0.15_f32, f32::max)
            .min(1.6)
    }

    fn mark_shown(&self, age: f32) -> bool {
        self.outcome.mark().is_some() && age >= self.mark_delay()
    }

    fn spots(&self, sc: &Scene, mark: bool) -> Hotspots<Part> {
        let mut h = Hotspots::new();
        for (i, place) in sc.places.iter().enumerate() {
            let Place::Slot(k) = place else { continue };
            if self.fate(sc.seq[i]).is_some_and(Fate::leaves) {
                continue;
            }
            h = h.circle(Part::Tile(i), slot_centre(*k), TILE_HIT, Layer::Front);
        }
        if sc.hidden > 0 {
            h = h.rect(Part::More, MORE_C, 52.0, 18.0, Layer::Mid);
        }
        if mark {
            h = h.circle(Part::Mark, MARK_C, MARK_R + 3.0, Layer::Mid);
        }
        h
    }

    pub(crate) fn label(&self, part: Part, sc: &Scene) -> String {
        let l = &self.labels;
        match part {
            Part::More => match sc.active_hidden {
                Some(j) => self.app_label(j),
                None if sc.hidden == 1 => l.more_one.clone(),
                None => l.more_many.replace("{n}", &sc.hidden.to_string()),
            },
            Part::Mark => l.result.clone(),
            Part::Tile(i) => match sc.seq.get(i) {
                Some(Who::Filler(j)) => self
                    .fillers
                    .get(*j)
                    .map(|f| f.name.clone())
                    .unwrap_or_default(),
                Some(Who::App(j)) => self.app_label(*j),
                None => String::new(),
            },
        }
    }

    fn app_label(&self, j: usize) -> String {
        let l = &self.labels;
        let Some(app) = self.apps.get(j) else {
            return String::new();
        };
        let template = match app.fate {
            Fate::Waiting => &l.waiting,
            Fate::Saving => &l.saving,
            Fate::Busy | Fate::Removed(_) => &l.removing,
            Fate::Refused(_) => &l.refused,
            Fate::Kept(_) => &l.kept,
            Fate::Stays(_) => &l.protected,
            Fate::Absent(_) => return app.name.clone(),
        };
        template.replace("{name}", &app.name)
    }

    fn busy(&self, st: &State, sc: &Scene) -> bool {
        if anim::reduced() {
            return false;
        }
        let looping = self.outcome == Outcome::Working
            && (sc.active_hidden.is_some()
                || sc.seq.iter().zip(&sc.places).any(|(w, p)| {
                    matches!(p, Place::Slot(_)) && self.fate(*w).is_some_and(Fate::lit)
                }));
        let settling = self.apps.iter().any(|a| {
            a.fate
                .at()
                .is_some_and(|at| since(at, sc.clock) < a.fate.end())
        });
        let t = st.live.ambient(self.now, STILL);
        let bumping = st
            .tiles
            .iter()
            .any(|s| s.bump.is_some_and(|b| t - b < BUMP_END) || s.moving());
        let age = st.live.age(self.changed, self.now);
        let marking = self.outcome.mark().is_some() && age < self.mark_delay() + 0.6;
        looping || settling || bumping || marking || st.pop.moving() || st.mark_sc.moving()
    }

    fn aim(&self, st: &mut State, sc: &Scene) {
        if st.tiles.len() != sc.seq.len() {
            st.tiles = sc
                .places
                .iter()
                .map(|p| match p {
                    Place::Slot(k) => TileSprings::at(slot_centre(*k), true),
                    _ => TileSprings::at(slot_centre(SLOTS - 1), false),
                })
                .collect();
        }
        for (i, (s, place)) in st.tiles.iter_mut().zip(&sc.places).enumerate() {
            match place {
                Place::Slot(k) => {
                    let c = slot_centre(*k);
                    if s.enter.target < 0.5 {
                        s.x = Spring::with(c.x, 120.0, 16.0);
                        s.y = Spring::with(c.y, 120.0, 16.0);
                        s.enter.aim(1.0);
                    }
                    s.x.aim(c.x);
                    s.y.aim(c.y);
                }
                Place::Hidden => {
                    s.enter = Spring::with(0.0, 180.0, 15.0);
                }
                Place::Gone => {}
            }
            let hovered = st.live.hover == Some(Part::Tile(i));
            s.sc.aim(if hovered { 1.14 } else { 1.0 });
        }
        st.mark_sc
            .aim(if st.live.hover == Some(Part::Mark) { 1.12 } else { 1.0 });
    }
}

impl<M> canvas::Program<M> for StartMenu {
    type State = State;

    fn update(
        &self,
        st: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        if st.live.fresh(self.changed) {
            st.pop = Spring::with(0.0, 200.0, 12.0);
        }
        let stage = Self::stage(bounds.size());
        let mut clock = st.live.clock(self.now);
        if let Event::Window(window::Event::RedrawRequested(t)) = event {
            clock = clock.max(*t);
        }
        let sc = self.scene(clock);
        let mark = self.mark_shown(st.live.age(self.changed, clock));
        let step = st
            .live
            .update(event, bounds, cursor, &stage, &self.spots(&sc, mark));
        self.aim(st, &sc);
        if let Some(dt) = step.dt {
            for s in &mut st.tiles {
                s.x.tick(dt);
                s.y.tick(dt);
                s.sc.tick(dt);
                s.enter.tick(dt);
            }
            st.pop.tick(dt);
            st.mark_sc.tick(dt);
        }
        if let Some(at) = step.click() {
            match st.live.hover {
                Some(Part::Tile(i)) => {
                    let t = st.live.ambient(self.now, STILL);
                    if let Some(s) = st.tiles.get_mut(i) {
                        s.bump = Some(t);
                    }
                }
                Some(Part::Mark) => st.pop.kick(9.0),
                _ => st.live.pulses.push(at, false),
            }
        }
        st.live.redraw(&step, self.busy(st, &sc))
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
        let stage = Self::stage(bounds.size());
        let ink = Ink::new(&self.palette, self.plate);
        let clock = st.live.clock(self.now);
        let sc = self.scene(clock);
        let age = st.live.age(self.changed, self.now);
        let t = st.live.ambient(self.now, STILL);
        let mid = st.live.layer(&stage, Layer::Mid);
        let front = st.live.layer(&stage, Layer::Front);

        let working_hidden = self.outcome == Outcome::Working && sc.active_hidden.is_some();
        panel(&mut f, &mid, &ink, !working_hidden);
        if working_hidden {
            sweep(&mut f, &mid, &ink, t);
        }
        if sc.hidden > 0 {
            f.fill_text(Text {
                content: format!("+{}", sc.hidden),
                position: mid.point(pt(MORE_X, RULE_Y)),
                color: if working_hidden { ink.accent } else { ink.line },
                size: Pixels(COUNT_SIZE),
                line_height: LineHeight::Absolute(Pixels(COUNT_SIZE)),
                font: theme::MEDIUM,
                align_x: Horizontal::Right.into(),
                align_y: Vertical::Center,
                shaping: Shaping::Advanced,
                ..Text::default()
            });
        }

        let flag = ink.of(self.outcome.flag());
        let mut order: Vec<(usize, Pose)> = Vec::with_capacity(sc.seq.len());
        for (i, who) in sc.seq.iter().enumerate() {
            if sc.places[i] == Place::Hidden {
                continue;
            }
            let Some(s) = st.tiles.get(i) else { continue };
            let fate = self.fate(*who);
            let fate_age = fate.and_then(Fate::at).map_or(0.0, |at| since(at, clock));
            let hov = ((s.sc.value - 1.0) / 0.14).clamp(0.0, 1.0);
            let wobble = if anim::reduced() {
                0.0
            } else if self.outcome == Outcome::Working {
                1.0 + 0.8 * hov
            } else {
                1.0
            };
            order.push((i, pose(fate, fate_age, t, i, wobble)));
        }
        order.sort_by_key(|(_, p)| (p.lift.abs() > 0.5 || p.rot.abs() > 0.5) as u8);
        let dash_shift = if anim::reduced() {
            0
        } else {
            (t * 14.0) as usize
        };
        for (i, p) in &order {
            let s = &st.tiles[*i];
            let who = sc.seq[*i];
            let (glyph, fate) = match who {
                Who::App(j) => (self.apps[j].glyph, Some(self.apps[j].fate)),
                Who::Filler(j) => (self.fillers[j].glyph, None),
            };
            let color = match fate {
                None => ink.line,
                Some(Fate::Refused(_) | Fate::Kept(_)) => mix(ink.accent, flag, p.tone),
                Some(Fate::Stays(_) | Fate::Absent(_)) => mix(ink.accent, ink.line, p.tone),
                Some(_) => ink.accent,
            };
            let bump = s.bump.map_or(0.0, |b| {
                let d = (t - b).max(0.0);
                if d < BUMP_END {
                    (-d * 6.0).exp() * (d * 18.0).sin()
                } else {
                    0.0
                }
            });
            if let Some(e) = p.spark {
                sparks(&mut f, &front, ink.good, pt(s.x.value, s.y.value - 12.0), e);
            }
            let scale = p.scale * s.sc.value * s.enter.value.max(0.0);
            if scale < 0.02 || sc.places[*i] == Place::Gone {
                continue;
            }
            let centre = pt(s.x.value, s.y.value + p.lift - 4.0 * bump);
            tile(
                &mut f,
                &front,
                &ink,
                TileLook {
                    centre,
                    rot: p.rot,
                    scale,
                    glyph,
                    color,
                    ring: p.ring,
                    dashed: p.dashed,
                    dash_shift,
                    flag: p.flag,
                    flag_color: flag,
                },
            );
        }

        if let Some((meaning, mark)) = self.outcome.mark() {
            let d = self.mark_delay();
            let scale = (0.7 + 0.3 * phase(age, d, d + 0.3, EMPHASIZED))
                * st.mark_sc.value
                * (1.0 + 0.08 * st.pop.value);
            result_mark(
                &mut f,
                &mid,
                &ink,
                ink.of(meaning),
                mark,
                scale,
                phase(age, d, d + 0.15, STANDARD),
                phase(age, d, d + 0.4, DECELERATE),
                phase(age, d + 0.15, d + 0.55, DECELERATE),
            );
        }

        st.live.pulses.draw(&mut f, &stage, ink.accent);
        let mark = self.mark_shown(age);
        let spots = self.spots(&sc, mark);
        st.live
            .draw_tooltip(&mut f, &self.palette, &stage, &spots, |part| {
                self.label(part, &sc)
            });
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


fn panel(f: &mut Frame, s: &Stage, ink: &Ink, rule: bool) {
    let plate = s.rounded_rect(62.0, 28.0, 196.0, 196.0, 12.0);
    f.fill(&plate, ink.plate);
    f.stroke(&plate, ink.ln());
    f.stroke(&s.rounded_rect(80.0, 42.0, 160.0, 18.0, 9.0), ink.ln2());
    f.stroke(&s.icon(Glyph::Search, pt(92.0, 51.0), 11.0), ink.ln2());
    f.stroke(&s.line(pt(102.0, 51.0), pt(146.0, 51.0)), ink.lo());
    f.stroke(&s.line(pt(80.0, 74.0), pt(106.0, 74.0)), ink.ln2());
    if rule {
        f.stroke(&s.line(pt(RULE_X.0, RULE_Y), pt(RULE_X.1, RULE_Y)), ink.lo());
    }
    f.stroke(&s.line(pt(62.0, 198.0), pt(258.0, 198.0)), ink.lo());
    f.stroke(&s.icon(Glyph::Person, pt(86.0, 211.0), 14.0), ink.ln2());
    f.stroke(&s.line(pt(98.0, 211.0), pt(128.0, 211.0)), ink.lo());
    f.stroke(&s.icon(Glyph::Power, pt(236.0, 211.0), 14.0), ink.ln2());
}

struct TileLook {
    centre: Point,
    rot: f32,
    scale: f32,
    glyph: Glyph,
    color: Color,
    ring: f32,
    dashed: bool,
    dash_shift: usize,
    flag: f32,
    flag_color: Color,
}

fn ring_points() -> &'static [Point] {
    static RING: OnceLock<Vec<Point>> = OnceLock::new();
    RING.get_or_init(|| {
        PathData::of(
            "M-8 -17h16a9 9 0 0 1 9 9v16a9 9 0 0 1-9 9h-16a9 9 0 0 1-9-9v-16a9 9 0 0 1 9-9z",
        )
        .samples(121)
    })
}
const DASH: usize = 3;
const GAP: usize = 5;

fn dash_runs(shift: usize, n: usize) -> Vec<(usize, usize)> {
    let start = shift % (DASH + GAP);
    let mut runs = Vec::with_capacity(n / (DASH + GAP) + 2);
    if start > GAP {
        runs.push((0, (start - GAP).min(n)));
    }
    let mut j = start;
    while j < n {
        runs.push((j, (j + DASH).min(n)));
        j += DASH + GAP;
    }
    runs
}

fn tile(f: &mut Frame, stage: &Stage, ink: &Ink, look: TileLook) {
    let c = stage.point(look.centre);
    let local = Stage {
        units: UNITS,
        k: stage.k * look.scale,
        origin: Point::ORIGIN,
    };
    let alpha = look.scale.clamp(0.0, 1.0).sqrt();
    f.with_save(|f| {
        f.translate(Vector::new(c.x, c.y));
        f.rotate(look.rot.to_radians());
        if look.ring > 0.01 {
            let col = look.color.scale_alpha(look.ring * alpha);
            if look.dashed {
                let pts = ring_points();
                let dashes = local.path(|s| {
                    for (a, b) in dash_runs(look.dash_shift, pts.len() - 1) {
                        s.move_to(pts[a]);
                        for p in &pts[a + 1..=b] {
                            s.line_to(*p);
                        }
                    }
                });
                f.stroke(&dashes, stroke(col, W_ACCENT));
            } else {
                f.stroke(
                    &local.rounded_rect(-17.0, -17.0, 34.0, 34.0, 9.0),
                    stroke(col, W_ACCENT),
                );
            }
        }
        let h = TILE / 2.0;
        let body = local.rounded_rect(-h, -h, TILE, TILE, TILE * 0.26);
        f.fill(&body, ink.tint(look.color).scale_alpha(alpha));
        f.stroke(&body, stroke(look.color.scale_alpha(alpha), W_PART));
        f.stroke(
            &local.icon(look.glyph, Point::ORIGIN, TILE * 0.58),
            stroke(look.color.scale_alpha(alpha), W_PART),
        );
        f.stroke(
            &local.line(pt(-TILE * 0.36, h + 6.0), pt(TILE * 0.36, h + 6.0)),
            stroke(ink.rule.scale_alpha(alpha), W_FAINT),
        );
        if look.flag > 0.01 {
            let col = look.flag_color.scale_alpha(look.flag * alpha);
            let dot = local.circle(pt(h, -h), 5.5);
            f.fill(&dot, ink.plate.scale_alpha(look.flag));
            f.stroke(&dot, stroke(col, W_ACCENT));
            let mark = local.path(|s| {
                s.move_to(pt(h, -h - 2.8));
                s.line_to(pt(h, -h + 0.4));
                s.move_to(pt(h, -h + 2.4));
                s.line_to(pt(h, -h + 2.5));
            });
            f.stroke(&mark, stroke(col, W_ACCENT));
        }
    });
}

fn sweep(f: &mut Frame, s: &Stage, ink: &Ink, t: f32) {
    let (x0, x1) = RULE_X;
    f.stroke(&s.line(pt(x0, RULE_Y), pt(x1, RULE_Y)), ink.lo());
    let u = 0.5 - 0.5 * (t * TAU / SWEEP_PERIOD).cos();
    let a = x0 + u * (x1 - x0 - SWEEP_LEN);
    f.stroke(
        &s.line(pt(a, RULE_Y), pt(a + SWEEP_LEN, RULE_Y)),
        stroke(ink.accent, W_ACCENT),
    );
}

fn sparks(f: &mut Frame, s: &Stage, color: Color, c: Point, e: f32) {
    let r0 = 6.0 + 14.0 * e;
    let r1 = r0 + 6.0 * (1.0 - e);
    let path = s.path(|p| {
        for n in 0..6 {
            let a = n as f32 / 6.0 * TAU + 0.3;
            let (sin, cos) = a.sin_cos();
            p.move_to(pt(c.x + r0 * cos, c.y + r0 * sin));
            p.line_to(pt(c.x + r1 * cos, c.y + r1 * sin));
        }
    });
    f.stroke(&path, stroke(color.scale_alpha(1.0 - e), W_PART));
}

fn ring_data(c: Point, r: f32) -> PathData {
    static UNIT: OnceLock<PathData> = OnceLock::new();
    UNIT.get_or_init(|| PathData::of("M0 -1a1 1 0 1 1 0 2a1 1 0 1 1 0-2"))
        .map(|p| pt(c.x + p.x * r, c.y + p.y * r))
}

#[allow(clippy::too_many_arguments)]
fn result_mark(
    f: &mut Frame,
    s: &Stage,
    ink: &Ink,
    color: Color,
    mark: Mark,
    scale: f32,
    alpha: f32,
    ring: f32,
    drawn: f32,
) {
    if alpha <= 0.0 {
        return;
    }
    let r = MARK_R * scale;
    f.fill(&s.circle(MARK_C, r), mix(ink.plate, ink.tint(color), alpha));
    if ring > 0.0 {
        f.stroke(
            &s.shape(&ring_data(MARK_C, r).partial(ring)),
            stroke(color.scale_alpha(alpha), W_PART),
        );
    }
    if drawn > 0.0 {
        let d = mark.glyph().data().placed(MARK_C, r * 1.9).partial(drawn);
        f.stroke(&s.shape(&d), stroke(color, W_ACCENT));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::theme::LIGHT;
    use crate::gui::widgets::anim::MOTION_LOCK;
    use std::time::Duration;

    fn menu(apps: Vec<Fate>, fillers: usize, outcome: Outcome, t0: Instant) -> StartMenu {
        StartMenu {
            palette: LIGHT,
            plate: Plate::Surface,
            outcome,
            changed: t0,
            now: t0,
            apps: apps
                .into_iter()
                .enumerate()
                .map(|(i, fate)| MenuApp {
                    glyph: Glyph::Game,
                    name: format!("App {i}"),
                    fate,
                })
                .collect(),
            fillers: (0..fillers)
                .map(|i| Filler {
                    glyph: Glyph::Gear,
                    name: format!("Stay {i}"),
                })
                .collect(),
            labels: Labels {
                waiting: "En attente : {name}".into(),
                saving: "Copie de {name}".into(),
                removing: "Suppression de {name}".into(),
                refused: "Impossible de supprimer {name}".into(),
                kept: "{name} reste".into(),
                protected: "Windows protège {name}".into(),
                more_one: "1 app de plus".into(),
                more_many: "{n} apps de plus".into(),
                result: "2 apps supprimées".into(),
            },
        }
    }

    #[test]
    fn sequence_spreads_the_apps_and_queues_the_rest() {
        let s = sequence(2, 9);
        assert_eq!(s.len(), 9);
        assert_eq!(s[1], Who::App(0));
        assert_eq!(s[5], Who::App(1));
        assert_eq!(s[0], Who::Filler(0));
        assert_eq!(s.iter().filter(|w| matches!(w, Who::Filler(_))).count(), 7);
        let s = sequence(20, 9);
        assert_eq!(s.len(), 23);
        assert_eq!(s[..SLOTS].iter().filter(|w| matches!(w, Who::App(_))).count(), MAX_LIT);
        assert_eq!(s[SLOTS..], (6..20).map(Who::App).collect::<Vec<_>>()[..]);
        let apps: Vec<usize> = s
            .iter()
            .filter_map(|w| match w {
                Who::App(i) => Some(*i),
                _ => None,
            })
            .collect();
        assert_eq!(apps, (0..20).collect::<Vec<_>>());
        let s = sequence(1, 2);
        assert_eq!(s, vec![Who::Filler(0), Who::App(0), Who::Filler(1)]);
        assert!(sequence(0, 0).is_empty());
    }

    #[test]
    fn gone_tiles_close_the_gap_and_queued_ones_slide_in() {
        let (p, hidden) = places(&[false; 11]);
        assert_eq!(hidden, 2);
        assert_eq!(p[8], Place::Slot(8));
        assert_eq!(p[9], Place::Hidden);
        let mut gone = [false; 11];
        gone[1] = true;
        gone[4] = true;
        let (p, hidden) = places(&gone);
        assert_eq!(hidden, 0);
        assert_eq!(p[1], Place::Gone);
        assert_eq!(p[2], Place::Slot(1));
        assert_eq!(p[5], Place::Slot(3));
        assert_eq!(p[10], Place::Slot(8));
    }

    #[test]
    fn removed_apps_leave_only_after_they_have_lifted_out() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let done = t0 + Duration::from_millis(100);
        let m = menu(
            vec![Fate::Removed(done), Fate::Busy, Fate::Refused(done)],
            9,
            Outcome::Working,
            t0,
        );
        let sc = m.scene(done + Duration::from_millis(300));
        assert_eq!(sc.places[1], Place::Slot(1));
        let sc = m.scene(done + Duration::from_secs(1));
        assert_eq!(sc.places[1], Place::Gone);
        assert_eq!(sc.places[2], Place::Slot(1));
        assert_eq!(sc.places.iter().filter(|p| matches!(p, Place::Slot(_))).count(), 8);
        anim::set_reduced_override(None);
    }

    #[test]
    fn poses_end_where_the_fate_says() {
        let rest = pose(Some(Fate::Refused(Instant::now())), SETTLED_AGE, 0.0, 2, 1.0);
        assert!(rest.lift.abs() < 1e-3 && rest.rot.abs() < 1e-3);
        assert_eq!((rest.tone, rest.flag, rest.ring), (1.0, 1.0, 1.0));
        assert!(!rest.dashed);
        let gone = pose(Some(Fate::Removed(Instant::now())), SETTLED_AGE, 0.0, 0, 1.0);
        assert_eq!(gone.scale, 0.0);
        assert_eq!(gone.spark, None);
        let mid = pose(Some(Fate::Removed(Instant::now())), 0.8, 0.0, 0, 1.0);
        assert!(mid.spark.is_some());
        let stays = pose(Some(Fate::Stays(Instant::now())), SETTLED_AGE, 0.0, 0, 1.0);
        assert_eq!((stays.ring, stays.tone, stays.flag), (0.0, 1.0, 0.0));
        let calm = pose(Some(Fate::Busy), 0.0, 0.1, 0, 1.0);
        let more = pose(Some(Fate::Busy), 0.0, 0.1, 0, 1.8);
        assert!(more.rot.abs() > calm.rot.abs());
        assert_eq!(pose(None, 0.0, 0.4, 3, 1.8), Pose::REST);
    }

    #[test]
    fn hotspots_name_tiles_count_and_mark_in_translation() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let mut fates = vec![Fate::Busy];
        fates.extend(vec![Fate::Waiting; 10]);
        let m = menu(fates, 9, Outcome::Working, t0);
        let sc = m.scene(t0);
        assert_eq!(sc.hidden, 5);
        let spots = m.spots(&sc, false);
        let still = super::super::Parallax::off();
        assert_eq!(spots.hit(slot_centre(0), &still), Some(Part::Tile(0)));
        assert_eq!(m.label(Part::Tile(0), &sc), "Suppression de App 0");
        assert_eq!(m.label(Part::Tile(1), &sc), "En attente : App 1");
        assert_eq!(m.label(Part::Tile(2), &sc), "Stay 0");
        assert_eq!(spots.hit(pt(104.0 + 10.0, 100.0), &still), Some(Part::Tile(0)));
        assert_eq!(spots.hit(pt(132.0, 117.0), &still), None);
        assert_eq!(spots.hit(MORE_C, &still), Some(Part::More));
        assert_eq!(m.label(Part::More, &sc), "5 apps de plus");
        assert_eq!(m.label(Part::More, &Scene { hidden: 1, ..m.scene(t0) }), "1 app de plus");
        assert_eq!(spots.hit(MARK_C, &still), None);
        let m = menu(
            vec![Fate::Refused(t0), Fate::Stays(t0), Fate::Kept(t0)],
            9,
            Outcome::Partly,
            t0,
        );
        let sc = m.scene(t0);
        let spots = m.spots(&sc, true);
        assert_eq!(spots.hit(MARK_C, &still), Some(Part::Mark));
        assert_eq!(m.label(Part::Mark, &sc), "2 apps supprimées");
        assert_eq!(m.label(Part::Tile(1), &sc), "Impossible de supprimer App 0");
        assert_eq!(m.label(Part::Tile(5), &sc), "Windows protège App 1");
        assert_eq!(m.label(Part::Tile(6), &sc), "App 2 reste");
        anim::set_reduced_override(None);
    }

    #[test]
    fn removing_tiles_have_no_hotspot() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let m = menu(vec![Fate::Removed(t0)], 3, Outcome::Working, t0);
        let sc = m.scene(t0);
        assert_eq!(sc.places[1], Place::Slot(1));
        let spots = m.spots(&sc, false);
        assert_eq!(spots.hit(slot_centre(1), &super::super::Parallax::off()), None);
        anim::set_reduced_override(None);
    }

    #[test]
    fn apps_no_longer_installed_shrink_away_quietly() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let m = menu(vec![Fate::Absent(t0), Fate::Removed(t0)], 9, Outcome::Removed, t0);
        let p = pose(Some(Fate::Absent(t0)), 0.5, 0.0, 1, 1.0);
        assert_eq!((p.lift, p.spark), (0.0, None));
        assert!(p.scale < 1.0 && p.tone == 1.0);
        let gone = pose(Some(Fate::Absent(t0)), SETTLED_AGE, 0.0, 1, 1.0);
        assert_eq!((gone.scale, gone.ring), (0.0, 0.0));
        let sc = m.scene(t0 + Duration::from_millis(300));
        assert_eq!(m.label(Part::Tile(1), &sc), "App 0");
        let still = super::super::Parallax::off();
        assert_eq!(m.spots(&sc, false).hit(slot_centre(1), &still), None);
        let sc = m.scene(t0 + Duration::from_secs(1));
        assert_eq!(sc.places[1], Place::Gone);
        assert_eq!(sc.places[5], Place::Gone);
        assert_eq!(sc.places[2], Place::Slot(1));
        anim::set_reduced_override(None);
    }

    #[test]
    fn work_out_of_sight_shows_on_the_count_and_keeps_frames() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let mut fates = vec![Fate::Refused(t0); 6];
        fates.extend([Fate::Busy, Fate::Waiting]);
        let m = menu(fates.clone(), 9, Outcome::Working, t0);
        let sc = m.scene(t0);
        assert_eq!((sc.hidden, sc.active_hidden), (2, Some(6)));
        assert_eq!(m.label(Part::More, &sc), "Suppression de App 6");
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(2.0, 2.0));
        let mut clock = t0 + Duration::from_secs(10);
        for _ in 0..10 {
            assert!(tick(&mut st, &m, off, &mut clock));
        }
        fates[6] = Fate::Waiting;
        let m = menu(fates, 9, Outcome::Working, t0);
        let sc = m.scene(clock);
        assert_eq!(sc.active_hidden, None);
        assert_eq!(m.label(Part::More, &sc), "2 apps de plus");
        let mut frames = 0;
        while tick(&mut st, &m, off, &mut clock) {
            frames += 1;
            assert!(frames < 600, "never settled");
        }
        anim::set_reduced_override(None);
    }

    #[test]
    fn the_marks_name_goes_below_it_without_covering_it() {
        let t0 = Instant::now();
        let m = menu(vec![Fate::Removed(t0)], 9, Outcome::Removed, t0);
        let spots = m.spots(&m.scene(t0), true);
        let still = super::super::Parallax::off();
        let stage = StartMenu::stage(BOUNDS.size());
        let r = super::super::pointer::tooltip_rect_around(
            stage.point(spots.anchor(Part::Mark, &still).unwrap()),
            stage.point(spots.below(Part::Mark, &still).unwrap()),
            80.0,
            BOUNDS.size(),
        );
        let disc_bottom = px(MARK_C).y + stage.len(MARK_R);
        assert!(r.y >= disc_bottom, "{r:?} over the disc ending at {disc_bottom}");
        assert!(r.y + r.height <= BOUNDS.height);
    }

    #[test]
    fn the_tilt_is_level_with_the_pointer_in_the_middle() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let m = menu(vec![Fate::Waiting], 9, Outcome::Working, t0);
        let mut st = State::default();
        let mid = Point::new(BOUNDS.width / 2.0, BOUNDS.height / 2.0);
        canvas::Program::<()>::update(
            &m,
            &mut st,
            &Event::Mouse(mouse::Event::CursorMoved { position: mid }),
            BOUNDS,
            mouse::Cursor::Available(mid),
        );
        assert!(st.live.tilt.x.target.abs() < 0.01);
        assert!(st.live.tilt.y.target.abs() < 0.01, "{}", st.live.tilt.y.target);
        anim::set_reduced_override(None);
    }

    #[test]
    fn the_mark_waits_for_the_last_tile() {
        let t0 = Instant::now();
        let mut m = menu(vec![Fate::Removed(t0)], 3, Outcome::Removed, t0);
        assert!((m.mark_delay() - REFLOW).abs() < 1e-3);
        m.changed = t0 + Duration::from_secs(5);
        assert!((m.mark_delay() - 0.15).abs() < 1e-3);
        assert_eq!(Outcome::Working.mark(), None);
        assert_eq!(Outcome::Unchanged.mark(), None);
        assert_eq!(Outcome::Failed.flag(), Meaning::Failed);
        assert_eq!(Outcome::Partly.flag(), Meaning::Attention);
    }

    #[test]
    fn dashes_march_without_changing_their_length() {
        let n = ring_points().len() - 1;
        assert_eq!(n, 120);
        for shift in 0..40 {
            let runs = dash_runs(shift, n);
            let total: usize = runs.iter().map(|(a, b)| b - a).sum();
            assert_eq!(total, n / (DASH + GAP) * DASH, "shift {shift}");
            assert!(runs.iter().all(|(a, b)| a < b && *b <= n));
        }
        assert_eq!(dash_runs(0, n)[0], (0, DASH));
        assert_eq!(dash_runs(1, n)[0], (1, 1 + DASH));
    }

    #[test]
    fn glyphs_follow_the_app() {
        assert_eq!(glyph_for("Microsoft.XboxGamingOverlay"), Glyph::Game);
        assert_eq!(glyph_for("Microsoft.MicrosoftSolitaireCollection"), Glyph::Cards);
        assert_eq!(glyph_for("Clipchamp.Clipchamp"), Glyph::Play);
        assert_eq!(glyph_for("Microsoft.BingNews"), Glyph::News);
        assert_eq!(glyph_for("Microsoft.OutlookForWindows"), Glyph::Mail);
        assert_eq!(glyph_for("SpotifyAB.SpotifyMusic"), Glyph::Music);
        assert_eq!(glyph_for("Microsoft.WindowsCamera"), Glyph::Camera);
        assert_eq!(glyph_for("Microsoft.3DBuilder"), Glyph::Doc);
        for app in secblitz::debloat::catalog() {
            let _ = glyph_for(app.family);
        }
    }


    const BOUNDS: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 512.0,
        height: HEIGHT,
    };

    fn frame(at: Instant) -> Event {
        Event::Window(window::Event::RedrawRequested(at))
    }

    fn wants_frame(a: Option<Action<()>>) -> bool {
        a.map(|a| a.into_inner().1 == window::RedrawRequest::NextFrame)
            .unwrap_or(false)
    }

    fn tick(st: &mut State, m: &StartMenu, cursor: mouse::Cursor, clock: &mut Instant) -> bool {
        *clock += Duration::from_millis(16);
        wants_frame(canvas::Program::<()>::update(m, st, &frame(*clock), BOUNDS, cursor))
    }

    fn px(p: Point) -> Point {
        StartMenu::stage(BOUNDS.size()).point(p)
    }

    #[test]
    fn working_loops_then_the_result_settles_and_goes_quiet() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let off = mouse::Cursor::Available(Point::new(2.0, 2.0));
        let mut st = State::default();
        let mut clock = t0;
        let working = menu(vec![Fate::Busy, Fate::Waiting], 9, Outcome::Working, t0);
        for _ in 0..20 {
            assert!(tick(&mut st, &working, off, &mut clock));
        }
        let done = menu(
            vec![Fate::Removed(clock), Fate::Removed(clock)],
            9,
            Outcome::Removed,
            clock,
        );
        let mut frames = 0;
        while tick(&mut st, &done, off, &mut clock) {
            frames += 1;
            assert!(frames < 600, "never settled");
        }
        assert!(frames > 30);
        let sc = done.scene(clock);
        assert_eq!(sc.places.iter().filter(|p| **p == Place::Gone).count(), 2);
        assert_eq!(st.tiles[2].x.value, slot_centre(1).x);
        anim::set_reduced_override(None);
    }

    #[test]
    fn hovering_and_clicking_a_tile_nudges_it() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        let t0 = Instant::now();
        let m = menu(vec![Fate::Refused(t0)], 9, Outcome::Partly, t0);
        let mut st = State::default();
        let mut clock = t0 + Duration::from_secs(10);
        let p = px(slot_centre(4));
        let over = mouse::Cursor::Available(p);
        canvas::Program::<()>::update(
            &m,
            &mut st,
            &Event::Mouse(mouse::Event::CursorMoved { position: p }),
            BOUNDS,
            over,
        );
        assert_eq!(st.live.hover, Some(Part::Tile(4)));
        assert_eq!(
            canvas::Program::<()>::mouse_interaction(&m, &st, BOUNDS, over),
            mouse::Interaction::Pointer
        );
        let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        canvas::Program::<()>::update(&m, &mut st, &down, BOUNDS, over);
        canvas::Program::<()>::update(&m, &mut st, &up, BOUNDS, over);
        assert!(st.tiles[4].bump.is_some());
        assert!(!st.live.pulses.alive());
        let mut frames = 0;
        while tick(&mut st, &m, over, &mut clock) {
            frames += 1;
            assert!(frames < 600, "never settled");
        }
        assert!(frames > 10);
        anim::set_reduced_override(None);
    }

    #[test]
    fn reduced_motion_is_still_but_still_answers() {
        let _g = MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(true));
        let t0 = Instant::now();
        let m = menu(
            vec![Fate::Busy, Fate::Removed(t0), Fate::Waiting],
            9,
            Outcome::Working,
            t0,
        );
        let mut st = State::default();
        let off = mouse::Cursor::Available(Point::new(2.0, 2.0));
        assert!(!wants_frame(canvas::Program::<()>::update(
            &m,
            &mut st,
            &frame(t0),
            BOUNDS,
            off
        )));
        let sc = m.scene(t0);
        assert_eq!(sc.places[5], Place::Gone);
        assert_eq!(st.tiles[6].x.value, slot_centre(5).x);
        let p = px(slot_centre(1));
        let over = mouse::Cursor::Available(p);
        canvas::Program::<()>::update(
            &m,
            &mut st,
            &Event::Mouse(mouse::Event::CursorMoved { position: p }),
            BOUNDS,
            over,
        );
        assert_eq!(st.live.hover, Some(Part::Tile(1)));
        assert!(!wants_frame(canvas::Program::<()>::update(
            &m,
            &mut st,
            &frame(t0 + Duration::from_millis(16)),
            BOUNDS,
            over
        )));
        anim::set_reduced_override(None);
    }
}
