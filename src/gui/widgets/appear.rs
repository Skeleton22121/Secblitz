//! One-shot appearances: toasts sliding in, pages and sheets easing in.
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{overlay, Clipboard, Shell, Widget};
use iced::{
    mouse, window, Border, Color, Element, Event, Length, Rectangle, Renderer, Shadow, Size, Theme,
    Transformation, Vector,
};
use std::time::{Duration, Instant};

use super::anim;

const SLIDE_MS: f32 = anim::NORMAL.as_millis() as f32;
const LEAVE_MS: f32 = anim::FAST.as_millis() as f32;

#[derive(Default)]
struct State {
    start: Option<Instant>,
    done: bool,
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

pub const ENTER: Duration = Duration::from_millis(180);
pub const ENTER_RISE: f32 = 8.0;
const ENTER_VEIL: f32 = 0.7;

pub const POP_IN: Duration = Duration::from_millis(180);
pub const POP_FROM: f32 = 0.98;
const STAGGER: Duration = Duration::from_millis(30);
const STAGGER_STEPS: usize = 3;

/// How a page or sheet looks part way through appearing: how far it still
/// has to rise, its size, and how much of the surface behind it still covers it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub rise: f32,
    pub scale: f32,
    pub veil: f32,
}

impl Look {
    pub const SETTLED: Look = Look {
        rise: 0.0,
        scale: 1.0,
        veil: 0.0,
    };
}

/// Progress (0..1, decelerated) of a page entrance that began at `start`.
/// Returns 1 under reduced motion.
pub fn enter_progress(start: Instant, now: Instant) -> f32 {
    if anim::reduced() {
        return 1.0;
    }
    let t = now.saturating_duration_since(start).as_secs_f32() / ENTER.as_secs_f32();
    anim::DECELERATE.at(t)
}

pub fn page_look(t: f32) -> Look {
    let k = 1.0 - t.clamp(0.0, 1.0);
    Look {
        rise: ENTER_RISE * k,
        scale: 1.0,
        veil: ENTER_VEIL * k,
    }
}

/// A page fading out in place, ending exactly where [`page_look`] starts.
pub fn leave_look(t: f32) -> Look {
    Look {
        rise: 0.0,
        scale: 1.0,
        veil: ENTER_VEIL * anim::ACCELERATE.at(t),
    }
}

pub fn pop_progress(start: Instant, now: Instant) -> f32 {
    if anim::reduced() {
        return 1.0;
    }
    let t = now.saturating_duration_since(start).as_secs_f32() / POP_IN.as_secs_f32();
    anim::DECELERATE.at(t)
}

pub fn pop_look(t: f32) -> Look {
    let t = t.clamp(0.0, 1.0);
    Look {
        rise: 0.0,
        scale: POP_FROM + (1.0 - POP_FROM) * t,
        veil: 1.0 - t,
    }
}

fn about(look: Look, bounds: Rectangle) -> Transformation {
    let c = bounds.center();
    Transformation::translate(c.x, c.y + look.rise)
        * Transformation::scale(look.scale)
        * Transformation::translate(-c.x, -c.y)
}

/// Lays `color` at `amount` over `bounds`, kept inside `viewport` because a new
/// layer is not clipped by the one it sits in.
fn veil_over(
    renderer: &mut Renderer,
    bounds: Rectangle,
    viewport: &Rectangle,
    color: Color,
    amount: f32,
    radius: f32,
) {
    let Some(bounds) = bounds.intersection(viewport) else {
        return;
    };
    if amount < 0.004 {
        return;
    }
    renderer.with_layer(bounds, |renderer| {
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    radius: radius.into(),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: false,
            },
            color.scale_alpha(amount.clamp(0.0, 1.0)),
        );
    });
}

struct Enter<'a, Message> {
    content: Element<'a, Message>,
    look: Look,
    veil: Color,
}

impl<Message> Widget<Message, Theme, Renderer> for Enter<'_, Message> {
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
        if self.look == Look::SETTLED {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
            return;
        }
        let bounds = layout.bounds();
        renderer.with_transformation(about(self.look, bounds), |renderer| {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
            veil_over(renderer, bounds, viewport, self.veil, self.look.veil, 0.0);
        });
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

/// `content` drawn part way through appearing, as `look` says, with `veil`
/// (the colour of the surface behind it) laid over it to fade it in.
pub fn enter<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    look: Look,
    veil: Color,
) -> Element<'a, Message> {
    Element::new(Enter {
        content: content.into(),
        look,
        veil,
    })
}

#[derive(Default)]
struct PopState {
    start: Option<Instant>,
}

struct Pop<'a, Message> {
    content: Element<'a, Message>,
    /// Drawn behind a sheet's panel; `None` when the content simply eases in.
    scrim: Option<Color>,
    veil: Color,
    radius: f32,
    delay: Duration,
}

