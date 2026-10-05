//! One-shot "slide in" for small overlays such as toasts.
//!
//! The child is drawn `distance` px lower and eases up to its place over
//! `SLIDE_MS` with the shared emphasized curve (`anim::EMPHASIZED`). It asks for a redraw only while the
//! slide runs (no subscription, no timer): when it ends the window is idle.
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{overlay, Clipboard, Shell, Widget};
use iced::{mouse, window, Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};
use std::time::Instant;

use super::anim;

/// Slide duration: the shared "normal" motion token.
const SLIDE_MS: f32 = anim::NORMAL.as_millis() as f32;

#[derive(Default)]
struct State {
    start: Option<Instant>,
    done: bool,
}

struct SlideIn<'a, Message> {
    content: Element<'a, Message>,
    distance: f32,
}

fn offset(state: &State, distance: f32, now: Option<Instant>) -> f32 {
    if state.done || anim::reduced() {
        return 0.0;
    }
    let Some(start) = state.start else {
        return distance;
    };
    let elapsed = now
        .map(|n| n.saturating_duration_since(start).as_secs_f32() * 1000.0)
        .unwrap_or(0.0);
    distance * (1.0 - anim::EMPHASIZED.at(elapsed / SLIDE_MS))
}

impl<Message> Widget<Message, Theme, Renderer> for SlideIn<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content]);
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let dy = offset(state, self.distance, Some(Instant::now()));
        let child = &tree.children[0];
        if dy.abs() < 0.5 {
            self.content
                .as_widget()
                .draw(child, renderer, theme, style, layout, cursor, viewport);
        } else {
            renderer.with_translation(Vector::new(0.0, dy), |renderer| {
                self.content
                    .as_widget()
                    .draw(child, renderer, theme, style, layout, cursor, viewport);
            });
        }
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let state = tree.state.downcast_mut::<State>();
            if !state.done {
                let start = *state.start.get_or_insert(*now);
                if now.saturating_duration_since(start).as_secs_f32() * 1000.0 >= SLIDE_MS {
                    state.done = true;
                } else {
                    shell.request_redraw();
                }
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Slide `content` up by `distance` px once, when it first appears.
pub fn slide_in<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    distance: f32,
) -> Element<'a, Message> {
    Element::new(SlideIn {
        content: content.into(),
        distance,
    })
}
