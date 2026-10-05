//! One bounded, redrawable interactive surface. CLI writers never enter it.
//! Input has per-key RAII: console on Unix and a complete Win32 event reader on
//! Windows. Screen/cursor/modes and the resize renderer restore on unwind too.
use crate::i18n::Lang;
use anyhow::{ensure, Result};
use console::{measure_text_width, truncate_str, Key, Term};
use std::{
    cell::RefCell,
    io::{self, Write},
    marker::PhantomData,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub const SELECT_HINT: &str = "↑/↓ · Enter · Esc";
pub const MULTI_HINT: &str = "↑/↓ · Space · Enter · Esc";
pub const PAGE_HINT: &str = "PgUp/PgDn: read details · Esc: back";
const ENTER_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[2J\x1b[H";
const LEAVE_SCREEN: &[u8] = b"\x1b[?25h\x1b[?1049l";
const BODY_LIMIT: usize = 8 * 1024 * 1024;

/// Presentation roles are supplied by typed report/navigation adapters. Labels
/// and translated strings are never inspected to infer health or severity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Text,
    Muted,
    Brand,
    Title,
    Accent,
    Focus,
    FocusAccent,
    Healthy,
    Review,
    Unknown,
    Failure,
    Working,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub text: String,
    pub role: Role,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub subtitle: String,
    pub badges: Vec<Badge>,
    /// (protected_count, total_count) for the progress bar. None = no bar.
    pub tally: Option<(usize, usize)>,
}
impl Header {
    pub fn message(text: String, role: Role) -> Self {
        Self {
            subtitle: String::new(),
            badges: vec![Badge { text, role }],
            tally: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Overview,
    Advanced,
    Review,
    Maintenance,
    Diagnostics,
    QualityUpdates,
    Tools,
    Details,
    Desktop,
    Settings,
}
impl Section {
    fn key(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Advanced => "Advanced",
            Self::Review => "Review",
            Self::Maintenance => "Maintenance",
            Self::Diagnostics => "Diagnostics",
            Self::QualityUpdates => "Quality updates",
            Self::Tools => "Extra tools",
            Self::Details => "Details",
            Self::Desktop => "Desktop",
            Self::Settings => "Windows settings",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorMode {
    None,
    Basic16,
    Ansi256,
}
#[derive(Clone, Copy, Debug)]
struct Appearance {
    colors: ColorMode,
    animate: bool,
}
impl Appearance {
    fn resolve(
        animate: bool,
        system_motion: bool,
        no_color: bool,
        term: &str,
        colors_supported: bool,
        high_contrast: bool,
    ) -> Self {
        let dumb = term.eq_ignore_ascii_case("dumb");
        let colors = if no_color || dumb || !colors_supported {
            ColorMode::None
        } else if high_contrast || !term.contains("256color") {
            ColorMode::Basic16
        } else {
            ColorMode::Ansi256
        };
        Self {
            colors,
            animate: animate && system_motion && !dumb,
        }
    }
    fn detect(animate: bool) -> Self {
        Self::resolve(
            animate,
            crate::ui::system_allows_animation(),
            std::env::var_os("NO_COLOR").is_some(),
            &std::env::var("TERM").unwrap_or_default(),
            colors_enabled_read_only(true),
            crate::ui::system_high_contrast(),
        )
    }
    fn style(self, role: Role) -> &'static str {
        if self.colors == ColorMode::None {
            return "";
        }
        match (self.colors, role) {
            (_, Role::Brand | Role::Title) => "\x1b[1;39m",
            (_, Role::Text) => "\x1b[0;39m",
            (ColorMode::Basic16, Role::Muted | Role::Unknown) => "\x1b[0;39m",
            (ColorMode::Basic16, Role::Healthy) => "\x1b[0;32m",
            (ColorMode::Basic16, Role::Review) => "\x1b[0;33m",
            (ColorMode::Basic16, Role::Failure) => "\x1b[1;31m",
            (ColorMode::Basic16, Role::Focus) => "\x1b[7m",
            (ColorMode::Basic16, Role::FocusAccent) => "\x1b[7;1m",
            (ColorMode::Basic16, _) => "\x1b[0;96m",
            (_, Role::Muted | Role::Unknown) => "\x1b[0;38;5;245m",
            (_, Role::Healthy) => "\x1b[0;38;5;78m",
            (_, Role::Review) => "\x1b[0;38;5;221m",
            (_, Role::Failure) => "\x1b[1;38;5;203m",
            (_, Role::Focus) => "\x1b[7m",
            (_, Role::FocusAccent) => "\x1b[7;1m",
            _ => "\x1b[0;38;5;80m",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Span {
    text: String,
    role: Role,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Row {
    left: usize,
    spans: Vec<Span>,
}
impl Row {
    fn text(left: usize, text: impl Into<String>, role: Role) -> Self {
        Self {
            left,
            spans: vec![Span {
                text: text.into(),
                role,
            }],
        }
    }
    #[cfg(test)]
    fn plain(&self) -> String {
        format!(
            "{}{}",
            " ".repeat(self.left),
            self.spans
                .iter()
                .map(|s| s.text.as_str())
                .collect::<String>()
        )
    }
    fn encoded(&self, appearance: Appearance) -> String {
        if self.spans.is_empty() {
            return String::new();
        }
        let mut out = " ".repeat(self.left);
        for span in &self.spans {
            out.push_str(appearance.style(span.role));
            out.push_str(&span.text);
        }
        if appearance.colors != ColorMode::None {
            out.push_str("\x1b[0m");
        }
        out
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PageKind {
    #[default]
    Menu,
    Home,
    Checklist,
    Consent,
    Document,
    Activity,
}

pub trait ChoiceInput {
    fn select(
        &mut self,
        lang: Lang,
        items: &[String],
        default: usize,
        enter_only: bool,
    ) -> Result<Option<usize>>;
    fn multi_select(&mut self, lang: Lang, items: &[String]) -> Result<Option<Vec<usize>>>;
    fn multi_select_with_defaults(
        &mut self,
        lang: Lang,
        items: &[String],
        _defaults: &[bool],
    ) -> Result<Option<Vec<usize>>> {
        self.multi_select(lang, items)
    }
    /// A read-only, paged screen. Test readers can record the document without
    /// pretending a page acknowledgment authorizes any mutation.
    fn view(&mut self, _lang: Lang, _title: &str, _text: &str) -> Result<()> {
        Ok(())
    }
}

#[derive(Default)]
pub struct TerminalMenu {
    cancelled: bool,
}

struct Alternate<W: Write> {
    writer: W,
    colors: bool,
}
impl<W: Write> Alternate<W> {
    fn enter(writer: W, colors: bool) -> io::Result<Self> {
        // Construct the guard before the first write: partial entry also restores.
        let mut guard = Self { writer, colors };
        guard.writer.write_all(ENTER_SCREEN)?;
        guard.writer.flush()?;
        Ok(guard)
    }
}
impl<W: Write> Drop for Alternate<W> {
    fn drop(&mut self) {
        if self.colors {
            let _ = self.writer.write_all(b"\x1b[0m");
        }
        let _ = self.writer.write_all(LEAVE_SCREEN);
        let _ = self.writer.flush();
    }
}

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static RESIZE_EPOCH: AtomicU64 = AtomicU64::new(0);

/// console's color capability/Lazy color probes enable VT on Windows. Query
/// without mutating any mode, including before Screen and after its cleanup.
pub(crate) fn colors_enabled_read_only(stderr: bool) -> bool {
    use std::io::IsTerminal;
    if std::env::var_os("NO_COLOR").is_some()
        || std::env::var("TERM").is_ok_and(|s| s.eq_ignore_ascii_case("dumb"))
        || std::env::var("CLICOLOR").is_ok_and(|s| s == "0")
        || if stderr {
            !io::stderr().is_terminal()
        } else {
            !io::stdout().is_terminal()
        }
    {
        return false;
    }
    #[cfg(windows)]
    {
        let handle = unsafe {
            GetStdHandle(if stderr {
                (-12i32) as u32
            } else {
                (-11i32) as u32
            })
        };
        let mut mode = 0;
        unsafe { GetConsoleMode(handle, &mut mode) != 0 && mode & 4 != 0 }
    }
    #[cfg(not(windows))]
    {
        true
    }
}
#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(which: u32) -> *mut std::ffi::c_void;
    fn GetConsoleMode(handle: *mut std::ffi::c_void, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: *mut std::ffi::c_void, mode: u32) -> i32;
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
    fn ReadConsoleInputW(
        handle: *mut std::ffi::c_void,
        events: *mut NativeInput,
        length: u32,
        read: *mut u32,
    ) -> i32;
}
#[cfg(any(windows, test))]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct NativeKey {
    down: i32,
    repeat: u16,
    virtual_key: u16,
    scan: u16,
    unicode: u16,
    modifiers: u32,
}
#[cfg(windows)]
#[repr(C)]
union NativeEvent {
    key: NativeKey,
    raw: [u32; 4],
}
#[cfg(windows)]
#[repr(C)]
struct NativeInput {
    kind: u16,
    padding: u16,
    event: NativeEvent,
}

#[cfg(any(windows, test))]
fn native_key(event: NativeKey) -> Option<Key> {
    if event.down == 0 {
        return None;
    }
    // Decode virtual navigation keys first, even when a console host attaches
    // a character. In particular VK_PRIOR/VK_NEXT must not become Unknown.
    let key = match event.virtual_key {
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x23 => Key::End,
        0x24 => Key::Home,
        0x25 => Key::ArrowLeft,
        0x26 => Key::ArrowUp,
        0x27 => Key::ArrowRight,
        0x28 => Key::ArrowDown,
        0x0d => Key::Enter,
        0x1b => Key::Escape,
        0x08 => Key::Backspace,
        0x2e => Key::Del,
        0x09 if event.modifiers & 0x10 != 0 => Key::BackTab,
        0x09 => Key::Tab,
        0x43 if event.modifiers & 0x0c != 0 && event.modifiers & 3 == 0 => Key::CtrlC,
        _ => match event.unicode {
            3 => Key::CtrlC,
            13 => Key::Enter,
            27 => Key::Escape,
            // These menus accept bindings, not text input. Ignore UTF-16
            // surrogate units rather than consuming a second keyboard event.
            0 | 0xd800..=0xdfff => Key::Unknown,
            value => char::from_u32(value as u32)
                .map(Key::Char)
                .unwrap_or(Key::Unknown),
        },
    };
    Some(key)
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct SavedMode {
    handle: *mut std::ffi::c_void,
    flags: u32,
}
#[cfg(windows)]
impl SavedMode {
    fn capture(handle: *mut std::ffi::c_void) -> io::Result<Self> {
        let mut flags = 0;
        if unsafe { GetConsoleMode(handle, &mut flags) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle, flags })
    }
    fn restore(self) {
        unsafe {
            SetConsoleMode(self.handle, self.flags);
        }
    }
}
#[cfg(windows)]
struct ReadMode(SavedMode);
#[cfg(windows)]
impl Drop for ReadMode {
    fn drop(&mut self) {
        self.0.restore();
    }
}

fn read_key_raw() -> io::Result<Key> {
    #[cfg(not(windows))]
    {
        Term::stderr().read_key_raw()
    }
    #[cfg(windows)]
    {
        let handle = unsafe { GetStdHandle((-10i32) as u32) };
        // One reader owns each event. Do not peek/consume an event then delegate
        // to console's reader. This replaces its per-read guard as well as its
        // incomplete VK mapping, restoring the exact input flags on every exit.
        let guard = ReadMode(SavedMode::capture(handle)?);
        if unsafe { SetConsoleMode(handle, guard.0.flags & !1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        loop {
            let mut event: NativeInput = unsafe { std::mem::zeroed() };
            let mut read = 0;
            if unsafe { ReadConsoleInputW(handle, &mut event, 1, &mut read) } == 0 {
                return Err(io::Error::last_os_error());
            }
            if read != 1 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            if event.kind == 4 {
                // WINDOW_BUFFER_SIZE_EVENT is not a keystroke.
                RESIZE_EPOCH.fetch_add(1, Ordering::SeqCst);
                return Ok(Key::Unknown);
            }
            if event.kind == 1 {
                if let Some(key) = native_key(unsafe { event.event.key }) {
                    return Ok(key);
                }
            }
        }
    }
}
#[cfg(windows)]
unsafe extern "system" fn interrupt(event: u32) -> i32 {
    if matches!(event, 0 | 1) {
        INTERRUPTED.store(true, Ordering::SeqCst);
        1
    } else {
        0
    }
}
#[cfg(target_os = "linux")]
extern "C" {
    fn signal(number: i32, handler: usize) -> usize;
    fn tcgetattr(fd: i32, value: *mut std::ffi::c_void) -> i32;
    fn tcsetattr(fd: i32, action: i32, value: *const std::ffi::c_void) -> i32;
}
#[cfg(target_os = "linux")]
extern "C" fn interrupt(_signal: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}
#[cfg(target_os = "linux")]
extern "C" fn resized(_signal: i32) {
    RESIZE_EPOCH.fetch_add(1, Ordering::SeqCst);
}

struct NativeMode {
    #[cfg(windows)]
    modes: [SavedMode; 3],
    #[cfg(target_os = "linux")]
    signal: usize,
    #[cfg(target_os = "linux")]
    resize_signal: Option<usize>,
    #[cfg(target_os = "linux")]
    termios: [u64; 32],
}
impl NativeMode {
    fn enter() -> Result<Self> {
        INTERRUPTED.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        {
            // Capture all three before changing any: stdout/stderr commonly
            // alias a buffer but can also be distinct console handles.
            let modes = [
                SavedMode::capture(unsafe { GetStdHandle((-10i32) as u32) })?,
                SavedMode::capture(unsafe { GetStdHandle((-11i32) as u32) })?,
                SavedMode::capture(unsafe { GetStdHandle((-12i32) as u32) })?,
            ];
            let guard = Self { modes };
            ensure!(
                unsafe { SetConsoleMode(modes[2].handle, modes[2].flags | 0x0004 | 0x0001) } != 0,
                "Interactive console unavailable"
            );
            ensure!(
                unsafe { SetConsoleMode(modes[0].handle, (modes[0].flags & !0x0004) | 0x0008) }
                    != 0,
                "Interactive console unavailable"
            ); // No echo; report WINDOW_BUFFER_SIZE_EVENT to the guarded reader.
            ensure!(
                unsafe { SetConsoleCtrlHandler(Some(interrupt), 1) } != 0,
                "Interactive console unavailable"
            );
            Ok(guard)
        }
        #[cfg(target_os = "linux")]
        {
            // Linux termios starts with four 32-bit tcflag_t fields. Opaque,
            // aligned storage preserves the complete native structure, including
            // speeds/control characters. console owns per-key raw mode itself.
            let mut original = [0u64; 32];
            ensure!(
                unsafe { tcgetattr(0, original.as_mut_ptr().cast()) } == 0,
                "Interactive console unavailable"
            );
            let previous = unsafe { signal(2, interrupt as *const () as usize) };
            ensure!(previous != usize::MAX, "Interactive console unavailable");
            let mut guard = Self {
                signal: previous,
                resize_signal: None,
                termios: original,
            };
            let previous = unsafe { signal(28, resized as *const () as usize) };
            ensure!(previous != usize::MAX, "Interactive console unavailable");
            guard.resize_signal = Some(previous);
            let mut quiet = original;
            unsafe {
                *quiet.as_mut_ptr().cast::<u32>().add(3) &= !8;
            } // ECHO
            ensure!(
                unsafe { tcsetattr(0, 0, quiet.as_ptr().cast()) } == 0,
                "Interactive console unavailable"
            );
            Ok(guard)
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            Ok(Self {})
        }
    }
}
impl Drop for NativeMode {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            SetConsoleCtrlHandler(Some(interrupt), 0);
            for saved in self.modes.iter().rev() {
                saved.restore();
            }
        }
        #[cfg(target_os = "linux")]
        unsafe {
            tcsetattr(0, 0, self.termios.as_ptr().cast());
            signal(2, self.signal);
            if let Some(previous) = self.resize_signal {
                signal(28, previous);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Document {
    title: String,
    body: String,
    options: Vec<String>,
    consent: bool,
    document: bool,
    kind: PageKind,
}
impl Document {
    fn from_page(p: &Page) -> Self {
        Self {
            title: p.title.clone(),
            body: p.body.clone(),
            options: p.options.clone(),
            consent: p.consent,
            document: p.document,
            kind: p.kind,
        }
    }
    fn matches(&self, p: &Page) -> bool {
        self.title == p.title
            && self.body == p.body
            && self.options == p.options
            && self.consent == p.consent
            && self.document == p.document
            && self.kind == p.kind
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReviewGeometry {
    size: (u16, u16),
    left: usize,
    width: usize,
    body_start: usize,
    body_capacity: usize,
    total: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderedView {
    content: Arc<Document>,
    geometry: ReviewGeometry,
    offset: usize,
    end: usize,
    focused: usize,
    checked: Option<Vec<bool>>,
    header: Header,
    sections: Vec<Section>,
    role: Role,
    resize_epoch: u64,
}
#[derive(Default, Clone, Debug)]
struct Review {
    content: Option<Arc<Document>>,
    geometry: Option<ReviewGeometry>,
    through: usize,
    rendered: Option<RenderedView>,
    observed: Option<((u16, u16), u64)>,
    reconfirm: bool,
}
impl Review {
    fn invalidate(&mut self) {
        self.through = 0;
        self.rendered = None;
        self.content = None;
        self.geometry = None;
    }
    fn observe(&mut self, size: (u16, u16), epoch: u64, consent: bool) {
        if self.observed.is_some_and(|before| before != (size, epoch)) {
            self.invalidate();
            self.reconfirm |= consent;
        }
        self.observed = Some((size, epoch));
    }
    fn same_basis(&self, view: &RenderedView) -> bool {
        self.content
            .as_ref()
            .is_some_and(|d| Arc::ptr_eq(d, &view.content))
            && self.geometry.as_ref() == Some(&view.geometry)
    }
    fn complete(&self, view: &RenderedView) -> bool {
        self.same_basis(view) && self.through >= view.geometry.total
    }
    // Only the successful write+flush path calls this, never layout/key handling.
    fn emitted(&mut self, view: RenderedView) {
        if !self.same_basis(&view) {
            self.invalidate();
            self.content = Some(view.content.clone());
            self.geometry = Some(view.geometry.clone());
        }
        if view.offset <= self.through {
            self.through = self.through.max(view.end);
        }
        self.rendered = Some(view);
    }
    fn can_approve(&self, view: &RenderedView, before_read: Option<&RenderedView>) -> bool {
        !self.reconfirm
            && self.complete(view)
            && self.rendered.as_ref() == Some(view)
            && before_read == Some(view)
    }
}

#[derive(Default, Clone)]
struct Page {
    title: String,
    body: String,
    options: Vec<String>,
    focused: usize,
    checked: Option<Vec<bool>>,
    consent: bool,
    document: bool,
    offset: usize,
    review: Review,
    content: Option<Arc<Document>>,
    wrap_width: usize,
    wrapped: Vec<String>,
    activity: Option<Instant>,
    kind: PageKind,
    helpers: Vec<String>,
    focus_at: Option<Instant>,
    role: Role,
    wrapped_roles: Vec<Role>,
}
struct Surface {
    lang: Lang,
    header: Header,
    appearance: Appearance,
    sections: Vec<Section>,
    pending: String,
    pending_title: String,
    pending_home_helpers: Vec<String>,
    pending_role: Role,
    page: Page,
    last_rows: Vec<String>,
    last_size: (u16, u16),
    last_epoch: Option<u64>,
    error: Option<String>,
    suspended: bool,
}
thread_local! { static SURFACE: RefCell<Option<Arc<Mutex<Surface>>>> = const { RefCell::new(None) }; }
fn with_surface<T>(f: impl FnOnce(&mut Surface) -> T) -> Option<T> {
    SURFACE.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|state| f(&mut state.lock().unwrap_or_else(|e| e.into_inner())))
    })
}
pub fn screen_active() -> bool {
    SURFACE.with(|s| s.borrow().is_some())
}

/// Nested guides reuse the outer surface. Only the owner starts/stops rendering
/// and restores the caller's screen. The guard cannot be moved between threads.
pub struct Screen {
    stop: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
    alternate: Option<Alternate<io::Stderr>>,
    mode: Option<NativeMode>,
    _local: PhantomData<Rc<()>>,
}
impl Screen {
    pub fn enter(lang: Lang, animate: bool) -> Result<Self> {
        let mut guard = Self {
            stop: None,
            worker: None,
            alternate: None,
            mode: None,
            _local: PhantomData,
        };
        if screen_active() {
            return Ok(guard);
        }
        crate::guided::require_terminal(lang)?; // Before any escape or mode change.
        guard.mode = Some(NativeMode::enter()?);
        let appearance = Appearance::detect(animate);
        guard.alternate = Some(Alternate::enter(
            io::stderr(),
            appearance.colors != ColorMode::None,
        )?);
        let state = Arc::new(Mutex::new(Surface {
            lang,
            header: Header {
                subtitle: lang.t("You choose what changes."),
                badges: vec![Badge {
                    text: lang.t("Unverified"),
                    role: Role::Unknown,
                }],
                tally: None,
            },
            appearance,
            sections: Vec::new(),
            pending: String::new(),
            pending_title: String::new(),
            pending_home_helpers: Vec::new(),
            pending_role: Role::Text,
            page: Page::default(),
            last_rows: Vec::new(),
            last_size: (0, 0),
            last_epoch: None,
            error: None,
            suspended: false,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        guard.stop = Some(stop.clone());
        SURFACE.with(|slot| *slot.borrow_mut() = Some(state.clone()));
        guard.worker = Some(
            std::thread::Builder::new()
                .name("interactive-screen".into())
                .spawn(move || {
                    while !stop.load(Ordering::SeqCst) {
                        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                        paint(&mut state);
                        drop(state);
                        std::thread::sleep(Duration::from_millis(100));
                    }
                })?,
        );
        Ok(guard)
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::SeqCst);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            SURFACE.with(|slot| *slot.borrow_mut() = None);
        }
        drop(self.alternate.take());
        drop(self.mode.take());
    }
}

pub fn screen_header(header: Header) {
    with_surface(|s| {
        s.header = header;
    });
}
pub struct SectionGuard(Option<usize>);
pub fn section(section: Section) -> SectionGuard {
    SectionGuard(with_surface(|s| {
        let depth = s.sections.len();
        s.sections.push(section);
        depth
    }))
}
impl Drop for SectionGuard {
    fn drop(&mut self) {
        if let Some(depth) = self.0 {
            with_surface(|s| s.sections.truncate(depth));
        }
    }
}
pub fn screen_home(helpers: Vec<String>) {
    with_surface(|s| s.pending_home_helpers = helpers);
}
pub fn screen_role(role: Role) {
    with_surface(|s| s.pending_role = role);
}
pub fn screen_title(title: &str) {
    with_surface(|s| s.pending_title = title.to_owned());
}
pub fn screen_note(text: &str) -> Result<bool> {
    with_surface(|s| {
        ensure!(
            s.pending.len().saturating_add(text.len()).saturating_add(1) <= BODY_LIMIT,
            "Screen content is too large to review safely"
        );
        s.pending.push_str(text);
        s.pending.push('\n');
        Ok(true)
    })
    .unwrap_or(Ok(false))
}
pub fn screen_content(text: &str) -> Result<bool> {
    with_surface(|s| s.pending.clear());
    screen_note(text)
}
pub fn screen_clear() {
    with_surface(|s| {
        s.pending.clear();
        s.pending_title.clear();
        s.pending_home_helpers.clear();
        s.pending_role = Role::Text;
    });
}
pub fn screen_progress(title: &str, message: &str) -> bool {
    with_surface(|s| {
        let activity = s.page.activity.or(Some(Instant::now()));
        s.pending.clear();
        s.pending_title.clear();
        s.pending_home_helpers.clear();
        s.page = Page {
            title: title.into(),
            body: message.into(),
            activity,
            kind: PageKind::Activity,
            ..Page::default()
        };
        paint(s);
    })
    .is_some()
}
pub fn screen_progress_end() {
    with_surface(|s| {
        s.page.activity = None;
    });
}

fn check_size(rows: u16, columns: u16) -> bool {
    rows >= 10 && columns >= 24
}

/// Six-row ANSI Shadow wordmark, each row padded to exactly 60 display columns.
pub(crate) const LOGO_ROWS: [&str; 6] = [
    "███████╗███████╗ ██████╗██████╗ ██╗     ██╗████████╗███████╗",
    "██╔════╝██╔════╝██╔════╝██╔══██╗██║     ██║╚══██╔══╝╚══███╔╝",
    "███████╗█████╗  ██║     ██████╔╝██║     ██║   ██║     ███╔╝ ",
    "╚════██║██╔══╝  ██║     ██╔══██╗██║     ██║   ██║    ███╔╝  ",
    "███████║███████╗╚██████╗██████╔╝███████╗██║   ██║   ███████╗",
    "╚══════╝╚══════╝ ╚═════╝╚═════╝ ╚══════╝╚═╝   ╚═╝   ╚══════╝",
];

/// Build a Row for one logo line, splitting █ (Brand) from box-drawing/spaces (Muted).
fn logo_row(logo_left: usize, row_str: &str) -> Row {
    let mut spans: Vec<Span> = Vec::new();
    let mut run = String::new();
    let mut run_is_block = false;
    for ch in row_str.chars() {
        let block = ch == '█';
        if run.is_empty() {
            run_is_block = block;
        }
        if block == run_is_block {
            run.push(ch);
        } else {
            spans.push(Span {
                text: std::mem::take(&mut run),
                role: if run_is_block { Role::Brand } else { Role::Muted },
            });
            run_is_block = block;
            run.push(ch);
        }
    }
    if !run.is_empty() {
        spans.push(Span {
            text: run,
            role: if run_is_block { Role::Brand } else { Role::Muted },
        });
    }
    Row {
        left: logo_left,
        spans,
    }
}

/// Wrapping is terminal-cell aware and never slices UTF-8. Terminal controls and
/// bidi overrides are removed from evidence, not interpreted as UI instructions.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    text.lines()
        .flat_map(|line| crate::ui::wrap(&console::strip_ansi_codes(line), width))
        .collect()
}
fn fit(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let plain = crate::ui::safe(&console::strip_ansi_codes(text));
    truncate_str(&plain, width, "…").into_owned()
}

struct Layout {
    #[cfg(test)]
    lines: Vec<String>,
    rows: Vec<Row>,
    left: usize,
    width: usize,
    body_start: usize,
    body_capacity: usize,
    body_total: usize,
    #[cfg(test)]
    approvable: bool,
    view: Option<RenderedView>,
    unclipped: bool,
}
fn badge_rows(badges: &[Badge], left: usize, width: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut row = Row {
        left,
        spans: Vec::new(),
    };
    let mut used = 0;
    for badge in badges {
        let icon = match badge.role {
            Role::Healthy => "✓",
            Role::Review => "!",
            Role::Unknown => "?",
            Role::Failure => "×",
            Role::Working => "·",
            _ => "·",
        };
        let text = format!(
            "{icon} {}",
            crate::ui::safe(&console::strip_ansi_codes(&badge.text))
        );
        let parts = wrap_text(&text, width);
        for part in parts {
            let cells = measure_text_width(&part);
            if used > 0 && used + 3 + cells > width {
                rows.push(row);
                row = Row {
                    left,
                    spans: Vec::new(),
                };
                used = 0;
            }
            if used > 0 {
                row.spans.push(Span {
                    text: "   ".into(),
                    role: Role::Muted,
                });
                used += 3;
            }
            row.spans.push(Span {
                text: part,
                role: badge.role,
            });
            used += cells;
        }
    }
    if !row.spans.is_empty() {
        rows.push(row);
    }
    rows
}

/// Per-source-line role from its first non-space token.
fn source_line_role(line: &str, default_role: Role) -> Role {
    let trimmed = line.trim_start();
    if trimmed.starts_with("✓ ") {
        Role::Healthy
    } else if trimmed.starts_with("! ") || trimmed.starts_with("↻ ") {
        Role::Review
    } else if trimmed.starts_with("? ") {
        Role::Unknown
    } else if trimmed.starts_with("✗ ") {
        Role::Failure
    } else if trimmed.starts_with("▸ ") {
        Role::Title
    } else if trimmed.starts_with("· ") {
        Role::Muted
    } else {
        default_role
    }
}

/// Build rounded-card rows for the status header. Returns empty if width < 10.
fn status_card_rows(
    lang: Lang,
    header: &Header,
    left: usize,
    width: usize,
    show_bar: bool,
) -> Vec<Row> {
    if width < 10 {
        return Vec::new();
    }
    // Inner content area (between │ borders): width - 2
    let inner = width.saturating_sub(2);
    // Content area inside │ + space margins: inner - 2
    let content_w = inner.saturating_sub(2);

    // ── Top border: ╭─ Title ─────────╮ ─────────────────────────────────
    let subtitle = fit(&header.subtitle, content_w.saturating_sub(4).max(1));
    let sub_w = measure_text_width(&subtitle);
    let top_row = if sub_w > 0 {
        let fill = inner.saturating_sub(sub_w + 3); // "─ " + title + " "
        Row {
            left,
            spans: vec![
                Span { text: "╭─ ".into(), role: Role::Muted },
                Span { text: subtitle.clone(), role: Role::Title },
                Span { text: format!(" {}╮", "─".repeat(fill)), role: Role::Muted },
            ],
        }
    } else {
        Row::text(left, format!("╭{}╮", "─".repeat(inner)), Role::Muted)
    };

    // ── Badge content row ──────────────────────────────────────────────
    let badge_content = badge_rows(&header.badges, 0, content_w);
    let badge_spans: Vec<Span> = badge_content
        .into_iter()
        .next()
        .map(|r| r.spans)
        .unwrap_or_default();
    let badge_used: usize = badge_spans
        .iter()
        .map(|s| measure_text_width(&s.text))
        .sum();
    let badge_pad = content_w.saturating_sub(badge_used);
    let mut badge_row_spans = vec![Span { text: "│ ".into(), role: Role::Muted }];
    badge_row_spans.extend(badge_spans);
    if badge_pad > 0 {
        badge_row_spans.push(Span { text: " ".repeat(badge_pad), role: Role::Muted });
    }
    badge_row_spans.push(Span { text: " │".into(), role: Role::Muted });
    let badge_row = Row { left, spans: badge_row_spans };

    // ── Progress bar row (optional) ────────────────────────────────────
    let mut rows = vec![top_row, badge_row];
    if show_bar {
        if let Some((protected, total)) = header.tally {
            if total > 0 {
                let bar_w = 20_usize.min(content_w.saturating_sub(12));
                let filled = (protected * bar_w + total / 2) / total;
                let empty_cells = bar_w.saturating_sub(filled);
                let bar_label = fit(
                    &lang
                        .t("{protected} of {total} checks protected")
                        .replace("{protected}", &protected.to_string())
                        .replace("{total}", &total.to_string()),
                    content_w.saturating_sub(bar_w + 3).max(1),
                );
                let label_w = measure_text_width(&bar_label);
                let bar_pad = content_w.saturating_sub(bar_w + 2 + label_w);
                let mut bar_spans = vec![Span { text: "│ ".into(), role: Role::Muted }];
                if filled > 0 {
                    bar_spans.push(Span { text: "█".repeat(filled), role: Role::Healthy });
                }
                if empty_cells > 0 {
                    bar_spans.push(Span { text: "░".repeat(empty_cells), role: Role::Muted });
                }
                bar_spans.push(Span { text: format!("  {bar_label}"), role: Role::Muted });
                if bar_pad > 0 {
                    bar_spans.push(Span { text: " ".repeat(bar_pad), role: Role::Muted });
                }
                bar_spans.push(Span { text: " │".into(), role: Role::Muted });
                rows.push(Row { left, spans: bar_spans });
            }
        }
    }

    // ── Bottom border ──────────────────────────────────────────────────
    rows.push(Row::text(left, format!("╰{}╯", "─".repeat(inner)), Role::Muted));
    rows
}

/// Symbols for the 5 root home-menu choices (single-cell-width BMP characters).
pub(crate) const HOME_SYMBOLS: [&str; 5] = ["✦", "☰", "↻", "⋯", "×"];

/// Split a localized hint (e.g. "↑/↓ · Enter · Esc") into Accent key spans
/// separated by Muted " · " dividers. Falls back to a single span if no " · ".
fn hint_spans(hint: &str, fallback_role: Role) -> Vec<Span> {
    if hint.contains(" · ") {
        let parts: Vec<&str> = hint.split(" · ").collect();
        let mut spans = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                spans.push(Span { text: " · ".into(), role: Role::Muted });
            }
            spans.push(Span { text: (*part).to_owned(), role: Role::Accent });
        }
        spans
    } else {
        vec![Span { text: hint.to_owned(), role: fallback_role }]
    }
}

fn focus_role(page: &Page, appearance: Appearance, now: Instant) -> Role {
    if appearance.animate
        && page
            .focus_at
            .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_millis(140))
    {
        Role::FocusAccent
    } else {
        Role::Focus
    }
}
fn activity_text(lang: Lang, start: Instant, appearance: Appearance, now: Instant) -> String {
    if !appearance.animate {
        return format!("· {}", lang.t("Working"));
    }
    let elapsed = now.saturating_duration_since(start);
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    format!(
        "{} {}  {}s",
        frames[(elapsed.as_millis() / 100 % 10) as usize],
        lang.t("Working"),
        elapsed.as_secs()
    )
}
fn layout(
    lang: Lang,
    header: &Header,
    sections: &[Section],
    page: &mut Page,
    size: (u16, u16),
    appearance: Appearance,
    now: Instant,
) -> Layout {
    let (height, columns) = (
        size.0.saturating_sub(1) as usize,
        size.1.saturating_sub(1) as usize,
    );
    let container = columns.min(96);
    let width = container.saturating_sub(4).max(1);
    let left = (columns.saturating_sub(container)) / 2 + 2;
    let compact = height < 20;
    let unavailable = || {
        let rows = wrap_text(
            &lang.t("Enlarge the terminal to continue. Esc goes back."),
            columns.max(1),
        )
        .into_iter()
        .take(height)
        .map(|s| Row::text(0, s, Role::Unknown))
        .collect::<Vec<_>>();
        Layout {
            #[cfg(test)]
            lines: rows.iter().map(Row::plain).collect(),
            rows,
            left: 0,
            width: columns,
            body_start: 0,
            body_capacity: 0,
            body_total: 0,
            #[cfg(test)]
            approvable: false,
            view: None,
            unclipped: false,
        }
    };
    if !check_size(size.0, size.1) {
        return unavailable();
    }
    // Show the big logo only on the home screen with a generous terminal size.
    let use_logo = page.kind == PageKind::Home && size.0 >= 30 && size.1 >= 62;
    let logo_left = (size.1 as usize).saturating_sub(60) / 2;
    let mut rows = Vec::new();
    if !compact {
        rows.push(Row::default());
    }
    if use_logo {
        for &row_str in &LOGO_ROWS {
            rows.push(logo_row(logo_left, row_str));
        }
        let tagline = format!(
            "{}  v{}",
            lang.t("Less worry. More protection."),
            env!("CARGO_PKG_VERSION")
        );
        let tagline_width = measure_text_width(&tagline);
        let tagline_left = (size.1 as usize).saturating_sub(tagline_width) / 2;
        rows.push(Row::text(tagline_left, tagline, Role::Muted));
    } else {
        rows.push(Row {
            left,
            spans: vec![
                Span {
                    text: "Secblitz".into(),
                    role: Role::Brand,
                },
                Span {
                    text: fit(
                        &format!("  v{}", env!("CARGO_PKG_VERSION")),
                        width.saturating_sub(8),
                    ),
                    role: Role::Muted,
                },
            ],
        });
    }
    // ── Status header (card or flat) ──────────────────────────────────────
    if !compact && header.tally.is_some() && width >= 10 {
        // Rounded status card. Row count: card_rows + post-card empty.
        // card_rows = top + badges + [bar] + bottom = 3 or 4 rows.
        // To keep total equal to the flat header (subtitle+empty+badges+divider+empty=5),
        // we emit: card(4 rows with bar, or 3 without) + 1 or 2 empty rows.
        let show_bar = header.tally.is_some_and(|(_, total)| total > 0);
        let card = status_card_rows(lang, header, left, width, show_bar);
        let card_len = card.len();
        rows.extend(card);
        // Pad to match flat-header height so downstream row counts are stable.
        let flat_rows: usize = if header.subtitle.is_empty() { 4 } else { 5 };
        let padding_rows = flat_rows.saturating_sub(card_len);
        for _ in 0..padding_rows {
            rows.push(Row::default());
        }
    } else {
        // Flat header (compact mode, or no tally).
        if !header.subtitle.is_empty() {
            rows.push(Row::text(left, fit(&header.subtitle, width), Role::Muted));
        }
        if !compact {
            rows.push(Row::default());
        }
        rows.extend(badge_rows(&header.badges, left, width));
        rows.push(Row::text(left, "─".repeat(width), Role::Muted));
        if !compact {
            rows.push(Row::default());
        }
    }
    // ── Breadcrumb or section path + title ────────────────────────────────
    let section_path: String = if sections.is_empty() {
        String::new()
    } else {
        sections
            .iter()
            .skip(if width < 60 {
                sections.len().saturating_sub(2)
            } else {
                0
            })
            .map(|s| lang.t(s.key()))
            .collect::<Vec<_>>()
            .join(" › ")
    };
    let title_role = if page.role == Role::Text { Role::Title } else { page.role };
    let breadcrumb_w = measure_text_width(&section_path)
        + if !section_path.is_empty() && !page.title.is_empty() { 3 } else { 0 }
        + measure_text_width(&page.title);
    if !section_path.is_empty() && !page.title.is_empty() && breadcrumb_w <= width {
        // Combined breadcrumb: "Section › Title"
        rows.push(Row {
            left,
            spans: vec![
                Span { text: section_path.clone(), role: Role::Muted },
                Span { text: " › ".into(), role: Role::Muted },
                Span { text: fit(&page.title, width.saturating_sub(measure_text_width(&section_path) + 3)), role: title_role },
            ],
        });
    } else {
        if !section_path.is_empty() {
            rows.push(Row::text(left, fit(&section_path, width), Role::Muted));
        }
        if !page.title.is_empty() {
            rows.push(Row::text(left, fit(&page.title, width), title_role));
        }
    }
    if let Some(checked) = &page.checked {
        let selected = lang
            .t("Selected: {selected} of {total}")
            .replace(
                "{selected}",
                &checked.iter().filter(|v| **v).count().to_string(),
            )
            .replace("{total}", &checked.len().to_string());
        rows.push(Row::text(left, fit(&selected, width), Role::Accent));
    }
    if !compact {
        rows.push(Row::default());
    }
    let changed = page
        .content
        .as_ref()
        .is_none_or(|content| !content.matches(page));
    if changed {
        page.content = Some(Arc::new(Document::from_page(page)));
    }
    if page.wrap_width != width || changed {
        let full_title = if measure_text_width(&crate::ui::safe(&page.title)) > width {
            format!("{}\n", page.title)
        } else {
            String::new()
        };
        let body_text = format!("{full_title}{}", page.body);
        // Build wrapped lines and parallel role vector from source-line prefixes.
        let mut wrapped = Vec::new();
        let mut wrapped_roles = Vec::new();
        let default_role = if page.role == Role::Text { Role::Text } else { page.role };
        for source_line in body_text.lines() {
            let role = source_line_role(source_line, default_role);
            let lines = crate::ui::wrap(&console::strip_ansi_codes(source_line), width);
            for line in lines {
                wrapped_roles.push(role);
                wrapped.push(line);
            }
        }
        // A body ending in \n produces a trailing empty line via `lines()` only
        // when there is content before it; match old wrap_text behavior.
        if body_text.ends_with('\n') && !body_text.trim().is_empty() {
            let last = body_text.trim_end_matches('\n');
            if last.ends_with('\n') {
                wrapped.push(String::new());
                wrapped_roles.push(default_role);
            }
        }
        page.wrapped = wrapped;
        page.wrapped_roles = wrapped_roles;
        page.wrap_width = width;
        page.offset = 0;
    }
    let body_total = page.wrapped.len();
    let home = page.kind == PageKind::Home;
    let helper_rows = if home && !page.helpers.is_empty() {
        if compact {
            2
        } else {
            3
        }
    } else {
        0
    };
    let footer_rows = if home && body_total == 0 { 1 } else { 2 };
    let body_gap = usize::from(body_total > 0 && !page.options.is_empty());
    let required_body = usize::from(body_total > 0 || page.activity.is_some());
    if rows.len()
        + footer_rows
        + helper_rows
        + required_body
        + body_gap
        + usize::from(!page.options.is_empty())
        > height
    {
        return unavailable();
    }
    let remaining = height - rows.len() - footer_rows - helper_rows;
    let option_rows = page
        .options
        .len()
        .min(7)
        .min(remaining.saturating_sub(required_body + body_gap));
    let spaced = !compact
        && !page.options.is_empty()
        && page.options.len() <= 5
        && option_rows == page.options.len()
        && option_rows * 2 - 1 + body_total + body_gap <= remaining;
    let option_height = if option_rows == 0 {
        0
    } else if spaced {
        option_rows * 2 - 1
    } else {
        option_rows
    };
    let body_capacity = remaining
        .saturating_sub(option_height + body_gap + usize::from(page.activity.is_some()))
        .max(1);
    let body_start = rows.len();
    page.offset = page.offset.min(body_total.saturating_sub(body_capacity));
    let end = (page.offset + body_capacity).min(body_total);
    rows.extend(
        page.wrapped
            .iter()
            .enumerate()
            .skip(page.offset)
            .take(body_capacity)
            .map(|(i, s)| {
                let role = page.wrapped_roles.get(i).copied().unwrap_or(Role::Text);
                Row::text(left, s.clone(), role)
            }),
    );
    if let Some(start) = page.activity {
        if rows.len() < height - footer_rows {
            rows.push(Row::text(
                left,
                fit(&activity_text(lang, start, appearance, now), width),
                Role::Working,
            ));
        }
    }
    if body_gap != 0 {
        rows.push(Row::default());
    }
    let use_color_focus = appearance.colors != ColorMode::None;
    let mut unclipped = true;
    if option_rows > 0 {
        let first = page.focused / option_rows * option_rows;
        for (visible, (index, item)) in page
            .options
            .iter()
            .enumerate()
            .skip(first)
            .take(option_rows)
            .enumerate()
        {
            if spaced && visible != 0 {
                rows.push(Row::default());
            }
            let focused = page.focused == index;
            let checkbox = page
                .checked
                .as_ref()
                .map(|v| if v[index] { "[✓] " } else { "[ ] " })
                .unwrap_or("");
            // Symbol prefix for Home page root options (single-cell-width BMP chars).
            let symbol = if home && page.checked.is_none() && index < HOME_SYMBOLS.len() {
                format!("{} ", HOME_SYMBOLS[index])
            } else {
                String::new()
            };
            // In color modes: full-width highlighted bar with › marker (no brackets).
            // In None mode: keep the existing ❨ › … ❩ accessibility markers.
            let (prefix, suffix) = if focused && use_color_focus {
                (format!("  › {checkbox}{symbol}"), String::new())
            } else if focused {
                (format!("❨ › {checkbox}{symbol}"), " ❩".to_owned())
            } else {
                (format!("    {checkbox}{symbol}"), String::new())
            };
            let label = fit(
                item,
                width.saturating_sub(measure_text_width(&prefix) + measure_text_width(&suffix)),
            );
            if measure_text_width(&crate::ui::safe(item)) > measure_text_width(&label) {
                unclipped = false;
            }
            let padding = if focused {
                " ".repeat(width.saturating_sub(
                    measure_text_width(&prefix)
                        + measure_text_width(&label)
                        + measure_text_width(&suffix),
                ))
            } else {
                String::new()
            };
            rows.push(Row::text(
                left,
                format!("{prefix}{label}{padding}{suffix}"),
                if focused {
                    focus_role(page, appearance, now)
                } else {
                    Role::Text
                },
            ));
        }
    }
    if helper_rows > 0 {
        if !compact {
            rows.push(Row::default());
        }
        if let Some(helper) = page.helpers.get(page.focused) {
            rows.extend(
                wrap_text(helper, width.saturating_sub(4))
                    .into_iter()
                    .take(2)
                    .map(|s| Row {
                        left: left + 2,
                        spans: vec![
                            Span { text: "┊ ".into(), role: Role::Muted },
                            Span { text: s, role: Role::Muted },
                        ],
                    }),
            );
        }
    }
    let view = RenderedView {
        content: page.content.as_ref().unwrap().clone(),
        geometry: ReviewGeometry {
            size,
            left,
            width,
            body_start,
            body_capacity,
            total: body_total,
        },
        offset: page.offset,
        end,
        focused: page.focused,
        checked: page.checked.clone(),
        header: header.clone(),
        sections: sections.to_vec(),
        role: page.role,
        resize_epoch: RESIZE_EPOCH.load(Ordering::SeqCst),
    };
    let approvable = !page.consent || (!page.review.reconfirm && page.review.complete(&view));
    while rows.len() < height - footer_rows {
        rows.push(Row::default());
    }
    if footer_rows == 2 {
        let text = if page.activity.is_some() {
            String::new()
        } else if page.consent && !unclipped {
            lang.t("Enlarge the terminal to continue. Esc goes back.")
        } else if page.consent && page.review.reconfirm {
            lang.t("Screen changed. Review, then confirm again.")
        } else if body_total > body_capacity {
            format!(
                "{}  {}-{}/{}",
                lang.t(if approvable {
                    PAGE_HINT
                } else {
                    "PgDn: read all details before approval"
                }),
                page.offset + 1,
                end,
                body_total
            )
        } else if option_rows > 0 && option_rows < page.options.len() {
            format!("{} / {}", page.focused + 1, page.options.len())
        } else {
            String::new()
        };
        rows.push(Row::text(
            left,
            fit(&text, width),
            if page.checked.is_some() {
                Role::Accent
            } else {
                Role::Muted
            },
        ));
    }
    let hint_key = if page.activity.is_some() {
        "Keep this window open while Windows finishes."
    } else if page.document {
        "Enter or Esc: back"
    } else if page.checked.is_some() {
        MULTI_HINT
    } else {
        SELECT_HINT
    };
    let mut hint = lang.t(hint_key);
    if home && option_rows > 0 && option_rows < page.options.len() {
        hint.push_str(&format!("  {}/{}", page.focused + 1, page.options.len()));
    }
    let hint_truncated = fit(&hint, width);
    let hint_fallback = if page.checked.is_some() { Role::Accent } else { Role::Muted };
    let hint_row_spans = hint_spans(&hint_truncated, hint_fallback);
    rows.push(Row { left, spans: hint_row_spans });
    unclipped &= rows.len() <= height
        && body_start + end.saturating_sub(page.offset) <= rows.len()
        && rows.iter().all(|row| {
            row.left
                + row
                    .spans
                    .iter()
                    .map(|span| measure_text_width(&span.text))
                    .sum::<usize>()
                < size.1 as usize
        });
    rows.truncate(height);
    Layout {
        #[cfg(test)]
        lines: rows.iter().map(Row::plain).collect(),
        rows,
        left,
        width,
        body_start,
        body_capacity,
        body_total,
        #[cfg(test)]
        approvable,
        view: Some(view),
        unclipped,
    }
}

/// Only changed physical rows are cleared/painted. No whole-screen erase on
/// clocks, focus accents or spinner ticks; resize explicitly clears stale rows.
fn row_diff(
    previous: &[String],
    next: &[String],
    previous_size: (u16, u16),
    size: (u16, u16),
) -> String {
    let resized = previous_size != size;
    let last = if resized {
        size.0.saturating_sub(1) as usize
    } else {
        previous
            .len()
            .max(next.len())
            .min(size.0.saturating_sub(1) as usize)
    };
    let mut out = String::new();
    for index in 0..last {
        let old = previous.get(index).map(String::as_str).unwrap_or("");
        let new = next.get(index).map(String::as_str).unwrap_or("");
        if resized || old != new {
            out.push_str(&format!("\x1b[{};1H\x1b[2K{new}", index + 1));
        }
    }
    out
}
fn paint(s: &mut Surface) {
    if s.error.is_some() || s.suspended {
        return;
    }
    let result = paint_to(
        s,
        Term::stderr().size(),
        RESIZE_EPOCH.load(Ordering::SeqCst),
        &mut io::stderr(),
        || (Term::stderr().size(), RESIZE_EPOCH.load(Ordering::SeqCst)),
    );
    if let Err(error) = result {
        s.error = Some(error.to_string());
    }
}

fn paint_to(
    s: &mut Surface,
    size: (u16, u16),
    epoch: u64,
    writer: &mut impl Write,
    after: impl FnOnce() -> ((u16, u16), u64),
) -> io::Result<()> {
    s.page.review.observe(size, epoch, s.page.consent);
    let mut frame = layout(
        s.lang,
        &s.header,
        &s.sections,
        &mut s.page,
        size,
        s.appearance,
        Instant::now(),
    );
    if let Some(view) = &mut frame.view {
        view.resize_epoch = epoch;
    }
    // Replacing content, or clipping even temporarily, cannot inherit credit.
    if !frame.unclipped
        || frame
            .view
            .as_ref()
            .is_none_or(|v| s.page.review.content.is_some() && !s.page.review.same_basis(v))
    {
        s.page.review.invalidate();
        s.page.review.reconfirm |= s.page.consent;
    }
    let next = frame
        .rows
        .iter()
        .map(|r| r.encoded(s.appearance))
        .collect::<Vec<_>>();
    // A -> tiny -> A may clip terminal cells even when dimensions and cached
    // strings are unchanged. Bind the physical-output cache to the resize epoch
    // independently of Review, which key handling may already have invalidated.
    let previous_size = if s.last_epoch == Some(epoch) {
        s.last_size
    } else {
        (0, 0)
    };
    let output = row_diff(&s.last_rows, &next, previous_size, size);
    let result = (|| {
        if !output.is_empty() {
            writer.write_all(output.as_bytes())?;
        }
        writer.flush()
    })();
    if let Err(error) = result {
        s.page.review.invalidate();
        s.page.review.reconfirm |= s.page.consent;
        s.error = Some(error.to_string());
        return Err(error);
    }
    let (actual_size, actual_epoch) = after();
    if (actual_size, actual_epoch) != (size, epoch) {
        s.page
            .review
            .observe(actual_size, actual_epoch, s.page.consent);
        s.page.review.invalidate();
        s.page.review.reconfirm |= s.page.consent;
        s.last_size = (0, 0);
        s.last_rows.clear();
        s.last_epoch = None;
        return Ok(());
    }
    s.last_rows = next;
    s.last_size = size;
    s.last_epoch = Some(epoch);
    if frame.unclipped {
        if let Some(view) = frame.view {
            s.page.review.emitted(view);
        }
    }
    Ok(())
}

fn handle_key(
    s: &mut Surface,
    key: &Key,
    before_read: Option<&RenderedView>,
    size: (u16, u16),
    epoch: u64,
) -> Option<Option<Vec<usize>>> {
    if s.error.is_some() {
        return Some(None);
    }
    s.page.review.observe(size, epoch, s.page.consent);
    if matches!(key, Key::Escape | Key::CtrlC | Key::Char('q')) {
        return Some(None);
    }
    let mut frame = layout(
        s.lang,
        &s.header,
        &s.sections,
        &mut s.page,
        size,
        s.appearance,
        Instant::now(),
    );
    if let Some(view) = &mut frame.view {
        view.resize_epoch = epoch;
    }
    if !check_size(size.0, size.1) || frame.body_capacity == 0 {
        s.page.review.invalidate();
        return None;
    }
    let p = &mut s.page;
    let old_focus = p.focused;
    match key {
        Key::PageDown | Key::ArrowRight => {
            p.offset = (p.offset + frame.body_capacity)
                .min(frame.body_total.saturating_sub(frame.body_capacity))
        }
        Key::PageUp | Key::ArrowLeft => p.offset = p.offset.saturating_sub(frame.body_capacity),
        Key::ArrowDown if p.document => {
            p.offset = (p.offset + 1).min(frame.body_total.saturating_sub(frame.body_capacity))
        }
        Key::ArrowUp if p.document => p.offset = p.offset.saturating_sub(1),
        Key::Home if p.document => p.offset = 0,
        Key::End if p.document => p.offset = frame.body_total.saturating_sub(frame.body_capacity),
        Key::Char(' ') if p.checked.is_some() => {
            let checked = p.checked.as_mut().unwrap();
            checked[p.focused] = !checked[p.focused];
        }
        _ => {
            if let Some(answer) = enter_choice(&mut p.focused, p.options.len(), key) {
                if answer == Some(0)
                    && p.consent
                    && !(frame.unclipped
                        && frame
                            .view
                            .as_ref()
                            .is_some_and(|v| p.review.can_approve(v, before_read)))
                {
                    // The key that discovers a resize/stale frame is consumed,
                    // never replayed after painting the new recap. Even if a
                    // background repaint won the race, resize needs a new Enter.
                    p.review.reconfirm = false;
                    return None;
                }
                return Some(answer.map(|index| {
                    p.checked
                        .as_ref()
                        .map(|checked| {
                            checked
                                .iter()
                                .enumerate()
                                .filter_map(|(i, v)| v.then_some(i))
                                .collect()
                        })
                        .unwrap_or_else(|| vec![index])
                }));
            }
        }
    }
    if old_focus != p.focused {
        p.focus_at = Some(Instant::now());
    }
    None
}

/// Passwords bypass generic page bodies, frame caches, report buffers and logs.
/// The resize renderer is suspended until this private display is dismissed.
/// The secret is written directly from the caller's zeroized byte buffer.
pub fn private_view(lang: Lang, title: &str, secret: &[u8], note: &str) -> Result<()> {
    struct Resume(Arc<Mutex<Surface>>);
    impl Drop for Resume {
        fn drop(&mut self) {
            let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            state.suspended = false;
            state.page = Page::default();
            state.last_size = (0, 0); // The direct secret overlay is intentionally not cached.
            paint(&mut state);
        }
    }
    let state = SURFACE
        .with(|slot| slot.borrow().clone())
        .ok_or_else(|| io::Error::other("Interactive console unavailable"))?;
    let _resume = Resume(state.clone());
    loop {
        {
            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
            s.suspended = false;
            s.page = Page {
                title: title.into(),
                body: format!("\n\n{note}"),
                options: vec![lang.t("Back")],
                document: true,
                kind: PageKind::Document,
                ..Page::default()
            };
            let (rows, columns) = Term::stderr().size();
            let header = s.header.clone();
            let sections = s.sections.clone();
            let appearance = s.appearance;
            let frame = layout(
                lang,
                &header,
                &sections,
                &mut s.page,
                (rows, columns),
                appearance,
                Instant::now(),
            );
            paint(&mut s);
            s.suspended = true;
            let title_rows = if measure_text_width(&crate::ui::safe(title)) > frame.width {
                wrap_text(title, frame.width).len()
            } else {
                0
            };
            if check_size(rows, columns)
                && secret.len() <= frame.width
                && frame.body_capacity >= title_rows + 2
            {
                let mut out = io::stderr();
                write!(
                    out,
                    "\x1b[{};{}H\x1b[K",
                    frame.body_start + title_rows + 1,
                    frame.left + 1
                )?;
                out.write_all(secret)?;
                out.flush()?;
            } else {
                write!(
                    io::stderr(),
                    "\x1b[{};{}H\x1b[K{}",
                    frame.body_start + 1,
                    frame.left + 1,
                    fit(
                        &lang.t("Enlarge the terminal to continue. Esc goes back."),
                        frame.width
                    )
                )?;
            }
        }
        match read_key_raw()? {
            Key::Enter | Key::Escape | Key::Char('q') => return Ok(()),
            Key::CtrlC => {
                INTERRUPTED.store(true, Ordering::SeqCst);
                return Ok(());
            }
            _ => {} // A key after resize redraws without keeping a secret copy.
        }
    }
}

fn enter_choice(selected: &mut usize, count: usize, key: &Key) -> Option<Option<usize>> {
    match key {
        Key::Enter => return Some(Some(*selected)),
        Key::Escape | Key::CtrlC | Key::Char('q') => return Some(None),
        Key::ArrowUp => *selected = (*selected + count - 1) % count,
        Key::ArrowDown => *selected = (*selected + 1) % count,
        _ => {} // Space never approves.
    }
    None
}

impl TerminalMenu {
    fn interact(
        &mut self,
        lang: Lang,
        items: &[String],
        default: usize,
        checked: Option<Vec<bool>>,
        consent: bool,
        document: bool,
    ) -> Result<Option<Vec<usize>>> {
        if self.cancelled {
            return Ok(None);
        }
        let _screen = Screen::enter(lang, true)?;
        ensure!(
            !items.is_empty() && default < items.len(),
            "Invalid menu default"
        );
        let multiple = checked.is_some();
        with_surface(|s| {
            let helpers = std::mem::take(&mut s.pending_home_helpers);
            s.page = Page {
                title: std::mem::take(&mut s.pending_title),
                body: std::mem::take(&mut s.pending),
                options: items.to_vec(),
                focused: default,
                checked,
                consent,
                document,
                kind: if consent {
                    PageKind::Consent
                } else if document {
                    PageKind::Document
                } else if multiple {
                    PageKind::Checklist
                } else if !helpers.is_empty() {
                    PageKind::Home
                } else {
                    PageKind::Menu
                },
                helpers,
                focus_at: Some(Instant::now()),
                role: std::mem::take(&mut s.pending_role),
                ..Page::default()
            };
            paint(s);
        });
        loop {
            if INTERRUPTED.load(Ordering::SeqCst) {
                self.cancelled = true;
                return Ok(None);
            }
            if let Some(error) = with_surface(|s| s.error.clone()).flatten() {
                return Err(io::Error::other(error).into());
            }
            let before_read = with_surface(|s| s.page.review.rendered.clone()).flatten();
            let key = match read_key_raw() {
                Ok(key) => key,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::UnexpectedEof
                    ) =>
                {
                    self.cancelled = true;
                    return Ok(None);
                }
                Err(error) => return Err(error.into()),
            };
            if key == Key::CtrlC {
                self.cancelled = true;
            }
            let answer = with_surface(|s| {
                let answer = handle_key(
                    s,
                    &key,
                    before_read.as_ref(),
                    Term::stderr().size(),
                    RESIZE_EPOCH.load(Ordering::SeqCst),
                );
                if answer.is_none() {
                    paint(s);
                }
                answer
            })
            .flatten();
            if let Some(answer) = answer {
                return Ok(answer);
            }
        }
    }
}
impl ChoiceInput for TerminalMenu {
    fn select(
        &mut self,
        lang: Lang,
        items: &[String],
        default: usize,
        enter_only: bool,
    ) -> Result<Option<usize>> {
        Ok(self
            .interact(lang, items, default, None, enter_only, false)?
            .map(|v| v[0]))
    }
    fn multi_select(&mut self, lang: Lang, items: &[String]) -> Result<Option<Vec<usize>>> {
        self.multi_select_with_defaults(lang, items, &vec![false; items.len()])
    }
    fn multi_select_with_defaults(
        &mut self,
        lang: Lang,
        items: &[String],
        defaults: &[bool],
    ) -> Result<Option<Vec<usize>>> {
        ensure!(defaults.len() == items.len(), "Invalid menu default");
        if items.is_empty() {
            return Ok(Some(Vec::new()));
        }
        self.interact(lang, items, 0, Some(defaults.to_vec()), false, false)
    }
    fn view(&mut self, lang: Lang, title: &str, text: &str) -> Result<()> {
        if self.cancelled {
            return Ok(());
        }
        let _screen = Screen::enter(lang, true)?;
        screen_content(text)?;
        screen_title(title);
        let _ = self.interact(lang, &[lang.t("Back")], 0, None, false, true)?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn section_path_for_test() -> Vec<Section> {
    with_surface(|s| s.sections.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn still() -> Appearance {
        Appearance {
            colors: ColorMode::Basic16,
            animate: false,
        }
    }
    fn test_layout(
        lang: Lang,
        header: &Header,
        page: &mut Page,
        rows: u16,
        columns: u16,
    ) -> Layout {
        layout(
            lang,
            header,
            &[],
            page,
            (rows, columns),
            still(),
            Instant::now(),
        )
    }
    fn home_fixture() -> (Header, Page) {
        (
            Header {
                subtitle: "Last protection check".into(),
                badges: vec![
                    Badge {
                        text: "8 protected".into(),
                        role: Role::Healthy,
                    },
                    Badge {
                        text: "2 fixes".into(),
                        role: Role::Review,
                    },
                    Badge {
                        text: "1 unknown".into(),
                        role: Role::Unknown,
                    },
                ],
                tally: Some((8, 11)),
            },
            Page {
                title: "Your next step".into(),
                kind: PageKind::Home,
                options: [
                    "Fix recommended",
                    "Review and choose fixes",
                    "Check again",
                    "Advanced",
                    "Exit",
                ]
                .map(String::from)
                .to_vec(),
                helpers: [
                    "Review the recommended set before anything changes.",
                    "Choose individual fixes and review your exact selection.",
                    "Refresh the read-only protection check.",
                    "Undo, maintenance, diagnostics and specialist tools.",
                    "Return to your terminal.",
                ]
                .map(String::from)
                .to_vec(),
                ..Page::default()
            },
        )
    }
    fn normalized(frame: &Layout) -> Vec<String> {
        frame
            .lines
            .iter()
            .map(|line| {
                let line = line.replace(&format!("v{}", env!("CARGO_PKG_VERSION")), "vTEST");
                if let Some(prefix) = line.strip_suffix('❩') {
                    format!("{} ❩", prefix.trim_end())
                } else {
                    line.trim_end().to_owned()
                }
            })
            .collect()
    }
    #[test]
    fn home_snapshots_are_quiet_centered_and_fit_wide_and_narrow_terminals() {
        let (header, mut page) = home_fixture();
        let wide = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            still(),
            Instant::now(),
        );
        let expected: Vec<String> = [
            "",
            "  Secblitz  vTEST",
            "  ╭─ Last protection check ─────────────────────────────────────────────────╮",
            "  │ ✓ 8 protected   ! 2 fixes   ? 1 unknown                                 │",
            "  │ ███████████████░░░░░  8 of 11 checks protected                          │",
            "  ╰─────────────────────────────────────────────────────────────────────────╯",
            "",
            "  Overview › Your next step",
            "",
            "    › ✦ Fix recommended",
            "",
            "      ☰ Review and choose fixes",
            "",
            "      ↻ Check again",
            "",
            "      ⋯ Advanced",
            "",
            "      × Exit",
            "",
            "    ┊ Review the recommended set before anything changes.",
            "",
            "",
            "  ↑/↓ · Enter · Esc",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(normalized(&wide), expected);
        // Focused option spans the full content width (left=2, content=75 → total 77).
        assert_eq!(measure_text_width(&wide.lines[9]), 77);
        let narrow = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (16, 40),
            still(),
            Instant::now(),
        );
        let mut expected = [
            "  Secblitz  vTEST",
            "  Last protection check",
            "  ✓ 8 protected   ! 2 fixes",
            "  ? 1 unknown",
        ]
        .map(String::from)
        .to_vec();
        expected.push(format!("  {}", "─".repeat(35)));
        expected.extend(
            [
                "  Overview › Your next step",
                "    › ✦ Fix recommended",
                "      ☰ Review and choose fixes",
                "      ↻ Check again",
                "      ⋯ Advanced",
                "      × Exit",
                "    ┊ Review the recommended set",
                "    ┊ before anything changes.",
                "",
                "  ↑/↓ · Enter · Esc",
            ]
            .map(String::from),
        );
        assert_eq!(normalized(&narrow), expected);
        let large = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (30, 140),
            still(),
            Instant::now(),
        );
        assert_eq!(large.width, 92);
        assert_eq!(large.left, 23);
        assert!(large
            .rows
            .iter()
            .all(|r| r.plain().chars().all(|c| !c.is_control())));
    }
    #[test]
    fn palette_snapshots_use_typed_roles_and_default_background() {
        let unknown = Row::text(2, "Everything is healthy", Role::Unknown);
        assert_eq!(
            unknown.encoded(still()),
            "  \x1b[0;39mEverything is healthy\x1b[0m"
        );
        let rich = Appearance {
            colors: ColorMode::Ansi256,
            animate: true,
        };
        assert_eq!(
            [
                Role::Healthy,
                Role::Review,
                Role::Failure,
                Role::Unknown,
                Role::FocusAccent
            ]
            .map(|r| rich.style(r)),
            [
                "\x1b[0;38;5;78m",
                "\x1b[0;38;5;221m",
                "\x1b[1;38;5;203m",
                "\x1b[0;38;5;245m",
                "\x1b[7;1m"
            ]
        );
        assert_eq!(
            [Role::Healthy, Role::Review, Role::Failure].map(|r| still().style(r)),
            ["\x1b[0;32m", "\x1b[0;33m", "\x1b[1;31m"]
        );
        let (header, mut page) = home_fixture();
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            rich,
            Instant::now(),
        );
        let encoded = frame
            .rows
            .iter()
            .map(|r| r.encoded(rich))
            .collect::<String>();
        assert!(!encoded.contains("48;"));
        let plain = Appearance {
            colors: ColorMode::None,
            animate: false,
        };
        assert!(frame
            .rows
            .iter()
            .all(|r| !r.encoded(plain).contains('\x1b')));
        assert!(frame.lines.join("\n").contains("? 1 unknown"));
    }
    #[test]
    fn preferences_honor_no_color_dumb_reduced_motion_and_basic16() {
        assert_eq!(
            Appearance::resolve(true, true, true, "xterm-256color", true, false).colors,
            ColorMode::None
        );
        let dumb = Appearance::resolve(true, true, false, "dumb", true, false);
        assert_eq!(dumb.colors, ColorMode::None);
        assert!(!dumb.animate);
        assert!(!Appearance::resolve(false, true, false, "xterm-256color", true, false).animate);
        assert!(!Appearance::resolve(true, false, false, "xterm-256color", true, false).animate);
        assert_eq!(
            Appearance::resolve(true, true, false, "xterm-256color", true, true).colors,
            ColorMode::Basic16
        );
        assert_eq!(
            Appearance::resolve(true, true, false, "", true, false).colors,
            ColorMode::Basic16
        );
        let capture = Capture(Arc::new(Mutex::new(Vec::new())));
        let bytes = capture.0.clone();
        drop(Alternate::enter(capture, false).unwrap());
        assert!(!String::from_utf8(bytes.lock().unwrap().clone())
            .unwrap()
            .contains("[0m"));
    }
    #[test]
    fn animation_snapshots_are_optional_local_and_never_gate_input() {
        let now = Instant::now();
        let animated = Appearance {
            colors: ColorMode::Basic16,
            animate: true,
        };
        assert_eq!(activity_text(Lang::En, now, animated, now), "⠋ Working  0s");
        assert_eq!(
            activity_text(Lang::En, now, animated, now + Duration::from_millis(200)),
            "⠹ Working  0s"
        );
        assert_eq!(
            activity_text(Lang::En, now, still(), now + Duration::from_secs(900)),
            "· Working"
        );
        let (header, mut page) = home_fixture();
        page.focus_at = Some(now);
        let hot = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            animated,
            now,
        );
        let settled = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            animated,
            now + Duration::from_millis(150),
        );
        assert_eq!(hot.lines, settled.lines);
        let a = hot
            .rows
            .iter()
            .map(|r| r.encoded(animated))
            .collect::<Vec<_>>();
        let b = settled
            .rows
            .iter()
            .map(|r| r.encoded(animated))
            .collect::<Vec<_>>();
        assert_eq!(
            row_diff(&a, &b, (24, 80), (24, 80)).matches("[2K").count(),
            1
        );
        assert_eq!(enter_choice(&mut 0, 5, &Key::Enter), Some(Some(0)));
        let mut activity = Page {
            title: "Maintenance progress".into(),
            body: "Verifying selected settings".into(),
            activity: Some(now),
            kind: PageKind::Activity,
            ..Page::default()
        };
        let first = layout(
            Lang::En,
            &header,
            &[],
            &mut activity,
            (24, 80),
            animated,
            now,
        );
        let second = layout(
            Lang::En,
            &header,
            &[],
            &mut activity,
            (24, 80),
            animated,
            now + Duration::from_millis(200),
        );
        let a = first
            .rows
            .iter()
            .map(|r| r.encoded(animated))
            .collect::<Vec<_>>();
        let b = second
            .rows
            .iter()
            .map(|r| r.encoded(animated))
            .collect::<Vec<_>>();
        let diff = row_diff(&a, &b, (24, 80), (24, 80));
        assert_eq!(diff.matches("[2K").count(), 1);
        assert!(!diff.contains("[2J"));
        let first = layout(
            Lang::En,
            &header,
            &[],
            &mut activity,
            (24, 80),
            still(),
            now,
        );
        let later = layout(
            Lang::En,
            &header,
            &[],
            &mut activity,
            (24, 80),
            still(),
            now + Duration::from_secs(900),
        );
        assert_eq!(first.lines, later.lines);
    }
    #[test]
    fn differential_focus_and_resize_remove_stale_rows_without_tick_flashing() {
        let (header, mut page) = home_fixture();
        let a = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            still(),
            Instant::now(),
        )
        .rows
        .iter()
        .map(|r| r.encoded(still()))
        .collect::<Vec<_>>();
        assert!(row_diff(&a, &a, (24, 80), (24, 80)).is_empty());
        page.focused = 1;
        let b = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            still(),
            Instant::now(),
        )
        .rows
        .iter()
        .map(|r| r.encoded(still()))
        .collect::<Vec<_>>();
        let diff = row_diff(&a, &b, (24, 80), (24, 80));
        assert!((2..=4).contains(&diff.matches("[2K").count()));
        assert!(!diff.contains("[2J") && !diff.contains("\x1b[J"));
        let smaller = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (16, 40),
            still(),
            Instant::now(),
        )
        .rows
        .iter()
        .map(|r| r.encoded(still()))
        .collect::<Vec<_>>();
        assert_eq!(
            row_diff(&b, &smaller, (24, 80), (16, 40))
                .matches("[2K")
                .count(),
            15
        );
        assert_eq!(
            row_diff(&smaller, &b, (16, 40), (24, 80))
                .matches("[2K")
                .count(),
            23
        );
    }
    #[test]
    fn visual_frame_samples() {
        let (header, mut page) = home_fixture();
        for size in [(24, 80), (16, 40)] {
            let frame = layout(
                Lang::En,
                &header,
                &[Section::Overview],
                &mut page,
                size,
                still(),
                Instant::now(),
            );
            println!(
                "FRAME {}x{} (presentation fixture)\n{}\nEND FRAME",
                size.1,
                size.0,
                frame.lines.join("\n")
            );
        }
    }
    #[test]
    fn confirmation_requires_enter_and_never_space_or_typed_yes() {
        let mut selected = 1;
        assert_eq!(enter_choice(&mut selected, 2, &Key::Enter), Some(Some(1)));
        enter_choice(&mut selected, 2, &Key::ArrowUp);
        for key in [Key::Char(' '), Key::Char('y'), Key::Char('1'), Key::Unknown] {
            assert_eq!(enter_choice(&mut selected, 2, &key), None);
        }
        assert_eq!(enter_choice(&mut selected, 2, &Key::Enter), Some(Some(0)));
        assert_eq!(enter_choice(&mut selected, 2, &Key::Escape), Some(None));
    }
    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn screen_restores_on_normal_exit_error_and_panic() {
        for mode in 0..3 {
            let bytes = Arc::new(Mutex::new(Vec::new()));
            let capture = Capture(bytes.clone());
            let _ = std::panic::catch_unwind(|| -> io::Result<()> {
                let _guard = Alternate::enter(capture, true)?;
                match mode {
                    1 => Err(io::ErrorKind::BrokenPipe.into()),
                    2 => panic!("fixture"),
                    _ => Ok(()),
                }
            });
            let bytes = bytes.lock().unwrap();
            assert!(bytes.starts_with(ENTER_SCREEN));
            assert!(bytes.ends_with(LEAVE_SCREEN));
        }
    }
    #[test]
    fn redraw_fits_24_rows_unicode_and_all_languages_after_resize() {
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let mut p = Page {
                title: lang.t("Review and choose fixes"),
                body: "\x1b[31mUntrusted\r\u{202e} text\n界é🙂 ".repeat(50),
                options: vec!["界é🙂 ".repeat(40); 30],
                focused: 17,
                checked: Some(vec![false; 30]),
                ..Page::default()
            };
            for (rows, columns) in [(24, 80), (24, 40), (30, 120), (10, 24), (3, 6), (24, 80)] {
                let frame = test_layout(
                    lang,
                    &Header::message(lang.t("Your PC, checked."), Role::Text),
                    &mut p,
                    rows,
                    columns,
                );
                assert!(frame.lines.len() < rows as usize);
                for line in frame.lines {
                    assert!(measure_text_width(&line) < columns as usize);
                    assert!(!line.contains(['\x1b', '\r', '\u{202e}']));
                }
            }
        }
    }
    #[test]
    fn every_recap_page_must_be_visible_before_approval_and_resize_restarts_review() {
        let p = Page {
            title: "Recap".into(),
            body: (0..80).map(|i| format!("Exact control {i}\n")).collect(),
            options: vec!["Apply".into(), "Change".into(), "Back".into()],
            focused: 0,
            consent: true,
            ..Page::default()
        };
        let mut s = fixture_surface(p);
        let first = test_layout(Lang::En, &Header::default(), &mut s.page, 24, 80);
        assert!(!first.approvable);
        s.page.offset = 70;
        assert!(!test_layout(Lang::En, &Header::default(), &mut s.page, 24, 80).approvable);
        assert_eq!(s.page.review.through, 0);
        emit(&mut s, (24, 80), 0);
        assert_eq!(s.page.review.through, 0); // Displaying only the end skips a gap.
        s.page.offset = 0;
        loop {
            emit(&mut s, (24, 80), 0);
            let frame = test_layout(Lang::En, &Header::default(), &mut s.page, 24, 80);
            if frame.approvable {
                break;
            }
            s.page.offset += frame.body_capacity;
        }
        assert!(!test_layout(Lang::En, &Header::default(), &mut s.page, 24, 40).approvable);
        let before = s.page.review.rendered.clone();
        assert_eq!(
            handle_key(&mut s, &Key::Enter, before.as_ref(), (80, 80), 1),
            None
        );
        assert_eq!(s.page.review.through, 0);
    }

    fn fixture_surface(page: Page) -> Surface {
        Surface {
            lang: Lang::En,
            header: Header::default(),
            appearance: still(),
            sections: vec![],
            pending: String::new(),
            pending_title: String::new(),
            pending_home_helpers: vec![],
            pending_role: Role::Text,
            page,
            last_rows: vec![],
            last_size: (0, 0),
            last_epoch: None,
            error: None,
            suspended: false,
        }
    }
    fn recap() -> Page {
        Page {
            title: "Recap".into(),
            body: (0..20).map(|i| format!("Exact control {i}\n")).collect(),
            options: vec!["Apply".into(), "Back".into()],
            focused: 0,
            consent: true,
            kind: PageKind::Consent,
            ..Page::default()
        }
    }
    fn emit(s: &mut Surface, size: (u16, u16), epoch: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        paint_to(s, size, epoch, &mut bytes, || (size, epoch)).unwrap();
        bytes
    }
    #[test]
    fn layout_and_resize_enter_cannot_credit_unemitted_lines() {
        let mut s = fixture_surface(recap());
        emit(&mut s, (3, 20), 0);
        let before = s.page.review.rendered.clone();
        for _ in 0..5 {
            let frame = test_layout(Lang::En, &Header::default(), &mut s.page, 80, 80);
            assert!(frame
                .lines
                .iter()
                .any(|line| line.contains("Exact control 19")));
            assert!(!frame.approvable);
            assert_eq!(s.page.review.through, 0);
        }
        assert_eq!(
            handle_key(&mut s, &Key::Enter, before.as_ref(), (80, 80), 1),
            None
        );
        assert_eq!(s.page.review.through, 0);
        let output = emit(&mut s, (80, 80), 1);
        assert!(String::from_utf8(output)
            .unwrap()
            .contains("Exact control 19"));
        let before = s.page.review.rendered.clone();
        assert_eq!(
            handle_key(&mut s, &Key::Enter, before.as_ref(), (80, 80), 1),
            Some(Some(vec![0]))
        );
    }
    #[test]
    fn background_resize_render_still_requires_a_new_confirmation() {
        let mut s = fixture_surface(recap());
        emit(&mut s, (3, 20), 0);
        emit(&mut s, (80, 80), 1);
        let before = s.page.review.rendered.clone();
        assert_eq!(
            handle_key(&mut s, &Key::Enter, before.as_ref(), (80, 80), 1),
            None
        );
        emit(&mut s, (80, 80), 1);
        let before = s.page.review.rendered.clone();
        assert_eq!(
            handle_key(&mut s, &Key::Enter, before.as_ref(), (80, 80), 1),
            Some(Some(vec![0]))
        );
    }
    #[test]
    fn resize_round_trip_repaints_recap_before_crediting_review() {
        let mut s = fixture_surface(recap());
        emit(&mut s, (80, 80), 0);
        assert!(emit(&mut s, (80, 80), 0).is_empty());
        let before = s.page.review.rendered.clone();
        // Two resize events, but no paint at the intermediate tiny geometry.
        // The input handler sees the final size first and invalidates Review.
        assert_eq!(
            handle_key(&mut s, &Key::Enter, before.as_ref(), (80, 80), 2),
            None
        );
        assert_eq!(s.page.review.through, 0);
        let output = String::from_utf8(emit(&mut s, (80, 80), 2)).unwrap();
        assert!(output.contains("Exact control 0"));
        assert!(output.contains("Exact control 19"));
        assert_eq!(output.matches("[2K").count(), 79);
        let before = s.page.review.rendered.clone();
        assert_eq!(
            handle_key(&mut s, &Key::Enter, before.as_ref(), (80, 80), 2),
            Some(Some(vec![0]))
        );
        // Subsequent unchanged frames still use differential output.
        assert!(emit(&mut s, (80, 80), 2).is_empty());
    }
    #[test]
    fn write_flush_failure_and_resize_during_flush_invalidate_review() {
        struct Broken {
            partial: bool,
            flush: bool,
        }
        impl Write for Broken {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.flush {
                    return Ok(bytes.len());
                }
                if self.partial {
                    self.partial = false;
                    return Ok(bytes.len().min(12));
                }
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::ErrorKind::Other.into())
            }
        }
        for (partial, flush) in [(false, false), (true, false), (false, true)] {
            let mut s = fixture_surface(recap());
            emit(&mut s, (80, 80), 0);
            s.page.focused = 1; // A differential frame can fail too.
            assert!(
                paint_to(&mut s, (80, 80), 0, &mut Broken { partial, flush }, || (
                    (80, 80),
                    0
                ))
                .is_err()
            );
            assert_eq!(s.page.review.through, 0);
            assert!(s.page.review.rendered.is_none());
        }
        let mut s = fixture_surface(recap());
        paint_to(&mut s, (80, 80), 0, &mut Vec::new(), || ((3, 20), 1)).unwrap();
        assert!(s.page.review.rendered.is_none());
        assert_eq!(s.page.review.through, 0);
    }
    #[test]
    fn clipping_offscreen_content_and_stale_key_receipts_never_authorize() {
        let mut s = fixture_surface(recap());
        emit(&mut s, (80, 80), 0);
        let before = s.page.review.rendered.clone();
        s.page.body.push_str("Unseen replacement target\n");
        assert_eq!(
            handle_key(&mut s, &Key::Enter, before.as_ref(), (80, 80), 0),
            None
        );
        assert!(!test_layout(Lang::En, &Header::default(), &mut s.page, 80, 80).approvable);
        let mut s = fixture_surface(recap());
        s.page.body = "short".into();
        s.page.options[0] = "Long apply label ".repeat(20);
        emit(&mut s, (24, 40), 0);
        assert!(s.page.review.rendered.is_none());
        let mut s = fixture_surface(recap());
        emit(&mut s, (80, 80), 0);
        assert_eq!(handle_key(&mut s, &Key::Enter, None, (80, 80), 0), None);
    }
    #[test]
    fn native_virtual_key_mapping_includes_page_keys_and_never_steals_next_key() {
        for (vk, expected) in [
            (0x21, Key::PageUp),
            (0x22, Key::PageDown),
            (0x24, Key::Home),
            (0x23, Key::End),
            (0x26, Key::ArrowUp),
            (0x28, Key::ArrowDown),
            (0x0d, Key::Enter),
            (0x1b, Key::Escape),
        ] {
            assert_eq!(
                native_key(NativeKey {
                    down: 1,
                    virtual_key: vk,
                    ..NativeKey::default()
                }),
                Some(expected)
            );
        }
        assert_eq!(
            native_key(NativeKey {
                down: 0,
                virtual_key: 0x22,
                ..NativeKey::default()
            }),
            None
        );
        assert_eq!(
            native_key(NativeKey {
                down: 1,
                unicode: 32,
                ..NativeKey::default()
            }),
            Some(Key::Char(' '))
        );
        assert_eq!(
            native_key(NativeKey {
                down: 1,
                unicode: 3,
                ..NativeKey::default()
            }),
            Some(Key::CtrlC)
        );
        assert_eq!(
            native_key(NativeKey {
                down: 1,
                unicode: 0xd800,
                ..NativeKey::default()
            }),
            Some(Key::Unknown)
        );
        assert_eq!(
            native_key(NativeKey {
                down: 1,
                virtual_key: 0x0d,
                ..NativeKey::default()
            }),
            Some(Key::Enter)
        );
    }
    #[test]
    fn redirected_sessions_never_enter_alternate_screen() {
        use std::io::IsTerminal;
        if io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal() {
            return;
        }
        assert!(Screen::enter(Lang::En, false).is_err());
        assert!(!screen_active());
        assert!(!screen_note("no terminal").unwrap());
    }
    #[test]
    #[ignore = "driven by the Linux PTY test or an attended terminal"]
    fn native_scene_probe() {
        let _screen = Screen::enter(Lang::En, false).unwrap();
        screen_header(Header::message(
            "PTY status remains visible".into(),
            Role::Text,
        ));
        let mut menu = TerminalMenu::default();
        screen_title("PTY checklist");
        let labels = (0..20)
            .map(|i| format!("Control {i} 界 é {}", "long ".repeat(20)))
            .collect::<Vec<_>>();
        assert_eq!(
            menu.multi_select(Lang::En, &labels).unwrap(),
            Some(vec![0, 1])
        );
        screen_content("Exact recap: Control 0 and Control 1\nNo other controls.").unwrap();
        screen_title("PTY confirmation");
        assert_eq!(
            menu.select(
                Lang::En,
                &["Apply".into(), "Change".into(), "Back".into()],
                2,
                true
            )
            .unwrap(),
            Some(2)
        );
        screen_title("PTY keep selection");
        assert_eq!(
            menu.multi_select_with_defaults(Lang::En, &labels[..3], &[true, true, false])
                .unwrap(),
            Some(vec![1, 2])
        );
        menu.view(
            Lang::En,
            "PTY results",
            "Partial changes recorded.\nPost-check failed separately.",
        )
        .unwrap();
        private_view(
            Lang::En,
            "PTY private view",
            b"abcdefghijklmnopqrstuvwx",
            "Private fixture display",
        )
        .unwrap();
        with_surface(|s| {
            assert!(!s.page.body.contains("abcdefghijkl"));
            assert!(!s.last_rows.iter().any(|row| row.contains("abcdefghijkl")));
        });
        screen_title("PTY escaped");
        assert_eq!(
            menu.select(Lang::En, &["Back".into()], 0, false).unwrap(),
            None
        );
        screen_content(
            &(0..40)
                .map(|i| format!("Exact named control {i}\n"))
                .collect::<String>(),
        )
        .unwrap();
        screen_title("PTY long recap");
        assert_eq!(
            menu.select(Lang::En, &["Apply".into(), "Back".into()], 1, true)
                .unwrap(),
            Some(0)
        );
        screen_title("PTY resize");
        assert_eq!(
            menu.select(Lang::En, &["Choose".into(), "Back".into()], 1, true)
                .unwrap(),
            Some(1)
        );
    }
    #[test]
    #[ignore = "driven by the Linux PTY test"]
    fn native_panic_probe() {
        let result = std::panic::catch_unwind(|| {
            let _screen = Screen::enter(Lang::En, false).unwrap();
            panic!("PTY deliberate unwind");
        });
        assert!(result.is_err());
        assert!(!screen_active());
    }
    #[test]
    #[ignore = "driven by the Linux PTY test"]
    fn native_error_probe() {
        let result = (|| -> Result<()> {
            let _screen = Screen::enter(Lang::En, false)?;
            Err(io::Error::from(io::ErrorKind::BrokenPipe).into())
        })();
        assert!(result.is_err());
        assert!(!screen_active());
    }
    #[test]
    #[ignore = "driven by the Linux PTY test"]
    fn native_cancel_probe() {
        let _screen = Screen::enter(Lang::En, false).unwrap();
        screen_title("PTY cancel");
        let mut menu = TerminalMenu::default();
        assert_eq!(
            menu.select(Lang::En, &["Back".into()], 0, false).unwrap(),
            None
        );
        assert!(menu.cancelled);
    }
    #[test]
    #[ignore = "driven by the Linux PTY test"]
    fn native_no_animation_probe() {
        let matches = crate::command(Lang::En)
            .try_get_matches_from(["secblitz", "diagnostics", "guide", "--no-animation"])
            .unwrap();
        let _screen = Screen::enter(Lang::En, !matches.get_flag("no-animation")).unwrap();
        screen_progress("PTY static activity", "Waiting for the fixture");
        let before = with_surface(|s| {
            assert!(!s.appearance.animate);
            s.last_rows.clone()
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(350));
        with_surface(|s| assert_eq!(s.last_rows, before));
        screen_title("PTY static done");
        let mut menu = TerminalMenu::default();
        assert_eq!(
            menu.select(Lang::En, &["Back".into()], 0, false).unwrap(),
            Some(0)
        );
    }
    #[test]
    #[ignore = "native driver: tiny, then 80x80 plus immediate Enter; no operations"]
    fn native_resize_consent_probe() {
        let mut screen = Screen::enter(Lang::En, false).unwrap();
        // Deterministically reproduce the race: the foreground key handler is
        // the first code to see the enlarged geometry, not the timer renderer.
        screen.stop.as_ref().unwrap().store(true, Ordering::SeqCst);
        screen.worker.take().unwrap().join().unwrap();
        screen_content(&format!(
            "{}LAST-RECAP-LINE\n",
            (0..30)
                .map(|n| format!("Exact reviewed control {n}\n"))
                .collect::<String>()
        ))
        .unwrap();
        screen_title("RESIZE-CONSENT");
        let mut menu = TerminalMenu::default();
        assert_eq!(
            menu.select(Lang::En, &["Apply".into(), "Back".into()], 0, true)
                .unwrap(),
            Some(0)
        );
        with_surface(|s| {
            assert!(s
                .last_rows
                .iter()
                .any(|row| row.contains("LAST-RECAP-LINE")));
            let receipt = s.page.review.rendered.as_ref().unwrap();
            assert_eq!(receipt.geometry.size, (80, 80));
            assert_eq!(s.page.review.through, receipt.geometry.total);
        });
        screen_title("RESIZE-APPROVED");
        assert_eq!(
            menu.select(Lang::En, &["Back".into()], 0, false).unwrap(),
            Some(0)
        );
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "Windows native driver: exact virtual keys after KEY-* markers; no operations"]
    fn native_windows_mode_and_page_keys() {
        fn capture() -> [SavedMode; 3] {
            [-10i32, -11, -12]
                .map(|id| SavedMode::capture(unsafe { GetStdHandle(id as u32) }).unwrap())
        }
        struct Restore([SavedMode; 3]);
        impl Drop for Restore {
            fn drop(&mut self) {
                for m in self.0.iter().rev() {
                    m.restore();
                }
            }
        }
        crate::guided::require_terminal(Lang::En).unwrap();
        let original = Restore(capture());
        for m in &original.0[1..] {
            assert_ne!(unsafe { SetConsoleMode(m.handle, m.flags & !4) }, 0);
        }
        let disabled = capture().map(|m| m.flags);
        // Exercise the real pre-Screen path, including helpers which used to
        // invoke console's mutating color probes and lazy style initialization.
        let _ = crate::command(Lang::En)
            .try_get_matches_from(["secblitz", "guide", "--no-animation"])
            .unwrap();
        let view = crate::ui::Ui::new(Lang::En, true, false);
        view.brand();
        view.report(&secblitz::engine::Report::default(), true)
            .unwrap();
        crate::ui::error(Lang::En, &anyhow::anyhow!("native-mode-fixture"));
        assert_eq!(capture().map(|m| m.flags), disabled);
        for failure in 0..3 {
            let result = std::panic::catch_unwind(|| -> Result<()> {
                let _screen = Screen::enter(Lang::En, false)?;
                assert_ne!(capture()[2].flags & 4, 0);
                if failure == 1 {
                    return Err(io::Error::from(io::ErrorKind::BrokenPipe).into());
                }
                if failure == 2 {
                    panic!("expected-native-mode-unwind");
                }
                Ok(())
            });
            assert_eq!(result.is_ok_and(|r| r.is_ok()), failure == 0);
            assert_eq!(capture().map(|m| m.flags), disabled);
        }
        {
            let _screen = Screen::enter(Lang::En, false).unwrap();
            assert_eq!(std::mem::size_of::<NativeKey>(), 16);
            assert_eq!(std::mem::size_of::<NativeInput>(), 20);
            for (marker, key) in [
                ("KEY-PGDN", Key::PageDown),
                ("KEY-PGUP", Key::PageUp),
                ("KEY-HOME", Key::Home),
                ("KEY-END", Key::End),
                ("KEY-RIGHT", Key::ArrowRight),
                ("KEY-LEFT", Key::ArrowLeft),
                ("KEY-ENTER", Key::Enter),
                ("KEY-ESC", Key::Escape),
                ("KEY-SPACE", Key::Char(' ')),
                ("KEY-CTRL-C", Key::CtrlC),
            ] {
                screen_progress(marker, "Send the named Win32 key event once.");
                let before = capture()[0].flags;
                let received = loop {
                    let value = read_key_raw().unwrap();
                    assert_eq!(capture()[0].flags, before);
                    if value != Key::Unknown {
                        break value;
                    }
                };
                assert_eq!(received, key, "{marker}");
            }
        }
        assert_eq!(capture().map(|m| m.flags), disabled);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_pty_navigation_resize_and_restoration() {
        // Only native keyboard/display code executes in the child, never any
        // engine, Windows operation, update, network request or elevation.
        let script = r#"
import os, pty, fcntl, termios, struct, subprocess, select, time, sys, re
exe=sys.argv[1]
def run(name, drive, extra=None, dimensions=(24,80)):
 master,slave=pty.openpty()
 original_modes=termios.tcgetattr(slave)
 def size(r,c): fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',r,c,0,0))
 size(*dimensions)
 def setup(): os.setsid(); fcntl.ioctl(slave,termios.TIOCSCTTY,0)
 env={**os.environ,'TERM':'xterm-256color'}; env.pop('NO_COLOR',None); env.update(extra or {})
 proc=subprocess.Popen([exe,'--ignored','--exact',name,'--nocapture'],stdin=slave,stdout=slave,stderr=slave,preexec_fn=setup,env=env)
 data=bytearray()
 def wait(token):
  deadline=time.monotonic()+10
  while token not in data:
   if time.monotonic()>deadline: raise AssertionError((token,bytes(data[-8000:])))
   if select.select([master],[],[],.1)[0]:
    try: data.extend(os.read(master,65536))
    except OSError: break
  assert token in data,(token,bytes(data[-8000:]))
 def key(k): os.write(master,k); time.sleep(.12)
 if drive == 'menus':
  wait(b'PTY checklist'); key(b' '); key(b'\x1b[B'); key(b' '); wait(b'Selected: 2 of 20'); key(b'\r')
  wait(b'PTY confirmation'); key(b' '); assert proc.poll() is None; key(b'\r')
  wait(b'PTY keep selection'); key(b' '); key(b'\x1b[B'); key(b'\x1b[B'); key(b' '); key(b'\r')
  wait(b'PTY results'); key(b'\r'); wait(b'PTY private view'); wait(b'abcdefghijklmnopqrstuvwx'); key(b'\r')
  wait(b'PTY escaped'); key(b'\x1b')
  wait(b'PTY long recap'); key(b'\x1b[A'); key(b'\r'); assert b'PTY resize' not in data
  for _ in range(12): key(b'\x1b[6~')
  key(b'\r')
  wait(b'PTY resize'); size(3,20); time.sleep(.3); wait(b'Enlarge'); key(b'\r'); assert proc.poll() is None
  size(24,40); time.sleep(.3); key(b'\r')
 elif drive == 'flow':
  wait(b'Your next step'); key(b'\x1b[B'); key(b'\r'); wait(b'Review and choose fixes')
  key(b' '); key(b'\x1b[B'); key(b' '); key(b'\r'); wait(b'Selected fixes: 2')
  for _ in range(6): key(b'\x1b[6~')
  key(b'\r') # Default Back, no mutation.
  key(b'\x1b[B'); key(b'\r'); key(b'\x1b[B'); key(b' '); key(b'\r'); wait(b'Selected fixes: 1')
  for _ in range(6): key(b'\x1b[6~')
  key(b'\x1b[A'); key(b'\x1b[A'); key(b'\r')
  wait(b'Applying selected fixes'); wait(b'Checking after your changes'); wait(b'Fix results'); key(b'\r')
  for _ in range(4): key(b'\x1b[B')
  key(b'\r')
 elif drive == 'cancel':
  wait(b'PTY cancel'); key(b'\x03')
 elif drive == 'static':
  wait(b'PTY static done'); key(b'\r')
 elif drive == 'resize-consent':
  wait(b'RESIZE-CONSENT'); size(3,20); key(b'z'); wait(b'Enlarge')
  size(80,80); os.write(master,b'\r') # No sleep/repaint opportunity first.
  wait(b'LAST-RECAP-LINE')
  deadline=time.monotonic()+.25
  while time.monotonic()<deadline:
   if select.select([master],[],[],.03)[0]: data.extend(os.read(master,65536))
  assert b'RESIZE-APPROVED' not in data,bytes(data[-6000:])
  key(b'\r'); wait(b'RESIZE-APPROVED'); key(b'\r')
 elif drive == 'navigation':
  for step,down in [(0,3),(1,5),(2,None),(3,4),(4,4),(5,4),(6,None),(7,None),(8,4)]:
   wait(('NAV-'+str(step)).encode())
   if down is None: key(b'\x1b')
   else:
    for _ in range(down): key(b'\x1b[B')
    key(b'\r')
 deadline=time.monotonic()+10
 while proc.poll() is None and time.monotonic()<deadline:
  if select.select([master],[],[],.1)[0]:
   try: data.extend(os.read(master,65536))
   except OSError: break
 proc.wait(timeout=3)
 assert termios.tcgetattr(slave)==original_modes,'terminal input modes were not restored'
 while select.select([master],[],[],0)[0]:
  try: data.extend(os.read(master,65536))
  except OSError: break
 os.close(master); os.close(slave)
 assert proc.returncode==0,bytes(data[-8000:])
 assert data.count(b'\x1b[?1049h')==1 and data.count(b'\x1b[?1049l')==1,bytes(data[-8000:])
 assert b'\x1b[?25h' in data
 body=data.split(b'\x1b[?1049h',1)[1].split(b'\x1b[?1049l',1)[0]
 if 'NO_COLOR' in env or env['TERM']=='dumb':
  assert not re.search(rb'\x1b\[[0-9;]*m',body),'color sequences leaked in no-color mode'
 if drive in ('menus','flow','cancel'):
  assert b'\n' not in body,'interactive navigation appended scrolling output'
run('menu::tests::native_scene_probe','menus')
run('menu::tests::native_panic_probe',None)
run('menu::tests::native_error_probe',None)
run('menu::tests::native_cancel_probe','cancel')
run('guided::tests::native_guided_flow_probe','flow')
run('guided::tests::native_guided_flow_probe','flow',{'NO_COLOR':''},(16,40))
run('menu::tests::native_scene_probe','menus',{'TERM':'dumb'})
run('menu::tests::native_no_animation_probe','static')
run('menu::tests::native_resize_consent_probe','resize-consent')
run('guided::tests::native_navigation_probe','navigation')
"#;
        let output = std::process::Command::new("python3")
            .arg("-c")
            .arg(script)
            .arg(std::env::current_exe().unwrap())
            .output()
            .expect("Python 3 is required for the native PTY test");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    // ── Logo tests ────────────────────────────────────────────────────────────
    fn no_color() -> Appearance {
        Appearance {
            colors: ColorMode::None,
            animate: false,
        }
    }
    /// Returns true when `needle` appears verbatim in any of the layout lines.
    fn lines_contain(frame: &Layout, needle: &str) -> bool {
        frame.lines.iter().any(|l| l.contains(needle))
    }
    fn has_logo(frame: &Layout) -> bool {
        // The first logo row is the unique marker; all 6 rows should follow.
        frame.lines.iter().any(|l| l.trim_start() == LOGO_ROWS[0])
    }
    fn count_logo_rows(frame: &Layout) -> usize {
        // Count lines that match any LOGO_ROWS entry (trimmed from the left).
        frame
            .lines
            .iter()
            .filter(|l| LOGO_ROWS.iter().any(|lr| l.trim_start() == *lr))
            .count()
    }
    #[test]
    fn logo_shown_on_home_at_30x80() {
        let (header, mut page) = home_fixture();
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (30, 80),
            still(),
            Instant::now(),
        );
        let lines = normalized(&frame);
        // First non-empty line should be the first logo row (no "Secblitz" text).
        let first_content = lines.iter().find(|l| !l.trim().is_empty()).unwrap();
        assert!(
            first_content.trim_start() == LOGO_ROWS[0],
            "Expected first logo row, got: {first_content:?}"
        );
        // All 6 logo rows must be present.
        assert_eq!(count_logo_rows(&frame), 6, "Expected 6 logo rows");
        // Every row must fit within columns-1 (79).
        let cols = 80usize;
        for line in &frame.lines {
            let w = measure_text_width(line);
            assert!(w < cols, "Row width {w} >= {cols}: {line:?}");
        }
        // All menu choices must be visible.
        for choice in [
            "Fix recommended",
            "Review and choose fixes",
            "Check again",
            "Advanced",
            "Exit",
        ] {
            assert!(lines_contain(&frame, choice), "Missing menu choice: {choice}");
        }
    }
    #[test]
    fn logo_shown_on_home_at_40x120() {
        let (header, mut page) = home_fixture();
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (40, 120),
            still(),
            Instant::now(),
        );
        // Logo rows present.
        assert_eq!(count_logo_rows(&frame), 6, "Expected 6 logo rows");
        // Every row fits within columns-1 (119).
        let cols = 120usize;
        for line in &frame.lines {
            let w = measure_text_width(line);
            assert!(w < cols, "Row width {w} >= {cols}: {line:?}");
        }
        // All menu choices visible.
        for choice in [
            "Fix recommended",
            "Review and choose fixes",
            "Check again",
            "Advanced",
            "Exit",
        ] {
            assert!(lines_contain(&frame, choice), "Missing menu choice: {choice}");
        }
    }
    #[test]
    fn logo_absent_on_home_below_height_threshold_29x120() {
        let (header, mut page) = home_fixture();
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (29, 120),
            still(),
            Instant::now(),
        );
        let lines = normalized(&frame);
        // No logo rows.
        assert!(!has_logo(&frame), "Logo must not appear at height 29");
        // Compact header present.
        assert!(
            lines.iter().any(|l| l.contains("Secblitz")),
            "Expected compact Secblitz header"
        );
    }
    #[test]
    fn logo_absent_on_home_below_width_threshold_40x60() {
        let (header, mut page) = home_fixture();
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (40, 60),
            still(),
            Instant::now(),
        );
        let lines = normalized(&frame);
        // No logo rows.
        assert!(!has_logo(&frame), "Logo must not appear at width 60");
        // Compact header present.
        assert!(
            lines.iter().any(|l| l.contains("Secblitz")),
            "Expected compact Secblitz header"
        );
    }
    #[test]
    fn logo_absent_on_inner_section_page() {
        // An inner screen (kind != Home) must never show the logo,
        // even at a large size.
        let (header, _) = home_fixture();
        let mut page = Page {
            title: "Review".into(),
            kind: PageKind::Menu,
            options: vec!["Apply".into(), "Back".into()],
            ..Page::default()
        };
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview, Section::Review],
            &mut page,
            (40, 120),
            still(),
            Instant::now(),
        );
        let lines = normalized(&frame);
        assert!(!has_logo(&frame), "Logo must not appear on inner (non-Home) pages");
        assert!(
            lines.iter().any(|l| l.contains("Secblitz")),
            "Expected compact header on inner page"
        );
    }
    #[test]
    fn logo_no_color_emits_no_escape_sequences() {
        let (header, mut page) = home_fixture();
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (30, 80),
            no_color(),
            Instant::now(),
        );
        for row in &frame.rows {
            let encoded = row.encoded(no_color());
            assert!(
                !encoded.contains('\x1b'),
                "Escape sequence leaked in NO_COLOR mode: {encoded:?}"
            );
        }
        // Logo rows are still present as plain text.
        assert_eq!(
            count_logo_rows(&frame),
            6,
            "Logo rows must appear in NO_COLOR mode (as plain text)"
        );
    }
    #[test]
    fn card_tally_shows_border_and_bar_text() {
        let (header, mut page) = home_fixture();
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            still(),
            Instant::now(),
        );
        // Card borders must be present.
        assert!(lines_contain(&frame, "╭─"), "Missing card top border");
        assert!(lines_contain(&frame, "╰─"), "Missing card bottom border");
        // Tally bar shows localized text.
        assert!(
            lines_contain(&frame, "8 of 11 checks protected"),
            "Missing tally bar text"
        );
        // Badges row inside the card uses │ borders.
        assert!(lines_contain(&frame, "│ ✓"), "Missing badge inside card");
    }
    #[test]
    fn card_rows_share_one_width_at_every_size() {
        let (header, _) = home_fixture();
        for width in [10, 36, 60, 76, 92] {
            let rows = status_card_rows(Lang::En, &header, 0, width, true);
            assert_eq!(rows.len(), 4);
            for row in &rows {
                assert_eq!(measure_text_width(&row.plain()), width, "{}", row.plain());
            }
        }
    }
    #[test]
    fn card_bar_absent_when_tally_total_is_zero() {
        let mut header = {
            let (h, _) = home_fixture();
            h
        };
        header.tally = Some((0, 0));
        let mut page = {
            let (_, p) = home_fixture();
            p
        };
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            still(),
            Instant::now(),
        );
        // Card still renders (tally is Some) but no bar row because total=0.
        assert!(lines_contain(&frame, "╭─"), "Card must render even when total=0");
        assert!(
            !lines_contain(&frame, "checks protected"),
            "Bar must be absent when tally total=0"
        );
    }
    #[test]
    fn card_absent_when_tally_is_none() {
        let header = Header {
            subtitle: "Flat subtitle".into(),
            badges: vec![Badge { text: "ok".into(), role: Role::Healthy }],
            tally: None,
        };
        let mut page = Page {
            title: "Page title".into(),
            kind: PageKind::Menu,
            ..Page::default()
        };
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            still(),
            Instant::now(),
        );
        // No tally → flat header, no rounded card.
        assert!(
            !lines_contain(&frame, "╭─"),
            "Card must be absent when tally=None"
        );
        assert!(
            lines_contain(&frame, "Flat subtitle"),
            "Subtitle must appear in flat header"
        );
    }
    #[test]
    fn breadcrumb_combines_section_and_title_when_it_fits() {
        let (header, mut page) = home_fixture();
        let frame = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            still(),
            Instant::now(),
        );
        // "Overview › Your next step" combined on one row.
        assert!(
            lines_contain(&frame, "Overview › Your next step"),
            "Breadcrumb must be combined when it fits on one row"
        );
    }
    #[test]
    fn focus_uses_brackets_in_none_mode_and_bar_in_color_mode() {
        let (header, mut page) = home_fixture();
        // ColorMode::None (no_color) → ❨ ❩ accessibility brackets.
        let no_c = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            no_color(),
            Instant::now(),
        );
        assert!(
            no_c.lines.iter().any(|l| l.contains('❨')),
            "ColorMode::None must use ❨❩ brackets for the focused option"
        );
        // Basic16 (still) → reverse-video bar, no ❨❩ brackets.
        page.wrap_width = 0;
        let color = layout(
            Lang::En,
            &header,
            &[Section::Overview],
            &mut page,
            (24, 80),
            still(),
            Instant::now(),
        );
        assert!(
            !color.lines.iter().any(|l| l.contains('❨')),
            "Basic16 must NOT use ❨❩ brackets for the focused option"
        );
    }
    #[test]
    fn footer_hint_is_always_the_last_row() {
        let (header, mut page) = home_fixture();
        for &(rows, cols) in &[(24u16, 80u16), (30, 120), (40, 80)] {
            let frame = layout(
                Lang::En,
                &header,
                &[Section::Overview],
                &mut page,
                (rows, cols),
                still(),
                Instant::now(),
            );
            let height = rows as usize - 1;
            assert_eq!(
                frame.rows.len(),
                height,
                "Frame must fill exactly height-1 rows at {rows}x{cols}"
            );
            let last = frame.rows.last().unwrap().plain();
            assert!(
                last.contains('↑') || last.contains('↓') || last.contains("Enter"),
                "Last row must be the hint bar, got: {last:?}"
            );
        }
    }
    #[test]
    fn body_line_role_prefixes_map_to_correct_roles() {
        let body = "✓ All good\n! Fix needed\n? Not sure\n✗ Failed\n▸ Section\n· Info\nPlain";
        let mut page = Page {
            title: "Test".into(),
            body: body.into(),
            kind: PageKind::Document,
            document: true,
            ..Page::default()
        };
        let header = Header::default();
        let frame = layout(
            Lang::En,
            &header,
            &[],
            &mut page,
            (30, 80),
            still(),
            Instant::now(),
        );
        // Each prefix must map to the right Role in wrapped_roles.
        let pairs: &[(&str, Role)] = &[
            ("✓ All good", Role::Healthy),
            ("! Fix needed", Role::Review),
            ("? Not sure", Role::Unknown),
            ("✗ Failed", Role::Failure),
            ("▸ Section", Role::Title),
            ("· Info", Role::Muted),
            ("Plain", Role::Text),
        ];
        for (text, expected_role) in pairs {
            let idx = page
                .wrapped
                .iter()
                .position(|l| l == *text)
                .unwrap_or_else(|| panic!("line {text:?} missing from wrapped"));
            let role = page
                .wrapped_roles
                .get(idx)
                .copied()
                .unwrap_or(Role::Text);
            assert_eq!(
                role, *expected_role,
                "line {text:?} must have role {expected_role:?}, got {role:?}"
            );
        }
        // Encoded rows must apply the role's color (in Basic16, they differ for
        // Healthy vs Review vs Unknown; absence of "48;" validates no background).
        let encoded = frame.rows.iter().map(|r| r.encoded(still())).collect::<String>();
        assert!(!encoded.contains("48;"), "No background color codes allowed");
        let encoded_none = frame.rows.iter().map(|r| r.encoded(no_color())).collect::<String>();
        assert!(!encoded_none.contains('\x1b'), "No escapes in None mode");
    }
}
