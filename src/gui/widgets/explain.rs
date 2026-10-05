//! Row disclosure that explains a check in plain words.
//!
//! A small info button sits on the row; pressing it opens a quiet tonal inset
//! under the row with three labelled lines (what it is, what happens if it is
//! off, what changes when it is on or what the person can do). Only one inset
//! is open at a time: the shell keeps a single `Ctx::explain_open` key and
//! `Message::Explain(key)` flips it. The button is a normal focusable press
//! button, so Tab and Enter / Space work.
use super::{body, icon_button, section_label, ButtonKind};
use crate::explain::{self, Explainer};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette};
use crate::gui::{Ctx, Message};
use iced::widget::{column, container, row, space};
use iced::{Background, Border, Element, Length};

/// Key that names one row of one list on the page.
pub fn key(scope: &str, id: &str) -> String {
    format!("{scope}:{id}")
}

/// Left inset that lines the inset up with a row's title: row padding, plain
/// icon and the gap after it.
pub const INDENT: f32 = theme::S4 + theme::ICON_ROW + theme::S4;

fn is_open(ctx: &Ctx, key: &str) -> bool {
    ctx.explain_open.as_deref() == Some(key)
}

/// The info button for a row, or `None` when the check has no explanation.
pub fn toggle<'a>(ctx: &Ctx, scope: &str, id: &str) -> Option<Element<'a, Message>> {
    explain::for_check(id)?;
    let k = key(scope, id);
    let open = is_open(ctx, &k);
    Some(icon_button(
        ctx.palette,
        if open {
            ButtonKind::Secondary
        } else {
            ButtonKind::Ghost
        },
        if open { Icon::ChevronDown } else { Icon::Info },
        Some(Message::Explain(k)),
    ))
}

fn block<'a>(p: Palette, label: String, text: String) -> Element<'a, Message> {
    column![section_label(p, label), body(p, text)]
        .spacing(theme::S1)
        .width(Length::Fill)
        .into()
}

/// The three-line inset. `report_only` checks (Secblitz cannot change them)
/// say "What you can do" instead of "If you turn it on".
fn lines<'a>(ctx: &Ctx, e: Explainer, report_only: bool) -> Element<'a, Message> {
    let p = ctx.palette;
    let third = if report_only {
        ctx.t("What you can do")
    } else {
        ctx.t("If you turn it on")
    };
    container(
        column![
            block(p, ctx.t("What it is"), ctx.t(e.what)),
            block(p, ctx.t("If it's off"), ctx.t(e.risk)),
            block(p, third, ctx.t(e.change)),
        ]
        .spacing(theme::S3),
    )
    .padding(theme::S3)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(p.surface_alt)),
        border: Border {
            radius: theme::R.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// The inset for an open row, indented by `indent`; `None` while closed.
pub fn panel<'a>(
    ctx: &Ctx,
    scope: &str,
    id: &str,
    report_only: bool,
    indent: f32,
) -> Option<Element<'a, Message>> {
    if !is_open(ctx, &key(scope, id)) {
        return None;
    }
    let e = explain::for_check(id)?;
    Some(
        row![
            space::horizontal().width(indent),
            lines(ctx, e, report_only)
        ]
        .into(),
    )
}

/// `row` with the info button on its right and the inset below when open.
/// `lead_indent` is the inset's left offset.
pub fn with_disclosure<'a>(
    ctx: &Ctx,
    scope: &str,
    id: &str,
    report_only: bool,
    lead_indent: f32,
    head: Element<'a, Message>,
) -> Element<'a, Message> {
    let Some(button) = toggle(ctx, scope, id) else {
        return head;
    };
    let line = row![head, button]
        .spacing(theme::S1)
        .align_y(iced::Alignment::Center);
    match panel(ctx, scope, id, report_only, lead_indent) {
        Some(inset) => column![line, inset].spacing(theme::S1).into(),
        None => line.into(),
    }
}
