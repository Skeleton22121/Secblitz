//! Borderless layout primitives: region, group, row_item, collapsible.
//!
//! No 1 px outlines and no boxes around rows. Separation comes from tonal
//! steps (`bg` < `surface` < `surface_alt`, a few percent apart) and from
//! whitespace. See docs/DESIGN-SYSTEM.md, "Layout".
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
    mouse, window, Alignment, Background, Border, Color, Element, Event, Length, Pixels, Radians,
    Rectangle, Renderer, Shadow, Size, Theme, Vector,
};
use std::time::Instant;

// ---------------------------------------------------------------------------
// region
// ---------------------------------------------------------------------------

/// A borderless tonal block: `surface` tone, `R_LARGE` corners, `S6` padding.
/// Use for at most one or two hero / primary regions per page; everything
/// else is a [`group`] on the bare page background.
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

// ---------------------------------------------------------------------------
// hoverable: soft tonal hover for rows that are not buttons
// ---------------------------------------------------------------------------

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
        if let Event::Window(window::Event::RedrawRequested(_)) = event {
            // Settle after the hit-test below flagged a change.
        } else if state.hovered != over {
            state.hovered = over;
            shell.request_redraw();
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

/// Soft tonal hover behind `content` (no border, no shadow). For rows that
/// are not buttons but hold controls, e.g. a label with a switch.
pub fn hoverable<'a>(p: Palette, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Hoverable {
        content: content.into(),
        color: p.hover,
        radius: theme::R,
    })
}

// ---------------------------------------------------------------------------
// row_item
// ---------------------------------------------------------------------------

/// One line of a [`group`]: plain 20 px icon (no badge), title, optional
/// subtitle, a trailing slot and, with `on_press`, a whole-row click target.
/// At least `ROW_ITEM` (56 px) tall. `icon_tone` tints the glyph; `None`
/// uses the muted text tone.
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

/// [`row_item`] with a status tint on the icon.
pub fn row_item_tinted<'a>(
    p: Palette,
    glyph: Option<Icon>,
    tone: Option<Tone>,
    title: impl Into<String>,
    subtitle: Option<String>,
    trailing: impl Into<Element<'a, Message>>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
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
    let mut line = row![].spacing(theme::S4).align_y(Alignment::Center);
    if let Some(g) = glyph {
        let c = tone.map(|t| p.tone(t)).unwrap_or(p.text_muted);
        line = line.push(icon(g, theme::ICON_ROW, c));
    }
    // The invisible spacer gives the row its minimum height.
    line = line
        .push(iced::widget::space::vertical().height(theme::ROW_ITEM - theme::S2 * 2.0))
        .push(texts)
        .push(trailing.into());
    let inner = container(line)
        .padding([theme::S2, theme::S4])
        .width(Length::Fill)
        .center_y(Length::Shrink);
    match on_press {
        Some(m) => arrow(
            super::press::button(inner)
                .scale(false)
                .focus_color(p.focus_ring)
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

// ---------------------------------------------------------------------------
// group
// ---------------------------------------------------------------------------

/// A titled group of rows with NO box: a heading (title, optional muted
/// subtitle, optional trailing control such as an [`overflow_menu`]) over
/// rows separated by `S1` gaps. Sits directly on the page background or
/// inside a [`region`].
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

// ---------------------------------------------------------------------------
// chevron (rotating disclosure arrow)
// ---------------------------------------------------------------------------

struct ChevronState {
    shown_open: bool,
    from: f32,
    start: Option<Instant>,
}

/// Right chevron that eases 0 -> 90 degrees (and back) when `open` changes.
/// Asks for redraws only while turning.
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
        let a = chevron_angle(st, Instant::now());
        let bounds = layout.bounds();
        renderer.draw_svg(
            svg::Svg {
                handle: svg::Handle::from_memory(Icon::ChevronRight.svg()),
                color: Some(self.color),
                rotation: Radians(a * std::f32::consts::FRAC_PI_2),
                opacity: 1.0,
            },
            bounds,
            bounds,
        );
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

// ---------------------------------------------------------------------------
// collapsible
// ---------------------------------------------------------------------------

/// Header row (rotating chevron, title, muted `summary` such as "24 items")
/// that shows `body` when `open`. The page owns `open` and flips it on
/// `on_toggle`. Use for any list longer than about six rows: collapsed by
/// default, with the summary doing the talking.
pub fn collapsible<'a>(
    p: Palette,
    title: impl Into<String>,
    summary: Option<String>,
    open: bool,
    on_toggle: Message,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut head = row![
        iced::widget::space::vertical().height(theme::CONTROL + theme::S2 - theme::S2 * 2.0),
        chevron(16.0, p.text_muted, open),
        text(title.into())
            .size(theme::BODY)
            .line_height(LineHeight::Absolute(Pixels(theme::LINE_BODY)))
            .font(theme::SEMIBOLD)
            .color(p.text)
            .wrapping(Wrapping::None),
        iced::widget::space::horizontal(),
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center);
    if let Some(s) = summary {
        head = head.push(
            text(s)
                .size(theme::SMALL)
                .font(theme::REGULAR)
                .color(p.text_muted)
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
        .focus_color(p.focus_ring)
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
    let mut c = column![header].spacing(theme::S1).width(Length::Fill);
    if open {
        c = c.push(body.into());
    }
    c.into()
}

// ---------------------------------------------------------------------------
// show more
// ---------------------------------------------------------------------------

/// The slice of `items` to render: all of them when `expanded`, otherwise the
/// first `limit`. Pair with [`show_more_button`] when `items.len() > limit`.
pub fn limited<T>(items: &[T], limit: usize, expanded: bool) -> &[T] {
    if expanded || items.len() <= limit {
        items
    } else {
        &items[..limit]
    }
}

/// Quiet text button under a truncated list ("Show 12 more" / "Show less").
pub fn show_more_button<'a>(
    p: Palette,
    label: impl Into<String>,
    on_press: Message,
) -> Element<'a, Message> {
    super::action(p, ButtonKind::Ghost, label, None, Some(on_press))
}
