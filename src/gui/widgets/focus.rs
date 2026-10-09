//! Keyboard focus zones. A window zone keeps Tab inside an open sheet, a page
//! zone marks where the current page starts.
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::{self, Focusable, Operation, Outcome};
use iced::advanced::widget::{tree, Id, Tree};
use iced::advanced::{overlay, Clipboard, Shell, Widget};
use iced::keyboard::{self, key::Named};
use iced::{mouse, Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};
use std::any::Any;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Window,
    Page,
}

#[derive(Debug, Clone, Copy)]
enum Mark {
    Enter(Kind),
    Leave(Kind),
}

struct Zone<'a, Message> {
    kind: Kind,
    content: Element<'a, Message>,
}

/// Marks an open sheet: while it exists, Tab and Shift+Tab stay inside it.
pub fn window<'a, Message: 'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Zone {
        kind: Kind::Window,
        content: content.into(),
    })
}

/// Marks the page itself, so a new page can take the first Tab.
pub fn page<'a, Message: 'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Zone {
        kind: Kind::Page,
        content: content.into(),
    })
}

fn is_activation(key: &keyboard::Key) -> bool {
    matches!(
        key,
        keyboard::Key::Named(
            Named::Enter
                | Named::Space
                | Named::ArrowLeft
                | Named::ArrowRight
                | Named::ArrowUp
                | Named::ArrowDown
        )
    )
}

impl<Message> Widget<Message, Theme, Renderer> for Zone<'_, Message> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }
    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }
    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }
    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
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
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let bounds = layout.bounds();
        let kind = self.kind;
        operation.traverse(&mut |operation| {
            operation.custom(None, bounds, &mut Mark::Enter(kind));
            self.content
                .as_widget_mut()
                .operate(tree, layout, renderer, operation);
            operation.custom(None, bounds, &mut Mark::Leave(kind));
        });
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
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
        // A key the sheet did not use must not reach a control hidden under it.
        if self.kind == Kind::Window && !shell.is_event_captured() {
            if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event {
                if is_activation(key) {
                    shell.capture_event();
                }
            }
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

/// One focusable control, in the order Tab visits them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop {
    /// The open sheet the control sits in, counted in drawing order.
    pub window: Option<usize>,
    pub page: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Next,
    Previous,
    /// The first control of the page, for the first Tab after a page change.
    PageStart,
}

/// Which control should have focus after `step`, or none.
pub fn target(stops: &[Stop], step: Move) -> Option<usize> {
    let top = stops.iter().filter_map(|s| s.window).max();
    let (reachable, wrap): (Vec<usize>, bool) = match top {
        Some(top) => (
            (0..stops.len())
                .filter(|i| stops[*i].window == Some(top))
                .collect(),
            true,
        ),
        None => ((0..stops.len()).collect(), false),
    };
    if step == Move::PageStart && top.is_none() && !stops.iter().any(|s| s.focused) {
        let page: Vec<usize> = reachable
            .iter()
            .copied()
            .filter(|i| stops[*i].page)
            .collect();
        return page.first().or(reachable.first()).copied();
    }
    let at = reachable.iter().position(|i| stops[*i].focused);
    match (step, at) {
        (Move::Previous, Some(0)) if wrap => reachable.last().copied(),
        (Move::Previous, Some(0)) => None,
        (Move::Previous, Some(i)) => Some(reachable[i - 1]),
        (Move::Previous, None) => reachable.last().copied(),
        (_, Some(i)) if i + 1 < reachable.len() => Some(reachable[i + 1]),
        (_, Some(_)) if wrap => reachable.first().copied(),
        (_, Some(_)) => None,
        (_, None) => reachable.first().copied(),
    }
}

struct Survey {
    step: Move,
    stops: Vec<Stop>,
    windows: usize,
    open: Vec<Option<usize>>,
    pages: usize,
}

impl<T: 'static> Operation<T> for Survey {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }
    fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        self.stops.push(Stop {
            window: self.open.iter().rev().find_map(|w| *w),
            page: self.pages > 0,
            focused: state.is_focused(),
        });
    }
    fn custom(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Any) {
        match state.downcast_ref::<Mark>() {
            Some(Mark::Enter(Kind::Window)) => {
                self.open.push(Some(self.windows));
                self.windows += 1;
            }
            Some(Mark::Leave(Kind::Window)) => {
                self.open.pop();
            }
            Some(Mark::Enter(Kind::Page)) => self.pages += 1,
            Some(Mark::Leave(Kind::Page)) => self.pages -= 1,
            None => {}
        }
    }
    fn finish(&self) -> Outcome<T> {
        Outcome::Chain(Box::new(Apply {
            target: target(&self.stops, self.step),
            at: 0,
        }))
    }
}

