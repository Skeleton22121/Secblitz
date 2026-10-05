//! Home: score ring, verdict, one primary action, attention/protected cards,
//! first-run scanning view. OWNER: shell agent.
use crate::app::score::Verdict;
use crate::gui::theme::Tone;
use crate::gui::widgets::{self, ring, ButtonKind};
use crate::gui::{Ctx, Message};
use iced::widget::{column, row};
use iced::{Element, Subscription, Task};

#[derive(Debug, Default)]
pub struct State {}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Animation tick while scanning.
    Tick,
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, msg, ctx);
    Task::none()
}

pub fn subscription(state: &State, ctx: &Ctx) -> Subscription<Message> {
    let _ = (state, ctx);
    Subscription::none()
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let _ = state;
    let p = ctx.palette;
    let Some(score) = ctx.score() else {
        return widgets::card(p, column![
            widgets::h2(p, ctx.t("Checking your PC")),
            widgets::muted(p, ctx.t("This takes about a minute. Nothing is changed.")),
        ].spacing(8)).into();
    };
    let (tone, title) = match score.verdict() {
        Verdict::Protected => (Tone::Good, ctx.t("You're protected")),
        Verdict::Attention => (Tone::Warn, ctx.t("Some things need your attention")),
        Verdict::Unknown => (Tone::Neutral, ctx.t("We couldn't finish checking")),
    };
    let ids = ctx.report.as_deref().map(|r| crate::app::flow::recommended(r, &ctx.catalog.available)).unwrap_or_default();
    let primary = if ids.is_empty() {
        widgets::action(p, ButtonKind::Primary, ctx.t("Check again"), None, Some(Message::CheckNow))
    } else {
        widgets::action(p, ButtonKind::Primary, ctx.t("Fix problems"), None, (!ctx.busy).then(|| Message::ReviewFixes(ids.clone())))
    };
    let r = ring::ring(ring::Ring { p, ratio: score.ratio(), tone, label: format!("{}/{}", score.protected, score.total), caption: ctx.t("protected") }, 168.0);
    widgets::card(p, row![r, column![widgets::h1(p, title), primary].spacing(16)].spacing(32)).into()
}
