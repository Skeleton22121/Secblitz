//! The pointer over a drawing, the parts it can point at, and their names.
use super::parallax::Parallax;
use super::stage::Stage;
use crate::gui::theme::{self, Palette};
use iced::advanced::text::{self as adv_text, Paragraph as _};
use iced::widget::canvas::{Event, Frame, Path, Text};
use iced::widget::text::{LineHeight, Shaping, Wrapping};
use iced::{mouse, touch, Pixels, Point, Rectangle, Renderer, Size, Vector};

pub const CLICK_SLOP: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    pub at: Point,
    pub inside: bool,
    pub pressed: bool,
    pub drag: Vector,
    pub was_click: bool,
}

impl Default for Pointer {
    fn default() -> Self {
        Pointer {
            at: Point::ORIGIN,
            inside: false,
            pressed: false,
            drag: Vector::ZERO,
            was_click: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    Hover,
    Press(Point),
    Drag { delta: Vector, at: Point },
    Release { at: Point, click: bool },
}

impl Pointer {
    pub fn handle(
        &mut self,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        stage: &Stage,
    ) -> Option<Gesture> {
        let to_units = |p: Point| stage.to_units(Point::new(p.x - bounds.x, p.y - bounds.y));
        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let pos = cursor.position().unwrap_or(*position);
                self.moved(to_units(pos), bounds.contains(pos))
            }
            Event::Touch(touch::Event::FingerMoved { position, .. }) => {
                self.moved(to_units(*position), bounds.contains(*position))
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let pos = cursor.position_over(bounds)?;
                Some(self.pressed_at(to_units(pos)))
            }
            Event::Touch(touch::Event::FingerPressed { position, .. }) => {
                if !bounds.contains(*position) {
                    return None;
                }
                self.inside = true;
                Some(self.pressed_at(to_units(*position)))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let inside = cursor.is_over(bounds);
                if let Some(pos) = cursor.position() {
                    self.at = to_units(pos);
                }
                self.released(inside)
            }
            Event::Touch(touch::Event::FingerLifted { position, .. }) => {
                self.at = to_units(*position);
                let g = self.released(bounds.contains(*position));
                self.inside = false;
                g
            }
            Event::Touch(touch::Event::FingerLost { .. }) => {
                let was = self.pressed || self.inside;
                self.pressed = false;
                self.inside = false;
                was.then_some(Gesture::Hover)
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                let was = self.inside || self.pressed;
                self.inside = false;
                self.pressed = false;
                was.then_some(Gesture::Hover)
            }
            _ => None,
        }
    }

    fn moved(&mut self, at: Point, inside: bool) -> Option<Gesture> {
        if self.pressed {
            let delta = Vector::new(at.x - self.at.x, at.y - self.at.y);
            self.drag += delta;
            self.at = at;
            self.inside = inside;
            return Some(Gesture::Drag { delta, at });
        }
        if at == self.at && inside == self.inside {
            return None;
        }
        self.at = at;
        self.inside = inside;
        Some(Gesture::Hover)
    }

    fn pressed_at(&mut self, at: Point) -> Gesture {
        self.at = at;
        self.inside = true;
        self.pressed = true;
        self.drag = Vector::ZERO;
        self.was_click = false;
        Gesture::Press(at)
    }

