//! Reusable list, form and feedback pieces built on the core widgets.
//! OWNER: design-system agent.
use super::appear::slide_in;
use super::cursor::arrow;
use super::{icon, ButtonKind};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::Message;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, column, container, progress_bar, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Pixels, Shadow};

/// Small caption that introduces a group of rows or cards.
pub fn section_label<'a>(p: Palette, s: impl Into<String>) -> Element<'a, Message> {
    text(s.into())
        .size(theme::SMALL)
        .font(theme::SEMIBOLD)
        .color(p.text_muted)
        .into()
}

/// Wraps a row so the whole line is clickable (hover / pressed fill, normal
/// arrow cursor) and sends `on_press`. Rows are at least `ROW` tall when the
/// content is a 36 px badge plus text.
pub fn list_button<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
    on_press: Message,
) -> Element<'a, Message> {
    arrow(
        button(content)
            .width(Length::Fill)
            .padding([theme::S3, theme::S4])
            .on_press(on_press)
            .style(move |_, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Background::Color(p.hover)),
                    button::Status::Pressed => Some(Background::Color(p.pressed)),
                    _ => None,
                },
                text_color: p.text,
                border: Border {
                    radius: theme::R.into(),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: true,
            }),
    )
}

/// Friendly placeholder for an empty list or missing data: icon, title, one line of help, optional action.
pub fn empty_state<'a>(
    p: Palette,
    i: Icon,
    title: impl Into<String>,
    body_text: impl Into<String>,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    empty_state_art(p, icon(i, 32.0, p.text_muted), title, body_text, action)
}

/// [`empty_state`] with a drawing (or any element) in place of the icon.
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
    let (glyph, color) = match state {
        StepState::Running => (Icon::Refresh, p.text_muted),
        StepState::Done => (Icon::CheckCircle, p.good),
    };
    row![
        container(icon(glyph, 18.0, color)).center_x(18),
        text(label.into())
            .size(theme::BODY)
            .font(if state == StepState::Running {
                theme::MEDIUM
            } else {
                theme::REGULAR
            })
            .color(p.text)
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center)
    .into()
}

/// Slim rounded progress bar; `ratio` is 0.0..=1.0.
pub fn bar<'a>(p: Palette, ratio: f32, tone: Tone) -> Element<'a, Message> {
    let fill = p.tone(tone);
    progress_bar(0.0..=1.0, ratio.clamp(0.0, 1.0))
        .girth(6)
        .style(move |_| progress_bar::Style {
            background: Background::Color(p.hover_strong),
            bar: Background::Color(fill),
            border: Border {
                radius: 3.0.into(),
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

/// Collapsible section ("More details"); `on_toggle` flips `open` in the page state.
pub fn expander<'a>(
    p: Palette,
    title: impl Into<String>,
    open: bool,
    on_toggle: Message,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let head = arrow(
        button(
            row![
                text(title.into())
                    .size(theme::SMALL)
                    .font(theme::MEDIUM)
                    .wrapping(Wrapping::None),
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
            .spacing(theme::S2)
            .align_y(Alignment::Center),
        )
        .padding([theme::S1, 0.0])
        .on_press(on_toggle)
        .style(move |_, status| button::Style {
            background: None,
            text_color: if status == button::Status::Active {
                p.text_muted
            } else {
                p.text
            },
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        }),
    );
    let mut c = column![head].spacing(theme::S2);
    if open {
        c = c.push(
            container(content)
                .padding(theme::S3)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(p.surface_alt)),
                    border: Border {
                        radius: theme::R.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
        );
    }
    c.into()
}

/// Small floating confirmation message with a close button; the shell places
/// it bottom-centre. It slides up once when it appears (no timers after).
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
        button(icon(Icon::X, 14.0, p.on_brand))
            .padding(theme::S1)
            .on_press(Message::DismissToast)
            .style(move |_, status| button::Style {
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

/// Convenience: a secondary "See all"-style ghost button.
pub fn link<'a>(p: Palette, label: impl Into<String>, on_press: Message) -> Element<'a, Message> {
    super::action(p, ButtonKind::Ghost, label, None, Some(on_press))
}

/// A real hyperlink (opens a web page): text with an external-link glyph and
/// the hand cursor. This is the only widget that shows the hand.
pub fn hyperlink<'a>(
    p: Palette,
    label: impl Into<String>,
    on_press: Message,
) -> Element<'a, Message> {
    button(
        row![
            text(label.into())
                .size(theme::BODY)
                .font(theme::MEDIUM)
                .wrapping(Wrapping::None),
            icon(Icon::ExternalLink, 14.0, p.text_muted),
        ]
        .spacing(theme::S2)
        .align_y(Alignment::Center),
    )
    .padding([theme::S1, 0.0])
    .on_press(on_press)
    .style(move |_, status| button::Style {
        background: None,
        text_color: if status == button::Status::Active {
            p.text
        } else {
            p.text_muted
        },
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    })
    .into()
}
