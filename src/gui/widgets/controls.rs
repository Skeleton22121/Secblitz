//! Form controls: dropdown, segmented control, switch, checkbox, text field.
//! All are 36 px tall (28 px segments inside a 36 px track), keep the native
//! arrow cursor and use only theme colours.
use super::anim;
use super::cursor::arrow;
use super::icon;
use crate::gui::icons::Icon;
use crate::gui::theme::{self, mix, Palette};
use crate::gui::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, container, pick_list, row, stack, text, text_input};
use iced::{
    mouse, window, Alignment, Background, Border, Color, Element, Event, Length, Padding, Pixels,
    Rectangle, Renderer, Shadow, Size, Theme,
};
use std::time::Instant;

fn line(px: f32) -> LineHeight {
    LineHeight::Absolute(Pixels(px))
}

// ---------------------------------------------------------------- dropdown

/// Polished drop-down list: 36 px, 1 px border, chevron on the right, focus
/// ring while open. `selected: None` shows `placeholder`.
///
/// Menu rows are as tall as the control (36 px): iced derives both from the
/// same padding. Wrap in a fixed-width container to size it.
pub fn dropdown<'a, T>(
    p: Palette,
    options: &'a [T],
    selected: Option<&'a T>,
    placeholder: impl Into<String>,
    on_select: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
{
    let list = pick_list(options, selected, on_select)
        .placeholder(placeholder.into())
        .width(Length::Fill)
        .font(theme::REGULAR)
        .text_size(theme::BODY)
        .text_line_height(line(theme::LINE_BODY))
        .padding(Padding {
            top: (theme::CONTROL - theme::LINE_BODY) / 2.0,
            bottom: (theme::CONTROL - theme::LINE_BODY) / 2.0,
            left: theme::S3,
            right: theme::S10,
        })
        .handle(pick_list::Handle::None)
        .style(move |_, status| {
            let (bg, border_color, width) = match status {
                pick_list::Status::Active => (p.surface, p.border_strong, 1.0),
                pick_list::Status::Hovered => (p.hover, p.text_muted, 1.0),
                pick_list::Status::Opened { .. } => (p.surface, p.focus_ring, 2.0),
            };
            pick_list::Style {
                text_color: p.text,
                placeholder_color: p.text_muted,
                handle_color: p.text_muted,
                background: Background::Color(bg),
                border: Border {
                    radius: theme::R.into(),
                    width,
                    color: border_color,
                },
            }
        })
        .menu_style(move |_| iced::overlay::menu::Style {
            background: Background::Color(p.surface),
            border: Border {
                radius: theme::R.into(),
                width: 1.0,
                color: p.border_strong,
            },
            text_color: p.text,
            selected_text_color: p.text,
            selected_background: Background::Color(p.surface_alt),
            shadow: Shadow::default(),
        });
    let chevron = container(icon(Icon::ChevronDown, 16.0, p.text_muted))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::Center)
        .padding([0.0, theme::S3]);
    arrow(stack![list, chevron])
}

// --------------------------------------------------------------- segmented

/// Two-to-four way choice shown as one pill-shaped control (Light / Dark…).
pub fn segmented<'a, T>(
    p: Palette,
    options: &[(T, String)],
    selected: T,
    on_select: impl Fn(T) -> Message,
) -> Element<'a, Message>
where
    T: Copy + PartialEq + 'a,
{
    let mut r = row![].spacing(theme::S1);
    for (value, label) in options {
        let active = *value == selected;
        let content = container(
            text(label.clone())
                .size(theme::BODY)
                .line_height(line(theme::LINE_BODY))
                .font(if active {
                    theme::SEMIBOLD
                } else {
                    theme::MEDIUM
                })
                .wrapping(Wrapping::None),
        )
        .height(Length::Fill)
        .align_y(Alignment::Center);
        r = r.push(
            button(content)
                .height(theme::CONTROL_SMALL)
                .padding([0.0, theme::S4])
                .on_press(on_select(*value))
                .style(move |_, status| {
                    let hovered =
                        matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: if active {
                            Some(Background::Color(p.surface))
                        } else if hovered {
                            Some(Background::Color(p.hover_strong))
                        } else {
                            None
                        },
                        text_color: if active || hovered {
                            p.text
                        } else {
                            p.text_muted
                        },
                        border: Border {
                            radius: theme::R_SMALL.into(),
                            width: if active { 1.0 } else { 0.0 },
                            color: p.border,
                        },
                        shadow: Shadow::default(),
                        snap: true,
                    }
                }),
        );
    }
    arrow(
        container(r)
            .padding(theme::S1)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.surface_alt)),
                border: Border {
                    radius: theme::R.into(),
                    width: 1.0,
                    color: p.border,
                },
                ..container::Style::default()
            }),
    )
}

// ------------------------------------------------------------------ switch

