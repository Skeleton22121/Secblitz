//! Reusable visual building blocks. OWNER: shell agent.
//!
//! Every page builds its UI from these so the product looks consistent.
//! Signatures are a contract; styling may be refined.
pub mod ring;

use super::icons::Icon;
use super::theme::{self, Palette, Tone};
use super::Message;
use iced::widget::{button, column, container, opaque, row, stack, svg, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Vector};

pub fn h1<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into()).size(theme::H1).font(theme::BOLD).color(p.text).into()
}
pub fn h2<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into()).size(theme::H2).font(theme::SEMIBOLD).color(p.text).into()
}
pub fn body<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into()).size(theme::BODY).font(theme::REGULAR).color(p.text).into()
}
pub fn muted<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into()).size(theme::BODY).font(theme::REGULAR).color(p.text_muted).into()
}
pub fn small<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into()).size(theme::SMALL).font(theme::REGULAR).color(p.text_muted).into()
}

pub fn icon<'a>(i: Icon, size: f32, color: Color) -> Element<'a, Message> {
    svg(svg::Handle::from_memory(i.svg()))
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
        .center(36)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg)),
            border: Border { radius: 18.0.into(), ..Border::default() },
            ..container::Style::default()
        })
        .into()
}

/// Rounded surface with border. Use for every content block.
pub fn card<'a>(p: Palette, content: impl Into<Element<'a, Message>>) -> container::Container<'a, Message> {
    container(content)
        .padding(theme::PAD)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface)),
            border: Border { radius: theme::RADIUS.into(), width: 1.0, color: p.border },
            shadow: Shadow { color: Color::from_rgba(0.0, 0.0, 0.0, 0.12), offset: Vector::new(0.0, 2.0), blur_radius: 8.0 },
            text_color: Some(p.text),
            snap: true,
        })
}

/// Small status label, e.g. "Needs attention".
pub fn pill<'a>(p: Palette, label: impl Into<String>, tone: Tone) -> Element<'a, Message> {
    let fg = p.tone(tone);
    let bg = p.tint(tone);
    container(text(label.into()).size(theme::SMALL).font(theme::MEDIUM).color(fg))
        .padding([3, 10])
        .style(move |_| container::Style {
            background: Some(Background::Color(bg)),
            border: Border { radius: 999.0.into(), ..Border::default() },
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

/// Standard button. `on_press: None` renders disabled.
pub fn action<'a>(
    p: Palette,
    kind: ButtonKind,
    label: impl Into<String>,
    leading: Option<Icon>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let (bg, fg, border) = match kind {
        ButtonKind::Primary => (p.brand, p.on_brand, p.brand),
        ButtonKind::Danger => (p.bad, p.on_brand, p.bad),
        ButtonKind::Secondary => (p.surface_alt, p.text, p.border),
        ButtonKind::Ghost => (Color::TRANSPARENT, p.brand, Color::TRANSPARENT),
    };
    let mut content = row![].spacing(8).align_y(Alignment::Center);
    if let Some(i) = leading {
        content = content.push(icon(i, 16.0, fg));
    }
    content = content.push(text(label.into()).size(theme::BODY).font(theme::SEMIBOLD));
    button(content)
        .padding([10, 18])
        .on_press_maybe(on_press)
        .style(move |_, status| {
            let (bg, alpha) = match status {
                button::Status::Hovered => (bg, 0.88),
                button::Status::Pressed => (bg, 0.75),
                button::Status::Disabled => (bg, 0.45),
                button::Status::Active => (bg, 1.0),
            };
            button::Style {
                background: Some(Background::Color(Color { a: bg.a * alpha, ..bg })),
                text_color: Color { a: if status == button::Status::Disabled { 0.6 } else { 1.0 }, ..fg },
                border: Border { radius: theme::RADIUS_SMALL.into(), width: 1.0, color: border },
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .into()
}

/// Modal sheet centred over `base` with a dimmed backdrop. Esc handling is
/// done by the shell via `Message::Escape`.
pub fn sheet<'a>(
    p: Palette,
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let panel = container(content)
        .max_width(560)
        .padding(28)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface)),
            border: Border { radius: 16.0.into(), width: 1.0, color: p.border },
            shadow: Shadow { color: Color::from_rgba(0.0, 0.0, 0.0, 0.35), offset: Vector::new(0.0, 12.0), blur_radius: 32.0 },
            text_color: Some(p.text),
            snap: true,
        });
    stack![
        base.into(),
        opaque(
            container(panel)
                .center(Length::Fill)
                .padding(24)
                .style(move |_| container::Style {
                    background: Some(Background::Color(p.scrim)),
                    ..container::Style::default()
                })
        )
    ]
    .into()
}

/// Page header: title + optional subtitle.
pub fn page_header<'a>(p: Palette, title: impl Into<String>, subtitle: Option<String>) -> Element<'a, Message> {
    let mut c = column![h1(p, title)].spacing(4);
    if let Some(s) = subtitle {
        c = c.push(muted(p, s));
    }
    c.into()
}
