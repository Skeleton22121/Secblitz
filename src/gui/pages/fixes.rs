//! Protection page: every check grouped, attention rows selectable.
//! OWNER: fixes agent.
//!
//! Layout (docs/DESIGN-SYSTEM.md): borderless groups of `row_item`s. "Needs
//! attention" is open, followed by the selectable "Privacy extras" (optional,
//! never counted); "Worth a look" is a plain group; "Can't check" and
//! "Protected" are collapsibles. Secondary actions live in an overflow menu.
//! While a check runs the page shows the compact magnifying glass plus the
//! status ticker. Row text is translated and built once per check result
//! (`Cache`) so `view()` only assembles widgets.
use crate::advice::{self, Group, NextStep};
use crate::app::flow;
use crate::app::score::{self, Class};
use crate::broker::{Reply, Request};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::anim;
use crate::gui::widgets::hairline::magnifier::{self, Labels, Magnifier, Status};
use crate::gui::widgets::hairline::Plate;
use crate::gui::widgets::scan;
use crate::gui::widgets::{self, ButtonKind, CheckState};
use crate::gui::{Ctx, Message};
use crate::i18n::Lang;
use iced::widget::{column, container, row, space, Column};
use iced::{Alignment, Background, Border, Element, Length};
use iced::{Subscription, Task};
use secblitz::engine::Report;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

/// Rows shown before "See more" in a long list.
const FIRST_ROWS: usize = 8;
/// Status lines kept for the ticker.
const TICKER_LINES: usize = 6;
/// Left inset that lines expanded text up with a row's title:
/// checkbox box + its padding, gap, row padding, icon, gap.
const INDENT: f32 =
    theme::CHECK + theme::S1 * 2.0 + theme::S1 + theme::S4 + theme::ICON_ROW + theme::S4;
/// Same inset for rows that have no checkbox.
const INDENT_PLAIN: f32 = theme::S4 + theme::ICON_ROW + theme::S4;

#[derive(Debug)]
pub struct State {
    /// `Ctx::checked_at` the selection below belongs to.
    synced_at: Option<u64>,
    selected: HashSet<String>,
    expanded: HashSet<String>,
    open_protected: bool,
    open_cant: bool,
    open_managed: bool,
    open_info: bool,
    all_attention: bool,
    all_protected: bool,
    show_error: bool,
    /// Translated row text for the current report (built lazily, once).
    cache: RefCell<Option<Cached>>,
    /// Start of the running check on this page (set by the first frame).
    scan: Option<Instant>,
    /// Timestamp of the latest frame (never `Instant::now()` in `view()`).
    now: Instant,
    /// Status lines for the ticker, oldest first.
    lines: Vec<(String, Instant)>,
    /// How many `ctx.checking` items are already in `lines`.
    processed: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            synced_at: None,
            selected: HashSet::new(),
            expanded: HashSet::new(),
            open_protected: false,
            open_cant: false,
            open_managed: false,
            open_info: false,
            all_attention: false,
            all_protected: false,
            show_error: false,
            cache: RefCell::new(None),
            scan: None,
            now: Instant::now(),
            lines: Vec::new(),
            processed: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Toggle(String),
    SelectAll,
    SelectNone,
    Expand(String),
    ShowProtected,
    ToggleCant,
    ToggleManaged,
    ToggleInfo,
    AllAttention,
    AllProtected,
    ErrorDetails,
    /// Animation frame (only while a check runs).
    Frame(Instant),
    /// Open a Windows Settings page through the launcher.
    Open(Request),
}

// ---------------------------------------------------------------- row data

#[derive(Debug)]
struct Cached {
    /// Identity of what the rows were built from.
    key: (usize, Lang, usize),
    rows: Rows,
}

#[derive(Debug, Default)]
struct Rows {
    attention: Vec<Att>,
    /// Optional privacy tidy-ups: selectable, but not protection gaps and
    /// never counted (score::to_check leaves them out too).
    privacy: Vec<Att>,
    others: Vec<Other>,
    protected: Vec<Prot>,
}

/// A fixable, selectable row.
#[derive(Debug)]
struct Att {
    id: String,
    name: String,
    line: String,
    why: String,
    tech: String,
    restart: bool,
    /// A choice the person makes: shown unticked with its consequence.
    choice: bool,
}

