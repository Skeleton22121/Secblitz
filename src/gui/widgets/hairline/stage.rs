//! The drawing's own unit box fitted into the canvas, and the inks.
//!
//! A drawing is designed in units (the prototype's `viewBox`, for example 320
//! by 256). [`Stage::fit`] scales that box to fit the canvas bounds, keeps
//! its aspect and centres it. Every path is mapped to pixels by hand
//! ([`Stage::point`], [`Stage::path`], [`Stage::shape`]) and never with
//! `Frame::scale`: the GPU renderer would keep line widths in screen pixels
//! but the CPU renderer would scale them, so the drawing would look different
//! on customer PCs without a GPU. Translations and rotations on the frame are
//! fine on both.
//!
//! [`Ink`] holds the colours a drawing may use and the prototype's line
//! weights, so every drawing looks like it belongs to the same set.
use super::glyph::Glyph;
use super::svg::{ellipse_cubics, ellipse_point, PathData, Seg};
use crate::gui::theme::{mix, Palette};
use iced::widget::canvas::path::{arc::Elliptical, Builder};
use iced::widget::canvas::{LineCap, LineJoin, Path, Stroke};
use iced::{mouse, Color, Point, Radians, Rectangle, Size, Vector};

/// Shorthand for a point in units.
pub const fn pt(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

/// A unit box fitted and centred in a canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage {
    /// The drawing's own box, in units.
    pub units: Size,
    /// Pixels per unit.
    pub k: f32,
    /// Where unit (0, 0) sits, in pixels from the canvas's top left.
    pub origin: Point,
}

impl Stage {
    /// Fit `units` inside `bounds` (canvas size in pixels), keeping its
    /// aspect, centred. A zero-sized canvas gives `k = 0` (nothing visible).
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

    /// The same stage with everything moved by `by` units: one parallax
    /// layer (`stage.shifted(tilt.offset(Layer::Front))`) or a group that
    /// slides as one piece.
    pub fn shifted(&self, by: Vector) -> Stage {
        Stage {
            origin: Point::new(self.origin.x + by.x * self.k, self.origin.y + by.y * self.k),
            ..*self
        }
    }

    /// A group placed at `centre` and scaled by `scale` (the prototype's
    /// `translate(x y) scale(s)`): unit (0, 0) of the returned stage is
    /// `centre` here, and every length is `scale` times larger. Line widths
    /// stay in pixels, as they must.
    pub fn local(&self, centre: Point, scale: f32) -> Stage {
        Stage {
            units: self.units,
            k: self.k * scale.max(0.0),
            origin: self.point(centre),
        }
    }

    /// Unit point to canvas pixels.
    pub fn point(&self, p: Point) -> Point {
        Point::new(self.origin.x + p.x * self.k, self.origin.y + p.y * self.k)
    }

    /// Unit coordinates to canvas pixels.
    pub fn px(&self, x: f32, y: f32) -> Point {
        self.point(Point::new(x, y))
    }

    /// A length in units to pixels (radii, offsets). Not for line widths:
    /// those are pixels already, see [`stroke`].
    pub fn len(&self, units: f32) -> f32 {
        units * self.k
    }

    /// Canvas pixels (from the canvas's top left) back to units.
    pub fn to_units(self, px: Point) -> Point {
        if self.k <= 0.0 {
            return Point::ORIGIN;
        }
        Point::new((px.x - self.origin.x) / self.k, (px.y - self.origin.y) / self.k)
    }

    /// Where the cursor is, in units, wherever it is in the window (so a
    /// drag can leave the canvas). `None` when the window has no cursor.
    pub fn cursor(&self, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Point> {
        let p = cursor.position()?;
        Some(self.to_units(Point::new(p.x - bounds.x, p.y - bounds.y)))
    }

    /// Whether a unit point is inside the unit box.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= 0.0 && p.y >= 0.0 && p.x <= self.units.width && p.y <= self.units.height
    }

    /// Build a path in units with a [`Sketch`]; it comes out in pixels.
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

    /// Parsed path data (in units) as a pixel path.
    pub fn shape(&self, d: &PathData) -> Path {
        d.to_path(|p| self.point(p))
    }

