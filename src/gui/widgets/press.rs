//! Alive buttons: a drop-in for `iced::widget::button` with micro-interactions.
use super::anim;
use crate::gui::theme::mix;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, tree, Operation, Tree};
use iced::advanced::{overlay, Clipboard, Shell, Widget};
use iced::widget::button::{Status, Style};
use iced::{
    keyboard, mouse, touch, window, Background, Border, Color, Element, Event, Length, Padding,
    Rectangle, Renderer, Size, Theme, Transformation, Vector,
};
use std::time::{Duration, Instant};

const PRESS_SCALE: f32 = 0.03;
pub const FOCUS_ALPHA: f32 = 0.14;
const HOVER: Duration = Duration::from_millis(110);
const DOWN: Duration = anim::FASTER;
const UP: Duration = Duration::from_millis(220);
pub const SCALE_MAX_WIDTH: f32 = 260.0;

type StyleFn<'a> = Box<dyn Fn(&Theme, Status) -> Style + 'a>;

pub fn button<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
) -> Press<'a, Message> {
    Press::new(content)
}

pub struct Press<'a, Message> {
    content: Element<'a, Message>,
    on_press: Option<Message>,
    width: Length,
    height: Length,
    padding: Padding,
    clip: bool,
    scale: bool,
    id: Option<widget::Id>,
    style: StyleFn<'a>,
}

