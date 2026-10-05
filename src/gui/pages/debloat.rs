//! Clean up apps page. OWNER: debloat agent.
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

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, ctx);
    match msg {}
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let _ = state;
    widgets::page_header(ctx.palette, ctx.t("Clean up apps"), None)
}
