//! Objects several drawings share: a monitor and the tick, exclamation and cross marks.
use super::glyph::Glyph;
use super::stage::{pt, Ink, Stage};
use iced::widget::canvas::Frame;
use iced::{Color, Point, Rectangle, Size};

fn screen_box(x0: f32, y0: f32, x1: f32, y1: f32) -> Rectangle {
    Rectangle::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0))
}

#[allow(clippy::too_many_arguments)]
pub fn monitor(
    f: &mut Frame,
    s: &Stage,
    ink: &Ink,
    cx: f32,
    top: f32,
    w: f32,
    h: f32,
    screen: Color,
    alpha: f32,
) -> Rectangle {
    let x0 = cx - w / 2.0;
    let y1 = top + h;
    let a = |c: Color| c.scale_alpha(alpha);
    let line = ink.ln().with_color(a(ink.line));
    let neck = s.polyline(
        &[
            pt(cx - 10.0, y1),
            pt(cx + 10.0, y1),
            pt(cx + 14.0, y1 + 22.0),
            pt(cx - 14.0, y1 + 22.0),
        ],
        true,
    );
    f.fill(&neck, a(ink.plate));
    f.stroke(&neck, line);
    let foot = s.rounded_rect(cx - 40.0, y1 + 21.0, 80.0, 7.0, 3.5);
    f.fill(&foot, a(ink.plate));
    f.stroke(&foot, line);
    let body = s.rounded_rect(x0, top, w, h, 7.0);
    f.fill(&body, a(ink.plate));
    f.stroke(&body, line);
    let scr = s.rounded_rect(x0 + 6.0, top + 6.0, w - 12.0, h - 12.0, 3.0);
    f.fill(&scr, a(screen));
    f.stroke(&scr, ink.lo().with_color(a(ink.rule)));
    screen_box(x0 + 6.0, top + 6.0, x0 + w - 6.0, y1 - 6.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Tick,
    Excl,
    Cross,
}

impl Mark {
    pub fn glyph(self) -> Glyph {
        match self {
            Mark::Tick => Glyph::Tick,
            Mark::Excl => Glyph::Excl,
            Mark::Cross => Glyph::Cross,
        }
    }
}
