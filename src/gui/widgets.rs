//! Reusable visual building blocks.

pub mod anim;
pub mod appear;
pub mod bars;
pub mod chart;
pub mod controls;
pub mod cursor;
pub mod explain;
pub mod hairline;
pub mod handoff;
pub mod menu;
pub mod parts;
pub mod press;
pub mod progress;
pub mod ring;
pub mod scan;
pub mod search_field;
pub mod section;

pub use controls::{checkbox, dropdown, segmented, slide_marker, switch, CheckState};
pub use cursor::arrow;
pub use menu::overflow_menu;
pub use parts::*;
pub use search_field::{no_matches, search_field};
pub use section::*;

use super::icons::Icon;
use super::theme::{self, Palette, Tone};
use super::Message;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, column, container, opaque, row, svg, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Pixels, Shadow};

pub fn h1<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::H1)
        .font(theme::SEMIBOLD)
        .color(p.text)
        .into()
}
pub fn h2<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::H2)
        .font(theme::SEMIBOLD)
        .color(p.text)
        .into()
}
pub fn body<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::BODY)
        .font(theme::REGULAR)
        .color(p.text)
        .into()
}
pub fn h2_centred<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::H2)
        .font(theme::SEMIBOLD)
        .color(p.text)
        .width(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .into()
}
pub fn muted_centred<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::BODY)
        .font(theme::REGULAR)
        .color(p.text_muted)
        .width(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .into()
}
pub fn muted<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::BODY)
        .font(theme::REGULAR)
        .color(p.text_muted)
        .into()
}
pub fn small<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::SMALL)
        .font(theme::REGULAR)
        .color(p.text_muted)
        .into()
}

pub fn icon<'a>(i: Icon, size: f32, color: Color) -> Element<'a, Message> {
    svg(svg::Handle::from_memory(i.svg()))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

pub fn brand_mark<'a>(size: f32, color: Color) -> Element<'a, Message> {
    svg(svg::Handle::from_memory(super::icons::BRAND_SVG))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

pub fn icon_filled<'a>(i: Icon, size: f32, color: Color) -> Element<'a, Message> {
    svg(svg::Handle::from_memory(i.svg_filled()))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

pub fn pill<'a>(p: Palette, label: impl Into<String>, tone: Tone) -> Element<'a, Message> {
    if tone == Tone::Neutral {
        return tag(p, None, label);
    }
    let fg = p.tone_text(tone);
    let bg = p.tint(tone);
    container(
        text(label.into())
            .size(theme::SMALL)
            .line_height(LineHeight::Absolute(Pixels(theme::LINE_SMALL)))
            .font(theme::MEDIUM)
            .wrapping(Wrapping::None)
            .color(fg),
    )
    .padding([theme::S1, theme::S3])
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: theme::R_PILL.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

pub fn tag<'a>(p: Palette, glyph: Option<Icon>, label: impl Into<String>) -> Element<'a, Message> {
    let label = text(label.into())
        .size(theme::SMALL)
        .line_height(LineHeight::Absolute(Pixels(theme::LINE_SMALL)))
        .wrapping(Wrapping::None)
        .color(p.text_muted);
    match glyph {
        Some(g) => row![icon(g, 14.0, p.text_muted), label]
            .spacing(theme::S1)
            .align_y(Alignment::Center)
            .into(),
        None => label.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

fn button_colors(
    p: &Palette,
    kind: ButtonKind,
    status: button::Status,
) -> (Option<Color>, Color, Color) {
    use button::Status::*;
    if status == Disabled {
        return match kind {
            ButtonKind::Ghost => (None, p.disabled_fg, Color::TRANSPARENT),
            ButtonKind::Secondary => (Some(p.disabled_bg), p.disabled_fg, Color::TRANSPARENT),
            _ => (Some(p.disabled_bg), p.disabled_fg, p.disabled_bg),
        };
    }
    match kind {
        ButtonKind::Primary => {
            let bg = match status {
                Hovered => p.brand_hover,
                Pressed => p.brand_pressed,
                _ => p.brand,
            };
            (Some(bg), p.on_brand, bg)
        }
        ButtonKind::Danger => {
            let bg = match status {
                Hovered => p.danger_hover,
                Pressed => p.danger_pressed,
                _ => p.danger,
            };
            (Some(bg), Color::WHITE, bg)
        }
        ButtonKind::Secondary => {
            let bg = match status {
                Hovered => p.hover_strong,
                Pressed => p.pressed,
                _ => p.hover,
            };
            (Some(bg), p.text, Color::TRANSPARENT)
        }
        ButtonKind::Ghost => {
            let bg = match status {
                Hovered => Some(p.hover_strong),
                Pressed => Some(p.pressed),
                _ => None,
            };
            (bg, p.text, Color::TRANSPARENT)
        }
    }
}

fn button_style(
    p: Palette,
    kind: ButtonKind,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let (bg, fg, _) = button_colors(&p, kind, status);
        button::Style {
            background: bg.map(Background::Color),
            text_color: fg,
            border: Border {
                radius: theme::R.into(),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

fn button_fg(p: &Palette, kind: ButtonKind, enabled: bool) -> Color {
    let status = if enabled {
        button::Status::Active
    } else {
        button::Status::Disabled
    };
    button_colors(p, kind, status).1
}

pub fn action<'a>(
    p: Palette,
    kind: ButtonKind,
    label: impl Into<String>,
    leading: Option<Icon>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let fg = button_fg(&p, kind, on_press.is_some());
    let mut content = row![].spacing(theme::S2).align_y(Alignment::Center);
    if let Some(i) = leading {
        content = content.push(icon(i, 16.0, fg));
    }
    content = content.push(
        text(label.into())
            .size(theme::BODY)
            .line_height(LineHeight::Absolute(Pixels(theme::LINE_BODY)))
            .font(theme::MEDIUM)
            .wrapping(Wrapping::None),
    );
    arrow(
        press::button(container(content).center_y(Length::Fill))
            .height(theme::CONTROL)
            .padding([0.0, theme::S4])
            .on_press_maybe(on_press)
            .style(button_style(p, kind)),
    )
}

pub fn icon_button<'a>(
    p: Palette,
    kind: ButtonKind,
    glyph: Icon,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let fg = button_fg(&p, kind, on_press.is_some());
    arrow(
        press::button(container(icon(glyph, 16.0, fg)).center(Length::Fill))
            .width(theme::CONTROL)
            .height(theme::CONTROL)
            .padding(0)
            .on_press_maybe(on_press)
            .style(button_style(p, kind)),
    )
}

pub const SHEET_FIT_HEIGHT: f32 = theme::WINDOW_MIN_HEIGHT - 4.0 * theme::S6;

pub fn sheet_layer<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let panel = container(content)
        .max_width(theme::CONTENT_MAX)
        .padding(theme::S6)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface)),
            border: Border {
                radius: theme::R_LARGE.into(),
                ..Border::default()
            },
            shadow: Shadow::default(), // never: tiny-skia draws shadows unclipped
            text_color: Some(p.text),
            snap: true,
        });
    opaque(appear::pop(
        container(panel).center(Length::Fill).padding(theme::S6),
        p.scrim,
        p.surface,
        theme::R_LARGE,
    ))
}

pub fn page_header<'a>(
    p: Palette,
    title: impl Into<String>,
    subtitle: Option<String>,
) -> Element<'a, Message> {
    let mut c = column![h1(p, title)].spacing(theme::S1);
    if let Some(s) = subtitle {
        c = c.push(muted(p, s));
    }
    c.into()
}