struct Apply {
    target: Option<usize>,
    at: usize,
}

impl<T: 'static> Operation<T> for Apply {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }
    fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        if self.target == Some(self.at) {
            state.focus();
        } else {
            state.unfocus();
        }
        self.at += 1;
    }
}

/// Moves focus one control, never leaving the topmost open sheet.
pub fn step<T: Send + 'static>(step: Move) -> impl Operation<T> {
    Survey {
        step,
        stops: Vec::new(),
        windows: 0,
        open: Vec::new(),
        pages: 0,
    }
}

/// Takes focus off every control.
pub fn clear<T: Send + 'static>() -> impl Operation<T> {
    operation::focusable::unfocus()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(window: Option<usize>, page: bool, focused: bool) -> Stop {
        Stop {
            window,
            page,
            focused,
        }
    }

    #[test]
    fn without_a_sheet_tab_walks_everything_and_stops_at_the_end() {
        let s = [
            stop(None, false, false),
            stop(None, true, true),
            stop(None, true, false),
        ];
        assert_eq!(target(&s, Move::Next), Some(2));
        assert_eq!(target(&s, Move::Previous), Some(0));
        let end = [stop(None, true, false), stop(None, true, true)];
        assert_eq!(target(&end, Move::Next), None);
        let start = [stop(None, true, true), stop(None, true, false)];
        assert_eq!(target(&start, Move::Previous), None);
    }

    #[test]
    fn nothing_focused_starts_at_either_end() {
        let s = [stop(None, true, false), stop(None, true, false)];
        assert_eq!(target(&s, Move::Next), Some(0));
        assert_eq!(target(&s, Move::Previous), Some(1));
    }

    #[test]
    fn an_open_sheet_holds_tab_and_wraps() {
        let s = [
            stop(None, true, false),
            stop(Some(0), false, false),
            stop(Some(0), false, true),
            stop(None, false, false),
        ];
        assert_eq!(target(&s, Move::Next), Some(1));
        assert_eq!(target(&s, Move::Previous), Some(1));
        let first = [
            stop(None, true, false),
            stop(Some(0), false, true),
            stop(Some(0), false, false),
        ];
        assert_eq!(target(&first, Move::Previous), Some(2));
    }

    #[test]
    fn focus_hidden_under_a_sheet_moves_into_it() {
        let s = [
            stop(None, true, true),
            stop(Some(0), false, false),
            stop(Some(0), false, false),
        ];
        assert_eq!(target(&s, Move::Next), Some(1));
        assert_eq!(target(&s, Move::Previous), Some(2));
    }

    #[test]
    fn the_topmost_sheet_wins() {
        let s = [
            stop(Some(0), false, false),
            stop(Some(1), false, false),
            stop(Some(1), false, true),
        ];
        assert_eq!(target(&s, Move::Next), Some(1));
    }

    #[test]
    fn a_sheet_without_controls_leaves_nothing_to_focus() {
        let s = [stop(None, true, true)];
        let sheet = [stop(None, true, true), stop(Some(0), false, false)];
        assert_eq!(target(&s, Move::Next), None);
        assert_eq!(target(&sheet, Move::Next), Some(1));
    }

    #[test]
    fn the_first_tab_after_a_page_change_lands_on_the_page() {
        let s = [
            stop(None, false, false),
            stop(None, false, false),
            stop(None, true, false),
            stop(None, true, false),
        ];
        assert_eq!(target(&s, Move::PageStart), Some(2));
        let no_page = [stop(None, false, false)];
        assert_eq!(target(&no_page, Move::PageStart), Some(0));
        let sheet = [stop(None, true, false), stop(Some(0), false, false)];
        assert_eq!(target(&sheet, Move::PageStart), Some(1));
        let clicked = [
            stop(None, true, false),
            stop(None, true, true),
            stop(None, true, false),
        ];
        assert_eq!(target(&clicked, Move::PageStart), Some(2));
    }
}