impl<Message> Widget<Message, Theme, Renderer> for Pop<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<PopState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(PopState::default())
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
        let state = tree.state.downcast_ref::<PopState>();
        let t = state
            .start
            .map_or(0.0, |s| pop_progress(s + self.delay, Instant::now()));
        let child = &tree.children[0];
        if let Some(scrim) = self.scrim {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    ..renderer::Quad::default()
                },
                scrim.scale_alpha(t),
            );
        }
        let panel = match self.scrim {
            Some(_) => layout.children().next().map(|l| l.bounds()),
            None => Some(layout.bounds()),
        };
        match panel {
            Some(panel) if t < 1.0 => {
                let look = if self.scrim.is_some() {
                    pop_look(t)
                } else {
                    page_look(t)
                };
                renderer.with_transformation(about(look, panel), |renderer| {
                    self.content
                        .as_widget()
                        .draw(child, renderer, theme, style, layout, cursor, viewport);
                    veil_over(renderer, panel, viewport, self.veil, look.veil, self.radius);
                });
            }
            _ => self
                .content
                .as_widget()
                .draw(child, renderer, theme, style, layout, cursor, viewport),
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
            let state = tree.state.downcast_mut::<PopState>();
            let start = *state.start.get_or_insert(*now);
            if pop_progress(start + self.delay, *now) < 1.0 {
                shell.request_redraw();
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

/// A full-window layer whose first child is a panel: the first time it is
/// drawn the scrim fades in while the panel grows from 98% and fades in.
/// `content` must draw no background of its own.
pub fn pop<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    scrim: Color,
    panel: Color,
    radius: f32,
) -> Element<'a, Message> {
    Element::new(Pop {
        content: content.into(),
        scrim: Some(scrim),
        veil: panel,
        radius,
        delay: Duration::ZERO,
    })
}

/// `content` eases in (a small rise and a fade from `surface`) the first time
/// it is drawn, for a result replacing a progress view inside a sheet.
pub fn settle<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    surface: Color,
) -> Element<'a, Message> {
    settle_after(content, surface, Duration::ZERO)
}

/// How long the `index`th card of a page waits before easing in: a short
/// step each, never more than a few steps in total.
pub fn stagger_delay(index: usize) -> Duration {
    STAGGER * index.min(STAGGER_STEPS) as u32
}

/// [`settle`], starting after `delay`.
pub fn settle_after<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    surface: Color,
    delay: Duration,
) -> Element<'a, Message> {
    Element::new(Pop {
        content: content.into(),
        scrim: None,
        veil: surface,
        radius: 0.0,
        delay,
    })
}

#[cfg(test)]
mod enter_tests {
    use super::*;
    use crate::gui::widgets::anim::forced;

    #[test]
    fn enter_progress_is_monotonic() {
        let _m = forced::set(false);
        let t0 = Instant::now();
        let mut last = -1.0;
        for ms in (0..=250).step_by(10) {
            let v = enter_progress(t0, t0 + Duration::from_millis(ms));
            assert!(v >= last);
            last = v;
        }
        assert!((last - 1.0).abs() < 1e-4);
    }

    #[test]
    fn page_enters_quickly_with_a_small_rise() {
        assert!(ENTER >= Duration::from_millis(160) && ENTER <= Duration::from_millis(200));
        assert_eq!(ENTER_RISE, 8.0);
        let start = page_look(0.0);
        assert_eq!(start.rise, ENTER_RISE);
        assert!((start.veil - ENTER_VEIL).abs() < 1e-6);
        assert_eq!(page_look(1.0), Look::SETTLED);
    }

    #[test]
    fn leaving_ends_where_entering_starts() {
        assert_eq!(leave_look(0.0).veil, 0.0);
        assert!((leave_look(1.0).veil - page_look(0.0).veil).abs() < 1e-6);
    }

    #[test]
    fn sheets_grow_from_98_percent_and_settle() {
        let a = pop_look(0.0);
        assert_eq!((a.scale, a.veil, a.rise), (POP_FROM, 1.0, 0.0));
        assert_eq!(pop_look(1.0), Look::SETTLED);
        assert!(POP_IN <= Duration::from_millis(200) && POP_IN >= Duration::from_millis(150));
    }

    #[test]
    fn pop_progress_runs_then_settles() {
        let _m = forced::set(false);
        let t0 = Instant::now();
        assert_eq!(pop_progress(t0, t0), 0.0);
        assert_eq!(pop_progress(t0, t0 + POP_IN), 1.0);
        let mid = pop_progress(t0, t0 + POP_IN / 2);
        assert!(mid > 0.5 && mid < 1.0);
    }

    #[test]
    fn stagger_is_short_and_capped() {
        assert_eq!(stagger_delay(0), Duration::ZERO);
        assert_eq!(stagger_delay(1), STAGGER);
        assert!(STAGGER <= Duration::from_millis(30));
        assert_eq!(stagger_delay(3), stagger_delay(40));
        assert!(stagger_delay(40) + POP_IN < Duration::from_millis(300));
    }

    #[test]
    fn reduced_motion_skips_both() {
        let _m = forced::set(true);
        let t0 = Instant::now();
        assert_eq!(enter_progress(t0, t0), 1.0);
        assert_eq!(pop_progress(t0, t0), 1.0);
    }
}