    /// Parse-and-map for data written inline. Prefer parsing once (a
    /// `OnceLock` or the drawing's state) for paths drawn every frame.
    pub fn svg(&self, d: &str) -> Path {
        self.shape(&PathData::of(d))
    }

    /// Icon `g` centred on `centre` at `size` units square.
    pub fn icon(&self, g: Glyph, centre: Point, size: f32) -> Path {
        let k = size / 24.0;
        let o = Point::new(centre.x - size / 2.0, centre.y - size / 2.0);
        g.data()
            .to_path(|p| self.px(o.x + p.x * k, o.y + p.y * k))
    }

    /// Rounded rectangle, top left `x, y`, in units (the prototype's `rr`).
    pub fn rounded_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
        Path::rounded_rectangle(
            self.px(x, y),
            Size::new(self.len(w), self.len(h)),
            self.len(r.max(0.0)).into(),
        )
    }

    /// Plain rectangle, top left `x, y`, in units.
    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Path {
        Path::rectangle(self.px(x, y), Size::new(self.len(w), self.len(h)))
    }

    pub fn circle(&self, c: Point, r: f32) -> Path {
        Path::circle(self.point(c), self.len(r.max(0.0)))
    }

    /// Whole ellipse, closed.
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

    /// Straight lines through `pts`, closed when asked (the prototype's `P`).
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

    /// Open elliptical arc around `c` from angle `a0` to `a1` (radians,
    /// growing angles turn clockwise on screen; the prototype's `arc`).
    pub fn arc(&self, c: Point, rx: f32, ry: f32, a0: f32, a1: f32) -> Path {
        self.path(|s| s.arc(c, rx, ry, a0, a1))
    }
}

/// A path builder that takes units. Arcs continue the current sub-path (iced's
/// own `Builder::ellipse` would start a new one and break fills).
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
    /// Elliptical arc around `c` from angle `a0` to `a1`; a line joins the
    /// pen to the arc's start (like a canvas `arc`), or the arc starts a new
    /// sub-path when the pen is up.
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
    /// Append parsed segments (already in units).
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
    /// The stage this sketch maps through.
    pub fn stage(&self) -> Stage {
        self.stage
    }
}

// ---------------------------------------------------------------------------
// Inks
// ---------------------------------------------------------------------------

/// A round-capped, round-joined stroke `width_px` logical pixels wide in
/// `color`. Widths are screen pixels whatever the stage scale, so drawings
/// look the same at every size and on both renderers. Keep visible lines
/// within [`W_FAINT`]..=[`W_THICK`].
pub fn stroke(color: Color, width_px: f32) -> Stroke<'static> {
    Stroke::default()
        .with_width(width_px)
        .with_color(color)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
}

/// How much of a part's colour goes into its fill.
pub const TINT: f32 = 0.13;

/// `color` mixed [`TINT`] (13 percent) into `plate`: the fill of a coloured
/// part. Opaque, so it hides what is behind like a plate does.
pub fn tint(color: Color, plate: Color) -> Color {
    tint_by(color, plate, TINT)
}

/// `color` mixed `amount` (0..=1) into `plate`.
pub fn tint_by(color: Color, plate: Color, amount: f32) -> Color {
    mix(plate, color, amount)
}

// Line weights from the prototype's classes, in logical pixels.
/// `.ln2` faint detail and `.lo` rules.
pub const W_FAINT: f32 = 1.0;
/// `.ln`, the ordinary outline.
pub const W_LINE: f32 = 1.25;
/// `.k`, the outline of a coloured part (badges, tiles).
pub const W_PART: f32 = 1.35;
/// `.ink`, the strongest grey line.
pub const W_INK: f32 = 1.5;
/// `.gk`, a done mark (small ticks).
pub const W_MARK: f32 = 1.6;
/// `.ac`, an accent line (work in progress).
pub const W_ACCENT: f32 = 1.75;
/// `.thick`, big marks drawn in (the shield's tick, cross, exclamation).
pub const W_THICK: f32 = 2.5;

/// The background a drawing sits on: plates (the parts that hide what is
/// behind them) are filled with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Plate {
    /// The window background (a drawing straight on the page).
    Bg,
    /// A region or sheet.
    #[default]
    Surface,
}