/// A row of the "worth a look / can't check / managed" groups.
#[derive(Debug)]
struct Other {
    key: String,
    /// Control id or finding title the explanation is looked up by.
    explain: String,
    /// Findings are reported only; controls can be turned on.
    report_only: bool,
    name: String,
    line: String,
    status: String,
    step: NextStep,
    tone: Tone,
    bucket: Bucket,
    icon: Icon,
    tech: String,
}

#[derive(Debug)]
struct Prot {
    id: String,
    name: String,
    line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Look,
    Managed,
    Unavailable,
    /// Notes and "not offered" items: never counted, no action.
    GoodToKnow,
}

/// Plain-words "More details" text: what the status means and what to do.
/// The raw backend detail is never shown.
fn tech_line(status: &str, a: &advice::Advice, lang: Lang) -> String {
    let (st, next) = crate::app::flow::plain_detail(status, a);
    format!("{} · {}", lang.t(st), lang.t(next))
}

fn build(ctx: &Ctx, report: &Report) -> Rows {
    let lang = ctx.lang;
    let mut rows = Rows::default();
    let fixable = candidates(ctx);

    for id in &fixable {
        let Some(r) = report.results.iter().find(|r| r.id == *id) else {
            continue;
        };
        let a = advice::for_outcome(r);
        let impact = advice::control_impact(id);
        let choice = advice::is_choice(id);
        // A choice always shows its one-line consequence, never a generic impact.
        let line = if choice || impact.is_empty() {
            ctx.t(a.next)
        } else {
            format!("{} {}", ctx.t(a.impact_prefix()), ctx.t(impact))
        };
        let restart = ctx.catalog.restart.iter().any(|x| x == id)
            || r.detail.to_lowercase().contains("restart");
        let list = if score::classify(r) == Class::Excluded {
            &mut rows.privacy
        } else {
            &mut rows.attention
        };
        list.push(Att {
            id: id.clone(),
            name: lang.control(id),
            line,
            why: ctx.t(a.next),
            tech: tech_line(&r.status, &a, lang),
            restart,
            choice,
        });
    }

    for r in &report.results {
        if fixable.contains(&r.id) {
            continue;
        }
        let class = score::classify(r);
        let a = advice::for_outcome(r);
        if class == Class::Protected {
            let impact = advice::control_impact(&r.id);
            let line = if impact.is_empty() {
                ctx.t(a.next)
            } else {
                format!("{} {}", ctx.t(a.impact_prefix()), ctx.t(impact))
            };
            rows.protected.push(Prot {
                id: r.id.clone(),
                name: lang.control(&r.id),
                line,
            });
            continue;
        }
        let (bucket, tone) = if class == Class::Managed {
            (Bucket::Managed, Tone::Neutral)
        } else if class == Class::Excluded {
            (Bucket::GoodToKnow, Tone::Neutral)
        } else if class == Class::Unknown {
            (Bucket::Unavailable, Tone::Neutral)
        } else {
            (Bucket::Look, Tone::Warn)
        };
        rows.others.push(other(
            ctx,
            rows.others.len(),
            (r.id.as_str(), false),
            lang.control(&r.id),
            &a,
            bucket,
            tone,
            tech_line(&r.status, &a, lang),
        ));
    }
    for f in &report.findings {
        let a = advice::for_finding(&f.title, &f.status, &f.detail);
        if a.group == Group::Protected {
            continue;
        }
        // Same classes as the count (score::to_check), so they always agree.
        let (bucket, tone) = match score::classify_finding(f) {
            Class::Managed => (Bucket::Managed, Tone::Neutral),
            Class::Excluded => (Bucket::GoodToKnow, Tone::Neutral),
            Class::Unknown => (Bucket::Unavailable, Tone::Neutral),
            Class::Protected | Class::Fixable | Class::Review => (Bucket::Look, Tone::Warn),
        };
        rows.others.push(other(
            ctx,
            rows.others.len(),
            (f.title.as_str(), true),
            ctx.t(a.label),
            &a,
            bucket,
            tone,
            tech_line(&f.status, &a, lang),
        ));
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn other(
    ctx: &Ctx,
    index: usize,
    explain: (&str, bool),
    name: String,
    a: &advice::Advice,
    bucket: Bucket,
    tone: Tone,
    tech: String,
) -> Other {
    let icon = match bucket {
        Bucket::Managed => Icon::Lock,
        Bucket::Unavailable => Icon::Info,
        Bucket::GoodToKnow => Icon::Info,
        Bucket::Look => Icon::AlertTriangle,
    };
    Other {
        key: format!("other:{index}"),
        explain: explain.0.to_owned(),
        report_only: explain.1,
        name,
        line: ctx.t(a.next),
        status: ctx.t(a.status),
        step: a.step,
        tone,
        bucket,
        icon,
        tech,
    }
}

/// Build the row cache if the report (or language) changed since last time.
fn ensure(state: &State, ctx: &Ctx, report: &Arc<Report>) {
    let key = (
        Arc::as_ptr(report) as usize,
        ctx.lang,
        ctx.catalog.available.len(),
    );
    let mut cache = state.cache.borrow_mut();
    if cache.as_ref().is_some_and(|c| c.key == key) {
        return;
    }
    *cache = Some(Cached {
        key,
        rows: build(ctx, report),
    });
}

// ------------------------------------------------------------------ update

fn candidates(ctx: &Ctx) -> Vec<String> {
    ctx.report
        .as_deref()
        .map(|r| flow::candidates(r, &ctx.catalog.available))
        .unwrap_or_default()
}

/// Bring the stored selection in line with the latest check: a new check
/// resets it to the recommended set.
fn sync(state: &mut State, ctx: &Ctx) {
    if state.synced_at != ctx.checked_at || state.synced_at.is_none() {
        state.synced_at = ctx.checked_at;
        state.selected = ctx
            .report
            .as_deref()
            .map(|r| flow::recommended(r, &ctx.catalog.available))
            .unwrap_or_default()
            .into_iter()
            .collect();
        state.expanded.clear();
    }
}

/// Selected ids in list order, limited to what is currently fixable.
fn selection(state: &State, ctx: &Ctx, all: &[String]) -> Vec<String> {
    if state.synced_at == ctx.checked_at && ctx.checked_at.is_some() {
        all.iter()
            .filter(|id| state.selected.contains(*id))
            .cloned()
            .collect()
    } else {
        // Not synced yet (no message since the check): show exactly what
        // `sync` will select, so choices are never drawn ticked.
        let recommended = ctx
            .report
            .as_deref()
            .map(|r| flow::recommended(r, &ctx.catalog.available))
            .unwrap_or_default();
        all.iter()
            .filter(|id| recommended.contains(*id))
            .cloned()
            .collect()
    }
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    sync(state, ctx);
    match msg {
        Msg::Toggle(id) => {
            if !state.selected.remove(&id) && candidates(ctx).contains(&id) {
                state.selected.insert(id);
            }
        }
        Msg::SelectAll => state.selected = candidates(ctx).into_iter().collect(),
        Msg::SelectNone => state.selected.clear(),
        Msg::Expand(id) => flip(&mut state.expanded, id),
        Msg::ShowProtected => state.open_protected = !state.open_protected,
        Msg::ToggleCant => state.open_cant = !state.open_cant,
        Msg::ToggleManaged => state.open_managed = !state.open_managed,
        Msg::ToggleInfo => state.open_info = !state.open_info,
        Msg::AllAttention => state.all_attention = !state.all_attention,
        Msg::AllProtected => state.all_protected = !state.all_protected,
        Msg::Frame(now) => track_scan(state, ctx, now),
        Msg::ErrorDetails => state.show_error = !state.show_error,
        Msg::Open(request) => return open_settings(ctx, request),
    }
    Task::none()
}

/// Feed the ticker from the live check: one line per newly finished item.
fn track_scan(state: &mut State, ctx: &Ctx, now: Instant) {
    state.now = now;
    let Some(progress) = &ctx.checking else {
        state.scan = None;
        state.lines.clear();
        state.processed = 0;
        return;
    };
    if state.scan.is_none() {
        state.scan = Some(now);
        state.lines = vec![(ctx.t("Looking at your settings…"), now)];
        state.processed = 0;
    }
    if state.processed > progress.items.len() {
        state.processed = 0;
    }
    for (id, _) in progress.items.iter().skip(state.processed) {
        let label = ctx.t(advice::control_label(id));
        if state.lines.last().is_none_or(|(l, _)| *l != label) {
            state.lines.push((label, now));
        }
    }
    state.processed = progress.items.len();
    let extra = state.lines.len().saturating_sub(TICKER_LINES);
    state.lines.drain(..extra);
}

fn flip(set: &mut HashSet<String>, id: String) {
    if !set.remove(&id) {
        set.insert(id);
    }
}

/// Ask the launcher to open a Windows Settings page and tell the person
/// calmly how it went.
pub fn open_settings(ctx: &Ctx, request: Request) -> Task<Message> {
    let lang = ctx.lang;
    ctx.broker_task(request, move |reply| match reply {
        Ok(Reply::Done | Reply::OpenedStore) => {
            Message::Toast(lang.t("Opened in a new window."), Tone::Good)
        }
        _ => Message::Toast(
            lang.t("We couldn't open that. You can find it in the Windows Settings app."),
            Tone::Warn,
        ),
    })
}

// ------------------------------------------------------- shared row pieces

/// Row title (medium weight) with one muted line under it. Shared by the
/// fix-flow sheets so their rows read the same as the page.
pub fn row_text<'a>(p: Palette, title: String, line: Option<String>) -> Element<'a, Message> {
    let mut c = column![iced::widget::text(title)
        .size(theme::BODY)
        .font(theme::MEDIUM)
        .color(p.text)]
    .spacing(theme::S1)
    .width(Length::Fill);
    if let Some(l) = line {
        c = c.push(widgets::small(p, l));
    }
    c.into()
}

/// Quiet tonal inset for extra detail (expanded rows). No border.
pub fn well<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Container<'a, Message> {
    container(content)
        .padding(theme::S3)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface_alt)),
            border: Border {
                radius: theme::R.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
}

