//! One-shot "slide in" for small overlays such as toasts.
//!
//! The child is drawn `distance` px lower and eases up to its place over
//! `NORMAL` (250 ms) on `anim::DECELERATE`. When `leaving` is set it slides back
//! down over `FAST` (150 ms) on `anim::ACCELERATE` (leave faster than enter).
//! It asks for a redraw only while a slide runs (no subscription, no timer):
//! when it ends the window is idle.
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{overlay, Clipboard, Shell, Widget};
use iced::{mouse, window, Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};
use std::time::Instant;

use super::anim;

/// Slide duration: the shared "normal" motion token.
const SLIDE_MS: f32 = anim::NORMAL.as_millis() as f32;
/// Exit duration: the shared "fast" token.
const LEAVE_MS: f32 = anim::FAST.as_millis() as f32;

#[derive(Default)]
struct State {
    start: Option<Instant>,
    done: bool,
    /// When the exit began (set on the first redraw with `leaving`).
    leave_start: Option<Instant>,
}

struct SlideIn<'a, Message> {
    content: Element<'a, Message>,
    distance: f32,
    leaving: bool,
}

fn offset(state: &State, distance: f32, leaving: bool, now: Option<Instant>) -> f32 {
    if anim::reduced() {
        return 0.0;
    }
    if leaving {
        let Some(start) = state.leave_start else {
            return 0.0;
        };
        let elapsed = now
            .map(|n| n.saturating_duration_since(start).as_secs_f32() * 1000.0)
            .unwrap_or(0.0);
        return distance * anim::ACCELERATE.at(elapsed / LEAVE_MS);
    }
    if state.done {
        return 0.0;
    }
    let Some(start) = state.start else {
        return distance;
    };
    let elapsed = now
        .map(|n| n.saturating_duration_since(start).as_secs_f32() * 1000.0)
        .unwrap_or(0.0);
    distance * (1.0 - anim::DECELERATE.at(elapsed / SLIDE_MS))
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
        let dy = offset(state, self.distance, self.leaving, Some(Instant::now()));
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
            if self.leaving {
                let start = *state.leave_start.get_or_insert(*now);
                if now.saturating_duration_since(start).as_secs_f32() * 1000.0 < LEAVE_MS {
                    shell.request_redraw();
                }
            } else if state.leave_start.take().is_some() {
                // A new toast arrived mid-exit: stay put.
                state.done = true;
            } else if !state.done {
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

/// Slide `content` up by `distance` px once, when it first appears, and back
/// down when `leaving` is set.
pub fn slide_in<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    distance: f32,
    leaving: bool,
) -> Element<'a, Message> {
    Element::new(SlideIn {
        content: content.into(),
        distance,
        leaving,
    })
}

// ------------------------------------------------------------ page entrance

/// Length of the page entrance (fade + rise).
pub const ENTER: std::time::Duration = std::time::Duration::from_millis(220);
/// How far the incoming page rises, in px.
pub const ENTER_RISE: f32 = 12.0;

/// Progress (0..1, decelerated) of a page entrance that began at `start`.
/// Returns 1 under reduced motion.
pub fn enter_progress(start: Instant, now: Instant) -> f32 {
    if anim::reduced() {
        return 1.0;
    }
    let t = now.saturating_duration_since(start).as_secs_f32() / ENTER.as_secs_f32();
    anim::DECELERATE.at(t)
}

/// `p` with every colour moved towards `to` by `1 - t` (t = 1: unchanged,
/// t = 0: invisible against `to`). iced has no whole-subtree opacity, so a
/// page that is built from a faded palette fades cheaply: nothing is drawn
/// off-screen and no layer is composited.
pub fn fade_palette(p: &crate::gui::theme::Palette, to: iced::Color, t: f32) -> crate::gui::theme::Palette {
    use crate::gui::theme::mix;
    let t = t.clamp(0.0, 1.0);
    if t >= 1.0 {
        return *p;
    }
    let f = |c: iced::Color| mix(to, c, t);
    crate::gui::theme::Palette {
        surface: f(p.surface),
        surface_alt: f(p.surface_alt),
        border: f(p.border),
        border_strong: f(p.border_strong),
        hover: f(p.hover),
        hover_strong: f(p.hover_strong),
        pressed: f(p.pressed),
        selected: f(p.selected),
        focus_ring: f(p.focus_ring),
        disabled_bg: f(p.disabled_bg),
        disabled_fg: f(p.disabled_fg),
        text: f(p.text),
        text_muted: f(p.text_muted),
        brand: f(p.brand),
        on_brand: f(p.on_brand),
        brand_hover: f(p.brand_hover),
        brand_pressed: f(p.brand_pressed),
        good: f(p.good),
        warn: f(p.warn),
        bad: f(p.bad),
        neutral: f(p.neutral),
        good_text: f(p.good_text),
        warn_text: f(p.warn_text),
        bad_text: f(p.bad_text),
        danger: f(p.danger),
        danger_hover: f(p.danger_hover),
        danger_pressed: f(p.danger_pressed),
        ..*p
    }
}

struct Lift<'a, Message> {
    content: Element<'a, Message>,
    dy: f32,
}

impl<Message> Widget<Message, Theme, Renderer> for Lift<'_, Message> {
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
    fn layout(&mut self, tree: &mut Tree, r: &Renderer, limits: &layout::Limits) -> layout::Node {
        self.content.as_widget_mut().layout(tree, r, limits)
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
        if self.dy.abs() < 0.25 {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
        } else {
            // The pointer sits over the shifted drawing, so hit-testing the
            // content at its real place is off by at most `ENTER_RISE` px for
            // 220 ms; hover colours are drawn for the unshifted cursor.
            renderer.with_translation(Vector::new(0.0, self.dy), |renderer| {
                self.content
                    .as_widget()
                    .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
            .operate(tree, layout, renderer, operation);
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

/// Draw `content` shifted down by `dy` px (transparent to events and layout:
/// the tree is the child's own, so state survives when `dy` goes back to 0).
pub fn lift<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    dy: f32,
) -> Element<'a, Message> {
    Element::new(Lift {
        content: content.into(),
        dy,
    })
}

#[cfg(test)]
mod enter_tests {
    use super::*;
    use crate::gui::theme::{DARK, LIGHT};

    #[test]
    fn fade_endpoints() {
        for p in [LIGHT, DARK] {
            assert_eq!(fade_palette(&p, p.bg, 1.0), p);
            let hidden = fade_palette(&p, p.bg, 0.0);
            assert_eq!(hidden.text, p.bg);
            assert_eq!(hidden.bg, p.bg);
        }
    }

    #[test]
    fn enter_progress_is_monotonic() {
        let t0 = Instant::now();
        let mut last = -1.0;
        for ms in (0..=250).step_by(10) {
            let v = enter_progress(t0, t0 + std::time::Duration::from_millis(ms));
            assert!(v >= last);
            last = v;
        }
        assert!((last - 1.0).abs() < 1e-4 || anim::reduced());
    }
}
