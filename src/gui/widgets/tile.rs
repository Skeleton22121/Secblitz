//! Topic tiles: a grid of big, quiet buttons that each open one topic.
use super::cursor::arrow;
use super::{icon, press};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::Message;
use iced::widget::button;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{column, container, responsive, row, space, text};
use iced::{Alignment, Background, Border, Element, Length, Pixels, Shadow};

pub const NARROW: f32 = 640.0;
const TALL: f32 = 124.0;
const SHORT: f32 = 96.0;

#[derive(Debug, Clone)]
pub struct Tile {
    pub glyph: Icon,
    pub title: String,
    pub status: String,
    pub tone: Tone,
    pub done: bool,
    pub selected: bool,
    pub on_press: Message,
}

pub fn columns_for(width: f32) -> usize {
    if width < NARROW {
        2
    } else {
        4
    }
}

fn height_for(columns: usize) -> f32 {
    if columns >= 4 {
        TALL
    } else {
        SHORT
    }
}

fn view<'a>(p: Palette, t: Tile, height: f32) -> Element<'a, Message> {
    let status_color = match t.tone {
        Tone::Neutral | Tone::Brand => p.text_muted,
        tone => p.tone_text(tone),
    };
    let glyph_color = match t.tone {
        Tone::Neutral | Tone::Brand => p.text_muted,
        tone => p.tone(tone),
    };
    let mut status = row![].spacing(theme::S1).align_y(Alignment::Center);
    if t.done {
        status = status.push(icon(Icon::Check, 14.0, status_color));
    }
    status = status.push(
        text(t.status)
            .size(theme::SMALL)
            .line_height(LineHeight::Absolute(Pixels(theme::LINE_SMALL)))
            .font(theme::REGULAR)
            .color(status_color),
    );
    let body = column![
        icon(t.glyph, 22.0, glyph_color),
        space::vertical(),
        text(t.title)
            .size(theme::BODY)
            .line_height(LineHeight::Absolute(Pixels(theme::LINE_BODY)))
            .font(theme::SEMIBOLD)
            .color(p.text)
            .wrapping(Wrapping::Word),
        status,
    ]
    .spacing(theme::S1)
    .width(Length::Fill)
    .height(Length::Fill);
    let selected = t.selected;
    arrow(
        press::button(body)
            .scale(false)
            .clip(true)
            .padding([theme::S3, theme::S4])
            .width(Length::Fill)
            .height(height)
            .on_press(t.on_press)
            .style(move |_, status| {
                use button::Status::*;
                let fill = match (selected, status) {
                    (_, Pressed) => p.pressed,
                    (true, Hovered) => p.hover_strong,
                    (true, _) => p.selected,
                    (false, Hovered) => p.surface_alt,
                    (false, _) => p.surface,
                };
                button::Style {
                    background: Some(Background::Color(fill)),
                    text_color: p.text,
                    border: Border {
                        radius: theme::R_LARGE.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: true,
                }
            }),
    )
}

pub fn grid<'a>(p: Palette, tiles: Vec<Tile>) -> Element<'a, Message> {
    responsive(move |size| {
        let columns = columns_for(size.width);
        let height = height_for(columns);
        let mut rows = column![].spacing(theme::S3).width(Length::Fill);
        for chunk in tiles.chunks(columns) {
            let mut line = row![].spacing(theme::S3).width(Length::Fill);
            for tile in chunk {
                line = line.push(container(view(p, tile.clone(), height)).width(Length::Fill));
            }
            for _ in chunk.len()..columns {
                line = line.push(space::horizontal().width(Length::Fill));
            }
            rows = rows.push(line);
        }
        rows.into()
    })
    .height(Length::Shrink)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_tiles_across_unless_the_page_is_narrow() {
        assert_eq!(columns_for(960.0), 4);
        assert_eq!(columns_for(640.0), 4);
        assert_eq!(columns_for(639.9), 2);
        assert_eq!(columns_for(300.0), 2);
        assert!(height_for(2) < height_for(4));
    }
}