    fn released(&mut self, inside: bool) -> Option<Gesture> {
        if !self.pressed {
            return None;
        }
        self.pressed = false;
        self.inside = inside;
        let moved = (self.drag.x * self.drag.x + self.drag.y * self.drag.y).sqrt();
        self.was_click = inside && moved <= CLICK_SLOP;
        Some(Gesture::Release {
            at: self.at,
            click: self.was_click,
        })
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layer {
    Back,
    Mid,
    #[default]
    Front,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Area {
    Circle { centre: Point, r: f32 },
    Rect { centre: Point, w: f32, h: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot<Id> {
    pub id: Id,
    pub area: Area,
    pub layer: Layer,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hotspots<Id> {
    pub spots: Vec<Spot<Id>>,
}

impl<Id> Default for Hotspots<Id> {
    fn default() -> Self {
        Hotspots { spots: Vec::new() }
    }
}

impl<Id: Copy + PartialEq> Hotspots<Id> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn circle(mut self, id: Id, centre: Point, r: f32, layer: Layer) -> Self {
        self.spots.push(Spot {
            id,
            area: Area::Circle { centre, r },
            layer,
        });
        self
    }

    pub fn rect(mut self, id: Id, centre: Point, w: f32, h: f32, layer: Layer) -> Self {
        self.spots.push(Spot {
            id,
            area: Area::Rect { centre, w, h },
            layer,
        });
        self
    }

    pub fn push(&mut self, spot: Spot<Id>) {
        self.spots.push(spot);
    }

    pub fn hit(&self, at: Point, tilt: &Parallax) -> Option<Id> {
        let mut best = None;
        let mut best_d = f32::INFINITY;
        for s in &self.spots {
            let o = tilt.offset(s.layer);
            let d = match s.area {
                Area::Circle { centre, r } => {
                    let d = (at.x - centre.x - o.x).hypot(at.y - centre.y - o.y);
                    if d < r {
                        d
                    } else {
                        continue;
                    }
                }
                Area::Rect { centre, w, h } => {
                    let dx = (at.x - centre.x - o.x).abs();
                    let dy = (at.y - centre.y - o.y).abs();
                    if dx < w / 2.0 && dy < h / 2.0 {
                        dy
                    } else {
                        continue;
                    }
                }
            };
            if d < best_d {
                best_d = d;
                best = Some(s.id);
            }
        }
        best
    }

    pub fn anchor(&self, id: Id, tilt: &Parallax) -> Option<Point> {
        let s = self.spots.iter().find(|s| s.id == id)?;
        let o = tilt.offset(s.layer);
        let (c, r) = match s.area {
            Area::Circle { centre, r } => (centre, r),
            Area::Rect { centre, h, .. } => (centre, h / 2.0),
        };
        Some(Point::new(c.x + o.x, c.y + o.y - r * 0.7))
    }

    pub fn below(&self, id: Id, tilt: &Parallax) -> Option<Point> {
        let s = self.spots.iter().find(|s| s.id == id)?;
        let o = tilt.offset(s.layer);
        let (c, r) = match s.area {
            Area::Circle { centre, r } => (centre, r),
            Area::Rect { centre, h, .. } => (centre, h / 2.0),
        };
        Some(Point::new(c.x + o.x, c.y + o.y + r))
    }
}


pub const TIP_SIZE: f32 = 12.0;
const TIP_PAD_X: f32 = 8.0;
const TIP_PAD_Y: f32 = 6.0;
const TIP_RADIUS: f32 = 6.0;
const TIP_GAP: f32 = 6.0;
const TIP_MARGIN: f32 = 2.0;
pub const TIP_ROOM: f32 = TIP_MARGIN + TIP_GAP + TIP_SIZE + 2.0 * TIP_PAD_Y;

pub fn label_width(label: &str) -> f32 {
    thread_local! {
        static LAST: std::cell::RefCell<(String, f32)> = const { std::cell::RefCell::new((String::new(), 0.0)) };
    }
    if let Some(w) = LAST.with(|l| {
        let l = l.borrow();
        (l.0 == label).then_some(l.1)
    }) {
        return w;
    }
    let w = measure(label);
    LAST.with(|l| *l.borrow_mut() = (label.to_string(), w));
    w
}

fn measure(label: &str) -> f32 {
    if label.is_empty() {
        return 0.0;
    }
    let p = <<Renderer as adv_text::Renderer>::Paragraph as adv_text::Paragraph>::with_text(
        adv_text::Text {
            content: label,
            bounds: Size::INFINITE,
            size: Pixels(TIP_SIZE),
            line_height: LineHeight::Absolute(Pixels(TIP_SIZE)),
            font: theme::MEDIUM,
            align_x: adv_text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: Shaping::Advanced,
            wrapping: Wrapping::None,
        },
    );
    p.min_width()
}

#[cfg(test)]
pub fn tooltip_rect(anchor: Point, text_width: f32, canvas: Size) -> Rectangle {
    tooltip_rect_around(anchor, anchor, text_width, canvas)
}

pub fn tooltip_rect_around(
    above: Point,
    below: Point,
    text_width: f32,
    canvas: Size,
) -> Rectangle {
    let w = (text_width + 2.0 * TIP_PAD_X).ceil();
    let h = TIP_SIZE + 2.0 * TIP_PAD_Y;
    let max_x = (canvas.width - w - TIP_MARGIN).max(TIP_MARGIN);
    let mut x = above.x;
    let mut y = above.y - TIP_GAP - h;
    if y < TIP_MARGIN {
        x = below.x;
        y = below.y + TIP_GAP;
    }
    let x = (x - w / 2.0).clamp(TIP_MARGIN, max_x);
    let max_y = (canvas.height - h - TIP_MARGIN).max(TIP_MARGIN);
    Rectangle::new(
        Point::new(x.round(), y.clamp(TIP_MARGIN, max_y).round()),
        Size::new(w, h),
    )
}

pub fn tooltip(frame: &mut Frame, p: &Palette, stage: &Stage, anchor: Point, label: &str) {
    tooltip_around(frame, p, stage, anchor, anchor, label);
}

pub fn tooltip_around(
    frame: &mut Frame,
    p: &Palette,
    stage: &Stage,
    above: Point,
    below: Point,
    label: &str,
) {
    if label.is_empty() {
        return;
    }
    let r = tooltip_rect_around(
        stage.point(above),
        stage.point(below),
        label_width(label),
        frame.size(),
    );
    frame.fill(
        &Path::rounded_rectangle(r.position(), r.size(), TIP_RADIUS.into()),
        p.text,
    );
    frame.fill_text(Text {
        content: label.to_string(),
        position: r.center(),
        color: p.surface,
        size: Pixels(TIP_SIZE),
        line_height: LineHeight::Absolute(Pixels(TIP_SIZE)),
        font: theme::MEDIUM,
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: iced::alignment::Vertical::Center,
        shaping: Shaping::Advanced,
        ..Text::default()
    });
}

pub fn interaction(
    pointer: &Pointer,
    hovering: bool,
    draggable: bool,
    bounds: Rectangle,
    cursor: mouse::Cursor,
) -> mouse::Interaction {
    if draggable && pointer.pressed {
        return mouse::Interaction::Grabbing;
    }
    if !cursor.is_over(bounds) {
        return mouse::Interaction::None;
    }
    if hovering {
        mouse::Interaction::Pointer
    } else if draggable {
        mouse::Interaction::Grab
    } else {
        mouse::Interaction::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage() -> Stage {
        Stage::fit(Size::new(100.0, 100.0), Size::new(200.0, 200.0))
    }
    const B: Rectangle = Rectangle {
        x: 10.0,
        y: 10.0,
        width: 200.0,
        height: 200.0,
    };
    fn at(x: f32, y: f32) -> mouse::Cursor {
        mouse::Cursor::Available(Point::new(x, y))
    }
    fn mv(x: f32, y: f32) -> Event {
        Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(x, y),
        })
    }
    const DOWN: Event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    const UP: Event = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));

    #[test]
    fn click_is_a_press_and_release_in_place() {
        let s = stage();
        let mut p = Pointer::default();
        assert_eq!(p.handle(&mv(110.0, 110.0), B, at(110.0, 110.0), &s), Some(Gesture::Hover));
        assert!(p.inside);
        assert_eq!(p.at, Point::new(50.0, 50.0));
        assert_eq!(
            p.handle(&DOWN, B, at(110.0, 110.0), &s),
            Some(Gesture::Press(Point::new(50.0, 50.0)))
        );
        p.handle(&mv(116.0, 110.0), B, at(116.0, 110.0), &s);
        assert_eq!(
            p.handle(&UP, B, at(116.0, 110.0), &s),
            Some(Gesture::Release {
                at: Point::new(53.0, 50.0),
                click: true
            })
        );
        assert!(p.was_click && !p.pressed);
        assert_eq!(p.handle(&mv(116.0, 110.0), B, at(116.0, 110.0), &s), None);
    }

    #[test]
    fn drag_is_not_a_click_and_may_leave() {
        let s = stage();
        let mut p = Pointer::default();
        p.handle(&DOWN, B, at(110.0, 110.0), &s);
        let g = p.handle(&mv(130.0, 110.0), B, at(130.0, 110.0), &s);
        assert_eq!(
            g,
            Some(Gesture::Drag {
                delta: Vector::new(10.0, 0.0),
                at: Point::new(60.0, 50.0)
            })
        );
        p.handle(&mv(400.0, 110.0), B, at(400.0, 110.0), &s);
        assert!(p.pressed && !p.inside);
        assert_eq!(
            p.handle(&UP, B, at(400.0, 110.0), &s),
            Some(Gesture::Release {
                at: Point::new(195.0, 50.0),
                click: false
            })
        );
        assert_eq!(p.handle(&DOWN, B, at(300.0, 300.0), &s), None);
        assert_eq!(p.handle(&UP, B, at(300.0, 300.0), &s), None);
    }

    #[test]
    fn hotspots_pick_the_nearest_and_follow_their_layer() {
        let still = Parallax::off();
        let spots = Hotspots::new()
            .circle(1, Point::new(20.0, 20.0), 14.0, Layer::Front)
            .circle(2, Point::new(40.0, 20.0), 14.0, Layer::Front)
            .rect(3, Point::new(50.0, 80.0), 80.0, 10.0, Layer::Mid);
        assert_eq!(spots.hit(Point::new(25.0, 20.0), &still), Some(1));
        assert_eq!(spots.hit(Point::new(35.0, 20.0), &still), Some(2));
        assert_eq!(spots.hit(Point::new(50.0, 82.0), &still), Some(3));
        assert_eq!(spots.hit(Point::new(95.0, 82.0), &still), None);
        assert_eq!(spots.hit(Point::new(70.0, 40.0), &still), None);
        assert_eq!(spots.anchor(1, &still), Some(Point::new(20.0, 20.0 - 14.0 * 0.7)));
        assert_eq!(spots.anchor(3, &still), Some(Point::new(50.0, 80.0 - 3.5)));
        assert_eq!(spots.anchor(9, &still), None);
        let _m = crate::gui::widgets::anim::forced::set(false);
        let mut tilt = Parallax::new();
        tilt.x.value = 1.0;
        let o = tilt.offset(Layer::Front);
        assert!(o.x > 0.0);
        let edge = Point::new(20.0 + o.x, 20.0 - 13.5);
        assert_eq!(spots.hit(edge, &tilt), Some(1));
        assert_eq!(spots.hit(edge, &still), None);
    }

    #[test]
    fn tooltip_stays_inside_and_sits_above() {
        let c = Size::new(320.0, 256.0);
        let r = tooltip_rect(Point::new(160.0, 100.0), 60.0, c);
        assert_eq!(r.width, 76.0);
        assert_eq!(r.height, 24.0);
        assert_eq!(r.x, 122.0);
        assert_eq!(r.y + r.height, 94.0);
        let l = tooltip_rect(Point::new(5.0, 100.0), 60.0, c);
        assert_eq!(l.x, 2.0);
        let rr = tooltip_rect(Point::new(318.0, 100.0), 60.0, c);
        assert_eq!(rr.x + rr.width, 318.0);
        let b = tooltip_rect(Point::new(160.0, 10.0), 60.0, c);
        assert_eq!(b.y, 16.0);
        let (top, bottom) = (Point::new(160.0, 10.0), Point::new(160.0, 40.0));
        let under = tooltip_rect_around(top, bottom, 60.0, c);
        assert_eq!(under.y, 46.0);
        let (top, bottom) = (Point::new(160.0, 100.0), Point::new(160.0, 130.0));
        let over = tooltip_rect_around(top, bottom, 60.0, c);
        assert_eq!(over, r);
    }

    #[test]
    fn labels_are_measured() {
        let short = label_width("Firewall");
        let long = label_width("Firewall and network protection");
        assert!(short > 20.0 && long > short * 2.0, "{short} {long}");
        assert_eq!(label_width(""), 0.0);
    }

    #[test]
    fn cursor_shapes() {
        let p = Pointer::default();
        let over = at(50.0, 50.0);
        let away = at(500.0, 500.0);
        use mouse::Interaction as I;
        assert_eq!(interaction(&p, true, false, B, over), I::Pointer);
        assert_eq!(interaction(&p, false, true, B, over), I::Grab);
        assert_eq!(interaction(&p, false, false, B, over), I::None);
        assert_eq!(interaction(&p, true, false, B, away), I::None);
        let held = Pointer {
            pressed: true,
            ..p
        };
        assert_eq!(interaction(&held, false, true, B, away), I::Grabbing);
    }
}