/// What a colour says. These are the only four colours a drawing may use;
/// everything else stays grey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meaning {
    /// Work in progress: the lens looking, a switch turning on, water
    /// filling, the globe turning, a removal highlight. `Palette::accent`.
    Working,
    /// Done or protected: ticks, the dome when on, the filled shield.
    Done,
    /// Needs attention, partly done, ads and trackers.
    Attention,
    /// Failed, scam sites.
    Failed,
}

/// The colours a drawing draws with, resolved for the theme and the plate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ink {
    /// Fill of plates: the background the drawing sits on.
    pub plate: Color,
    /// Ordinary outlines (`text_muted`).
    pub line: Color,
    /// Faint detail (`disabled_fg`).
    pub faint: Color,
    /// Rules, rings and screen edges (`border_strong`).
    pub rule: Color,
    /// The strongest grey (`text`).
    pub ink: Color,
    pub accent: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
    /// Tooltip label colour (`surface`).
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

    /// The colour for a meaning.
    pub fn of(&self, m: Meaning) -> Color {
        match m {
            Meaning::Working => self.accent,
            Meaning::Done => self.good,
            Meaning::Attention => self.warn,
            Meaning::Failed => self.bad,
        }
    }

    /// Fill for a part coloured `c`: 13 percent of it in the plate.
    pub fn tint(&self, c: Color) -> Color {
        tint(c, self.plate)
    }

    /// `.ln`: ordinary outline.
    pub fn ln(&self) -> Stroke<'static> {
        stroke(self.line, W_LINE)
    }
    /// `.ln2`: faint detail.
    pub fn ln2(&self) -> Stroke<'static> {
        stroke(self.faint, W_FAINT)
    }
    /// `.lo`: rules and rings.
    pub fn lo(&self) -> Stroke<'static> {
        stroke(self.rule, W_FAINT)
    }
    /// `.ink`: strongest grey line.
    pub fn strong(&self) -> Stroke<'static> {
        stroke(self.ink, W_INK)
    }
    /// `.k`: outline of a part coloured `c` (grey parts use `self.line`).
    pub fn part(&self, c: Color) -> Stroke<'static> {
        stroke(c, W_PART)
    }
    /// `.ac`: an accent line in `c` (usually `self.accent`, or the
    /// meaning's colour when a result is drawn in).
    pub fn accent_line(&self, c: Color) -> Stroke<'static> {
        stroke(c, W_ACCENT)
    }
    /// `.ac.thick`: a big mark in `c`.
    pub fn thick(&self, c: Color) -> Stroke<'static> {
        stroke(c, W_THICK)
    }
    /// `.knock`: a plate-coloured line `width_px` wide that cuts a gap
    /// where one line passes behind another. Not a visible line, so it may
    /// be wider than [`W_THICK`].
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
        // Wider than tall: centred across.
        let w = Stage::fit(Size::new(320.0, 256.0), Size::new(1000.0, 256.0));
        assert_eq!(w.k, 1.0);
        assert_eq!(w.origin, Point::new(340.0, 0.0));
        // A layer shifted by 2 units moves 2 * k pixels.
        let f = s.shifted(Vector::new(2.0, -1.0));
        assert_eq!(f.px(0.0, 0.0), Point::new(4.0, 42.0));
        // A group at (10, 20) twice as large.
        let g = s.local(Point::new(10.0, 20.0), 2.0);
        assert_eq!(g.px(0.0, 0.0), s.px(10.0, 20.0));
        assert_eq!(g.px(1.0, 0.0), Point::new(s.px(10.0, 20.0).x + 4.0, 84.0));
        // Nothing to draw into.
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
            // A tint is mostly plate, a little colour, and opaque.
            let t = on_surface.tint(p.good);
            assert_eq!(t.a, 1.0);
            let d_plate = (t.g - p.surface.g).abs();
            let d_good = (t.g - p.good.g).abs();
            assert!(d_plate < d_good);
            assert!((t.r - (p.surface.r + (p.good.r - p.surface.r) * TINT)).abs() < 1e-6);
            // Accent is the blue: more blue than red in both themes.
            assert!(p.accent.b > p.accent.r);
        }
        assert_ne!(LIGHT.accent, DARK.accent);
    }
}
