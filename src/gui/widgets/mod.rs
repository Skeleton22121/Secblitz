//! Reusable visual building blocks. OWNER: design-system agent.
//!
//! Every page builds its UI from these so the product looks consistent
//! (see docs/DESIGN-SYSTEM.md). Signatures are a contract; styling may be
//! refined. No widget here draws a shadow or a gradient.
// The catalogue is larger than what pages use today; pages migrate to it next.
#![allow(dead_code, unused_imports)]

pub mod anim;
pub mod appear;
pub mod controls;
pub mod cursor;
pub mod parts;
pub mod ring;

pub use controls::{checkbox, dropdown, segmented, switch, text_field, CheckState};
pub use cursor::arrow;
pub use parts::*;

use super::icons::Icon;
use super::theme::{self, Palette, Tone};
use super::Message;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, column, container, opaque, row, stack, svg, text};
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

/// The filled Secblitz shield, tinted `color`.
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

/// Round tinted badge with an icon inside (list leading element).
pub fn icon_badge<'a>(p: Palette, i: Icon, tone: Tone) -> Element<'a, Message> {
    let fg = p.tone(tone);
    let bg = p.tint(tone);
    container(icon(i, 18.0, fg))
        .center(theme::CONTROL)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: (theme::CONTROL / 2.0).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// Rounded surface with border. Use for every content block (padding S6).
pub fn card<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
) -> container::Container<'a, Message> {
    container(content)
        .padding(theme::S6)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface)),
            border: Border {
                radius: theme::R_LARGE.into(),
                width: 1.0,
                color: p.border,
            },
            shadow: Shadow::default(), // never: tiny-skia draws shadows unclipped
            text_color: Some(p.text),
            snap: true,
        })
}

/// Small status label, e.g. "Needs attention".
pub fn pill<'a>(p: Palette, label: impl Into<String>, tone: Tone) -> Element<'a, Message> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// The one main action on a screen.
    Primary,
    /// Secondary actions, Cancel.
    Secondary,
    /// Text-like low-emphasis action.
    Ghost,
    /// Destructive confirmation (remove apps, undo).
    Danger,
}

/// (background, text, border) for a button in a given state. Every state has
/// its own solid colour; nothing relies on opacity.
fn button_colors(
    p: &Palette,
    kind: ButtonKind,
    status: button::Status,
) -> (Option<Color>, Color, Color) {
    use button::Status::*;
    if status == Disabled {
        return match kind {
            ButtonKind::Ghost => (None, p.disabled_fg, Color::TRANSPARENT),
            ButtonKind::Secondary => (Some(p.surface), p.disabled_fg, p.border),
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
                Hovered => p.hover,
                Pressed => p.pressed,
                _ => p.surface,
            };
            (Some(bg), p.text, p.border_strong)
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
        let (bg, fg, border) = button_colors(&p, kind, status);
        button::Style {
            background: bg.map(Background::Color),
            text_color: fg,
            border: Border {
                radius: theme::R.into(),
                width: 1.0,
                color: border,
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// Foreground used for the icon of a button (matches `button_colors`).
fn button_fg(p: &Palette, kind: ButtonKind, enabled: bool) -> Color {
    let status = if enabled {
        button::Status::Active
    } else {
        button::Status::Disabled
    };
    button_colors(p, kind, status).1
}

/// Standard button: 36 px tall, S4 side padding, S2 between icon and label.
/// `on_press: None` renders disabled. Keeps the normal arrow cursor.
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
        button(container(content).center_y(Length::Fill))
            .height(theme::CONTROL)
            .padding([0.0, theme::S4])
            .on_press_maybe(on_press)
            .style(button_style(p, kind)),
    )
}

/// Square 36 px icon-only button. Always pair with nearby text or a tooltip
/// so its meaning is clear.
pub fn icon_button<'a>(
    p: Palette,
    kind: ButtonKind,
    glyph: Icon,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let fg = button_fg(&p, kind, on_press.is_some());
    arrow(
        button(container(icon(glyph, 16.0, fg)).center(Length::Fill))
            .width(theme::CONTROL)
            .height(theme::CONTROL)
            .padding(0)
            .on_press_maybe(on_press)
            .style(button_style(p, kind)),
    )
}

/// Modal sheet layer (scrim + panel) to stack above the page. The scrim is one static flat colour (no
/// blur), the panel has a 1 px border, R_LARGE corners and S6 padding. Esc
/// handling is done by the shell via `Message::Escape`.
pub fn sheet_layer<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let panel = container(content)
        .max_width(560)
        .padding(theme::S6)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface)),
            border: Border {
                radius: theme::R_LARGE.into(),
                width: 1.0,
                color: p.border_strong,
            },
            shadow: Shadow::default(), // never: tiny-skia draws shadows unclipped
            text_color: Some(p.text),
            snap: true,
        });
    opaque(
        container(panel)
            .center(Length::Fill)
            .padding(theme::S6)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.scrim)),
                ..container::Style::default()
            }),
    )
}

/// Page header: title + optional subtitle (S1 apart). Put S6 below it.
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
