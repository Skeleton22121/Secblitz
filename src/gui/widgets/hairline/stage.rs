//! The drawing's own unit box fitted into the canvas, and the inks.
use super::glyph::Glyph;
use super::svg::{ellipse_cubics, ellipse_point, PathData, Seg};
use crate::gui::theme::{mix, Palette};
use iced::widget::canvas::path::{arc::Elliptical, Builder};
use iced::widget::canvas::{LineCap, LineJoin, Path, Stroke};
use iced::{mouse, Color, Point, Radians, Rectangle, Size, Vector};

pub const fn pt(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage {
    pub units: Size,
    pub k: f32,
    pub origin: Point,
}

impl Stage {
    pub fn fit(units: Size, bounds: Size) -> Stage {
        let k = if units.width > 0.0 && units.height > 0.0 {
            (bounds.width / units.width)
                .min(bounds.height / units.height)
                .max(0.0)
        } else {
            0.0
        };
        Stage {
            units,
            k,
            origin: Point::new(
                (bounds.width - units.width * k) / 2.0,
                (bounds.height - units.height * k) / 2.0,
            ),
        }
    }

    pub fn shifted(&self, by: Vector) -> Stage {
        Stage {
            origin: Point::new(self.origin.x + by.x * self.k, self.origin.y + by.y * self.k),
            ..*self
        }
    }

    pub fn local(&self, centre: Point, scale: f32) -> Stage {
        Stage {
            units: self.units,
            k: self.k * scale.max(0.0),
            origin: self.point(centre),
        }
    }

    pub fn point(&self, p: Point) -> Point {
        Point::new(self.origin.x + p.x * self.k, self.origin.y + p.y * self.k)
    }

    pub fn px(&self, x: f32, y: f32) -> Point {
        self.point(Point::new(x, y))
    }

    pub fn len(&self, units: f32) -> f32 {
        units * self.k
    }

    pub fn to_units(self, px: Point) -> Point {
        if self.k <= 0.0 {
            return Point::ORIGIN;
        }
        Point::new((px.x - self.origin.x) / self.k, (px.y - self.origin.y) / self.k)
    }

    pub fn cursor(&self, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Point> {
        let p = cursor.position()?;
        Some(self.to_units(Point::new(p.x - bounds.x, p.y - bounds.y)))
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= 0.0 && p.y >= 0.0 && p.x <= self.units.width && p.y <= self.units.height
    }

    pub fn path(&self, f: impl FnOnce(&mut Sketch<'_>)) -> Path {
        Path::new(|b| {
            let mut s = Sketch {
                b,
                stage: *self,
                pen: None,
                start: None,
            };
            f(&mut s);
        })
    }

    pub fn shape(&self, d: &PathData) -> Path {
        d.to_path(|p| self.point(p))
    }

    pub fn svg(&self, d: &str) -> Path {
        self.shape(&PathData::of(d))
    }

    pub fn icon(&self, g: Glyph, centre: Point, size: f32) -> Path {
        let k = size / 24.0;
        let o = Point::new(centre.x - size / 2.0, centre.y - size / 2.0);
        g.data()
            .to_path(|p| self.px(o.x + p.x * k, o.y + p.y * k))
    }

    pub fn rounded_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
        Path::rounded_rectangle(
            self.px(x, y),
            Size::new(self.len(w), self.len(h)),
            self.len(r.max(0.0)).into(),
        )
    }

    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Path {
        Path::rectangle(self.px(x, y), Size::new(self.len(w), self.len(h)))
    }

    pub fn circle(&self, c: Point, r: f32) -> Path {
        Path::circle(self.point(c), self.len(r.max(0.0)))
    }

    pub fn ellipse(&self, c: Point, rx: f32, ry: f32) -> Path {
        Path::new(|b| {
            b.ellipse(Elliptical {
                center: self.point(c),
                radii: Vector::new(self.len(rx.max(0.0)), self.len(ry.max(0.0))),
                rotation: Radians(0.0),
                start_angle: Radians(0.0),
                end_angle: Radians(std::f32::consts::TAU),
            });
            b.close();
        })
    }

    pub fn line(&self, a: Point, b: Point) -> Path {
        Path::line(self.point(a), self.point(b))
    }

    pub fn polyline(&self, pts: &[Point], close: bool) -> Path {
        self.path(|s| {
            for (i, p) in pts.iter().enumerate() {
                if i == 0 {
                    s.move_to(*p);
                } else {
                    s.line_to(*p);
                }
            }
            if close && pts.len() > 2 {
                s.close();
            }
        })
    }

    pub fn arc(&self, c: Point, rx: f32, ry: f32, a0: f32, a1: f32) -> Path {
        self.path(|s| s.arc(c, rx, ry, a0, a1))
    }
}

pub struct Sketch<'a> {
    b: &'a mut Builder,
    stage: Stage,
    pen: Option<Point>,
    start: Option<Point>,
}

impl Sketch<'_> {
    pub fn move_to(&mut self, p: Point) {
        self.b.move_to(self.stage.point(p));
        self.pen = Some(p);
        self.start = Some(p);
    }
    pub fn line_to(&mut self, p: Point) {
        if self.pen.is_none() {
            return self.move_to(p);
        }
        self.b.line_to(self.stage.point(p));
        self.pen = Some(p);
    }
    pub fn quad_to(&mut self, c: Point, p: Point) {
        if self.pen.is_none() {
            self.move_to(c);
        }
        self.b
            .quadratic_curve_to(self.stage.point(c), self.stage.point(p));
        self.pen = Some(p);
    }
    pub fn bezier_to(&mut self, c1: Point, c2: Point, p: Point) {
        if self.pen.is_none() {
            self.move_to(c1);
        }
        self.b.bezier_curve_to(
            self.stage.point(c1),
            self.stage.point(c2),
            self.stage.point(p),
        );
        self.pen = Some(p);
    }
    pub fn arc(&mut self, c: Point, rx: f32, ry: f32, a0: f32, a1: f32) {
        let cc = (c.x as f64, c.y as f64);
        let r = (rx as f64, ry as f64);
        let from = ellipse_point(cc, r, 0.0, a0 as f64);
        if self.pen.is_none() {
            self.move_to(from);
        } else {
            self.line_to(from);
        }
        let mut segs = Vec::new();
        ellipse_cubics(&mut segs, cc, r, 0.0, a0 as f64, (a1 - a0) as f64);
        self.segs(&segs);
    }
    pub fn segs(&mut self, segs: &[Seg]) {
        for s in segs {
            match *s {
                Seg::Move(p) => self.move_to(p),
                Seg::Line(p) => self.line_to(p),
                Seg::Quad(c, p) => self.quad_to(c, p),
                Seg::Cubic(a, b, p) => self.bezier_to(a, b, p),
                Seg::Close => self.close(),
            }
        }
    }
    pub fn close(&mut self) {
        self.b.close();
        self.pen = self.start;
    }
    pub fn stage(&self) -> Stage {
        self.stage
    }
}


