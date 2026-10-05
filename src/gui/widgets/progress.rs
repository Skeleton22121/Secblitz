//! Progress bars: thin, round, eased, borderless.
//!
//! * [`bar`]: draws the value of a [`Tween`] the page keeps (it controls the
//!   clock, so it also works with a frame subscription it already runs).
//! * [`bar_eased`]: give it the latest value and it eases there by itself,
//!   asking for redraws only while it moves.
//! * [`indeterminate`]: a soft highlight gliding along the track; asks for
//!   redraws itself for as long as it is on screen.
//! * [`steps`]: a row of short segments for multi-step flows.
//!
//! The track is a whisper of the text colour, so it reads on `bg`,
//! `surface` and `surface_alt` alike. No borders, no shadows.
use super::anim::{self, Tween, DECELERATE};
use crate::gui::theme::{Palette, Tone};
use crate::gui::Message;
use iced::widget::canvas::{self, Frame, Geometry, Path};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use std::time::Instant;

/// Height of a bar.
pub const HEIGHT: f32 = 6.0;
/// Height of one segment of [`steps`].
pub const STEP_HEIGHT: f32 = 4.0;
const STEP_GAP: f32 = 4.0;

fn track_color(p: &Palette) -> Color {
    p.text.scale_alpha(0.08)
}

/// Capsule from `x0` to `x1` filling the frame height `h`.
fn capsule(x0: f32, x1: f32, h: f32) -> Path {
    let w = (x1 - x0).max(0.0);
    Path::rounded_rectangle(
        Point::new(x0, 0.0),
        Size::new(w, h),
        (h.min(w) / 2.0).into(),
    )
}

/// Width of the filled part for `value` 0..=1 on a track of `w`; a nonzero
/// value is never thinner than the bar is tall, so it stays a clean dot.
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

/// Position of the indeterminate highlight's left edge at `secs`: it enters
/// from the left, glides across on an ease and leaves on the right, then
/// rests briefly. Returns `(left, length)` for a track of `w`.
pub fn shimmer_span(secs: f32, w: f32) -> (f32, f32) {
    const PERIOD: f32 = 1.7;
    let len = (w * 0.34).max(24.0);
    let p = (secs / PERIOD).rem_euclid(1.0);
    let k = anim::STANDARD.at((p / 0.85).min(1.0));
    (-len + (w + len) * k, len)
}

fn paint_bar(f: &mut Frame, p: &Palette, tone: Tone, value: f32) {
    let (w, h) = (f.width(), f.height());
    f.fill(&capsule(0.0, w, h), track_color(p));
    let fw = fill_width(value, w, h);
    if fw > 0.0 {
        f.fill(&capsule(0.0, fw, h), p.tone(tone));
    }
}

// --- stateless: value comes from the page's Tween --------------------------

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
        let mut f = Frame::new(r, b.size());
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

/// Bar showing `tween` at the frame timestamp `now`. Retarget the tween with
/// `Tween::retarget(now, value)` when the value changes and keep the frame
/// subscription alive until `tween.done(now)`.
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

// --- self-easing -----------------------------------------------------------

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
        let mut f = Frame::new(r, b.size());
        paint_bar(&mut f, &self.p, self.tone, s.shown);
        vec![f.into_geometry()]
    }
}

/// Bar that eases to `value` by itself (400 ms decelerate). Idle cost is zero.
pub fn bar_eased<'a>(p: Palette, value: f32, tone: Tone) -> Element<'a, Message> {
    canvas_of(Eased { p, tone, value }, HEIGHT)
}

// --- indeterminate ---------------------------------------------------------

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
        let mut f = Frame::new(r, b.size());
        let (w, h) = (f.width(), f.height());
        f.fill(&capsule(0.0, w, h), track_color(&self.p));
        let color = self.p.tone(self.tone);
        if anim::reduced() {
            // Still: a short segment in the middle, not a frozen "loading" bar.
            f.fill(&capsule(w * 0.33, w * 0.67, h), color);
            return vec![f.into_geometry()];
        }
        let (left, len) = shimmer_span(s.secs, w);
        // Faint trail first, then the bright head; both clipped to the track.
        for (from, to, alpha) in [
            (left - len * 0.5, left + len, 0.28),
            (left, left + len, 1.0),
        ] {
            let (x0, x1) = (from.max(0.0), to.min(w));
            if x1 - x0 > 0.5 {
                f.fill(&capsule(x0, x1, h), color.scale_alpha(alpha));
            }
        }
        vec![f.into_geometry()]
    }
}

/// Bar for work of unknown length. Redraws itself while on screen.
pub fn indeterminate<'a>(p: Palette, tone: Tone) -> Element<'a, Message> {
    canvas_of(Shimmer { p, tone }, HEIGHT)
}

// --- steps -------------------------------------------------------------------

struct Steps {
    p: Palette,
    tone: Tone,
    total: usize,
    current: f32,
}

/// Fill of segment `i` for a progress of `current` segments (0..=total).
pub fn step_fill(current: f32, i: usize) -> f32 {
    (current - i as f32).clamp(0.0, 1.0)
}

impl canvas::Program<Message> for Steps {
    type State = ();
    fn draw(
        &self,
        _: &(),
        r: &Renderer,
        _: &Theme,
        b: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut f = Frame::new(r, b.size());
        let n = self.total.max(1);
        let h = f.height();
        let seg = (f.width() - STEP_GAP * (n - 1) as f32) / n as f32;
        for i in 0..n {
            let x = i as f32 * (seg + STEP_GAP);
            f.fill(&capsule(x, x + seg, h), track_color(&self.p));
            let fw = fill_width(step_fill(self.current, i), seg, h);
            if fw > 0.0 {
                f.fill(&capsule(x, x + fw, h), self.p.tone(self.tone));
            }
        }
        vec![f.into_geometry()]
    }
}

/// `total` short segments; `current` counts finished segments and may be
/// fractional (e.g. 2.5 = two done, third half way). Feed it an eased value.
pub fn steps<'a>(p: Palette, total: usize, current: f32, tone: Tone) -> Element<'a, Message> {
    canvas_of(
        Steps {
            p,
            tone,
            total,
            current,
        },
        STEP_HEIGHT,
    )
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

    #[test]
    fn step_fill_is_per_segment() {
        assert_eq!(step_fill(2.5, 0), 1.0);
        assert_eq!(step_fill(2.5, 2), 0.5);
        assert_eq!(step_fill(2.5, 3), 0.0);
    }
}
