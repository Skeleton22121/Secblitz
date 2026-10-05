//! Score ring drawn on a canvas. OWNER: shell agent.
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::Message;
use iced::widget::canvas::{self, path::Arc, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Element, Length, Point, Radians, Rectangle, Renderer, Theme};

pub struct Ring {
    pub p: Palette,
    /// 0.0..=1.0
    pub ratio: f32,
    pub tone: Tone,
    /// Big centre label, e.g. "15/18".
    pub label: String,
    /// Small caption under the label, e.g. "protected".
    pub caption: String,
}

impl canvas::Program<Message> for Ring {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = frame.center();
        let radius = bounds.width.min(bounds.height) / 2.0 - 10.0;
        let track = Path::circle(center, radius);
        frame.stroke(
            &track,
            Stroke::default()
                .with_width(12.0)
                .with_color(self.p.surface_alt),
        );
        let start = -std::f32::consts::FRAC_PI_2;
        let sweep = std::f32::consts::TAU * self.ratio.clamp(0.0, 1.0);
        if sweep > 0.0 {
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
                    .with_width(12.0)
                    .with_color(self.p.tone(self.tone))
                    .with_line_cap(canvas::LineCap::Round),
            );
        }
        frame.fill_text(Text {
            content: self.label.clone(),
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
        vec![frame.into_geometry()]
    }
}

pub fn ring<'a>(ring: Ring, size: f32) -> Element<'a, Message> {
    canvas::Canvas::new(ring)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}
