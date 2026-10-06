//! Score ring drawn on a canvas.
use super::anim;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::Message;
use iced::widget::canvas::{self, path::Arc, Geometry, Path, Stroke, Text};
use iced::{mouse, Element, Length, Point, Radians, Rectangle, Renderer, Theme};
use std::cell::RefCell;

pub struct Ring {
    pub p: Palette,
    pub ratio: f32,
    pub tone: Tone,
    pub label: String,
    pub caption: String,
}

type RingKey = (u32, Tone, u64, theme::Mode, i64);

struct Counted {
    ring: Ring,
    count: Option<i64>,
}

#[derive(Default)]
pub struct RingState {
    cache: canvas::Cache,
    key: RefCell<Option<RingKey>>,
    shown: f32,
    from: f32,
    target: f32,
    shown_n: i64,
    from_n: i64,
    target_n: i64,
    start: Option<std::time::Instant>,
}

pub fn stroke_width(size: f32) -> f32 {
    (size * 0.045).clamp(4.0, 9.0)
}

impl canvas::Program<Message> for Counted {
    type State = RingState;
    fn update(
        &self,
        state: &mut RingState,
        event: &iced::Event,
        _: Rectangle,
        _: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        let target = self.ring.ratio.clamp(0.0, 1.0);
        let target_n = self.count.unwrap_or(0);
        if (state.target - target).abs() > f32::EPSILON || state.target_n != target_n {
            state.from = state.shown;
            state.from_n = state.shown_n;
            state.target = target;
            state.target_n = target_n;
            state.start = Some(*now);
        }
        let start = state.start?;
        if anim::reduced() {
            state.start = None;
            state.shown = target;
            state.shown_n = target_n;
            return Some(canvas::Action::request_redraw());
        }
        let t = now.saturating_duration_since(start).as_secs_f32() / anim::SLOW.as_secs_f32();
        if t >= 1.0 {
            state.start = None;
            state.shown = target;
            state.shown_n = target_n;
        } else {
            state.shown = anim::ring_fill(state.from, target, t);
            state.shown_n = anim::count_up_int(state.from_n, target_n, t);
        }
        Some(canvas::Action::request_redraw())
    }
    fn draw(
        &self,
        state: &RingState,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        use std::hash::{Hash, Hasher};
        let mut text = std::collections::hash_map::DefaultHasher::new();
        self.ring.label.hash(&mut text);
        self.ring.caption.hash(&mut text);
        let p = &self.ring.p;
        for c in [p.text, p.text_muted, p.tone(self.ring.tone)] {
            c.into_rgba8().hash(&mut text);
        }
        let key = (
            state.shown.to_bits(),
            self.ring.tone,
            text.finish(),
            self.ring.p.mode,
            if self.count.is_some() {
                state.shown_n
            } else {
                i64::MIN
            },
        );
        if state.key.borrow().as_ref() != Some(&key) {
            state.cache.clear();
            *state.key.borrow_mut() = Some(key);
        }
        vec![state.cache.draw(renderer, bounds.size(), |frame| {
            let label = match self.count {
                Some(_) => state.shown_n.to_string(),
                None => self.ring.label.clone(),
            };
            self.ring.paint(frame, state.shown, label);
        })]
    }
}

impl Ring {
    fn paint(&self, frame: &mut canvas::Frame, ratio: f32, label: String) {
        let size = frame.width().min(frame.height());
        let center = frame.center();
        let w = stroke_width(size);
        let radius = size / 2.0 - w / 2.0 - 2.0;
        frame.stroke(
            &Path::circle(center, radius),
            Stroke::default()
                .with_width(w)
                .with_color(self.p.text.scale_alpha(0.07)),
        );
        let start = -std::f32::consts::FRAC_PI_2;
        let sweep = std::f32::consts::TAU * ratio.clamp(0.0, 1.0);
        let tone = self.p.tone(self.tone);
        if sweep > 0.001 {
            let arc = Path::new(|b| {
                b.arc(Arc {
                    center,
                    radius,
                    start_angle: Radians(start),
                    end_angle: Radians(start + sweep),
                })
            });
            frame.stroke(
                &arc,
                Stroke::default()
                    .with_width(w)
                    .with_color(tone)
                    .with_line_cap(canvas::LineCap::Round),
            );
        }
        frame.fill_text(Text {
            content: label,
            position: Point::new(center.x, center.y - 8.0),
            color: self.p.text,
            size: theme::DISPLAY.into(),
            font: theme::BOLD,
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Text::default()
        });
        frame.fill_text(Text {
            content: self.caption.clone(),
            position: Point::new(center.x, center.y + 20.0),
            color: self.p.text_muted,
            size: theme::SMALL.into(),
            font: theme::MEDIUM,
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Text::default()
        });
    }
}

pub fn ring<'a>(ring: Ring, size: f32) -> Element<'a, Message> {
    canvas::Canvas::new(Counted { ring, count: None })
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

pub fn ring_counting<'a>(ring: Ring, number: i64, size: f32) -> Element<'a, Message> {
    canvas::Canvas::new(Counted {
        ring,
        count: Some(number),
    })
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_is_thin_and_bounded() {
        assert_eq!(stroke_width(40.0), 4.0);
        assert!(stroke_width(176.0) < 9.0 && stroke_width(176.0) > 6.0);
        assert_eq!(stroke_width(1000.0), 9.0);
    }
}
