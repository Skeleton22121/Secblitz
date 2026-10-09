//! Form controls: dropdown, segmented control, switch, checkbox, text field.
use super::anim;
use super::cursor::arrow;
use super::icon;
use super::press::{self, Track};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, mix, Palette};
use crate::gui::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{overlay, Clipboard, Shell, Widget};
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, container, row, stack, text};
use iced::{
    keyboard, mouse, touch, window, Alignment, Background, Border, Color, Element, Event, Length,
    Pixels, Point, Rectangle, Renderer, Shadow, Size, Theme, Vector,
};

fn line(px: f32) -> LineHeight {
    LineHeight::Absolute(Pixels(px))
}

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
    let list = Dropdown {
        p,
        options,
        selected,
        placeholder: placeholder.into(),
        on_select: Box::new(on_select),
    };
    let chevron = container(icon(Icon::ChevronDown, 16.0, p.text_muted))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::Center)
        .padding([0.0, theme::S3]);
    arrow(stack![Element::new(list), chevron])
}

struct Dropdown<'a, T> {
    p: Palette,
    options: &'a [T],
    selected: Option<&'a T>,
    placeholder: String,
    on_select: Box<dyn Fn(T) -> Message + 'a>,
}

#[derive(Default)]
struct DropState {
    open: bool,
    hover: Option<usize>,
    focused: bool,
    over: bool,
}

impl Focusable for DropState {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
        self.open = false;
    }
}

/// Where the highlight goes when an arrow key is pressed in an open list.
fn drop_step(hover: Option<usize>, count: usize, key: &keyboard::Key) -> Option<usize> {
    use keyboard::key::Named;
    if count == 0 {
        return None;
    }
    match (key, hover) {
        (keyboard::Key::Named(Named::ArrowDown), None) => Some(0),
        (keyboard::Key::Named(Named::ArrowUp), None) => Some(count - 1),
        (keyboard::Key::Named(Named::ArrowDown), Some(i)) => Some((i + 1).min(count - 1)),
        (keyboard::Key::Named(Named::ArrowUp), Some(i)) => Some(i.saturating_sub(1)),
        (keyboard::Key::Named(Named::Home), _) => Some(0),
        (keyboard::Key::Named(Named::End), _) => Some(count - 1),
        _ => hover,
    }
}

impl<T: ToString + PartialEq + Clone> Dropdown<'_, T> {
    fn selected_index(&self) -> Option<usize> {
        let chosen = self.selected?;
        self.options.iter().position(|o| o == chosen)
    }
}

impl<'a, T> Widget<Message, Theme, Renderer> for Dropdown<'a, T>
where
    T: ToString + PartialEq + Clone + 'a,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<DropState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(DropState::default())
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(theme::CONTROL))
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fill, Length::Fixed(theme::CONTROL), Size::ZERO))
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let st = tree.state.downcast_mut::<DropState>();
        operation.focusable(None, layout.bounds(), st);
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
        let st = tree.state.downcast_mut::<DropState>();
        let over = cursor.is_over(layout.bounds());
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                st.focused = false;
                if over {
                    st.open = true;
                    st.hover = self.selected_index();
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key:
                    keyboard::Key::Named(
                        keyboard::key::Named::Enter
                        | keyboard::key::Named::Space
                        | keyboard::key::Named::ArrowDown
                        | keyboard::key::Named::ArrowUp,
                    ),
                ..
            }) if st.focused && !st.open => {
                st.open = true;
                st.hover = self.selected_index().or(Some(0));
                shell.capture_event();
                shell.request_redraw();
            }
            _ => {}
        }
        if over != st.over {
            st.over = over;
            shell.request_redraw();
        }
    }
    fn mouse_interaction(
        &self,
        _: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Rectangle,
        _: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::Idle
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::text::{self as txt, Renderer as _};
        let st = tree.state.downcast_ref::<DropState>();
        let p = self.p;
        let b = layout.bounds();
        let bg = if st.open {
            p.pressed
        } else if st.over {
            p.hover_strong
        } else {
            p.surface_alt
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: b,
                border: Border {
                    radius: theme::R.into(),
                    ..Border::default()
                },
                ..renderer::Quad::default()
            },
            Background::Color(bg),
        );
        let label = self.selected.map(ToString::to_string);
        let shown = label.clone().unwrap_or_else(|| self.placeholder.clone());
        renderer.fill_text(
            txt::Text {
                content: shown,
                size: Pixels(theme::BODY),
                line_height: line(theme::LINE_BODY),
                font: theme::REGULAR,
                bounds: Size::new(b.width - theme::S3 - theme::S10, theme::LINE_BODY),
                align_x: txt::Alignment::Default,
                align_y: iced::alignment::Vertical::Center,
                shaping: txt::Shaping::default(),
                wrapping: Wrapping::default(),
            },
            Point::new(b.x + theme::S3, b.center_y()),
            if label.is_some() {
                p.text
            } else {
                p.text_muted
            },
            *viewport,
        );
        if st.focused {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: b,
                    border: Border {
                        radius: theme::R.into(),
                        ..Border::default()
                    },
                    ..renderer::Quad::default()
                },
                Background::Color(Color {
                    a: press::FOCUS_ALPHA,
                    ..p.text
                }),
            );
        }
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        _: &Renderer,
        _: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let st = tree.state.downcast_mut::<DropState>();
        if !st.open {
            return None;
        }
        Some(overlay::Element::new(Box::new(DropList {
            p: self.p,
            options: self.options,
            on_select: &*self.on_select,
            state: st,
            anchor: Rectangle::new(layout.position() + translation, layout.bounds().size()),
        })))
    }
}

