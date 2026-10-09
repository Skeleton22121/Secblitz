//! Reusable list, form and feedback pieces built on the core widgets.
use super::appear::slide_in;
use super::cursor::arrow;
use super::{icon, small, ButtonKind};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::Message;
use iced::widget::text::LineHeight;
use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Alignment, Background, Border, Element, Length, Padding, Pixels, Shadow, Theme};

pub fn section_label<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::SMALL)
        .font(theme::SEMIBOLD)
        .color(p.text_muted)
        .into()
}

/// Container style for the rounded inset panels used behind grouped details.
pub fn well_style(p: Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.surface_alt)),
        border: Border {
            radius: theme::R.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// A well whose content scrolls once it is taller than `max`.
pub fn scroll_well<'a>(
    p: Palette,
    list: impl Into<Element<'a, Message>>,
    max: f32,
) -> Element<'a, Message> {
    container(
        scrollable(container(list).padding(theme::S3).width(Length::Fill))
            .direction(super::controls::scrollbar())
            .style(super::controls::scroll_style(p)),
    )
    .max_height(max)
    .style(well_style(p))
    .into()
}

/// One line of a progress list: the state mark, what it is about and a short note.
pub fn step_row<'a>(
    p: Palette,
    lead: Element<'a, Message>,
    label: Vec<Element<'a, Message>>,
    note: String,
) -> Element<'a, Message> {
    let mut line = row![container(lead).center(theme::CHECK)];
    for part in label {
        line = line.push(part);
    }
    line.push(space::horizontal())
        .push(small(p, note))
        .spacing(theme::S3)
        .align_y(Alignment::Center)
        .into()
}

/// Where a row's text starts, after its icon.
pub const INDENT: f32 = theme::S4 + theme::ICON_ROW + theme::S4;

/// Items set under a row's title, lined up with its text rather than its icon.
pub fn under_row<'a>(items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    container(column(items).spacing(theme::S2).width(Length::Fill))
        .padding(Padding {
            top: 0.0,
            right: theme::S4,
            bottom: theme::S2,
            left: INDENT,
        })
        .width(Length::Fill)
        .into()
}

pub fn empty_state<'a>(
    p: Palette,
    i: Icon,
    title: impl Into<String>,
    body_text: impl Into<String>,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    empty_state_art(p, icon(i, 32.0, p.text_muted), title, body_text, action)
}

pub fn empty_state_art<'a>(
    p: Palette,
    art: Element<'a, Message>,
    title: impl Into<String>,
    body_text: impl Into<String>,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut c = column![
        art,
        text(title.into())
            .size(theme::H2)
            .font(theme::SEMIBOLD)
            .color(p.text),
        text(body_text.into())
            .size(theme::BODY)
            .font(theme::REGULAR)
            .color(p.text_muted)
            .align_x(iced::alignment::Horizontal::Center),
    ]
    .spacing(theme::S3)
    .align_x(Alignment::Center)
    .max_width(420);
    if let Some(a) = action {
        c = c
            .push(iced::widget::space::vertical().height(theme::S1))
            .push(a);
    }
    container(c)
        .center_x(Length::Fill)
        .padding([theme::S10, theme::S5])
        .into()
}

fn tone_icon(tone: Tone) -> Icon {
    match tone {
        Tone::Good => Icon::CheckCircle,
        Tone::Warn => Icon::AlertTriangle,
        Tone::Bad => Icon::ShieldAlert,
        Tone::Neutral | Tone::Brand => Icon::Info,
    }
}

pub fn inline_notice<'a>(
    p: Palette,
    tone: Tone,
    message: impl Into<String>,
) -> Element<'a, Message> {
    let tint = p.tint(tone);
    container(
        row![
            icon(tone_icon(tone), 18.0, p.tone(tone)),
            text(message.into())
                .size(theme::BODY)
                .font(theme::REGULAR)
                .color(p.text)
                .width(Length::Fill)
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center),
    )
    .padding([theme::S3, theme::S4])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(tint)),
        border: Border {
            radius: theme::R.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

pub fn toast<'a>(
    p: Palette,
    message: impl Into<String>,
    tone: Tone,
    leaving: bool,
) -> Element<'a, Message> {
    let accent = if matches!(tone, Tone::Neutral | Tone::Brand) {
        p.on_brand
    } else {
        p.tone(tone)
    };
    let close = arrow(
        super::press::button(icon(Icon::X, 14.0, p.on_brand))
            .padding(theme::S1)
            .scale(false)
            .on_press(Message::DismissToast)
            .style(move |_, status| button::Style {
                text_color: p.on_brand,
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Some(Background::Color(theme::mix(p.brand, p.on_brand, 0.18)))
                    }
                    _ => None,
                },
                border: Border {
                    radius: theme::R_SMALL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    );
    let card = container(
        row![
            icon(tone_icon(tone), 16.0, accent),
            text(message.into())
                .size(theme::BODY)
                .line_height(LineHeight::Absolute(Pixels(theme::LINE_BODY)))
                .font(theme::MEDIUM)
                .color(p.on_brand),
            close,
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center),
    )
    .padding([theme::S3, theme::S4])
    .max_width(520)
    .style(move |_| container::Style {
        background: Some(Background::Color(p.brand)),
        border: Border {
            radius: theme::R.into(),
            ..Border::default()
        },
        shadow: Shadow::default(), // never: tiny-skia draws shadows unclipped
        text_color: Some(p.on_brand),
        snap: true,
    });
    slide_in(card, theme::S3, leaving)
}

pub fn link<'a>(p: Palette, label: impl Into<String>, on_press: Message) -> Element<'a, Message> {
    super::action(p, ButtonKind::Ghost, label, None, Some(on_press))
}
