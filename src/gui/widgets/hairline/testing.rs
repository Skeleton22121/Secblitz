//! Helpers the drawing tests share.
use iced::widget::canvas::{Action, Event};
use iced::window;
use std::time::Instant;

pub fn frame(at: Instant) -> Event {
    Event::Window(window::Event::RedrawRequested(at))
}

pub fn wants_frame(action: Option<Action<()>>) -> bool {
    action
        .map(|a| a.into_inner().1 == window::RedrawRequest::NextFrame)
        .unwrap_or(false)
}