struct DropList<'b, T> {
    p: Palette,
    options: &'b [T],
    on_select: &'b dyn Fn(T) -> Message,
    state: &'b mut DropState,
    anchor: Rectangle,
}

impl<T: ToString + Clone> DropList<'_, T> {
    fn row_at(&self, menu: Rectangle, cursor: mouse::Cursor) -> Option<usize> {
        let at = cursor.position_in(menu)?;
        let i = (at.y / theme::CONTROL) as usize;
        (i < self.options.len()).then_some(i)
    }
    fn choose(&mut self, i: usize, shell: &mut Shell<'_, Message>) {
        if let Some(option) = self.options.get(i) {
            shell.publish((self.on_select)(option.clone()));
        }
        self.state.open = false;
    }
}

impl<T: ToString + Clone> overlay::Overlay<Message, Theme, Renderer> for DropList<'_, T> {
    fn layout(&mut self, _: &Renderer, bounds: Size) -> layout::Node {
        let height = self.options.len() as f32 * theme::CONTROL;
        let below = bounds.height - (self.anchor.y + self.anchor.height);
        let above = self.anchor.y;
        let at = if below > above {
            Point::new(self.anchor.x, self.anchor.y + self.anchor.height)
        } else {
            Point::new(self.anchor.x, self.anchor.y - height.min(above))
        };
        let room = below.max(above);
        layout::Node::new(Size::new(self.anchor.width, height.min(room))).move_to(at)
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
    ) {
        use iced::advanced::text::{self as txt, Renderer as _};
        let p = self.p;
        let menu = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds: menu,
                border: Border {
                    radius: theme::R.into(),
                    width: 1.0,
                    color: p.border,
                },
                ..renderer::Quad::default()
            },
            Background::Color(p.surface),
        );
        for (i, option) in self.options.iter().enumerate() {
            let row = Rectangle {
                x: menu.x,
                y: menu.y + i as f32 * theme::CONTROL,
                width: menu.width,
                height: theme::CONTROL,
            };
            if row.y >= menu.y + menu.height {
                break;
            }
            if self.state.hover == Some(i) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: row.x + 1.0,
                            width: row.width - 2.0,
                            ..row
                        },
                        border: Border {
                            radius: theme::R.into(),
                            ..Border::default()
                        },
                        ..renderer::Quad::default()
                    },
                    Background::Color(p.surface_alt),
                );
            }
            renderer.fill_text(
                txt::Text {
                    content: option.to_string(),
                    size: Pixels(theme::BODY),
                    line_height: line(theme::LINE_BODY),
                    font: theme::REGULAR,
                    bounds: Size::new(f32::INFINITY, row.height),
                    align_x: txt::Alignment::Default,
                    align_y: iced::alignment::Vertical::Center,
                    shaping: txt::Shaping::default(),
                    wrapping: Wrapping::default(),
                },
                Point::new(row.x + theme::S3, row.center_y()),
                p.text,
                menu,
            );
        }
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
        use keyboard::key::Named;
        let menu = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(i) = self.row_at(menu, cursor) {
                    if self.state.hover != Some(i) {
                        self.state.hover = Some(i);
                        shell.request_redraw();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if cursor.is_over(menu) {
                    if let Some(i) = self.row_at(menu, cursor) {
                        self.choose(i, shell);
                    }
                    shell.capture_event();
                } else {
                    self.state.open = false;
                    // A press on the field itself only closes the list.
                    if cursor.is_over(self.anchor) {
                        shell.capture_event();
                    }
                }
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key {
                keyboard::Key::Named(Named::Escape) => {
                    self.state.open = false;
                    shell.capture_event();
                    shell.request_redraw();
                }
                keyboard::Key::Named(Named::Enter | Named::Space) => {
                    if let Some(i) = self.state.hover {
                        self.choose(i, shell);
                    } else {
                        self.state.open = false;
                    }
                    shell.capture_event();
                    shell.request_redraw();
                }
                keyboard::Key::Named(Named::Tab) => {
                    self.state.open = false;
                    shell.request_redraw();
                }
                keyboard::Key::Named(
                    Named::ArrowDown | Named::ArrowUp | Named::Home | Named::End,
                ) => {
                    self.state.hover = drop_step(self.state.hover, self.options.len(), key);
                    shell.capture_event();
                    shell.request_redraw();
                }
                _ => {}
            },
            _ => {}
        }
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Renderer,
    ) -> mouse::Interaction {
        if self.row_at(layout.bounds(), cursor).is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

const SLIDE: std::time::Duration = std::time::Duration::from_millis(200);
const MAX_SEGMENTS: usize = 4;

struct SegState {
    sel: usize,
    from: (f32, f32),
    slide: Track,
    hover: [Track; MAX_SEGMENTS],
    pressed: Option<usize>,
    focused: bool,
    cursor: Option<usize>,
}

impl Focusable for SegState {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
        self.cursor = None;
    }
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SegKey {
    Move(usize),
    Choose(usize),
    Ignore,
}

fn seg_key(key: &keyboard::Key, cursor: usize, count: usize) -> SegKey {
    use keyboard::key::Named;
    match key {
        keyboard::Key::Named(Named::ArrowLeft) => SegKey::Move(cursor.saturating_sub(1)),
        keyboard::Key::Named(Named::ArrowRight) => {
            SegKey::Move((cursor + 1).min(count.saturating_sub(1)))
        }
        keyboard::Key::Named(Named::Enter | Named::Space) => SegKey::Choose(cursor),
        _ => SegKey::Ignore,
    }
}

struct Segmented<'a> {
    p: Palette,
    labels: Vec<Element<'a, Message>>,
    on_select: Vec<Message>,
    selected: usize,
}