/// `[lead] [row] [trailing]`, vertically centred, `S1` apart.
fn line<'a>(
    lead: Option<Element<'a, Message>>,
    content: Element<'a, Message>,
    trailing: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut r = row![].spacing(theme::S1).align_y(Alignment::Center);
    if let Some(l) = lead {
        r = r.push(container(l).padding([0.0, theme::S2]));
    }
    r.push(content).push(trailing).into()
}

/// Expanded text under a row, lined up with the row's title.
fn expanded<'a>(
    p: Palette,
    indent: f32,
    why: Option<String>,
    label: String,
    tech: String,
) -> Element<'a, Message> {
    let mut c = column![].spacing(theme::S2);
    if let Some(w) = why {
        c = c.push(widgets::body(p, w));
    }
    c = c
        .push(widgets::section_label(p, label))
        .push(widgets::small(p, tech));
    row![space::horizontal().width(indent), well(p, c)].into()
}

fn settings_request(step: NextStep) -> Option<Request> {
    match step {
        NextStep::OpenWindowsSecurity => Some(Request::OpenWindowsSecurity),
        NextStep::OpenWindowsUpdate => Some(Request::OpenWindowsUpdate),
        NextStep::OpenEncryption => Some(Request::OpenEncryption),
        NextStep::OpenAccounts => Some(Request::OpenSignIn),
        _ => None,
    }
}

