//! Borderless layout primitives: region, group, row_item, collapsible.
use super::anim;
use super::cursor::arrow;
use super::{icon, ButtonKind};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::svg::{self, Renderer as _};
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{overlay, Clipboard, Shell, Widget};
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, column, container, row, text};
use iced::{
    mouse, window, Alignment, Background, Border, Color, Element, Event, Length, Padding, Pixels,
    Radians, Rectangle, Renderer, Shadow, Size, Theme, Vector,
};
use std::time::Instant;

pub fn region<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
) -> container::Container<'a, Message> {
    container(content)
        .padding(theme::S6)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface)),
            border: Border {
                radius: theme::R_LARGE.into(),
                ..Border::default()
            },
            shadow: Shadow::default(), // never: tiny-skia draws shadows unclipped
            text_color: Some(p.text),
            snap: true,
        })
}

#[derive(Default)]
struct HoverState {
    hovered: bool,
}

struct Hoverable<'a> {
    content: Element<'a, Message>,
    color: Color,
    radius: f32,
}

impl Widget<Message, Theme, Renderer> for Hoverable<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<HoverState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(HoverState::default())
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
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
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
        use iced::advanced::Renderer as _;
        let state = tree.state.downcast_ref::<HoverState>();
        if state.hovered {
            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        radius: self.radius.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: true,
                },
                Background::Color(self.color),
            );
        }
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            child_layout,
            cursor,
            viewport,
        );
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(l) = layout.children().next() {
            self.content
                .as_widget_mut()
                .operate(&mut tree.children[0], l, renderer, operation);
        }
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
        let state = tree.state.downcast_mut::<HoverState>();
        let over = cursor.is_over(layout.bounds());
        if state.hovered != over {
            state.hovered = over;
            if !matches!(event, Event::Window(window::Event::RedrawRequested(_))) {
                shell.request_redraw();
            }
        }
        if let Some(l) = layout.children().next() {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                l,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
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
        layout
            .children()
            .next()
            .map(|l| {
                self.content.as_widget().mouse_interaction(
                    &tree.children[0],
                    l,
                    cursor,
                    viewport,
                    renderer,
                )
            })
            .unwrap_or_default()
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let l = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            l,
            renderer,
            viewport,
            translation,
        )
    }
}

pub fn hoverable<'a>(p: Palette, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Hoverable {
        content: content.into(),
        color: p.surface,
        radius: theme::R,
    })
}

pub fn row_item<'a>(
    p: Palette,
    glyph: Option<Icon>,
    title: impl Into<String>,
    subtitle: Option<String>,
    trailing: impl Into<Element<'a, Message>>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    row_item_tinted(p, glyph, None, title, subtitle, trailing, on_press)
}

pub fn row_item_tinted<'a>(
    p: Palette,
    glyph: Option<Icon>,
    tone: Option<Tone>,
    title: impl Into<String>,
    subtitle: Option<String>,
    trailing: impl Into<Element<'a, Message>>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let c = tone.map(|t| p.tone(t)).unwrap_or(p.text_muted);
    let lead = glyph.map(|g| icon(g, theme::ICON_ROW, c));
    row_item_lead(p, lead, title, subtitle, trailing, Vec::new(), on_press)
}

pub fn row_item_below<'a>(
    p: Palette,
    glyph: Option<Icon>,
    title: impl Into<String>,
    subtitle: Option<String>,
    trailing: impl Into<Element<'a, Message>>,
    below: Vec<Element<'a, Message>>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let lead = glyph.map(|g| icon(g, theme::ICON_ROW, p.text_muted));
    row_item_lead(p, lead, title, subtitle, trailing, below, on_press)
}