impl Segmented<'_> {
    fn seg(&self, layout: Layout<'_>, i: usize) -> (f32, f32) {
        let left = layout.bounds().x;
        match layout.children().nth(i) {
            Some(c) => {
                let b = c.bounds();
                (b.x - theme::S4 - left, b.width + 2.0 * theme::S4)
            }
            None => (theme::S1, 0.0),
        }
    }
    fn pill(&self, st: &SegState, layout: Layout<'_>) -> (f32, f32) {
        let (tx, tw) = self.seg(layout, st.sel);
        let v = st.slide.value;
        (
            st.from.0 + (tx - st.from.0) * v,
            st.from.1 + (tw - st.from.1) * v,
        )
    }
    fn hit(&self, layout: Layout<'_>, cursor: mouse::Cursor) -> Option<usize> {
        let pos = cursor.position_over(layout.bounds())?;
        (0..self.labels.len()).find(|i| {
            let (x, w) = self.seg(layout, *i);
            let left = layout.bounds().x;
            pos.x >= left + x && pos.x <= left + x + w + theme::S1
        })
    }
}

impl Widget<Message, Theme, Renderer> for Segmented<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SegState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(SegState {
            sel: self.selected,
            from: (0.0, 0.0),
            slide: Track::at(1.0),
            hover: [Track::at(0.0); MAX_SEGMENTS],
            pressed: None,
            focused: false,
            cursor: None,
        })
    }
    fn children(&self) -> Vec<Tree> {
        self.labels.iter().map(Tree::new).collect()
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.labels);
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Fixed(theme::CONTROL))
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, _: &layout::Limits) -> layout::Node {
        let mut x = theme::S1;
        let mut kids = Vec::with_capacity(self.labels.len());
        for (i, label) in self.labels.iter_mut().enumerate() {
            let node = label.as_widget_mut().layout(
                &mut tree.children[i],
                renderer,
                &layout::Limits::new(Size::ZERO, Size::INFINITE),
            );
            let size = node.size();
            kids.push(node.move_to(Point::new(
                x + theme::S4,
                theme::S1 + (theme::CONTROL_SMALL - size.height) / 2.0,
            )));
            x += size.width + 2.0 * theme::S4 + theme::S1;
        }
        layout::Node::with_children(Size::new(x, theme::CONTROL), kids)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let st = tree.state.downcast_mut::<SegState>();
        operation.focusable(None, layout.bounds(), st);
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
        let hit = self.hit(layout, cursor);
        let st = tree.state.downcast_mut::<SegState>();
        if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_))) {
            st.focused = false;
        }
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event {
            if st.focused {
                let at = st.cursor.unwrap_or(self.selected);
                match seg_key(key, at, self.labels.len()) {
                    SegKey::Move(to) => {
                        st.cursor = Some(to);
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    SegKey::Choose(i) => {
                        if i != self.selected {
                            if let Some(m) = self.on_select.get(i) {
                                shell.publish(m.clone());
                            }
                        }
                        shell.capture_event();
                    }
                    SegKey::Ignore => {}
                }
            }
        }
        let before: Vec<f32> = st.hover.iter().map(Track::goal).collect();
        for (i, h) in st.hover.iter_mut().enumerate() {
            h.target(if hit == Some(i) { 1.0 } else { 0.0 });
        }
        if st.hover.iter().map(Track::goal).ne(before) {
            shell.request_redraw();
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if hit.is_some() => {
                st.pressed = hit;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if st.pressed.is_some() =>
            {
                if let (Some(i), Some(h)) = (st.pressed.take(), hit) {
                    if i == h && h != self.selected {
                        if let Some(m) = self.on_select.get(h) {
                            shell.publish(m.clone());
                        }
                    }
                }
                shell.capture_event();
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if st.sel != self.selected {
                    let (x, w) = self.pill(st, layout);
                    st.from = (x, w);
                    st.sel = self.selected;
                    st.slide = Track::at(0.0);
                    st.slide.target(1.0);
                }
                let mut busy = st.slide.step(*now, SLIDE, |t| anim::STANDARD.at(t));
                for h in st.hover.iter_mut() {
                    busy |= h.step(*now, anim::FAST, |t| anim::STANDARD.at(t));
                }
                if busy || st.slide.running() {
                    shell.request_redraw();
                }
            }
            _ => {}
        }
    }
    fn mouse_interaction(
        &self,
        _: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Rectangle,
        _: &Renderer,
    ) -> mouse::Interaction {
        match self.hit(layout, cursor) {
            Some(i) if i != self.selected => mouse::Interaction::Pointer,
            _ => mouse::Interaction::Idle,
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let st = tree.state.downcast_ref::<SegState>();
        let p = self.p;
        let b = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds: b,
                border: Border {
                    radius: theme::R.into(),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(p.surface_alt),
        );
        for i in (0..self.labels.len()).filter(|i| *i != st.sel) {
            let h = st.hover[i.min(MAX_SEGMENTS - 1)].value.clamp(0.0, 1.0);
            if h > 0.0 {
                let (x, w) = self.seg(layout, i);
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: b.x + x,
                            y: b.y + theme::S1,
                            width: w,
                            height: theme::CONTROL_SMALL,
                        },
                        border: Border {
                            radius: theme::R_SMALL.into(),
                            ..Border::default()
                        },
                        shadow: Shadow::default(),
                        snap: false,
                    },
                    Background::Color(Color {
                        a: h,
                        ..p.hover_strong
                    }),
                );
            }
        }
        let (px, pw) = self.pill(st, layout);
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: b.x + px,
                    y: b.y + theme::S1,
                    width: pw,
                    height: theme::CONTROL_SMALL,
                },
                border: Border {
                    radius: theme::R_SMALL.into(),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(p.surface),
        );
        if st.focused {
            let (x, w) = self.seg(layout, st.cursor.unwrap_or(self.selected));
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: b.x + x,
                        y: b.y + theme::S1,
                        width: w,
                        height: theme::CONTROL_SMALL,
                    },
                    border: Border {
                        radius: theme::R_SMALL.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                Background::Color(Color {
                    a: press::FOCUS_ALPHA,
                    ..p.text
                }),
            );
        }
        for (i, (label, child)) in self.labels.iter().zip(layout.children()).enumerate() {
            let h = st.hover[i.min(MAX_SEGMENTS - 1)].value.clamp(0.0, 1.0);
            let (x, w) = self.seg(layout, i);
            let under = ((px + pw).min(x + w) - px.max(x)).max(0.0) / w.max(1.0);
            let on = under.clamp(0.0, 1.0);
            let color = mix(mix(p.text_muted, p.text, h), p.text, on);
            label.as_widget().draw(
                &tree.children[i],
                renderer,
                theme,
                &renderer::Style { text_color: color },
                child,
                cursor,
                viewport,
            );
        }
    }
}

