//! Icon-only overflow button ("More", three dots) with a small popup menu.
use super::anim;
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette};
use crate::gui::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::svg::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _, Text};
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{overlay, Clipboard, Renderer as _, Shell, Widget};
use iced::keyboard::{self, key::Named, Key};
use iced::{
    alignment, mouse, window, Background, Border, Color, Element, Event, Length, Pixels, Point,
    Radians, Rectangle, Renderer, Shadow, Size, Theme, Vector,
};
use std::time::Instant;

const MORE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20" fill="none"><path d="M6.25 10C6.25 10.6904 5.69036 11.25 5 11.25C4.30964 11.25 3.75 10.6904 3.75 10C3.75 9.30964 4.30964 8.75 5 8.75C5.69036 8.75 6.25 9.30964 6.25 10ZM11.25 10C11.25 10.6904 10.6904 11.25 10 11.25C9.30964 11.25 8.75 10.6904 8.75 10C8.75 9.30964 9.30964 8.75 10 8.75C10.6904 8.75 11.25 9.30964 11.25 10ZM15 11.25C15.6904 11.25 16.25 10.6904 16.25 10C16.25 9.30964 15.6904 8.75 15 8.75C14.3096 8.75 13.75 9.30964 13.75 10C13.75 10.6904 14.3096 11.25 15 11.25Z" fill="currentColor"/></svg>"#;

const BUTTON: f32 = 32.0;
const SLIDE_PX: f32 = 6.0;
const SLIDE_MS: f32 = anim::FAST.as_millis() as f32;
const ICON: f32 = 16.0;

pub struct MenuItem {
    pub icon: Icon,
    pub label: String,
    pub message: Message,
    pub danger: bool,
}

#[derive(Default)]
struct State {
    open: bool,
    hover: Option<usize>,
    opened_at: Option<Instant>,
}

struct Overflow {
    p: Palette,
    items: Vec<MenuItem>,
    button: Element<'static, Message>,
}

fn menu_size(items: &[MenuItem]) -> Size {
    let longest = items
        .iter()
        .map(|i| i.label.chars().count())
        .max()
        .unwrap_or(4) as f32;
    let w =
        (longest * 7.6 + theme::S3 * 2.0 + ICON + theme::S3 + theme::S2 * 2.0).clamp(168.0, 300.0);
    Size::new(w, items.len() as f32 * theme::MENU_ROW + theme::S1 * 2.0)
}

fn row_rect(menu: Rectangle, i: usize) -> Rectangle {
    Rectangle {
        x: menu.x + theme::S1,
        y: menu.y + theme::S1 + i as f32 * theme::MENU_ROW,
        width: menu.width - theme::S1 * 2.0,
        height: theme::MENU_ROW,
    }
}

impl Widget<Message, Theme, Renderer> for Overflow {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.button)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.button));
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(BUTTON), Length::Fixed(BUTTON))
    }
    fn layout(&mut self, tree: &mut Tree, r: &Renderer, limits: &layout::Limits) -> layout::Node {
        let child = self
            .button
            .as_widget_mut()
            .layout(&mut tree.children[0], r, limits);
        layout::Node::with_children(child.size(), vec![child])
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
        let st = tree.state.downcast_ref::<State>();
        if st.open {
            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        radius: theme::R.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: true,
                },
                Background::Color(self.p.selected),
            );
        }
        self.button.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout.children().next().unwrap(),
            cursor,
            viewport,
        );
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        r: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let captured_before = shell.is_event_captured();
        self.button.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.children().next().unwrap(),
            cursor,
            r,
            clipboard,
            shell,
            viewport,
        );
        let st = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(layout.bounds()) =>
            {
                st.open = !st.open;
                st.hover = None;
                st.opened_at = None;
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(Named::Enter | Named::Space),
                ..
            }) if !captured_before && shell.is_event_captured() => {
                st.open = !st.open;
                st.hover = if st.open { Some(0) } else { None };
                st.opened_at = None;
                shell.request_redraw();
            }
            _ => {}
        }
    }
    fn mouse_interaction(
        &self,
        _: &Tree,
        _: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
        _: &Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::Idle
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        _: &Renderer,
        _: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let st = tree.state.downcast_mut::<State>();
        if !st.open {
            return None;
        }
        Some(overlay::Element::new(Box::new(Menu {
            p: self.p,
            items: &self.items,
            state: st,
            anchor: layout.bounds() + translation,
        })))
    }
}

struct Menu<'b> {
    p: Palette,
    items: &'b [MenuItem],
    state: &'b mut State,
    anchor: Rectangle,
}

impl Menu<'_> {
    fn slide(&self, now: Instant) -> f32 {
        if anim::reduced() {
            return 0.0;
        }
        match self.state.opened_at {
            Some(s) => {
                let t = now.saturating_duration_since(s).as_secs_f32() * 1000.0 / SLIDE_MS;
                SLIDE_PX * (1.0 - anim::DECELERATE.at(t))
            }
            None => SLIDE_PX,
        }
    }
}