fn nothing<'a>() -> Element<'a, Message> {
    space::horizontal().width(0.0).into()
}

// -------------------------------------------------------------------- rows

fn attention_row<'a>(
    state: &State,
    ctx: &Ctx,
    a: &Att,
    checked: bool,
    restart_label: &str,
    choice_label: &str,
    extra: bool,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let open = state.expanded.contains(&a.id);
    let mut pills = row![].spacing(theme::S3).align_y(Alignment::Center);
    // Every privacy extra is a choice; its group already says so.
    if a.choice && !extra {
        pills = pills.push(widgets::pill(p, choice_label.to_owned(), Tone::Neutral));
    }
    if a.restart {
        pills = pills.push(widgets::tag(
            p,
            Some(Icon::Restart),
            restart_label.to_owned(),
        ));
    }
    let trailing: Element<'a, Message> = pills.into();
    let toggle = Message::Fixes(Msg::Toggle(a.id.clone()));
    let mut tools = row![].spacing(theme::S1).align_y(Alignment::Center);
    if let Some(t) = widgets::explain::toggle(ctx, "fixes", &a.id) {
        tools = tools.push(t);
    }
    // One action per row: a light text button keeps the long list calm.
    tools = tools.push(widgets::action(
        p,
        ButtonKind::Ghost,
        ctx.t(if open { "Hide details" } else { "Details" }),
        None,
        Some(Message::Fixes(Msg::Expand(a.id.clone()))),
    ));
    let head = line(
        Some(widgets::checkbox(
            p,
            CheckState::from(checked),
            None,
            Some(toggle.clone()),
        )),
        widgets::row_item_tinted(
            p,
            Some(if extra { Icon::Eye } else { Icon::AlertTriangle }),
            Some(if extra { Tone::Neutral } else { Tone::Warn }),
            a.name.clone(),
            Some(a.line.clone()),
            trailing,
            Some(toggle),
        ),
        tools.into(),
    );
    let mut rows = column![head].spacing(theme::S1);
    if let Some(inset) = widgets::explain::panel(ctx, "fixes", &a.id, false, INDENT) {
        rows = rows.push(inset);
    }
    if open {
        rows = rows.push(expanded(
            p,
            INDENT,
            Some(a.why.clone()),
            ctx.t("More details"),
            a.tech.clone(),
        ));
    }
    rows.into()
}