pub fn row_item_lead<'a>(
    p: Palette,
    lead_icon: Option<Element<'a, Message>>,
    title: impl Into<String>,
    subtitle: Option<String>,
    trailing: impl Into<Element<'a, Message>>,
    below: Vec<Element<'a, Message>>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let indent = if lead_icon.is_some() {
        theme::ICON_ROW
    } else {
        0.0
    } + theme::S4;
    let mut texts = column![text(title.into())
        .size(theme::BODY)
        .line_height(LineHeight::Absolute(Pixels(theme::LINE_BODY)))
        .font(theme::REGULAR)
        .color(p.text)]
    .spacing(2)
    .width(Length::Fill);
    if let Some(s) = subtitle {
        texts = texts.push(
            text(s)
                .size(theme::SMALL)
                .line_height(LineHeight::Absolute(Pixels(theme::LINE_SMALL)))
                .font(theme::REGULAR)
                .color(p.text_muted),
        );
    }
    let mut lead = row![].align_y(Alignment::Center);
    if let Some(g) = lead_icon {
        lead = lead.push(g);
    }
    lead = lead.push(iced::widget::space::vertical().height(theme::ROW_ITEM - theme::S2 * 2.0));
    let line = row![lead, texts, trailing.into()]
        .spacing(theme::S4)
        .align_y(Alignment::Center);
    let body: Element<'a, Message> = if below.is_empty() {
        line.into()
    } else {
        column![
            line,
            container(column(below).spacing(theme::S2).width(Length::Fill))
                .padding(Padding {
                    left: indent,
                    bottom: theme::S2,
                    ..Padding::ZERO
                })
                .width(Length::Fill)
        ]
        .spacing(theme::S1)
        .into()
    };
    let inner = container(body)
        .padding([theme::S2, theme::S4])
        .width(Length::Fill)
        .center_y(Length::Shrink);
    match on_press {
        Some(m) => arrow(
            super::press::button(inner)
                .scale(false)
                .padding(0)
                .width(Length::Fill)
                .on_press(m)
                .style(move |_, status| button::Style {
                    background: match status {
                        button::Status::Hovered => Some(Background::Color(p.hover)),
                        button::Status::Pressed => Some(Background::Color(p.pressed)),
                        _ => None,
                    },
                    text_color: p.text,
                    border: Border {
                        radius: theme::R.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: true,
                }),
        ),
        None => hoverable(p, inner),
    }
}

pub fn group<'a>(
    p: Palette,
    title: impl Into<String>,
    subtitle: Option<String>,
    trailing: Option<Element<'a, Message>>,
    rows: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut heading = column![text(title.into())
        .size(theme::BODY)
        .line_height(LineHeight::Absolute(Pixels(theme::LINE_BODY)))
        .font(theme::SEMIBOLD)
        .color(p.text)]
    .spacing(2)
    .width(Length::Fill);
    if let Some(s) = subtitle {
        heading = heading.push(
            text(s)
                .size(theme::SMALL)
                .font(theme::REGULAR)
                .color(p.text_muted),
        );
    }
    let mut head = row![heading].align_y(Alignment::Center).spacing(theme::S3);
    if let Some(t) = trailing {
        head = head.push(t);
    }
    column![
        container(head).padding([0.0, theme::S4]),
        column(rows).spacing(theme::S1)
    ]
    .spacing(theme::S3)
    .width(Length::Fill)
    .into()
}

struct ChevronState {
    shown_open: bool,
    from: f32,
    start: Option<Instant>,
}

struct Chevron {
    size: f32,
    color: Color,
    open: bool,
}

const TURN_MS: f32 = anim::FAST.as_millis() as f32;

fn chevron_angle(st: &ChevronState, now: Instant) -> f32 {
    let target = if st.shown_open { 1.0 } else { 0.0 };
    match st.start {
        Some(s) if !anim::reduced() => {
            let t = now.saturating_duration_since(s).as_secs_f32() * 1000.0 / TURN_MS;
            st.from + (target - st.from) * anim::STANDARD.at(t)
        }
        _ => target,
    }
}

impl Widget<Message, Theme, Renderer> for Chevron {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ChevronState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(ChevronState {
            shown_open: self.open,
            from: if self.open { 1.0 } else { 0.0 },
            start: None,
        })
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size), Length::Fixed(self.size))
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::atomic(limits, self.size, self.size)
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
        let st = tree.state.downcast_ref::<ChevronState>();
        let a = chevron_angle(st, Instant::now()).clamp(0.0, 1.0);
        let bounds = layout.bounds();
        for (glyph, opacity) in [(Icon::ChevronRight, 1.0 - a), (Icon::ChevronDown, a)] {
            if opacity > 0.0 {
                renderer.draw_svg(
                    svg::Svg {
                        handle: svg::Handle::from_memory(glyph.svg()),
                        color: Some(self.color),
                        rotation: Radians(0.0),
                        opacity,
                    },
                    bounds,
                    bounds,
                );
            }
        }
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _: Layout<'_>,
        _: mouse::Cursor,
        _: &Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let st = tree.state.downcast_mut::<ChevronState>();
            if st.shown_open != self.open {
                st.from = chevron_angle(st, *now);
                st.shown_open = self.open;
                st.start = Some(*now);
            }
            if let Some(s) = st.start {
                if now.saturating_duration_since(s).as_secs_f32() * 1000.0 >= TURN_MS
                    || anim::reduced()
                {
                    st.start = None;
                } else {
                    shell.request_redraw();
                }
            }
        }
    }
}