pub fn segmented<'a, T>(
    p: Palette,
    options: &[(T, String)],
    selected: T,
    on_select: impl Fn(T) -> Message,
) -> Element<'a, Message>
where
    T: Copy + PartialEq + 'a,
{
    let options = &options[..options.len().min(MAX_SEGMENTS)];
    let labels = options
        .iter()
        .map(|(_, label)| {
            Element::from(
                text(label.clone())
                    .size(theme::BODY)
                    .line_height(line(theme::LINE_BODY))
                    .font(theme::MEDIUM)
                    .wrapping(Wrapping::None),
            )
        })
        .collect();
    Element::new(Segmented {
        p,
        labels,
        on_select: options.iter().map(|(v, _)| on_select(*v)).collect(),
        selected: options
            .iter()
            .position(|(v, _)| *v == selected)
            .unwrap_or(0),
    })
}

const SWITCH_W: f32 = 40.0;
const SWITCH_H: f32 = 20.0;

struct SwitchState {
    progress: Track,
    grow: Track,
    hover: Track,
    pressed: bool,
    focused: bool,
}

impl Focusable for SwitchState {
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
        tree::State::new(SwitchState {
            progress: Track::at(if self.on { 1.0 } else { 0.0 }),
            grow: Track::at(0.0),
            hover: Track::at(0.0),
            pressed: false,
            focused: false,
        })
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(SWITCH_W), Length::Fixed(SWITCH_H))
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, _: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(SWITCH_W, SWITCH_H))
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let st = tree.state.downcast_mut::<SwitchState>();
        // A switch that is busy saving keeps its place, so Tab goes on from it.
        if self.on_toggle.is_some() || st.focused {
            operation.focusable(None, layout.bounds(), st);
        }
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
        let enabled = self.on_toggle.is_some();
        let over = enabled && cursor.is_over(layout.bounds());
        if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_))) {
            st.focused = false;
        }
        match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Enter | keyboard::key::Named::Space),
                ..
            }) if st.focused && enabled => {
                if let Some(f) = &self.on_toggle {
                    shell.publish(f(!self.on));
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                st.pressed = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if st.pressed => {
                st.pressed = false;
                if over {
                    if let Some(f) = &self.on_toggle {
                        shell.publish(f(!self.on));
                    }
                }
                shell.capture_event();
            }
            _ => {}
        }
        let before = (st.hover.goal(), st.grow.goal(), st.progress.goal());
        st.hover.target(if over { 1.0 } else { 0.0 });
        st.grow.target(if st.pressed {
            1.0
        } else if over {
            0.67
        } else {
            0.0
        });
        st.progress.target(if self.on { 1.0 } else { 0.0 });
        if before != (st.hover.goal(), st.grow.goal(), st.progress.goal()) {
            shell.request_redraw();
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let a = st
                .progress
                .step(*now, anim::FAST, |t| anim::DECELERATE.at(t));
            let b = st.grow.step(*now, anim::FASTER, |t| anim::DECELERATE.at(t));
            let c = st.hover.step(*now, anim::FAST, |t| anim::STANDARD.at(t));
            if a || b || c || st.progress.running() || st.grow.running() || st.hover.running() {
                shell.request_redraw();
            }
        }
    }
    fn mouse_interaction(
        &self,
        _: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Rectangle,
        _: &Renderer,
    ) -> mouse::Interaction {
        if self.on_toggle.is_some() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::Idle
        }
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
        let t = st.progress.value.clamp(0.0, 1.0);
        let h = st.hover.value.clamp(0.0, 1.0);
        let enabled = self.on_toggle.is_some();
        let (off_fill, on_fill, off_ring, knob_off, knob_on) = if enabled {
            (
                mix(p.surface_alt, p.hover_strong, h),
                mix(p.brand, p.brand_hover, h),
                p.text_muted,
                p.text_muted,
                p.on_brand,
            )
        } else {
            (
                p.disabled_bg,
                p.disabled_fg,
                p.disabled_fg,
                p.disabled_fg,
                p.disabled_bg,
            )
        };
        let fill = mix(off_fill, on_fill, t);
        renderer.fill_quad(
            renderer::Quad {
                bounds: b,
                border: Border {
                    radius: (SWITCH_H / 2.0).into(),
                    width: 1.0,
                    color: mix(off_ring, on_fill, t),
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(fill),
        );
        if st.focused && enabled {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: b,
                    border: Border {
                        radius: (SWITCH_H / 2.0).into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                Background::Color(Color {
                    a: press::FOCUS_ALPHA,
                    ..p.text
                }),
            );
        }
        let d = 12.0 + 3.0 * st.grow.value.clamp(0.0, 1.0);
        let x = b.x + 10.0 + (b.width - 20.0) * t - d / 2.0;
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x,
                    y: b.y + (b.height - d) / 2.0,
                    width: d,
                    height: d,
                },
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

/// Windows 11 style on/off switch (40x20). The knob slides on a decelerate
/// curve, grows on hover and press, and the track colour tweens.
/// `on_toggle: None` renders it disabled.
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

const TICK: [(f32, f32); 3] = [(4.8, 9.4), (7.9, 12.5), (13.4, 5.9)];

fn tick_len() -> (f32, f32) {
    let seg = |a: (f32, f32), b: (f32, f32)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    (seg(TICK[0], TICK[1]), seg(TICK[1], TICK[2]))
}

struct CheckGlyphState {
    v: Track,
    state: CheckState,
}

struct CheckGlyph {
    p: Palette,
    state: CheckState,
    enabled: bool,
}

impl Widget<Message, Theme, Renderer> for CheckGlyph {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<CheckGlyphState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(CheckGlyphState {
            v: Track::at(if self.state == CheckState::Off {
                0.0
            } else {
                1.0
            }),
            state: self.state,
        })
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(theme::CHECK), Length::Fixed(theme::CHECK))
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, _: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(theme::CHECK, theme::CHECK))
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
        let st = tree.state.downcast_mut::<CheckGlyphState>();
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            if st.state != self.state {
                st.state = self.state;
                st.v.target(if self.state == CheckState::Off {
                    0.0
                } else {
                    1.0
                });
            }
            let on = st.v.goal() > 0.5;
            let busy = if on {
                st.v.step(*now, anim::NORMAL, |t| anim::DECELERATE.at(t))
            } else {
                st.v.step(*now, anim::FAST, |t| anim::ACCELERATE.at(t))
            };
            if busy || st.v.running() {
                shell.request_redraw();
            }
        } else if st.state != self.state {
            shell.request_redraw();
        }
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
        use iced::advanced::graphics::geometry::Renderer as _;
        use iced::widget::canvas::{Frame, LineCap, LineJoin, Path, Stroke};
        let st = tree.state.downcast_ref::<CheckGlyphState>();
        let p = self.p;
        let b = layout.bounds();
        let v = st.v.value.clamp(0.0, 1.0);
        let fill_t = (v / 0.4).clamp(0.0, 1.0);
        let tick_t = ((v - 0.25) / 0.75).clamp(0.0, 1.0);
        let (on_bg, on_fg, off_bg, ring) = if self.enabled {
            (p.brand, p.on_brand, p.surface_alt, p.text_muted)
        } else {
            (p.disabled_fg, p.disabled_bg, p.disabled_bg, p.border_strong)
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: b,
                border: Border {
                    radius: 4.0.into(),
                    width: 1.5,
                    color: mix(ring, on_bg, fill_t),
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(mix(off_bg, on_bg, fill_t)),
        );
        if v <= 0.0 {
            return;
        }
        if self.state == CheckState::Mixed {
            let w = 8.0 * fill_t;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: b.x + (b.width - w) / 2.0,
                        y: b.y + b.height / 2.0 - 1.0,
                        width: w,
                        height: 2.0,
                    },
                    border: Border {
                        radius: 1.0.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                Background::Color(on_fg),
            );
            return;
        }
        if tick_t <= 0.0 {
            return;
        }
        let (l1, l2) = tick_len();
        let mut reach = tick_t * (l1 + l2);
        let lerp = |a: (f32, f32), b: (f32, f32), t: f32| {
            Point::new(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        };
        let path = Path::new(|pb| {
            pb.move_to(Point::new(TICK[0].0, TICK[0].1));
            let first = reach.min(l1);
            pb.line_to(lerp(TICK[0], TICK[1], first / l1));
            reach -= first;
            if reach > 0.0 {
                pb.line_to(lerp(TICK[1], TICK[2], reach / l2));
            }
        });
        let mut frame = Frame::new(renderer, b.size());
        frame.stroke(
            &path,
            Stroke::default()
                .with_width(1.8)
                .with_color(on_fg)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
        );
        renderer.with_translation(Vector::new(b.x, b.y), |r| {
            r.draw_geometry(frame.into_geometry())
        });
    }
}