fn other_row<'a>(state: &State, ctx: &Ctx, o: &Other) -> Element<'a, Message> {
    let p = ctx.palette;
    let open = state.expanded.contains(&o.key);
    let mut menu = Vec::new();
    if let Some(request) = settings_request(o.step) {
        menu.push((
            Icon::ExternalLink,
            ctx.t("Open settings"),
            Message::Fixes(Msg::Open(request)),
            false,
        ));
    }
    if o.step == NextStep::CheckAgain && !ctx.busy && ctx.checking.is_none() {
        menu.push((
            Icon::Refresh,
            ctx.t("Check again"),
            Message::CheckNow,
            false,
        ));
    }
    menu.push((
        Icon::Info,
        ctx.t(if open { "Hide details" } else { "Details" }),
        Message::Fixes(Msg::Expand(o.key.clone())),
        false,
    ));
    let mut tools = row![].spacing(theme::S1).align_y(Alignment::Center);
    if let Some(t) = widgets::explain::toggle(ctx, "fixes", &o.explain) {
        tools = tools.push(t);
    }
    tools = tools.push(widgets::overflow_menu(p, menu));
    let head = line(
        None,
        widgets::row_item_tinted(
            p,
            Some(o.icon),
            Some(o.tone),
            o.name.clone(),
            Some(o.line.clone()),
            widgets::pill(p, o.status.clone(), o.tone),
            None,
        ),
        tools.into(),
    );
    let mut rows = column![head].spacing(theme::S1);
    if let Some(inset) =
        widgets::explain::panel(ctx, "fixes", &o.explain, o.report_only, INDENT_PLAIN)
    {
        rows = rows.push(inset);
    }
    if open {
        rows = rows.push(expanded(
            p,
            INDENT_PLAIN,
            None,
            ctx.t("More details"),
            o.tech.clone(),
        ));
    }
    rows.into()
}

fn protected_row<'a>(ctx: &Ctx, r: &Prot) -> Element<'a, Message> {
    let p = ctx.palette;
    let head = widgets::row_item_tinted(
        p,
        Some(Icon::Check),
        Some(Tone::Good),
        r.name.clone(),
        Some(r.line.clone()),
        nothing(),
        None,
    );
    widgets::explain::with_disclosure(ctx, "fixes", &r.id, false, INDENT_PLAIN, head)
}

/// "See 12 more" / "Show less" under a truncated list.
fn more<'a>(ctx: &Ctx, total: usize, all: bool, msg: Msg) -> Option<Element<'a, Message>> {
    (total > FIRST_ROWS).then(|| {
        let label = if all {
            ctx.t("Show less")
        } else {
            ctx.t("See {n} more")
                .replace("{n}", &(total - FIRST_ROWS).to_string())
        };
        widgets::show_more_button(ctx.palette, label, Message::Fixes(msg))
    })
}

fn count_text(ctx: &Ctx, n: usize) -> String {
    if n == 1 {
        ctx.t("1 item")
    } else {
        ctx.t("{n} items").replace("{n}", &n.to_string())
    }
}