impl overlay::Overlay<Message, Theme, Renderer> for Menu<'_> {
    fn layout(&mut self, _: &Renderer, bounds: Size) -> layout::Node {
        let size = menu_size(self.items);
        let gap = theme::S1;
        let mut x = self.anchor.x + self.anchor.width - size.width;
        x = x.clamp(0.0, (bounds.width - size.width).max(0.0));
        let below = self.anchor.y + self.anchor.height + gap;
        let y = if below + size.height <= bounds.height {
            below
        } else {
            (self.anchor.y - gap - size.height).max(0.0)
        };
        layout::Node::new(size).move_to(Point::new(x, y))
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
    ) {
        let p = self.p;
        let menu = layout.bounds();
        let dy = self.slide(Instant::now());
        renderer.with_translation(Vector::new(0.0, -dy), |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds: menu,
                    border: Border {
                        radius: theme::R.into(),
                        width: theme::HAIRLINE,
                        color: p.border,
                    },
                    shadow: Shadow::default(), // never: tiny-skia draws shadows unclipped
                    snap: true,
                },
                Background::Color(p.popup),
            );
            for (i, item) in self.items.iter().enumerate() {
                let r = row_rect(menu, i);
                if self.state.hover == Some(i) {
                    renderer.fill_quad(
                        Quad {
                            bounds: r,
                            border: Border {
                                radius: theme::R_SMALL.into(),
                                ..Border::default()
                            },
                            shadow: Shadow::default(),
                            snap: true,
                        },
                        Background::Color(p.hover_strong),
                    );
                }
                let fg: Color = if item.danger { p.bad_text } else { p.text };
                let ig = if item.danger {
                    p.bad_text
                } else {
                    p.text_muted
                };
                let ib = Rectangle {
                    x: r.x + theme::S3,
                    y: r.center_y() - ICON / 2.0,
                    width: ICON,
                    height: ICON,
                };
                renderer.draw_svg(
                    svg::Svg {
                        handle: svg::Handle::from_memory(item.icon.svg()),
                        color: Some(ig),
                        rotation: Radians(0.0),
                        opacity: 1.0,
                    },
                    ib,
                    r,
                );
                let tx = ib.x + ICON + theme::S3;
                renderer.fill_text(
                    Text {
                        content: item.label.clone(),
                        bounds: Size::new(r.x + r.width - tx - theme::S3, r.height),
                        size: Pixels(theme::BODY),
                        line_height: text::LineHeight::Absolute(Pixels(theme::LINE_BODY)),
                        font: theme::REGULAR,
                        align_x: text::Alignment::Left,
                        align_y: alignment::Vertical::Center,
                        shaping: text::Shaping::Advanced,
                        wrapping: text::Wrapping::None,
                    },
                    Point::new(tx, r.center_y()),
                    fg,
                    r,
                );
            }
        });
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let menu = layout.bounds();
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                let s = *self.state.opened_at.get_or_insert(*now);
                if !anim::reduced()
                    && now.saturating_duration_since(s).as_secs_f32() * 1000.0 < SLIDE_MS
                {
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let h = (0..self.items.len()).find(|&i| cursor.is_over(row_rect(menu, i)));
                if h != self.state.hover {
                    self.state.hover = h;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(menu) {
                    if let Some(i) =
                        (0..self.items.len()).find(|&i| cursor.is_over(row_rect(menu, i)))
                    {
                        shell.publish(self.items[i].message.clone());
                        self.state.open = false;
                    }
                    shell.capture_event();
                } else {
                    self.state.open = false;
                    // A press on the button itself only closes (it must not
                    // reach the button and reopen the menu).
                    if cursor.is_over(self.anchor) {
                        shell.capture_event();
                    }
                }
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(Named::Escape),
                ..
            }) => {
                self.state.open = false;
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(key @ (Named::ArrowDown | Named::ArrowUp)),
                ..
            }) if !self.items.is_empty() => {
                let n = self.items.len();
                self.state.hover = Some(match (self.state.hover, key) {
                    (None, Named::ArrowUp) => n - 1,
                    (None, _) => 0,
                    (Some(i), Named::ArrowUp) => (i + n - 1) % n,
                    (Some(i), _) => (i + 1) % n,
                });
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(Named::Enter | Named::Space),
                ..
            }) => {
                if let Some(item) = self.state.hover.and_then(|i| self.items.get(i)) {
                    shell.publish(item.message.clone());
                    self.state.open = false;
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            _ => {}
        }
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Idle
        } else {
            mouse::Interaction::None
        }
    }
}

pub fn overflow_menu<'a>(
    p: Palette,
    items: Vec<(Icon, String, Message, bool)>,
) -> Element<'a, Message> {
    if items.is_empty() {
        return iced::widget::space::horizontal().width(0).into();
    }
    if items.len() == 1 {
        let (_, label, message, _) = items.into_iter().next().expect("one item");
        return super::action(p, super::ButtonKind::Secondary, label, None, Some(message));
    }
    let items = items
        .into_iter()
        .map(|(icon, label, message, danger)| MenuItem {
            icon,
            label,
            message,
            danger,
        })
        .collect();
    let button = super::press::button(
        iced::widget::svg(svg::Handle::from_memory(MORE_SVG))
            .width(20)
            .height(20)
            .style(move |_, _| iced::widget::svg::Style {
                color: Some(p.text_muted),
            }),
    )
    .width(BUTTON)
    .height(BUTTON)
    .padding((BUTTON - 20.0) / 2.0)
    .on_press(Message::Noop)
    .style(move |_, status| iced::widget::button::Style {
        background: match status {
            iced::widget::button::Status::Hovered => Some(Background::Color(p.hover_strong)),
            iced::widget::button::Status::Pressed => Some(Background::Color(p.pressed)),
            _ => None,
        },
        text_color: p.text_muted,
        border: Border {
            radius: theme::R.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        snap: true,
    });
    super::arrow(Element::new(Overflow {
        p,
        items,
        button: button.into(),
    }))
}