pub fn checkbox<'a>(
    p: Palette,
    state: CheckState,
    label: Option<String>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let enabled = on_press.is_some();
    let mut content = row![Element::new(CheckGlyph { p, state, enabled })]
        .spacing(theme::S3)
        .align_y(Alignment::Center);
    if let Some(l) = label {
        content = content.push(
            text(l)
                .size(theme::BODY)
                .font(theme::REGULAR)
                .color(if enabled { p.text } else { p.disabled_fg }),
        );
    }
    arrow(
        press::button(content)
            .scale(false)
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

struct Marker {
    p: Palette,
    pitch: f32,
    item: f32,
    index: usize,
    count: usize,
}

struct MarkerState {
    from: f32,
    index: usize,
    slide: Track,
}

impl Marker {
    fn y(&self, i: f32) -> f32 {
        i * self.pitch
    }
    fn total(&self) -> f32 {
        self.y(self.count.saturating_sub(1) as f32) + self.item
    }
}

impl Widget<Message, Theme, Renderer> for Marker {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<MarkerState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(MarkerState {
            from: self.index as f32,
            index: self.index,
            slide: Track::at(1.0),
        })
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.total()))
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        let w = limits.max().width;
        layout::Node::new(Size::new(if w.is_finite() { w } else { 0.0 }, self.total()))
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
        let st = tree.state.downcast_mut::<MarkerState>();
        if st.index != self.index {
            st.from += (st.index as f32 - st.from) * st.slide.value;
            st.index = self.index;
            st.slide = Track::at(0.0);
            st.slide.target(1.0);
            shell.request_redraw();
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let busy = st
                .slide
                .step(*now, std::time::Duration::from_millis(220), |t| {
                    anim::POINT_TO_POINT.at(t)
                });
            if busy || st.slide.running() {
                shell.request_redraw();
            }
        }
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
        let st = tree.state.downcast_ref::<MarkerState>();
        let b = layout.bounds();
        let at = st.from + (st.index as f32 - st.from) * st.slide.value;
        let y = b.y + self.y(at);
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: b.x,
                    y,
                    width: b.width,
                    height: self.item,
                },
                border: Border {
                    radius: theme::R.into(),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(self.p.selected),
        );
        let bar_h = 16.0;
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: b.x + 1.0,
                    y: y + (self.item - bar_h) / 2.0,
                    width: 3.0,
                    height: bar_h,
                },
                border: Border {
                    radius: 1.5.into(),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(self.p.brand),
        );
    }
}