// -------------------------------------------------------------------- view

/// How far the running check is: (share done, subtitle).
fn check_status(ctx: &Ctx) -> (f32, String) {
    let done = ctx.checking.as_ref().map_or(0, |c| c.items.len());
    let total = ctx.catalog.available.len().max(done + 1);
    let ratio = (done as f32 / total as f32).min(0.96);
    let sub = if done > 0 {
        ctx.t("{a} of {b} checked")
            .replace("{a}", &done.to_string())
            .replace("{b}", &total.to_string())
    } else {
        ctx.t("This takes about a minute. Nothing is changed.")
    };
    (ratio, sub)
}

/// The magnifying glass for the running check, sitting on `plate`. Its lens
/// follows the real progress when the total is known.
fn check_art(state: &State, ctx: &Ctx, plate: Plate, ratio: f32) -> Magnifier {
    Magnifier {
        p: ctx.palette,
        plate,
        status: Status::Checking,
        progress: (!ctx.catalog.available.is_empty()).then_some(ratio),
        changed: state.scan.unwrap_or(state.now),
        now: state.now,
        labels: Labels::new(|s| ctx.t(s)),
    }
}

/// The first check: the magnifying glass, title, bar and ticker centred in
/// the page.
fn first_check<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let (ratio, sub) = check_status(ctx);
    scan::checking_screen(
        ctx.palette,
        ctx.t("Checking your PC"),
        sub,
        ratio,
        check_art(state, ctx, Plate::Bg, ratio),
        &state.lines,
        state.now,
    )
}

/// Compact magnifying glass with the live status ticker, above the last
/// results while a new check runs.
fn checking_region<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let (ratio, sub) = check_status(ctx);
    let live = anim::animating();
    let mut text = column![
        widgets::h2(p, ctx.t("Checking your PC")),
        widgets::muted(p, sub)
    ]
    .spacing(theme::S1)
    .width(Length::Fill);
    if live {
        text = text
            .push(space::vertical().height(theme::S2))
            .push(scan::status_ticker(p, &state.lines, state.now));
    }
    let content = row![
        check_art(state, ctx, Plate::Surface, ratio).view(magnifier::COMPACT),
        text,
    ]
    .spacing(theme::S6)
    .align_y(Alignment::Center);
    widgets::region(p, content).into()
}

/// Frames while a check runs, for the ticker (the magnifying glass drives
/// its own). Only asked for while the page is on screen.
pub fn subscription(ctx: &Ctx) -> Subscription<Message> {
    if ctx.checking.is_some() && anim::animating() {
        iced::window::frames().map(|now| Message::Fixes(Msg::Frame(now)))
    } else {
        Subscription::none()
    }
}

