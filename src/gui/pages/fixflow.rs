//! Fix / undo flow drawn over any page: review sheet → working → result.
//! OWNER: fixes agent.
use crate::app::worker;
use crate::gui::{Ctx, Message};
use iced::{Element, Task};

#[derive(Debug, Default)]
pub struct State {}

#[derive(Debug, Clone)]
pub enum Msg {}

pub fn open_fixes(state: &mut State, ids: Vec<String>, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, ids, ctx);
    Task::none()
}

pub fn open_undo(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, ctx);
    Task::none()
}

/// Esc: close the review sheet / result (never cancels running work).
pub fn escape(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, ctx);
    Task::none()
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, ctx);
    match msg {}
}

pub fn on_worker(state: &mut State, event: &worker::Event, ctx: &mut Ctx) -> Task<Message> {
    let _ = (state, event, ctx);
    Task::none()
}

/// Wrap the window body with the active sheet / working / result view.
pub fn overlay<'a>(state: &'a State, ctx: &'a Ctx, body: Element<'a, Message>) -> Element<'a, Message> {
    let _ = (state, ctx);
    body
}
