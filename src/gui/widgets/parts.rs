//! Reusable list, form and feedback pieces built on the core widgets. OWNER: shell agent.
use super::{icon, ButtonKind};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::Message;
use iced::widget::{button, column, container, progress_bar, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Vector};

/// Small caption that introduces a group of rows or cards.
pub fn section_label<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::SMALL)
        .font(theme::SEMIBOLD)
        .color(p.text_muted)
        .into()
}

/// Wraps a row so the whole line is clickable (soft hover highlight) and sends `on_press`.
pub fn list_button<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
    on_press: Message,
) -> Element<'a, Message> {
    button(content)
        .width(Length::Fill)
        .padding([10, 12])
        .on_press(on_press)
        .style(move |_, status| button::Style {
            background: Some(Background::Color(if status == button::Status::Hovered {
                p.surface_alt
            } else {
                Color::TRANSPARENT
            })),
            text_color: p.text,
            border: Border {
                radius: theme::RADIUS_SMALL.into(),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// Friendly placeholder for an empty list or missing data: icon, title, one line of help, optional action.
pub fn empty_state<'a>(
    p: Palette,
    i: Icon,
    title: impl Into<String>,
    body_text: impl Into<String>,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut c = column![
        container(icon(i, 26.0, p.text_muted))
            .center(56)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.surface_alt)),
                border: Border {
                    radius: 28.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
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
    .spacing(10)
    .align_x(Alignment::Center)
    .max_width(420);
    if let Some(a) = action {
        c = c.push(iced::widget::space::vertical().height(6)).push(a);
    }
    container(c).center_x(Length::Fill).padding([40, 20]).into()
}

/// State of one step in a progress checklist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Running,
    Done,
}

/// Checklist line: status icon on the left, label on the right.
pub fn progress_row<'a>(
    p: Palette,
    label: impl Into<String>,
    state: StepState,
) -> Element<'a, Message> {
    let (glyph, color, text_color) = match state {
        StepState::Running => (Some(Icon::Refresh), p.text_muted, p.text),
        StepState::Done => (Some(Icon::CheckCircle), p.good, p.text),
    };
    let lead: Element<'a, Message> = match glyph {
        Some(g) => icon(g, 18.0, color),
        None => container(iced::widget::space::horizontal())
            .width(14)
            .height(14)
            .style(move |_| container::Style {
                border: Border {
                    radius: 7.0.into(),
                    width: 1.5,
                    color,
                },
                ..container::Style::default()
            })
            .into(),
    };
    row![
        container(lead).center_x(18),
        text(label.into())
            .size(theme::BODY)
            .font(if state == StepState::Running {
                theme::MEDIUM
            } else {
                theme::REGULAR
            })
            .color(text_color)
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

/// Slim rounded progress bar; `ratio` is 0.0..=1.0.
pub fn bar<'a>(p: Palette, ratio: f32, tone: Tone) -> Element<'a, Message> {
    let fill = p.tone(tone);
    progress_bar(0.0..=1.0, ratio.clamp(0.0, 1.0))
        .girth(8)
        .style(move |_| progress_bar::Style {
            background: Background::Color(p.surface_alt),
            bar: Background::Color(fill),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
        })
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

/// Calm tinted message box with an icon, for tips, warnings and errors inside a page.
pub fn inline_notice<'a>(
    p: Palette,
    tone: Tone,
    message: impl Into<String>,
) -> Element<'a, Message> {
    let tint = p.tint(tone);
    let line = Color {
        a: 0.32,
        ..p.tone(tone)
    };
    container(
        row![
            icon(tone_icon(tone), 18.0, p.tone(tone)),
            text(message.into())
                .size(theme::BODY)
                .font(theme::REGULAR)
                .color(p.text)
                .width(Length::Fill)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([12, 14])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(tint)),
        border: Border {
            radius: theme::RADIUS_SMALL.into(),
            width: 1.0,
            color: line,
        },
        ..container::Style::default()
    })
    .into()
}

/// Collapsible section ("Technical details"); `on_toggle` flips `open` in the page state.
pub fn expander<'a>(
    p: Palette,
    title: impl Into<String>,
    open: bool,
    on_toggle: Message,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let head = button(
        row![
            text(title.into()).size(theme::SMALL).font(theme::MEDIUM),
            icon(
                if open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                14.0,
                p.text_muted
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([4, 0])
    .on_press(on_toggle)
    .style(move |_, status| button::Style {
        background: None,
        text_color: if status == button::Status::Hovered {
            p.text
        } else {
            p.text_muted
        },
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    });
    let mut c = column![head].spacing(8);
    if open {
        c = c.push(
            container(content)
                .padding(12)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(p.surface_alt)),
                    border: Border {
                        radius: theme::RADIUS_SMALL.into(),
                        width: 1.0,
                        color: p.border,
                    },
                    ..container::Style::default()
                }),
        );
    }
    c.into()
}

/// Small floating confirmation message with a close button; the shell places it bottom-centre.
pub fn toast<'a>(p: Palette, message: impl Into<String>, tone: Tone) -> Element<'a, Message> {
    let accent = if matches!(tone, Tone::Neutral | Tone::Brand) {
        p.on_brand
    } else {
        p.tone(tone)
    };
    container(
        row![
            icon(tone_icon(tone), 16.0, accent),
            text(message.into())
                .size(theme::BODY)
                .font(theme::MEDIUM)
                .color(p.on_brand),
            button(icon(Icon::X, 14.0, p.on_brand))
                .padding(2)
                .on_press(Message::DismissToast)
                .style(|_, _| button::Style {
                    background: None,
                    ..button::Style::default()
                }),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([10, 16])
    .max_width(520)
    .style(move |_| container::Style {
        background: Some(Background::Color(p.brand)),
        border: Border {
            radius: theme::RADIUS.into(),
            ..Border::default()
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        text_color: Some(p.on_brand),
        snap: true,
    })
    .into()
}

/// Convenience: a secondary "See all"-style ghost button.
pub fn link<'a>(p: Palette, label: impl Into<String>, on_press: Message) -> Element<'a, Message> {
    super::action(p, ButtonKind::Ghost, label, None, Some(on_press))
}