pub fn stroke(color: Color, width_px: f32) -> Stroke<'static> {
    Stroke::default()
        .with_width(width_px)
        .with_color(color)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
}

pub const TINT: f32 = 0.13;

pub fn tint(color: Color, plate: Color) -> Color {
    tint_by(color, plate, TINT)
}

pub fn tint_by(color: Color, plate: Color, amount: f32) -> Color {
    mix(plate, color, amount)
}

pub const W_FAINT: f32 = 1.0;
pub const W_LINE: f32 = 1.25;
pub const W_PART: f32 = 1.35;
pub const W_INK: f32 = 1.5;
pub const W_MARK: f32 = 1.6;
pub const W_ACCENT: f32 = 1.75;
pub const W_THICK: f32 = 2.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Plate {
    Bg,
    #[default]
    Surface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meaning {
    Working,
    Done,
    Attention,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ink {
    pub plate: Color,
    pub line: Color,
    pub faint: Color,
    pub rule: Color,
    pub ink: Color,
    pub accent: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
    pub on_ink: Color,
}

impl Ink {
    pub fn new(p: &Palette, plate: Plate) -> Ink {
        Ink {
            plate: match plate {
                Plate::Bg => p.bg,
                Plate::Surface => p.surface,
            },
            line: p.text_muted,
            faint: p.disabled_fg,
            rule: p.border_strong,
            ink: p.text,
            accent: p.accent,
            good: p.good,
            warn: p.warn,
            bad: p.bad,
            on_ink: p.surface,
        }
    }

    pub fn of(&self, m: Meaning) -> Color {
        match m {
            Meaning::Working => self.accent,
            Meaning::Done => self.good,
            Meaning::Attention => self.warn,
            Meaning::Failed => self.bad,
        }
    }

    pub fn tint(&self, c: Color) -> Color {
        tint(c, self.plate)
    }

    pub fn ln(&self) -> Stroke<'static> {
        stroke(self.line, W_LINE)
    }
    pub fn ln2(&self) -> Stroke<'static> {
        stroke(self.faint, W_FAINT)
    }
    pub fn lo(&self) -> Stroke<'static> {
        stroke(self.rule, W_FAINT)
    }
    pub fn strong(&self) -> Stroke<'static> {
        stroke(self.ink, W_INK)
    }
    pub fn part(&self, c: Color) -> Stroke<'static> {
        stroke(c, W_PART)
    }
    pub fn accent_line(&self, c: Color) -> Stroke<'static> {
        stroke(c, W_ACCENT)
    }
    pub fn thick(&self, c: Color) -> Stroke<'static> {
        stroke(c, W_THICK)
    }
    pub fn knock(&self, width_px: f32) -> Stroke<'static> {
        stroke(self.plate, width_px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::theme::{DARK, LIGHT};

    #[test]
    fn fit_centres_and_maps_both_ways() {
        let s = Stage::fit(Size::new(320.0, 256.0), Size::new(640.0, 600.0));
        assert_eq!(s.k, 2.0);
        assert_eq!(s.origin, Point::new(0.0, 44.0));
        assert_eq!(s.px(160.0, 128.0), Point::new(320.0, 300.0));
        assert_eq!(s.to_units(Point::new(320.0, 300.0)), Point::new(160.0, 128.0));
        assert_eq!(s.len(10.0), 20.0);
        let w = Stage::fit(Size::new(320.0, 256.0), Size::new(1000.0, 256.0));
        assert_eq!(w.k, 1.0);
        assert_eq!(w.origin, Point::new(340.0, 0.0));
        let f = s.shifted(Vector::new(2.0, -1.0));
        assert_eq!(f.px(0.0, 0.0), Point::new(4.0, 42.0));
        let g = s.local(Point::new(10.0, 20.0), 2.0);
        assert_eq!(g.px(0.0, 0.0), s.px(10.0, 20.0));
        assert_eq!(g.px(1.0, 0.0), Point::new(s.px(10.0, 20.0).x + 4.0, 84.0));
        let z = Stage::fit(Size::new(320.0, 256.0), Size::ZERO);
        assert_eq!(z.k, 0.0);
        assert_eq!(z.to_units(Point::new(5.0, 5.0)), Point::ORIGIN);
    }

    #[test]
    fn cursor_maps_from_window_to_units() {
        let s = Stage::fit(Size::new(100.0, 100.0), Size::new(200.0, 200.0));
        let bounds = Rectangle::new(Point::new(50.0, 30.0), Size::new(200.0, 200.0));
        let at = s.cursor(bounds, mouse::Cursor::Available(Point::new(150.0, 130.0)));
        assert_eq!(at, Some(Point::new(50.0, 50.0)));
        assert_eq!(s.cursor(bounds, mouse::Cursor::Unavailable), None);
        assert!(s.contains(Point::new(50.0, 50.0)) && !s.contains(Point::new(-1.0, 5.0)));
    }

    #[test]
    fn inks_follow_the_theme_and_the_plate() {
        for p in [LIGHT, DARK] {
            let on_bg = Ink::new(&p, Plate::Bg);
            let on_surface = Ink::new(&p, Plate::Surface);
            assert_eq!(on_bg.plate, p.bg);
            assert_eq!(on_surface.plate, p.surface);
            assert_eq!(on_bg.of(Meaning::Working), p.accent);
            assert_eq!(on_bg.of(Meaning::Done), p.good);
            assert_eq!(on_bg.of(Meaning::Attention), p.warn);
            assert_eq!(on_bg.of(Meaning::Failed), p.bad);
            let t = on_surface.tint(p.good);
            assert_eq!(t.a, 1.0);
            let d_plate = (t.g - p.surface.g).abs();
            let d_good = (t.g - p.good.g).abs();
            assert!(d_plate < d_good);
            assert!((t.r - (p.surface.r + (p.good.r - p.surface.r) * TINT)).abs() < 1e-6);
            assert!(p.accent.b > p.accent.r);
        }
        assert_ne!(LIGHT.accent, DARK.accent);
    }
}
