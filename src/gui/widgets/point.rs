//! A point with a visible first line; the rest opens when the person asks for it.
use super::cursor::arrow;
use super::section::chevron;
use super::{body, muted, small};
use crate::gui::theme::{self, Palette};
use crate::gui::Message;
use iced::widget::{button, column, container, row};
use iced::{Alignment, Background, Border, Element, Length, Padding, Shadow};
use std::collections::BTreeSet;

/// The points of one dialog that are open. It lives in the dialog's own state and is emptied when the dialog closes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Opened(BTreeSet<String>);

impl Opened {
    pub fn toggle(&mut self, key: &str) {
        if !self.0.remove(key) {
            self.0.insert(key.to_owned());
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.0.contains(key)
    }

    pub fn open(&mut self, key: &str) {
        self.0.insert(key.to_owned());
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// How the words of a point are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Words {
    Body,
    Muted,
    Small,
}

fn draw<'a>(p: Palette, words: Words, s: &str) -> Element<'a, Message> {
    match words {
        Words::Body => body(p, s),
        Words::Muted => muted(p, s),
        Words::Small => small(p, s),
    }
}

/// Splits a text at the end of its first sentence. The rest is `None` when there is nothing more.
/// A sentence ends at ". ", "? ", "! " or a line break; a full stop before a lower case letter
/// (such as "e.g. this") does not end it.
pub fn first_sentence(text: &str) -> (&str, Option<&str>) {
    let text = text.trim();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let end = i + c.len_utf8();
        if c == '\n' {
            return cut(text, i, end);
        }
        if matches!(c, '.' | '?' | '!' | '。' | '？' | '！') {
            let Some(&(_, next)) = chars.peek() else {
                break;
            };
            if next.is_whitespace() {
                let after = text[end..].trim_start();
                let lower = c == '.' && after.chars().next().is_some_and(char::is_lowercase);
                if !lower {
                    return cut(text, end, end);
                }
            }
        }
    }
    (text, None)
}

fn cut(text: &str, first_end: usize, rest_start: usize) -> (&str, Option<&str>) {
    let first = text[..first_end].trim();
    let rest = text[rest_start..].trim();
    (first, (!rest.is_empty()).then_some(rest))
}

/// The head of a point with `trailing` controls beside it. With `rest`, pressing the head
/// (mouse, Enter or Space) shows or hides it; without, the head is plain and has no chevron.
pub fn point<'a>(
    p: Palette,
    head: Element<'a, Message>,
    trailing: Vec<Element<'a, Message>>,
    rest: Option<Element<'a, Message>>,
    open: bool,
    on_toggle: Message,
) -> Element<'a, Message> {
    let mut line = row![].spacing(theme::S3).align_y(Alignment::Center);
    let Some(rest) = rest else {
        line = line.push(container(head).width(Length::Fill));
        for t in trailing {
            line = line.push(t);
        }
        return line.into();
    };
    let toggle = arrow(
        super::press::button(
            row![
                container(head).width(Length::Fill),
                chevron(16.0, p.text_muted, open)
            ]
            .spacing(theme::S3)
            .align_y(Alignment::Center),
        )
        .scale(false)
        .padding(Padding::ZERO)
        .width(Length::Fill)
        .on_press(on_toggle)
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
    );
    line = line.push(toggle);
    for t in trailing {
        line = line.push(t);
    }
    let mut c = column![line].spacing(theme::S1).width(Length::Fill);
    if open {
        c = c.push(rest);
    }
    c.into()
}

/// A sentence or two: the first shows, the rest opens with the chevron.
pub fn text_point<'a>(
    p: Palette,
    text: &str,
    words: Words,
    open: bool,
    on_toggle: Message,
) -> Element<'a, Message> {
    let (first, rest) = first_sentence(text);
    point(
        p,
        draw(p, words, first),
        Vec::new(),
        rest.map(|r| draw(p, words, r)),
        open,
        on_toggle,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sentence_shows_and_the_rest_waits() {
        assert_eq!(
            first_sentence("One thing. Another thing."),
            ("One thing.", Some("Another thing."))
        );
        assert_eq!(first_sentence("Only this."), ("Only this.", None));
        assert_eq!(first_sentence("  No end  "), ("No end", None));
        assert_eq!(first_sentence(""), ("", None));
    }

    #[test]
    fn questions_exclamations_and_line_breaks_end_a_sentence() {
        assert_eq!(first_sentence("Sure? Yes."), ("Sure?", Some("Yes.")));
        assert_eq!(first_sentence("Careful! Wet."), ("Careful!", Some("Wet.")));
        assert_eq!(
            first_sentence("Protects you from: x\nTurns off y"),
            ("Protects you from: x", Some("Turns off y"))
        );
        assert_eq!(first_sentence("Line one\n\n  \n"), ("Line one", None));
    }

    #[test]
    fn every_language_splits_where_a_reader_would() {
        for (text, first) in [
            (
                "Esto no se puede deshacer. Reinicia después.",
                "Esto no se puede deshacer.",
            ),
            ("¿Seguro? Sí, seguro.", "¿Seguro?"),
            ("Ceci est final. Fermez ensuite.", "Ceci est final."),
            (
                "Attention : danger ! Ensuite, rien.",
                "Attention : danger !",
            ),
            (
                "Das geht nicht rückgängig. Danach neu starten.",
                "Das geht nicht rückgängig.",
            ),
            (
                "Isso não pode ser desfeito. Reinicie depois.",
                "Isso não pode ser desfeito.",
            ),
            (
                "Non si può annullare. Riavvia dopo.",
                "Non si può annullare.",
            ),
        ] {
            let (got, rest) = first_sentence(text);
            assert_eq!(got, first);
            assert!(rest.is_some(), "{text}");
        }
    }

    #[test]
    fn a_full_stop_inside_a_sentence_does_not_split_it() {
        assert_eq!(
            first_sentence("Use e.g. a backup. Then go."),
            ("Use e.g. a backup.", Some("Then go."))
        );
        assert_eq!(
            first_sentence("Takes 1.5 minutes. Wait."),
            ("Takes 1.5 minutes.", Some("Wait."))
        );
        assert_eq!(
            first_sentence("Version 3.2.1 is out. Update."),
            ("Version 3.2.1 is out.", Some("Update."))
        );
    }

    #[test]
    fn the_open_set_toggles_and_clears() {
        let mut o = Opened::default();
        assert!(!o.has("a"));
        o.toggle("a");
        assert!(o.has("a"));
        o.toggle("a");
        assert!(!o.has("a"));
        o.open("b");
        o.clear();
        assert_eq!(o, Opened::default());
    }
}