pub fn slide_marker<'a>(
    p: Palette,
    index: usize,
    count: usize,
    item: f32,
    gap: f32,
) -> Element<'a, Message> {
    Element::new(Marker {
        p,
        pitch: item + gap,
        item,
        index,
        count,
    })
}

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

/// Whether a scroll list has more below what is shown.
pub fn more_below(view: Option<&iced::widget::scrollable::Viewport>) -> bool {
    view.is_some_and(|v| {
        let hidden = v.content_bounds().height - v.bounds().height;
        hidden > 0.5 && v.absolute_offset().y < hidden - 0.5
    })
}

/// Softens the bottom edge of a scroll list into the sheet while more is below,
/// so a row cut by the edge reads as "more to scroll", not as broken.
pub fn fade_below<'a, M: 'a>(
    list: impl Into<Element<'a, M>>,
    surface: Color,
    more: bool,
) -> Element<'a, M> {
    // The list stays in the same place in the tree either way: swapping it in
    // and out of the stack would reset its scroll position.
    let fade = container(iced::widget::space::vertical())
        .width(Length::Fill)
        .height(FADE)
        .style(move |_| container::Style {
            background: more.then(|| {
                Background::Gradient(iced::Gradient::Linear(
                    iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                        .add_stop(0.0, Color { a: 0.0, ..surface })
                        .add_stop(1.0, surface),
                ))
            }),
            ..container::Style::default()
        });
    let list: Element<'a, M> = list.into();
    stack![
        list,
        container(fade)
            .height(Length::Fill)
            .align_bottom(Length::Fill)
    ]
    .into()
}