const SWITCH_W: f32 = 40.0;
const SWITCH_H: f32 = 20.0;

#[derive(Default)]
struct SwitchState {
    /// 0.0 = off .. 1.0 = on, as currently drawn.
    progress: f32,
    from: f32,
    target: f32,
    start: Option<Instant>,
    hovered: bool,
    pressed: bool,
}

struct Switch<'a> {
    p: Palette,
    on: bool,
    on_toggle: Option<Box<dyn Fn(bool) -> Message + 'a>>,
}

impl Widget<Message, Theme, Renderer> for Switch<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SwitchState>()
    }
    fn state(&self) -> tree::State {
        let v = if self.on { 1.0 } else { 0.0 };
        tree::State::new(SwitchState {
            progress: v,
            from: v,
            target: v,
            ..SwitchState::default()
        })
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(SWITCH_W), Length::Fixed(SWITCH_H))
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, _: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(SWITCH_W, SWITCH_H))
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _: &Rectangle,
    ) {
        let st = tree.state.downcast_mut::<SwitchState>();
        let over = cursor.is_over(layout.bounds());
        let enabled = self.on_toggle.is_some();
        let target = if self.on { 1.0 } else { 0.0 };
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                if (st.target - target).abs() > f32::EPSILON {
                    st.from = st.progress;
                    st.target = target;
                    st.start = Some(*now);
                }
                if let Some(start) = st.start {
                    let t = now.saturating_duration_since(start).as_secs_f32()
                        / anim::FAST.as_secs_f32();
                    if t >= 1.0 || anim::reduced() {
                        st.start = None;
                        st.progress = st.target;
                    } else {
                        st.progress = st.from + (st.target - st.from) * anim::DECELERATE.at(t);
                        shell.request_redraw();
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                if st.hovered != over {
                    st.hovered = over;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over && enabled => {
                st.pressed = true;
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if st.pressed => {
                st.pressed = false;
                if over {
                    if let Some(f) = &self.on_toggle {
                        shell.publish(f(!self.on));
                    }
                }
                shell.capture_event();
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
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        let st = tree.state.downcast_ref::<SwitchState>();
        let p = self.p;
        let b = layout.bounds();
        let t = st.progress.clamp(0.0, 1.0);
        let enabled = self.on_toggle.is_some();
        let (off_border, on_fill, knob_off, knob_on) = if enabled {
            (p.text_muted, p.brand, p.text_muted, p.on_brand)
        } else {
            (p.disabled_fg, p.disabled_fg, p.disabled_fg, p.disabled_bg)
        };
        let hot = st.hovered && enabled;
        let off_fill = if hot { p.hover_strong } else { p.surface };
        let on_fill = if hot { p.brand_hover } else { on_fill };
        let fill = mix(off_fill, on_fill, t);
        let border = mix(off_border, on_fill, t);
        renderer.fill_quad(
            renderer::Quad {
                bounds: b,
                border: Border {
                    radius: (SWITCH_H / 2.0).into(),
                    width: 1.0,
                    color: border,
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(fill),
        );
        // Knob: 12 px, grows to 14 px while hovered / pressed (Windows 11).
        let d = if st.pressed {
            15.0
        } else if st.hovered && enabled {
            14.0
        } else {
            12.0
        };
        let x = b.x + 10.0 + (b.width - 20.0) * t - d / 2.0;
        let kb = Rectangle {
            x,
            y: b.y + (b.height - d) / 2.0,
            width: d,
            height: d,
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: kb,
                border: Border {
                    radius: (d / 2.0).into(),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(mix(knob_off, knob_on, t)),
        );
    }
}

/// Windows 11 style on/off switch (40x20, knob slides with a short decelerate
/// animation). `on_toggle: None` renders it disabled.
pub fn switch<'a>(
    p: Palette,
    on: bool,
    on_toggle: Option<impl Fn(bool) -> Message + 'a>,
) -> Element<'a, Message> {
    Element::new(Switch {
        p,
        on,
        on_toggle: on_toggle.map(|f| Box::new(f) as Box<dyn Fn(bool) -> Message + 'a>),
    })
}

// --------------------------------------------------------------- checkbox

/// Checkbox state; `Mixed` is for a group header whose children differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Off,
    On,
    Mixed,
}

impl From<bool> for CheckState {
    fn from(b: bool) -> Self {
        if b {
            CheckState::On
        } else {
            CheckState::Off
        }
    }
}

/// 18 px checkbox with an optional label; the whole line is the click target.
/// `on_press: None` renders it disabled.
pub fn checkbox<'a>(
    p: Palette,
    state: CheckState,
    label: Option<String>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let enabled = on_press.is_some();
    let checked = state != CheckState::Off;
    let mark: Element<'a, Message> = match state {
        CheckState::On => icon(
            Icon::Check,
            14.0,
            if enabled { p.on_brand } else { p.disabled_bg },
        ),
        CheckState::Mixed => container(iced::widget::space::horizontal())
            .width(8)
            .height(2)
            .style(move |_| container::Style {
                background: Some(Background::Color(if enabled {
                    p.on_brand
                } else {
                    p.disabled_bg
                })),
                border: Border {
                    radius: 1.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into(),
        CheckState::Off => iced::widget::space::horizontal().width(0).height(0).into(),
    };
    let boxed = container(mark)
        .center(theme::CHECK)
        .style(move |_| container::Style {
            background: Some(Background::Color(if checked {
                if enabled {
                    p.brand
                } else {
                    p.disabled_fg
                }
            } else if enabled {
                p.surface
            } else {
                p.disabled_bg
            })),
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: if checked {
                    if enabled {
                        p.brand
                    } else {
                        p.disabled_fg
                    }
                } else if enabled {
                    p.text_muted
                } else {
                    p.border_strong
                },
            },
            ..container::Style::default()
        });
    let mut content = row![boxed].spacing(theme::S3).align_y(Alignment::Center);
    if let Some(l) = label {
        content = content.push(
            text(l)
                .size(theme::BODY)
                .font(theme::REGULAR)
                .color(if enabled { p.text } else { p.disabled_fg }),
        );
    }
    arrow(
        button(content)
            .padding([theme::S1, theme::S1])
            .on_press_maybe(on_press)
            .style(move |_, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Background::Color(p.hover)),
                    button::Status::Pressed => Some(Background::Color(p.pressed)),
                    _ => None,
                },
                text_color: p.text,
                border: Border {
                    radius: theme::R_SMALL.into(),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: true,
            }),
    )
}

// ------------------------------------------------------------- text field

/// Single-line text field, 36 px: 1 px border, strong border on hover, focus ring.
pub fn text_field<'a>(
    p: Palette,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .size(theme::BODY)
        .font(theme::REGULAR)
        .line_height(line(theme::LINE_BODY))
        .padding([(theme::CONTROL - theme::LINE_BODY) / 2.0, theme::S3])
        .style(move |_, status| {
            let (border, width) = match status {
                text_input::Status::Active => (p.border_strong, 1.0),
                text_input::Status::Hovered => (p.text_muted, 1.0),
                text_input::Status::Focused { .. } => (p.focus_ring, 2.0),
                text_input::Status::Disabled => (p.border, 1.0),
            };
            let disabled = status == text_input::Status::Disabled;
            text_input::Style {
                background: Background::Color(if disabled { p.disabled_bg } else { p.surface }),
                border: Border {
                    radius: theme::R.into(),
                    width,
                    color: border,
                },
                icon: p.text_muted,
                placeholder: p.text_muted,
                value: if disabled { p.disabled_fg } else { p.text },
                selection: Color {
                    a: 0.25,
                    ..p.focus_ring
                },
            }
        })
}

// -------------------------------------------------------------- scrollbar

/// Slim, quiet scrollbar colours (no rail background, no autoscroll shadow).
pub fn scroll_style(
    p: Palette,
) -> impl Fn(&Theme, iced::widget::scrollable::Status) -> iced::widget::scrollable::Style {
    use iced::widget::scrollable::{AutoScroll, Rail, Scroller, Status, Style};
    move |_, status| {
        let rail = |c: Color| Rail {
            background: None,
            border: Border {
                radius: 3.0.into(),
                ..Border::default()
            },
            scroller: Scroller {
                background: Background::Color(c),
                border: Border {
                    radius: 3.0.into(),
                    ..Border::default()
                },
            },
        };
        let (v, h) = match status {
            Status::Active { .. } => (p.border_strong, p.border_strong),
            Status::Hovered {
                is_vertical_scrollbar_hovered,
                is_horizontal_scrollbar_hovered,
                ..
            } => (
                if is_vertical_scrollbar_hovered {
                    p.text_muted
                } else {
                    p.border_strong
                },
                if is_horizontal_scrollbar_hovered {
                    p.text_muted
                } else {
                    p.border_strong
                },
            ),
            Status::Dragged { .. } => (p.text_muted, p.text_muted),
        };
        Style {
            container: container::Style::default(),
            vertical_rail: rail(v),
            horizontal_rail: rail(h),
            gap: None,
            auto_scroll: AutoScroll {
                background: Background::Color(p.surface),
                border: Border {
                    radius: 999.0.into(),
                    width: 1.0,
                    color: p.border_strong,
                },
                shadow: Shadow::default(),
                icon: p.text_muted,
            },
        }
    }
}

/// Thin vertical scrollbar geometry to pair with `scroll_style`.
pub fn scrollbar() -> iced::widget::scrollable::Direction {
    iced::widget::scrollable::Direction::Vertical(
        iced::widget::scrollable::Scrollbar::new()
            .width(6)
            .scroller_width(6)
            .margin(2),
    )
}