/// Whether the page is the first check's screen, which fills the window
/// instead of scrolling: no result yet, and a check running or about to.
pub fn fills_window(ctx: &Ctx) -> bool {
    ctx.engine_error.is_none()
        && ctx.report.is_none()
        && (ctx.checking.is_some() || ctx.check_error.is_none())
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let to_check = ctx
        .report
        .as_deref()
        .filter(|_| ctx.checking.is_none())
        .map_or(0, score::to_check_count);
    let subtitle = if to_check > 0 {
        ctx.t("To check ({n})")
            .replace("{n}", &to_check.to_string())
    } else {
        ctx.t("Everything we check on your PC, in plain words.")
    };
    let header = widgets::page_header(p, ctx.t("Protection"), Some(subtitle));
    let page = |body: Column<'a, Message>| -> Element<'a, Message> {
        column![header, space::vertical().height(theme::S6), body].into()
    };
    let mut body = column![].spacing(theme::S8);

    // Could not start.
    if let Some(error) = &ctx.engine_error {
        let details = widgets::expander(
            p,
            ctx.t("More details"),
            state.show_error,
            Message::Fixes(Msg::ErrorDetails),
            widgets::small(p, ctx.t(crate::app::flow::plain_failure(error))),
        );
        return page(body.push(widgets::region(
            p,
            widgets::empty_state(
                p,
                Icon::ShieldAlert,
                ctx.t("We couldn't start the protection check"),
                ctx.t(
                    "Close Secblitz and open it again. If this keeps happening, restart your PC.",
                ),
                Some(details),
            ),
        )));
    }

    // First check not finished yet, or it failed.
    let Some(report) = ctx.report.as_ref() else {
        if fills_window(ctx) {
            return page(column![first_check(state, ctx)].height(Length::Fill));
        }
        if let Some(error) = &ctx.check_error {
            let details = widgets::expander(
                p,
                ctx.t("More details"),
                state.show_error,
                Message::Fixes(Msg::ErrorDetails),
                widgets::small(p, ctx.t(crate::app::flow::plain_failure(error))),
            );
            let again = widgets::action(
                p,
                ButtonKind::Primary,
                ctx.t("Check again"),
                Some(Icon::Refresh),
                (ctx.checking.is_none() && !ctx.busy).then_some(Message::CheckNow),
            );
            return page(
                body.push(widgets::region(
                    p,
                    widgets::empty_state(
                        p,
                        Icon::ShieldAlert,
                        ctx.t("We couldn't finish checking"),
                        ctx.t("Nothing was changed. Press Check again. If it keeps failing, restart your PC."),
                        Some(
                            column![again, details]
                                .spacing(theme::S3)
                                .align_x(Alignment::Center)
                                .into(),
                        ),
                    ),
                )),
            );
        }
        return page(body);
    };

    ensure(state, ctx, report);
    let cache = state.cache.borrow();
    let rows = &cache.as_ref().expect("filled by ensure").rows;

    if ctx.checking.is_some() {
        body = body.push(checking_region(state, ctx));
    } else if ctx.check_error.is_some() {
        body = body.push(widgets::inline_notice(
            p,
            Tone::Warn,
            ctx.t("We couldn't refresh the check. You're seeing the last result we could confirm."),
        ));
    }
    if report.results.iter().any(|r| r.status == "pending")
        || report.findings.iter().any(|f| f.status == "pending")
    {
        body = body.push(widgets::row_item_tinted(
            p,
            Some(Icon::Undo),
            Some(Tone::Warn),
            ctx.t("An earlier change isn't finished"),
            Some(ctx.t("Undo your last fixes before making new ones.")),
            widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t("Undo"),
                None,
                (!ctx.busy).then_some(Message::ReviewUndo),
            ),
            None,
        ));
    }

    // Needs attention (open, selectable), then optional privacy extras. One
    // selection spans both; the Fix button sits on whichever group comes first.
    if rows.attention.is_empty() && ctx.checking.is_none() {
        body = body.push(widgets::region(
            p,
            widgets::empty_state(
                p,
                Icon::ShieldCheck,
                ctx.t("Nothing needs fixing right now"),
                ctx.t("We'll tell you if anything changes."),
                None,
            ),
        ));
    }
    if !rows.attention.is_empty() || !rows.privacy.is_empty() {
        let all: Vec<String> = rows
            .attention
            .iter()
            .chain(&rows.privacy)
            .map(|a| a.id.clone())
            .collect();
        let chosen = selection(state, ctx, &all);
        let n = chosen.len();
        let restart_label = ctx.t("Restart needed");
        let choice_label = ctx.t("Your choice");
        let rows_of = |list: &[Att], extra: bool| -> Vec<Element<'a, Message>> {
            list.iter()
                .map(|a| {
                    attention_row(
                        state,
                        ctx,
                        a,
                        chosen.contains(&a.id),
                        &restart_label,
                        &choice_label,
                        extra,
                    )
                })
                .collect()
        };
        let count = match n {
            0 => ctx.t("Nothing selected"),
            1 => ctx.t("1 selected"),
            _ => ctx.t("{n} selected").replace("{n}", &n.to_string()),
        };
        let ready = !ctx.busy && ctx.checking.is_none() && ctx.check_error.is_none();
        let select = if n == all.len() {
            (
                Icon::X,
                ctx.t("Select none"),
                Message::Fixes(Msg::SelectNone),
            )
        } else {
            (
                Icon::Check,
                ctx.t("Select all"),
                Message::Fixes(Msg::SelectAll),
            )
        };
        let mut trailing = Some(
            row![
                widgets::action(
                    p,
                    ButtonKind::Primary,
                    ctx.t("Fix selected"),
                    Some(Icon::Wrench),
                    (ready && n > 0).then(|| Message::ReviewFixes(chosen.clone())),
                ),
                widgets::overflow_menu(p, vec![(select.0, select.1, select.2, false)]),
            ]
            .spacing(theme::S1)
            .align_y(Alignment::Center),
        );
        if !rows.attention.is_empty() {
            let shown = widgets::limited(&rows.attention, FIRST_ROWS, state.all_attention);
            let mut list = rows_of(shown, false);
            if let Some(m) = more(
                ctx,
                rows.attention.len(),
                state.all_attention,
                Msg::AllAttention,
            ) {
                list.push(m);
            }
            body = body.push(widgets::group(
                p,
                ctx.t("Needs your attention"),
                Some(count.clone()),
                trailing.take().map(Into::into),
                list,
            ));
        }
        if !rows.privacy.is_empty() {
            let note = ctx.t("Optional. Not part of your protection score.");
            // Without attention rows this group carries the selection count too.
            let subtitle = if trailing.is_some() {
                format!("{note} · {count}")
            } else {
                note
            };
            body = body.push(widgets::group(
                p,
                ctx.t("Privacy extras"),
                Some(subtitle),
                trailing.take().map(Into::into),
                rows_of(&rows.privacy, true),
            ));
        }
    }

    let bucket =
        |b: Bucket| -> Vec<&Other> { rows.others.iter().filter(|o| o.bucket == b).collect() };

    // Worth a look: a plain group, they need a decision.
    let look = bucket(Bucket::Look);
    if !look.is_empty() {
        body = body.push(widgets::group(
            p,
            ctx.t("Worth a look"),
            Some(ctx.t("These need a decision from you. We can't safely change them for you.")),
            None,
            look.iter().map(|o| other_row(state, ctx, o)).collect(),
        ));
    }

    // Can't check right now: collapsed.
    let cant = bucket(Bucket::Unavailable);
    if !cant.is_empty() {
        let mut list = column![].spacing(theme::S1);
        for o in &cant {
            list = list.push(other_row(state, ctx, o));
        }
        body = body.push(widgets::collapsible(
            p,
            ctx.t("Can't check right now"),
            Some(count_text(ctx, cant.len())),
            state.open_cant,
            Message::Fixes(Msg::ToggleCant),
            list,
        ));
    }

    // Settings this PC's owner controls: checked, but not ours to change.
    let managed = bucket(Bucket::Managed);
    if !managed.is_empty() {
        let mut list = column![].spacing(theme::S1);
        for o in &managed {
            list = list.push(other_row(state, ctx, o));
        }
        body = body.push(widgets::collapsible(
            p,
            ctx.t("Managed elsewhere"),
            Some(count_text(ctx, managed.len())),
            state.open_managed,
            Message::Fixes(Msg::ToggleManaged),
            list,
        ));
    }

    // Good to know: notes only, collapsed, never counted.
    let info = bucket(Bucket::GoodToKnow);
    if !info.is_empty() {
        let mut list = column![].spacing(theme::S1);
        for o in &info {
            list = list.push(other_row(state, ctx, o));
        }
        body = body.push(widgets::collapsible(
            p,
            ctx.t("Good to know"),
            Some(count_text(ctx, info.len())),
            state.open_info,
            Message::Fixes(Msg::ToggleInfo),
            list,
        ));
    }

    // Protected: collapsed by default.
    if !rows.protected.is_empty() {
        let shown = widgets::limited(&rows.protected, FIRST_ROWS, state.all_protected);
        let mut list = column![].spacing(theme::S1);
        for r in shown {
            list = list.push(protected_row(ctx, r));
        }
        if let Some(m) = more(
            ctx,
            rows.protected.len(),
            state.all_protected,
            Msg::AllProtected,
        ) {
            list = list.push(m);
        }
        body = body.push(widgets::collapsible(
            p,
            ctx.t("Protected"),
            Some(count_text(ctx, rows.protected.len())),
            state.open_protected,
            Message::Fixes(Msg::ShowProtected),
            list,
        ));
    }
    page(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flip_toggles_membership() {
        let mut set = HashSet::new();
        flip(&mut set, "a".into());
        assert!(set.contains("a"));
        flip(&mut set, "a".into());
        assert!(set.is_empty());
    }
}
