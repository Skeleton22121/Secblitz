//! The card shown when the saved undo history is damaged, and the two-step way to start fresh.
use crate::app::worker::Job;
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Tone};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{Ctx, Message};
use iced::widget::{column, row};
use iced::{Alignment, Element, Task};
use secblitz::engine::recover::{DamageKind, JournalDamaged};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Ask,
    Confirm,
    Working,
    Later,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageInfo {
    pub kind: DamageKind,
    pub files: usize,
    pub step: Step,
}

impl DamageInfo {
    pub fn new(damage: JournalDamaged) -> Self {
        Self {
            kind: damage.kind,
            files: damage.files,
            step: Step::Ask,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    Review,
    Back,
    Later,
    Start,
}

pub const FAILED: &str =
    "Windows wouldn't let Secblitz move the files. Close Secblitz everywhere and try again.";
pub const DONE: &str = "Done. Check your PC again.";

pub fn update(msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    let Some(info) = ctx.damage.as_mut() else {
        return Task::none();
    };
    match (msg, info.step) {
        (Msg::Review, Step::Ask | Step::Later) => info.step = Step::Confirm,
        (Msg::Back, Step::Confirm) => info.step = Step::Ask,
        (Msg::Later, Step::Ask) => info.step = Step::Later,
        (Msg::Start, Step::Confirm) => {
            info.step = Step::Working;
            ctx.busy = true;
            return Task::run(ctx.worker.run(Job::StartFresh), Message::Worker);
        }
        _ => {}
    }
    Task::none()
}

/// Puts the card back to its first question after a failed attempt.
pub fn failed(ctx: &mut Ctx) {
    ctx.busy = false;
    if let Some(info) = ctx.damage.as_mut() {
        info.step = Step::Ask;
    }
}

fn say(msg: Msg) -> Option<Message> {
    Some(Message::Recovery(msg))
}

pub fn card<'a>(ctx: &'a Ctx, info: &DamageInfo) -> Element<'a, Message> {
    let p = ctx.palette;
    let title = if info.kind == DamageKind::OtherPc {
        ctx.t("This undo history comes from another PC")
    } else {
        ctx.t("Secblitz's undo history is damaged")
    };
    let mut content = column![widgets::icon(
        Icon::ShieldAlert,
        theme::ICON_ROW,
        p.bad_text
    )]
    .spacing(theme::S4)
    .align_x(Alignment::Start);
    match info.step {
        Step::Later => {
            content = content
                .push(widgets::h2(p, title))
                .push(widgets::muted(
                    p,
                    ctx.t("Secblitz can't check your PC until this is sorted out."),
                ))
                .push(widgets::action(
                    p,
                    ButtonKind::Primary,
                    ctx.t("Start fresh"),
                    None,
                    say(Msg::Review),
                ));
        }
        Step::Ask | Step::Working => {
            let ready = info.step == Step::Ask;
            content = content
                .push(widgets::h2(p, title))
                .push(widgets::muted(
                    p,
                    ctx.t("Secblitz can't check your PC or undo earlier fixes until this is sorted out. Starting fresh keeps a copy of the damaged files. Fixes you already made stay on your PC, but Secblitz can no longer undo them."),
                ));
            if info.kind == DamageKind::Partial {
                content = content.push(widgets::small(
                    p,
                    ctx.t("Saved files that could not be read: {n}")
                        .replace("{n}", &info.files.to_string()),
                ));
            }
            content = content.push(
                row![
                    widgets::action(
                        p,
                        ButtonKind::Primary,
                        ctx.t("Start fresh"),
                        None,
                        ready.then_some(Message::Recovery(Msg::Review)),
                    ),
                    widgets::action(
                        p,
                        ButtonKind::Secondary,
                        ctx.t("Not now"),
                        None,
                        ready.then_some(Message::Recovery(Msg::Later)),
                    ),
                ]
                .spacing(theme::S3),
            );
        }
        Step::Confirm => {
            content = content
                .push(widgets::h2(p, ctx.t("Start a fresh undo history?")))
                .push(widgets::muted(
                    p,
                    ctx.t("This can't be undone. Earlier fixes stay as they are."),
                ))
                .push(
                    row![
                        widgets::action(
                            p,
                            ButtonKind::Danger,
                            ctx.t("Start fresh"),
                            None,
                            say(Msg::Start),
                        ),
                        widgets::action(
                            p,
                            ButtonKind::Secondary,
                            ctx.t("Cancel"),
                            None,
                            say(Msg::Back),
                        ),
                    ]
                    .spacing(theme::S3),
                );
        }
    }
    if info.step == Step::Working {
        content = content.push(widgets::inline_notice(
            p,
            Tone::Neutral,
            ctx.t("Starting fresh…"),
        ));
    }
    widgets::region(p, content).into()
}