impl<'a, Message: Clone + 'a> Press<'a, Message> {
    pub fn new(content: impl Into<Element<'a, Message>>) -> Self {
        let content = content.into();
        let size = content.as_widget().size_hint();
        Press {
            content,
            on_press: None,
            width: size.width.fluid(),
            height: size.height.fluid(),
            padding: Padding::new(5.0).left(10.0).right(10.0),
            clip: false,
            scale: true,
            id: None,
            style: Box::new(|_, _| Style::default()),
        }
    }
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
    pub fn padding<P: Into<Padding>>(mut self, padding: P) -> Self {
        self.padding = padding.into();
        self
    }
    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
        self
    }
    pub fn on_press_maybe(mut self, message: Option<Message>) -> Self {
        self.on_press = message;
        self
    }
    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }
    pub fn scale(mut self, scale: bool) -> Self {
        self.scale = scale;
        self
    }
    pub fn id(mut self, id: impl Into<widget::Id>) -> Self {
        self.id = Some(id.into());
        self
    }
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self {
        self.style = Box::new(style);
        self
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Track {
    pub value: f32,
    from: f32,
    to: f32,
    start: Option<Instant>,
    fresh: bool,
}

impl Track {
    pub fn at(v: f32) -> Self {
        Track {
            value: v,
            from: v,
            to: v,
            start: None,
            fresh: false,
        }
    }
    pub fn goal(&self) -> f32 {
        self.to
    }
    pub fn target(&mut self, to: f32) {
        if (self.to - to).abs() < f32::EPSILON {
            return;
        }
        self.from = self.value;
        self.to = to;
        if anim::reduced() {
            self.value = to;
            self.start = None;
            self.fresh = false;
        } else {
            self.fresh = true;
            self.start = None;
        }
    }
    pub fn running(&self) -> bool {
        self.fresh || self.start.is_some()
    }
    pub fn step(&mut self, now: Instant, dur: Duration, ease: impl Fn(f32) -> f32) -> bool {
        if self.fresh {
            self.fresh = false;
            self.start = Some(now);
        }
        let Some(start) = self.start else {
            return false;
        };
        let t = now.saturating_duration_since(start).as_secs_f32() / dur.as_secs_f32();
        if t >= 1.0 {
            self.value = self.to;
            self.start = None;
            return false;
        }
        self.value = self.from + (self.to - self.from) * ease(t);
        true
    }
}

fn back(t: f32) -> f32 {
    let c1 = 3.2_f32;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u * u * u + c1 * u * u
}

#[derive(Debug, Default)]
struct State {
    pressed: bool,
    focused: bool,
    hover: Track,
    press: Track,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

fn color_of(bg: Option<Background>) -> Option<Color> {
    match bg {
        Some(Background::Color(c)) => Some(c),
        _ => None,
    }
}

fn mix_fill(a: Option<Background>, b: Option<Background>, t: f32) -> Option<Background> {
    match (color_of(a), color_of(b)) {
        (None, None) => b.or(a),
        (x, y) => {
            let x = x.unwrap_or(Color {
                a: 0.0,
                ..y.unwrap_or(Color::TRANSPARENT)
            });
            let y = y.unwrap_or(Color { a: 0.0, ..x });
            Some(Background::Color(mix(x, y, t)))
        }
    }
}

fn mix_style(a: &Style, b: &Style, t: f32) -> Style {
    if t <= 0.0 {
        return *a;
    }
    Style {
        background: mix_fill(a.background, b.background, t),
        text_color: mix(a.text_color, b.text_color, t),
        border: Border {
            color: mix(a.border.color, b.border.color, t),
            width: a.border.width + (b.border.width - a.border.width) * t,
            radius: b.border.radius,
        },
        shadow: b.shadow,
        snap: b.snap,
    }
}

impl<Message: Clone> Press<'_, Message> {
    fn enabled(&self) -> bool {
        self.on_press.is_some()
    }
    fn look(&self, theme: &Theme, st: &State) -> Style {
        if !self.enabled() {
            return (self.style)(theme, Status::Disabled);
        }
        let h = st.hover.value.clamp(0.0, 1.0);
        let p = st.press.value.clamp(0.0, 1.0);
        let active = (self.style)(theme, Status::Active);
        if h <= 0.0 && p <= 0.0 {
            return active;
        }
        let hovered = (self.style)(theme, Status::Hovered);
        let mut s = mix_style(&active, &hovered, h);
        if p > 0.0 {
            let pressed = (self.style)(theme, Status::Pressed);
            s = mix_style(&s, &pressed, p);
        }
        s
    }
}

impl<'a, Message: Clone + 'a> Widget<Message, Theme, Renderer> for Press<'a, Message> {
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
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::padded(limits, self.width, self.height, self.padding, |limits| {
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, limits)
        })
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        if self.enabled() {
            let st = tree.state.downcast_mut::<State>();
            operation.focusable(self.id.as_ref(), layout.bounds(), st);
        }
        operation.traverse(&mut |operation| {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout.children().next().unwrap(),
                renderer,
                operation,
            );
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
            &mut tree.children[0],
            event,
            layout.children().next().unwrap(),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let enabled = self.enabled();
        let bounds = layout.bounds();
        let over = enabled && cursor.is_over(bounds);
        let st = tree.state.downcast_mut::<State>();

        if !shell.is_event_captured() {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                | Event::Touch(touch::Event::FingerPressed { .. })
                    if over =>
                {
                    st.pressed = true;
                    st.focused = false;
                    shell.capture_event();
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Touch(touch::Event::FingerLifted { .. })
                    if st.pressed =>
                {
                    st.pressed = false;
                    if over {
                        if let Some(m) = &self.on_press {
                            shell.publish(m.clone());
                        }
                    }
                    shell.capture_event();
                }
                Event::Touch(touch::Event::FingerLost { .. }) => st.pressed = false,
                Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                    if st.focused && enabled =>
                {
                    if matches!(
                        key,
                        keyboard::Key::Named(
                            keyboard::key::Named::Enter | keyboard::key::Named::Space
                        )
                    ) {
                        if let Some(m) = &self.on_press {
                            shell.publish(m.clone());
                        }
                        shell.capture_event();
                    }
                }
                _ => {}
            }
        }
        if !enabled {
            st.pressed = false;
        }
        if st.focused && !over && matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_))) {
            st.focused = false;
        }

        let (h0, p0) = (st.hover.to, st.press.to);
        st.hover.target(if over { 1.0 } else { 0.0 });
        let down = st.pressed && over;
        if down != (p0 > 0.5) {
            st.press.target(if down { 1.0 } else { 0.0 });
        }
        let changed = (st.hover.to - h0).abs() > 0.0 || (st.press.to - p0).abs() > 0.0;
        if changed {
            shell.request_redraw();
        }

        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let a = st.hover.step(*now, HOVER, |t| anim::STANDARD.at(t));
            let b = if st.press.to > 0.5 {
                st.press.step(*now, DOWN, |t| anim::DECELERATE.at(t))
            } else {
                st.press.step(*now, UP, back)
            };
            if a || b || st.hover.running() || st.press.running() {
                shell.request_redraw();
            }
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let st = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let style = self.look(theme, st);
        let content_layout = layout.children().next().unwrap();
        let viewport = if self.clip {
            bounds.intersection(viewport).unwrap_or(*viewport)
        } else {
            *viewport
        };

        let amount = if self.scale && bounds.width <= SCALE_MAX_WIDTH {
            st.press.value.clamp(-0.5, 1.0) * PRESS_SCALE
        } else {
            0.0
        };
        let focused = st.focused && self.enabled();
        let paint = |renderer: &mut Renderer| {
            if style.background.is_some() || style.border.width > 0.0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: style.border,
                        shadow: Default::default(),
                        snap: style.snap && amount == 0.0,
                    },
                    style
                        .background
                        .unwrap_or(Background::Color(Color::TRANSPARENT)),
                );
            }
            if focused {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: Border {
                            radius: style.border.radius,
                            ..Border::default()
                        },
                        shadow: Default::default(),
                        snap: false,
                    },
                    Background::Color(Color {
                        a: FOCUS_ALPHA,
                        ..style.text_color
                    }),
                );
            }
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                &renderer::Style {
                    text_color: style.text_color,
                },
                content_layout,
                cursor,
                &viewport,
            );
        };
        if amount.abs() > 0.0005 {
            let c = bounds.center();
            let m = Transformation::translate(c.x, c.y)
                * Transformation::scale(1.0 - amount)
                * Transformation::translate(-c.x, -c.y);
            renderer.with_transformation(m, |r| paint(r));
        } else {
            paint(renderer);
        }
    }
    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.enabled() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
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
            layout.children().next().unwrap(),
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: Clone + 'a> From<Press<'a, Message>> for Element<'a, Message> {
    fn from(p: Press<'a, Message>) -> Self {
        Element::new(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_overshoots_then_lands() {
        assert!((back(0.0)).abs() < 1e-5);
        assert!((back(1.0) - 1.0).abs() < 1e-5);
        assert!((0..100).any(|i| back(i as f32 / 100.0) > 1.0));
    }

    #[test]
    fn track_runs_and_stops() {
        let _m = anim::forced::set(false);
        let mut t = Track::default();
        t.target(1.0);
        assert!(t.running());
        let t0 = Instant::now();
        assert!(t.step(t0, anim::FAST, |x| x));
        assert!(!t.step(t0 + anim::FAST * 2, anim::FAST, |x| x));
        assert_eq!(t.value, 1.0);
        assert!(!t.running());
    }

    #[test]
    fn missing_fill_fades_from_transparent() {
        let b = Some(Background::Color(Color::from_rgb(1.0, 0.0, 0.0)));
        match mix_fill(None, b, 0.5) {
            Some(Background::Color(c)) => assert!((c.a - 0.5).abs() < 1e-5),
            _ => panic!("expected a colour"),
        }
    }
}
