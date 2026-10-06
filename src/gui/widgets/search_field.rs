//! Search box: a text field with a magnifier and a clear button.
use super::press;
use super::{button_style, icon, ButtonKind};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette};
use crate::gui::{Ctx, Message};
use iced::widget::text::LineHeight;
use iced::widget::{container, row, space, stack, text_input};
use iced::{Background, Border, Color, Element, Length, Padding, Pixels};

const GLYPH: f32 = 16.0;
const CLEAR: f32 = theme::CONTROL_SMALL;
const LEAD: f32 = theme::S3 + GLYPH + theme::S2;
const TRAIL: f32 = theme::S2 + CLEAR + theme::S1;

pub fn search_field<'a>(
    p: Palette,
    id: &'static str,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
    on_clear: Message,
) -> Element<'a, Message> {
    let pad = (theme::CONTROL - theme::LINE_BODY) / 2.0;
    let input = text_input(placeholder, value)
        .id(id)
        .on_input(on_input)
        .size(theme::BODY)
        .font(theme::REGULAR)
        .line_height(LineHeight::Absolute(Pixels(theme::LINE_BODY)))
        .padding(Padding {
            top: pad,
            bottom: pad,
            left: LEAD,
            right: TRAIL,
        })
        .width(Length::Fill)
        .style(move |_, status| {
            let background = match status {
                text_input::Status::Active => p.surface_alt,
                text_input::Status::Hovered => p.hover_strong,
                text_input::Status::Focused { .. } => p.surface,
                text_input::Status::Disabled => p.disabled_bg,
            };
            text_input::Style {
                background: Background::Color(background),
                border: Border {
                    radius: theme::R.into(),
                    ..Border::default()
                },
                icon: p.text_muted,
                placeholder: p.text_muted,
                value: p.text,
                selection: Color { a: 0.3, ..p.accent },
            }
        });
    let mut over = row![icon(Icon::Search, GLYPH, p.text_muted), space::horizontal()]
        .align_y(iced::Alignment::Center);
    if !value.is_empty() {
        over = over.push(
            press::button(container(icon(Icon::X, 14.0, p.text_muted)).center(Length::Fill))
                .width(CLEAR)
                .height(CLEAR)
                .padding(0)
                .on_press(on_clear)
                .style(button_style(p, ButtonKind::Ghost)),
        );
    }
    let over = container(over)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding {
            left: theme::S3,
            right: theme::S1,
            ..Padding::ZERO
        });
    container(stack![input, over])
        .max_width(theme::CONTENT_MAX)
        .width(Length::Fill)
        .into()
}

const NO_MATCHES: &str = "Nothing matches \"{q}\"";
const SHOWN_CHARS: usize = 32;

/// The calm page for a search that finds nothing.
pub fn no_matches<'a>(ctx: &Ctx, typed: &str, clear: Message) -> Element<'a, Message> {
    let p = ctx.palette;
    let typed = typed.trim();
    let shown: String = if typed.chars().count() > SHOWN_CHARS {
        typed.chars().take(SHOWN_CHARS).chain(['…']).collect()
    } else {
        typed.to_owned()
    };
    super::empty_state(
        p,
        Icon::Search,
        ctx.t(NO_MATCHES).replace("{q}", &shown),
        ctx.t("Check the spelling or try a different word."),
        Some(super::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Clear search"),
            Some(Icon::X),
            Some(clear),
        )),
    )
}