const FADE: f32 = 32.0;

pub fn scrollbar() -> iced::widget::scrollable::Direction {
    iced::widget::scrollable::Direction::Vertical(
        iced::widget::scrollable::Scrollbar::new()
            .width(6)
            .scroller_width(6)
            .margin(2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyboard::key::Named;

    fn named(n: Named) -> keyboard::Key {
        keyboard::Key::Named(n)
    }

    #[test]
    fn segmented_arrows_move_and_stop_at_the_ends() {
        assert_eq!(seg_key(&named(Named::ArrowRight), 0, 3), SegKey::Move(1));
        assert_eq!(seg_key(&named(Named::ArrowRight), 2, 3), SegKey::Move(2));
        assert_eq!(seg_key(&named(Named::ArrowLeft), 1, 3), SegKey::Move(0));
        assert_eq!(seg_key(&named(Named::ArrowLeft), 0, 3), SegKey::Move(0));
    }

    #[test]
    fn segmented_enter_and_space_choose_the_highlighted_option() {
        assert_eq!(seg_key(&named(Named::Enter), 2, 3), SegKey::Choose(2));
        assert_eq!(seg_key(&named(Named::Space), 1, 3), SegKey::Choose(1));
        assert_eq!(seg_key(&named(Named::ArrowDown), 1, 3), SegKey::Ignore);
        assert_eq!(seg_key(&named(Named::Tab), 1, 3), SegKey::Ignore);
    }

    #[test]
    fn dropdown_arrows_move_through_the_list() {
        assert_eq!(drop_step(None, 6, &named(Named::ArrowDown)), Some(0));
        assert_eq!(drop_step(None, 6, &named(Named::ArrowUp)), Some(5));
        assert_eq!(drop_step(Some(5), 6, &named(Named::ArrowDown)), Some(5));
        assert_eq!(drop_step(Some(0), 6, &named(Named::ArrowUp)), Some(0));
        assert_eq!(drop_step(Some(2), 6, &named(Named::ArrowDown)), Some(3));
        assert_eq!(drop_step(Some(2), 6, &named(Named::Home)), Some(0));
        assert_eq!(drop_step(Some(2), 6, &named(Named::End)), Some(5));
        assert_eq!(drop_step(Some(2), 0, &named(Named::ArrowDown)), None);
    }

    struct Stops {
        focus: bool,
        seen: Vec<bool>,
    }

    impl Operation for Stops {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn focusable(
            &mut self,
            _: Option<&iced::advanced::widget::Id>,
            _: Rectangle,
            state: &mut dyn Focusable,
        ) {
            if self.focus {
                state.focus();
            }
            self.seen.push(state.is_focused());
        }
    }

    fn stops(element: &mut Element<'_, Message>, tree: &mut Tree, focus: bool) -> Vec<bool> {
        use iced::advanced::renderer::Headless;
        let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            theme::REGULAR,
            14.0.into(),
            Some("tiny-skia"),
        ))
        .expect("tiny-skia renderer");
        tree.diff(&*element);
        let node = element.as_widget_mut().layout(
            tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(200.0, 50.0)),
        );
        let mut op = Stops {
            focus,
            seen: Vec::new(),
        };
        element
            .as_widget_mut()
            .operate(tree, Layout::new(&node), &renderer, &mut op);
        op.seen
    }

    #[test]
    fn a_switch_busy_saving_keeps_focus_and_its_tab_stop() {
        let p = theme::LIGHT;
        let mut on = switch(p, false, Some(|_| Message::Noop));
        let mut tree = Tree::new(&on);
        assert_eq!(stops(&mut on, &mut tree, true), vec![true]);
        let mut busy = switch(p, true, None::<fn(bool) -> Message>);
        assert_eq!(stops(&mut busy, &mut tree, false), vec![true]);
        let mut fresh = switch(p, true, None::<fn(bool) -> Message>);
        let mut fresh_tree = Tree::new(&fresh);
        assert!(stops(&mut fresh, &mut fresh_tree, false).is_empty());
    }
}
