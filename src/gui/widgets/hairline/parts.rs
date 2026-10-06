//! Objects several drawings share: a laptop, a monitor, a round badge and a big shield.
use super::glyph::Glyph;
use super::stage::{pt, stroke, tint_by, Ink, Stage, W_MARK};
use iced::widget::canvas::Frame;
use iced::{Color, Point, Rectangle, Size};

fn screen_box(x0: f32, y0: f32, x1: f32, y1: f32) -> Rectangle {
    Rectangle::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0))
}

#[allow(clippy::too_many_arguments)]
pub fn laptop(
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
    let body = s.rounded_rect(x0, top, w, h, 7.0);
    f.fill(&body, a(ink.plate));
    f.stroke(&body, ink.ln().with_color(a(ink.line)));
    let scr = s.rounded_rect(x0 + 6.0, top + 6.0, w - 12.0, h - 12.0, 3.0);
    f.fill(&scr, a(screen));
    f.stroke(&scr, ink.lo().with_color(a(ink.rule)));
    f.fill(&s.circle(pt(cx, top + 3.2), 0.9), a(ink.line));
    let (lip, depth) = (14.0, 13.0);
    let base = s.polyline(
        &[
            pt(x0 - 4.0, y1 + 2.0),
            pt(x0 + w + 4.0, y1 + 2.0),
            pt(x0 + w + lip, y1 + depth),
            pt(x0 - lip, y1 + depth),
        ],
        true,
    );
    f.fill(&base, a(ink.plate));
    f.stroke(&base, ink.ln().with_color(a(ink.line)));
    let faint = ink.ln2().with_color(a(ink.faint));
    f.stroke(
        &s.line(pt(cx - 16.0, y1 + depth - 0.2), pt(cx + 16.0, y1 + depth - 0.2)),
        faint,
    );
    f.stroke(
        &s.line(pt(x0 - 4.0, y1 + 2.0), pt(x0 + w + 4.0, y1 + 2.0)),
        faint,
    );
    screen_box(x0 + 6.0, top + 6.0, x0 + w - 6.0, y1 - 6.0)
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BadgeLook {
    pub color: Color,
    pub scale: f32,
    pub glow: f32,
    pub glow_color: Color,
    pub done: f32,
    pub flag: f32,
    pub flag_color: Color,
    pub alpha: f32,
}

impl BadgeLook {
    pub fn new(ink: &Ink) -> BadgeLook {
        BadgeLook {
            color: ink.line,
            scale: 1.0,
            glow: 0.0,
            glow_color: ink.accent,
            done: 0.0,
            flag: 0.0,
            flag_color: ink.warn,
            alpha: 1.0,
        }
    }
}

pub fn badge(
    f: &mut Frame,
    s: &Stage,
    ink: &Ink,
    glyph: Glyph,
    centre: Point,
    r: f32,
    look: &BadgeLook,
) {
    let g = s.local(centre, look.scale);
    let a = look.alpha;
    let ring = g.circle(Point::ORIGIN, r);
    f.fill(&ring, ink.tint(look.color).scale_alpha(a));
    f.stroke(&ring, ink.part(look.color.scale_alpha(a)));
    if look.glow > 0.004 {
        f.stroke(
            &g.circle(Point::ORIGIN, r + 4.0),
            ink.accent_line(look.glow_color.scale_alpha(look.glow.min(1.0) * a)),
        );
    }
    f.stroke(
        &g.icon(glyph, Point::ORIGIN, r * 1.15),
        ink.part(look.color.scale_alpha(a)),
    );
    let corner = pt(r * 0.72, -r * 0.72);
    if look.done > 0.004 {
        let o = look.done.min(1.0) * a;
        let dot = g.circle(corner, 5.5);
        f.fill(&dot, tint_by(ink.good, ink.plate, 0.16).scale_alpha(o));
        f.stroke(&dot, stroke(ink.good.scale_alpha(o), W_MARK));
        f.stroke(
            &g.polyline(
                &[
                    pt(corner.x - 2.6, corner.y + 0.1),
                    pt(corner.x - 0.8, corner.y + 1.9),
                    pt(corner.x + 2.6, corner.y - 1.7),
                ],
                false,
            ),
            stroke(ink.good.scale_alpha(o), W_MARK),
        );
    }
    if look.flag > 0.004 {
        let o = look.flag.min(1.0) * a;
        let dot = g.circle(corner, 6.0);
        f.fill(&dot, ink.plate.scale_alpha(o));
        let c = look.flag_color.scale_alpha(o);
        f.stroke(&dot, ink.accent_line(c));
        f.stroke(
            &g.path(|p| {
                p.move_to(pt(corner.x, corner.y - 3.0));
                p.line_to(pt(corner.x, corner.y + 0.4));
                p.move_to(pt(corner.x, corner.y + 2.6));
                p.line_to(pt(corner.x, corner.y + 2.7));
            }),
            ink.accent_line(c),
        );
    }
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShieldLook {
    pub back: f32,
    pub outline: f32,
    pub mark: Option<(Mark, f32)>,
    pub color: Color,
}

pub fn shield_mark(
    f: &mut Frame,
    s: &Stage,
    ink: &Ink,
    centre: Point,
    size: f32,
    look: &ShieldLook,
) {
    let shield = Glyph::Shield.data().placed(centre, size);
    if look.back > 0.004 {
        f.fill(&s.shape(&shield), ink.plate.scale_alpha(look.back.min(1.0)));
    }
    if look.outline > 0.001 {
        f.stroke(
            &s.shape(&shield.partial(look.outline)),
            ink.accent_line(look.color),
        );
    }
    if let Some((mark, p)) = look.mark {
        if p > 0.001 {
            let d = mark.glyph().data().placed(centre, size).partial(p);
            f.stroke(&s.shape(&d), ink.thick(look.color));
        }
    }
}
