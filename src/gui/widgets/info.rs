//! The "About this" dialog: one place to read about a setting or a result.
use super::{body, h2, icon_button, section_label, small, ButtonKind};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette};
use crate::gui::{Ctx, Message};
use iced::widget::{column, container, row, scrollable, space, tooltip};
use iced::{Border, Element, Length};

#[derive(Debug, Clone, PartialEq)]
pub struct InfoSheet {
    pub title: String,
    pub blocks: Vec<InfoBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InfoBlock {
    Text { label: String, body: String },
    List { label: String, items: Vec<String> },
}

fn words(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The same words, or one is a short sentence that the other mostly is.
fn repeats(a: &str, b: &str) -> bool {
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    let count = |s: &str| s.split(' ').count();
    count(short) >= 2 && count(short) * 2 >= count(long) && long.contains(short.as_str())
}

impl InfoSheet {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            blocks: Vec::new(),
        }
    }

    pub fn text(mut self, label: impl Into<String>, body: impl Into<String>) -> Self {
        let body = body.into();
        if !body.trim().is_empty() {
            self.blocks.push(InfoBlock::Text {
                label: label.into(),
                body,
            });
        }
        self
    }

    pub fn list(mut self, label: impl Into<String>, items: Vec<String>) -> Self {
        let items: Vec<String> = items.into_iter().filter(|i| !i.trim().is_empty()).collect();
        if !items.is_empty() {
            self.blocks.push(InfoBlock::List {
                label: label.into(),
                items,
            });
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Drops what the row already shows (each line of `shown`) and what an earlier block says.
    pub fn without(mut self, shown: &[&str]) -> Self {
        let mut seen: Vec<String> = shown
            .iter()
            .flat_map(|s| s.lines())
            .map(str::to_owned)
            .collect();
        let fresh = |text: &str, seen: &mut Vec<String>| {
            if text.trim().is_empty() || seen.iter().any(|s| repeats(s, text)) {
                return false;
            }
            seen.push(text.to_owned());
            true
        };
        self.blocks = self
            .blocks
            .into_iter()
            .filter_map(|block| match block {
                InfoBlock::Text { label, body } => {
                    fresh(&body, &mut seen).then_some(InfoBlock::Text { label, body })
                }
                InfoBlock::List { label, items } => {
                    let items: Vec<String> = items
                        .into_iter()
                        .filter(|item| fresh(item, &mut seen))
                        .collect();
                    (!items.is_empty()).then_some(InfoBlock::List { label, items })
                }
            })
            .collect();
        self
    }
}

/// What a check is, what happens when it is off, and what turning it on does.
pub fn for_check(ctx: &Ctx, title: String, id: &str, report_only: bool) -> Option<InfoSheet> {
    let e = secblitz::explain::for_check(id)?;
    let third = if report_only {
        ctx.t("What you can do")
    } else {
        ctx.t("If you turn it on")
    };
    Some(
        InfoSheet::new(title)
            .text(ctx.t("What it is"), ctx.t(e.what))
            .text(ctx.t("If it's off"), ctx.t(e.risk))
            .text(third, ctx.t(e.change)),
    )
}

fn about_tip<'a>(p: Palette, label: String, button: Element<'a, Message>) -> Element<'a, Message> {
    tooltip(
        button,
        container(small(p, label))
            .padding([theme::S1, theme::S2])
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(p.surface)),
                border: Border {
                    color: p.border,
                    width: theme::HAIRLINE,
                    radius: theme::R.into(),
                },
                ..container::Style::default()
            }),
        tooltip::Position::Left,
    )
    .into()
}

pub fn button<'a>(ctx: &Ctx, sheet: Option<InfoSheet>) -> Option<Element<'a, Message>> {
    let sheet = sheet.filter(|s| !s.is_empty())?;
    let p = ctx.palette;
    Some(about_tip(
        p,
        ctx.t("About this"),
        icon_button(
            p,
            ButtonKind::Ghost,
            Icon::Info,
            Some(Message::Info(Some(Box::new(sheet)))),
        ),
    ))
}

/// A words-and-icon variant for results, where a bare icon would not say enough.
pub fn link<'a>(
    ctx: &Ctx,
    label: String,
    sheet: Option<InfoSheet>,
) -> Option<Element<'a, Message>> {
    let sheet = sheet.filter(|s| !s.is_empty())?;
    Some(super::action(
        ctx.palette,
        ButtonKind::Ghost,
        label,
        Some(Icon::Info),
        Some(Message::Info(Some(Box::new(sheet)))),
    ))
}

pub fn dialog<'a>(ctx: &Ctx, sheet: &InfoSheet) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut blocks = column![].spacing(theme::S4).width(Length::Fill);
    for block in &sheet.blocks {
        blocks = blocks.push(match block {
            InfoBlock::Text { label, body: text } => {
                column![section_label(p, label.clone()), body(p, text.clone())]
                    .spacing(theme::S1)
                    .width(Length::Fill)
            }
            InfoBlock::List { label, items } => {
                let mut lines = column![section_label(p, label.clone())]
                    .spacing(theme::S1)
                    .width(Length::Fill);
                for item in items {
                    lines = lines.push(body(p, item.clone()));
                }
                lines
            }
        });
    }
    column![
        h2(p, sheet.title.clone()),
        container(
            scrollable(container(blocks).padding([0.0, theme::S3]))
                .direction(super::controls::scrollbar())
                .style(super::controls::scroll_style(p))
        )
        .max_height(super::SHEET_FIT_HEIGHT - 8.0 * theme::S4),
        row![
            space::horizontal(),
            super::action(
                p,
                ButtonKind::Secondary,
                ctx.t("Close"),
                None,
                Some(Message::Info(None)),
            )
        ],
    ]
    .spacing(theme::S4)
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> InfoSheet {
        InfoSheet::new("Firewall")
            .text("What it is", "A guard for your connection.")
            .text("If it's off", "")
            .text("More details", "a guard   for YOUR connection.")
            .list(
                "What will change",
                vec!["Firewall: on".into(), "  ".into(), "Firewall: ON".into()],
            )
    }

    #[test]
    fn empty_content_is_never_added() {
        let s = sheet();
        assert_eq!(s.blocks.len(), 3);
        assert!(InfoSheet::new("x")
            .text("a", " ")
            .list("b", vec![])
            .is_empty());
    }

    #[test]
    fn repeats_are_dropped_in_order() {
        let s = sheet().without(&["Turn on the firewall.\nFirewall: on"]);
        assert_eq!(
            s.blocks,
            vec![InfoBlock::Text {
                label: "What it is".into(),
                body: "A guard for your connection.".into()
            }]
        );
    }

    #[test]
    fn a_shown_line_inside_a_longer_text_counts_as_a_repeat() {
        let s = InfoSheet::new("x")
            .text("a", "Windows will block unknown apps from changing files.")
            .without(&["Windows will block unknown apps"]);
        assert!(s.is_empty());
        let s = InfoSheet::new("x")
            .text(
                "a",
                "Your choice of settings decides which apps are blocked here.",
            )
            .without(&["Your choice"]);
        assert!(
            !s.is_empty(),
            "a short pill inside a long text is not a repeat"
        );
        assert!(!repeats("On", "Only on weekends"));
    }

    #[test]
    fn a_list_that_loses_every_line_goes_away() {
        let s = InfoSheet::new("x")
            .list("Changes", vec!["Firewall: on".into()])
            .without(&["firewall: on"]);
        assert!(s.is_empty());
    }
}