fn chevron<'a>(size: f32, color: Color, open: bool) -> Element<'a, Message> {
    Element::new(Chevron { size, color, open })
}

pub fn collapsible<'a>(
    p: Palette,
    title: impl Into<String>,
    summary: Option<String>,
    open: bool,
    on_toggle: Message,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    collapsible_toned(
        p,
        title,
        summary.map(|s| (s, p.text_muted)),
        open,
        on_toggle,
        body,
    )
}

/// Like `collapsible`, with the summary in a color of the caller's choosing so
/// a result that needs attention stands out on the closed header.
pub fn collapsible_toned<'a>(
    p: Palette,
    title: impl Into<String>,
    summary: Option<(String, Color)>,
    open: bool,
    on_toggle: Message,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    collapsible_group(p, title, summary, open, on_toggle, None, body)
}

/// A collapsible group with controls beside its heading that stay usable while it is closed.
pub fn collapsible_with<'a>(
    p: Palette,
    title: impl Into<String>,
    summary: Option<String>,
    open: bool,
    on_toggle: Message,
    trailing: Option<Element<'a, Message>>,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let summary = summary.map(|s| (s, p.text_muted));
    collapsible_group(p, title, summary, open, on_toggle, trailing, body)
}

fn collapsible_group<'a>(
    p: Palette,
    title: impl Into<String>,
    summary: Option<(String, Color)>,
    open: bool,
    on_toggle: Message,
    trailing: Option<Element<'a, Message>>,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut head = row![
        row![
            container(chevron(16.0, p.text_muted, open)).center_x(theme::ICON_ROW),
            iced::widget::space::vertical().height(theme::CONTROL + theme::S2 - theme::S2 * 2.0),
        ]
        .align_y(Alignment::Center),
        text(title.into())
            .size(theme::BODY)
            .line_height(LineHeight::Absolute(Pixels(theme::LINE_BODY)))
            .font(theme::SEMIBOLD)
            .color(p.text)
            .wrapping(Wrapping::None),
        iced::widget::space::horizontal(),
    ]
    .spacing(theme::S4)
    .align_y(Alignment::Center);
    if let Some((s, color)) = summary {
        head = head.push(
            text(s)
                .size(theme::SMALL)
                .font(theme::REGULAR)
                .color(color)
                .wrapping(Wrapping::None),
        );
    }
    let header = arrow(
        super::press::button(
            container(head)
                .padding([theme::S2, theme::S4])
                .center_y(Length::Shrink)
                .width(Length::Fill),
        )
        .scale(false)
        .padding(0)
        .width(Length::Fill)
        .on_press(on_toggle)
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered => Some(Background::Color(p.hover)),
                button::Status::Pressed => Some(Background::Color(p.pressed)),
                _ => None,
            },
            text_color: p.text,
            border: Border {
                radius: theme::R.into(),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }),
    );
    let header: Element<'a, Message> = match trailing {
        Some(t) => row![header, t]
            .spacing(theme::S1)
            .align_y(Alignment::Center)
            .into(),
        None => header,
    };
    let mut c = column![header].spacing(theme::S1).width(Length::Fill);
    if open {
        c = c.push(body.into());
    }
    c.into()
}

pub fn limited<T>(items: &[T], limit: usize, expanded: bool) -> &[T] {
    if expanded || items.len() <= limit {
        items
    } else {
        &items[..limit]
    }
}

pub fn show_more_button<'a>(
    p: Palette,
    label: impl Into<String>,
    on_press: Message,
) -> Element<'a, Message> {
    super::action(p, ButtonKind::Ghost, label, None, Some(on_press))
}
