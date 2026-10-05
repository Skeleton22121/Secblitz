//! History: score trend + timeline of checks, fixes, undos, app clean-ups;
//! Undo last fixes; Restore removed apps. OWNER: fixes agent.
use crate::app::worker;
use crate::gui::{widgets, Ctx, Message};
use iced::{Element, Task};

#[derive(Debug, Default)]
pub struct State {}

#[derive(Debug, Clone)]
pub enum Msg {}

pub fn on_enter(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, ctx);
    Task::none()
}

pub fn on_worker(state: &mut State, event: &worker::Event, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, event, ctx);
    Task::none()
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, ctx);
    match msg {}
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let _ = state;
    widgets::page_header(ctx.palette, ctx.t("History"), None)
}
