//! Progress bars in the Windows 11 style: a 1 px track with a 3 px rounded
//! indicator, eased, borderless, drawn on whole pixels so edges stay crisp.
use super::anim::{self, Tween, DECELERATE};
use crate::gui::theme::{Palette, Tone};
use crate::gui::Message;
use iced::widget::canvas::{self, Frame, Geometry, Path};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::time::Instant;

pub const HEIGHT: f32 = 3.0;
fn pixel_frame(r: &Renderer, b: Rectangle) -> Frame {
    let mut f = Frame::new(r, b.size());
    f.translate(Vector::new(-b.x.fract(), -b.y.fract()));
    f
}

fn track_color(p: &Palette) -> Color {
    p.text.scale_alpha(0.22)
}

fn track(f: &mut Frame, x0: f32, x1: f32, color: Color) {
    let y = ((f.height() - 1.0) / 2.0).round();
    if x1 > x0 {
        f.fill_rectangle(Point::new(x0, y), Size::new(x1 - x0, 1.0), color);
    }
}

fn capsule(x0: f32, x1: f32, h: f32) -> Path {
    let w = (x1 - x0).max(0.0);
    Path::rounded_rectangle(
        Point::new(x0, 0.0),
        Size::new(w, h),
        (h.min(w) / 2.0).into(),
    )
}

pub fn fill_width(value: f32, w: f32, h: f32) -> f32 {
    let v = if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    };
    if v <= 0.0 {
        0.0
    } else {
        (w * v).max(h.min(w))
    }
}

pub fn shimmer_span(secs: f32, w: f32) -> (f32, f32) {
    const PERIOD: f32 = 1.7;
    let len = (w * 0.34).max(24.0);
    let p = (secs / PERIOD).rem_euclid(1.0);
    let k = anim::STANDARD.at((p / 0.85).min(1.0));
    (-len + (w + len) * k, len)
}

fn paint_bar(f: &mut Frame, p: &Palette, tone: Tone, value: f32) {
    let (w, h) = (f.width(), f.height());
    let fw = fill_width(value, w, h).round();
    track(f, fw, w, track_color(p));
    if fw > 0.0 {
        f.fill(&capsule(0.0, fw, h), p.tone(tone));
    }
}


struct Plain {
    p: Palette,
    tone: Tone,
    value: f32,
}

impl canvas::Program<Message> for Plain {
    type State = ();
    fn draw(
        &self,
        _: &(),
        r: &Renderer,
        _: &Theme,
        b: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut f = pixel_frame(r, b);
        paint_bar(&mut f, &self.p, self.tone, self.value);
        vec![f.into_geometry()]
    }
}

fn canvas_of<'a, P: canvas::Program<Message> + 'a>(program: P, h: f32) -> Element<'a, Message> {
    canvas::Canvas::new(program)
        .width(Length::Fill)
        .height(Length::Fixed(h))
        .into()
}

pub fn bar<'a>(p: Palette, tween: &Tween, tone: Tone, now: Instant) -> Element<'a, Message> {
    canvas_of(
        Plain {
            p,
            tone,
            value: tween.value(now),
        },
        HEIGHT,
    )
}


struct Eased {
    p: Palette,
    tone: Tone,
    value: f32,
}

#[derive(Default)]
pub struct EasedState {
    shown: f32,
    from: f32,
    target: f32,
    start: Option<Instant>,
}

impl canvas::Program<Message> for Eased {
    type State = EasedState;
    fn update(
        &self,
        s: &mut EasedState,
        event: &iced::Event,
        _: Rectangle,
        _: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        let target = if self.value.is_nan() {
            0.0
        } else {
            self.value.clamp(0.0, 1.0)
        };
        if (s.target - target).abs() > f32::EPSILON {
            s.from = s.shown;
            s.target = target;
            s.start = Some(*now);
        }
        let start = s.start?;
        let t = now.saturating_duration_since(start).as_secs_f32() / anim::SLOW.as_secs_f32();
        if anim::reduced() || t >= 1.0 {
            s.start = None;
            s.shown = target;
        } else {
            s.shown = s.from + (target - s.from) * DECELERATE.at(t);
        }
        Some(canvas::Action::request_redraw())
    }
    fn draw(
        &self,
        s: &EasedState,
        r: &Renderer,
        _: &Theme,
        b: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut f = pixel_frame(r, b);
        paint_bar(&mut f, &self.p, self.tone, s.shown);
        vec![f.into_geometry()]
    }
}

/// Bar that eases to `value` by itself (400 ms decelerate). Idle cost is zero.
pub fn bar_eased<'a>(p: Palette, value: f32, tone: Tone) -> Element<'a, Message> {
    canvas_of(Eased { p, tone, value }, HEIGHT)
}


struct Shimmer {
    p: Palette,
    tone: Tone,
}

#[derive(Default)]
pub struct ShimmerState {
    start: Option<Instant>,
    secs: f32,
}

impl canvas::Program<Message> for Shimmer {
    type State = ShimmerState;
    fn update(
        &self,
        s: &mut ShimmerState,
        event: &iced::Event,
        _: Rectangle,
        _: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        if anim::reduced() {
            return None;
        }
        let start = *s.start.get_or_insert(*now);
        s.secs = now.saturating_duration_since(start).as_secs_f32();
        Some(canvas::Action::request_redraw())
    }
    fn draw(
        &self,
        s: &ShimmerState,
        r: &Renderer,
        _: &Theme,
        b: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut f = pixel_frame(r, b);
        let (w, h) = (f.width(), f.height());
        let color = self.p.tone(self.tone);
        if anim::reduced() {
            track(&mut f, 0.0, w, track_color(&self.p));
            f.fill(&capsule((w * 0.33).round(), (w * 0.67).round(), h), color);
            return vec![f.into_geometry()];
        }
        let (left, len) = shimmer_span(s.secs, w);
        let (x0, x1) = (left.max(0.0).round(), (left + len).min(w).round());
        let track_color = track_color(&self.p);
        if x1 - x0 > 0.5 {
            track(&mut f, 0.0, x0, track_color);
            track(&mut f, x1, w, track_color);
            f.fill(&capsule(x0, x1, h), color);
        } else {
            track(&mut f, 0.0, w, track_color);
        }
        vec![f.into_geometry()]
    }
}

pub fn indeterminate<'a>(p: Palette, tone: Tone) -> Element<'a, Message> {
    canvas_of(Shimmer { p, tone }, HEIGHT)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_width_clamps_and_keeps_a_dot() {
        assert_eq!(fill_width(0.0, 200.0, 6.0), 0.0);
        assert_eq!(fill_width(f32::NAN, 200.0, 6.0), 0.0);
        assert_eq!(fill_width(0.001, 200.0, 6.0), 6.0);
        assert_eq!(fill_width(0.5, 200.0, 6.0), 100.0);
        assert_eq!(fill_width(7.0, 200.0, 6.0), 200.0);
        assert_eq!(fill_width(1.0, 4.0, 6.0), 4.0);
    }

    #[test]
    fn shimmer_enters_left_and_leaves_right() {
        let (l0, len) = shimmer_span(0.0, 300.0);
        assert!(l0 + len <= 0.5, "starts off-screen");
        let (lmid, _) = shimmer_span(0.7, 300.0);
        assert!(lmid > 0.0 && lmid < 300.0);
        let (lend, _) = shimmer_span(1.7 * 0.85 - 1e-4, 300.0);
        assert!(lend > 290.0, "{lend}");
    }
}
