//! Clean up apps page. OWNER: debloat agent.
//!
//! Flow: scan -> pick apps -> review sheet (Cancel is the safe choice) ->
//! working sheet with per-app progress -> result. A second tab lists removed
//! apps with a Restore button.
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{blocking, blocking_stream, Ctx, Message};
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Background, Border, Element, Length, Task};
use secblitz::debloat::{self, Batch, Group, Installed, ItemResult, Progress};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Apps,
    Removed,
}

#[derive(Debug, Default)]
enum Scan {
    #[default]
    Loading,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Waiting,
    Working,
    Done(ItemResult),
}

#[derive(Debug, Default)]
enum Sheet {
    #[default]
    None,
    Review,
    Working(Vec<(u16, Step)>),
    Done(Box<Finished>),
}

#[derive(Debug, Default)]
struct Finished {
    batch: Option<Batch>,
    error: Option<String>,
    /// Machine-wide setting written (None = not asked for).
    policy_ok: Option<bool>,
    /// Per-user setting written through the launcher (None = not yet known).
    user_ok: Option<bool>,
    asked_to_block: bool,
}

#[derive(Debug)]
pub struct State {
    tab: Tab,
    scan: Scan,
    installed: Vec<Installed>,
    selected: BTreeSet<u16>,
    initialised: bool,
    block_again: bool,
    sheet: Sheet,
    details: bool,
    journal: Vec<Batch>,
    restoring: Option<u16>,
    /// Machine-wide setting result, arrives just before the batch result.
    policy: Option<bool>,
}

impl Default for State {
    fn default() -> Self {
        State {
            tab: Tab::Apps,
            scan: Scan::Loading,
            installed: Vec::new(),
            selected: BTreeSet::new(),
            initialised: false,
            block_again: true,
            sheet: Sheet::None,
            details: false,
            journal: Vec::new(),
            restoring: None,
            policy: None,
        }
    }
}

/// Events from the removal thread.
#[derive(Debug, Clone)]
pub enum Run {
    Step(Progress),
    Policy(bool),
    Done(Result<Batch, String>),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Scanned(Result<Vec<Installed>, String>),
    JournalLoaded(Vec<Batch>),
    Rescan,
    SetTab(Tab),
    Toggle(u16),
    ToggleGroup(Group),
    ToggleBlock,
    Review,
    Cancel,
    Confirm,
    Run(Run),
    UserBlocked(bool),
    ToggleDetails,
    CloseResult,
    Restore(u16),
    Restored(u16, Result<crate::broker::Reply, String>),
}

fn wrap(msg: Msg) -> Message {
    Message::Debloat(msg)
}

fn scan_task() -> Task<Message> {
    Task::batch([
        Task::perform(
            blocking(|| debloat::inventory().map_err(|e| format!("{e:#}"))),
            |r| wrap(Msg::Scanned(r)),
        ),
        Task::perform(blocking(debloat::journal::load), |j| {
            wrap(Msg::JournalLoaded(j))
        }),
    ])
}

pub fn on_enter(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let _ = ctx;
    if !matches!(state.sheet, Sheet::None) {
        return Task::none();
    }
    state.scan = Scan::Loading;
    scan_task()
}

/// Close the review sheet or the result with Escape. Working cannot be dismissed.
pub fn escape(state: &mut State) {
    if matches!(state.sheet, Sheet::Review | Sheet::Done(_)) {
        state.sheet = Sheet::None;
    }
}

fn app_of(index: u16) -> &'static debloat::App {
    &debloat::catalog()[index as usize]
}

fn installed_indices(state: &State) -> Vec<u16> {
    let mut v: Vec<u16> = state.installed.iter().map(|p| p.index).collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn toast(text: String, tone: Tone) -> Task<Message> {
    Task::done(Message::Toast(text, tone))
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Scanned(Ok(found)) => {
            state.installed = found;
            let present: BTreeSet<u16> = installed_indices(state).into_iter().collect();
            if state.initialised {
                state.selected.retain(|i| present.contains(i));
            } else {
                state.selected = present
                    .iter()
                    .copied()
                    .filter(|i| app_of(*i).group.selected_by_default())
                    .collect();
                state.initialised = true;
            }
            state.scan = Scan::Ready;
            Task::none()
        }
        Msg::Scanned(Err(e)) => {
            state.scan = Scan::Failed(e);
            Task::none()
        }
        Msg::JournalLoaded(j) => {
            state.journal = j;
            Task::none()
        }
        Msg::Rescan => {
            state.scan = Scan::Loading;
            scan_task()
        }
        Msg::SetTab(tab) => {
            state.tab = tab;
            Task::none()
        }
        Msg::Toggle(i) => {
            if !state.selected.remove(&i) {
                state.selected.insert(i);
            }
            Task::none()
        }
        Msg::ToggleGroup(group) => {
            let members: Vec<u16> = installed_indices(state)
                .into_iter()
                .filter(|i| app_of(*i).group == group)
                .collect();
            if members.iter().all(|i| state.selected.contains(i)) {
                for i in members {
                    state.selected.remove(&i);
                }
            } else {
                state.selected.extend(members);
            }
            Task::none()
        }
        Msg::ToggleBlock => {
            state.block_again = !state.block_again;
            Task::none()
        }
        Msg::Review => {
            if !ctx.busy && !state.selected.is_empty() {
                state.sheet = Sheet::Review;
            }
            Task::none()
        }
        Msg::Cancel => {
            if matches!(state.sheet, Sheet::Review) {
                state.sheet = Sheet::None;
            }
            Task::none()
        }
        Msg::Confirm => confirm(state, ctx),
        Msg::Run(run) => on_run(state, ctx, run),
        Msg::UserBlocked(ok) => {
            if let Sheet::Done(done) = &mut state.sheet {
                done.user_ok = Some(ok);
            }
            Task::none()
        }
        Msg::ToggleDetails => {
            state.details = !state.details;
            Task::none()
        }
        Msg::CloseResult => {
            if matches!(state.sheet, Sheet::Done(_)) {
                state.sheet = Sheet::None;
                state.details = false;
            }
            Task::none()
        }
        Msg::Restore(index) => {
            if state.restoring.is_some() || ctx.busy || app_of(index).store_id.is_none() {
                return Task::none();
            }
            state.restoring = Some(index);
            ctx.broker_task(crate::broker::Request::ReinstallStoreApp(index), move |r| {
                wrap(Msg::Restored(index, r))
            })
        }
        Msg::Restored(index, result) => {
            state.restoring = None;
            let name = ctx.t(app_of(index).name);
            match result {
                Ok(crate::broker::Reply::Done) => {
                    for batch in &mut state.journal {
                        for r in batch.removed.iter_mut().filter(|r| r.index == index) {
                            r.restored = true;
                        }
                    }
                    if let Some(dir) = &ctx.state_dir {
                        let score = ctx.score().unwrap_or_default();
                        let _ = crate::app::history::record(
                            dir,
                            &crate::app::history::Entry {
                                t: crate::app::history::now(),
                                kind: crate::app::history::Kind::Restore,
                                protected: score.protected,
                                total: score.total,
                                n: 1,
                            },
                        );
                    }
                    Task::batch([
                        Task::perform(
                            blocking(move || {
                                let _ = debloat::journal::mark_restored(index);
                                debloat::journal::load()
                            }),
                            |j| wrap(Msg::JournalLoaded(j)),
                        ),
                        toast(
                            format!("{name} {}", ctx.t("is back on your PC.")),
                            Tone::Good,
                        ),
                    ])
                }
                Ok(crate::broker::Reply::OpenedStore) => toast(
                    format!(
                        "{} {name}.",
                        ctx.t("We opened the Microsoft Store so you can install")
                    ),
                    Tone::Neutral,
                ),
                _ => toast(
                    format!(
                        "{} {name}. {}",
                        ctx.t("We couldn't bring back"),
                        ctx.t("Please try again later.")
                    ),
                    Tone::Bad,
                ),
            }
        }
    }
}

fn confirm(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    if ctx.busy || !matches!(state.sheet, Sheet::Review) {
        return Task::none();
    }
    let present = installed_indices(state);
    let indices: Vec<u16> = present
        .into_iter()
        .filter(|i| state.selected.contains(i))
        .collect();
    if indices.is_empty() {
        state.sheet = Sheet::None;
        return Task::none();
    }
    ctx.busy = true;
    state.details = false;
    state.sheet = Sheet::Working(indices.iter().map(|i| (*i, Step::Waiting)).collect());
    let block = state.block_again;
    let stream = blocking_stream(move |emit: &dyn Fn(Run)| {
        let result =
            debloat::remove(&indices, &|p| emit(Run::Step(p))).map_err(|e| format!("{e:#}"));
        if block && result.is_ok() {
            emit(Run::Policy(debloat::set_consumer_features_policy().is_ok()));
        }
        emit(Run::Done(result));
    });
    Task::run(stream, |r| wrap(Msg::Run(r)))
}

fn on_run(state: &mut State, ctx: &mut Ctx, run: Run) -> Task<Message> {
    match run {
        Run::Step(Progress::Started(i)) => {
            if let Sheet::Working(items) = &mut state.sheet {
                if let Some(item) = items.iter_mut().find(|(n, _)| *n == i) {
                    item.1 = Step::Working;
                }
            }
            Task::none()
        }
        Run::Step(Progress::Finished(i, result)) => {
            if let Sheet::Working(items) = &mut state.sheet {
                if let Some(item) = items.iter_mut().find(|(n, _)| *n == i) {
                    item.1 = Step::Done(result);
                }
            }
            Task::none()
        }
        Run::Policy(ok) => {
            state.policy = Some(ok);
            Task::none()
        }
        Run::Done(result) => {
            ctx.busy = false;
            let policy_ok = state.policy.take();
            let asked = state.block_again && result.is_ok();
            let mut done = Finished {
                asked_to_block: asked,
                policy_ok,
                ..Finished::default()
            };
            let mut tasks = vec![];
            match result {
                Ok(batch) => {
                    let removed = batch.removed_apps();
                    if removed > 0 {
                        record_history(ctx, removed);
                        state.journal.push(batch.clone());
                    }
                    done.batch = Some(batch);
                    if asked {
                        tasks.push(ctx.broker_task(
                            crate::broker::Request::BlockSuggestedApps,
                            |r| {
                                wrap(Msg::UserBlocked(matches!(
                                    r,
                                    Ok(crate::broker::Reply::Done)
                                )))
                            },
                        ));
                    }
                }
                Err(e) => done.error = Some(e),
            }
            state.sheet = Sheet::Done(Box::new(done));
            tasks.push(scan_task());
            Task::batch(tasks)
        }
    }
}

fn record_history(ctx: &Ctx, removed: usize) {
    if let Some(dir) = &ctx.state_dir {
        let score = ctx.score().unwrap_or_default();
        let _ = crate::app::history::record(
            dir,
            &crate::app::history::Entry {
                t: crate::app::history::now(),
                kind: crate::app::history::Kind::Debloat,
                protected: score.protected,
                total: score.total,
                n: removed,
            },
        );
    }
}

// ---- view ------------------------------------------------------------------

fn count_text(ctx: &Ctx, n: usize, one: &str, many: &str) -> String {
    let key = if n == 1 { one } else { many };
    ctx.t(key).replace("{n}", &n.to_string())
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let header = widgets::page_header(
        p,
        ctx.t("Clean up apps"),
        Some(ctx.t(
            "Remove apps that came with Windows but you don't need. You can reinstall most of them later.",
        )),
    );
    let removed_count = removed_entries(state).len();
    let tabs = row![
        tab_button(ctx, Tab::Apps, state.tab, ctx.t("Apps to remove"), None),
        tab_button(
            ctx,
            Tab::Removed,
            state.tab,
            ctx.t("Removed apps"),
            (removed_count > 0).then_some(removed_count)
        ),
    ]
    .spacing(6);
    let body: Element<'a, Message> = match state.tab {
        Tab::Apps => apps_tab(state, ctx),
        Tab::Removed => removed_tab(state, ctx),
    };
    let page = column![header, tabs, body]
        .spacing(theme::GAP)
        .width(Length::Fill);
    page.into()
}

/// The open review / working / result sheet, drawn by the shell above the
/// whole window (outside the page's scrollable).
pub fn modal<'a>(state: &'a State, ctx: &'a Ctx) -> Option<Element<'a, Message>> {
    match &state.sheet {
        Sheet::None => None,
        sheet => Some(sheet_view(state, sheet, ctx)),
    }
}

fn tab_button<'a>(
    ctx: &Ctx,
    tab: Tab,
    current: Tab,
    label: String,
    badge: Option<usize>,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let active = tab == current;
    let mut content = row![text(label).size(theme::BODY).font(theme::SEMIBOLD)]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(n) = badge {
        content = content.push(
            container(
                text(n.to_string())
                    .size(theme::SMALL)
                    .font(theme::MEDIUM)
                    .color(p.text_muted),
            )
            .padding([1, 8])
            .style(move |_| container::Style {
                background: Some(Background::Color(p.surface_alt)),
                border: Border {
                    radius: 999.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
        );
    }
    let fg = if active { p.text } else { p.text_muted };
    button(content)
        .padding([8, 16])
        .on_press(wrap(Msg::SetTab(tab)))
        .style(move |_, status| button::Style {
            background: Some(Background::Color(if active {
                p.surface
            } else if status == button::Status::Hovered {
                p.surface_alt
            } else {
                iced::Color::TRANSPARENT
            })),
            text_color: fg,
            border: Border {
                radius: theme::RADIUS_SMALL.into(),
                width: if active { 1.0 } else { 0.0 },
                color: p.border,
            },
            ..button::Style::default()
        })
        .into()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tick {
    Off,
    Some,
    All,
}

fn tick<'a>(p: Palette, state: Tick) -> Element<'a, Message> {
    let filled = state != Tick::Off;
    let inner: Element<'a, Message> = match state {
        Tick::All => widgets::icon(Icon::Check, 13.0, p.on_brand),
        Tick::Some => container(Space::new())
            .width(10)
            .height(2)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.on_brand)),
                ..container::Style::default()
            })
            .into(),
        Tick::Off => Space::new().into(),
    };
    container(inner)
        .center(20)
        .style(move |_| container::Style {
            background: Some(Background::Color(if filled { p.brand } else { p.surface })),
            border: Border {
                radius: 6.0.into(),
                width: 1.5,
                color: if filled { p.brand } else { p.text_muted },
            },
            ..container::Style::default()
        })
        .into()
}

fn row_button<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    button(content)
        .width(Length::Fill)
        .padding([10, 8])
        .on_press_maybe(on_press)
        .style(move |_, status| button::Style {
            background: Some(Background::Color(if status == button::Status::Hovered {
                p.surface_alt
            } else {
                iced::Color::TRANSPARENT
            })),
            text_color: p.text,
            border: Border {
                radius: theme::RADIUS_SMALL.into(),
                ..Border::default()
            },
            ..button::Style::default()
        })
        .into()
}

fn status_card<'a>(
    ctx: &Ctx,
    icon: Icon,
    tone: Tone,
    title: String,
    detail: String,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut c = column![
        widgets::icon_badge(p, icon, tone),
        widgets::h2(p, title),
        widgets::muted(p, detail)
    ]
    .spacing(10)
    .align_x(Alignment::Center);
    if let Some(a) = action {
        c = c.push(a);
    }
    widgets::card(p, container(c).center_x(Length::Fill).padding([24, 0])).into()
}

fn apps_tab<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    match &state.scan {
        Scan::Loading => status_card(
            ctx,
            Icon::Refresh,
            Tone::Neutral,
            ctx.t("Looking for apps you can remove…"),
            ctx.t("This only takes a moment."),
            None,
        ),
        Scan::Failed(technical) => {
            let mut c = column![status_card(
                ctx,
                Icon::AlertTriangle,
                Tone::Warn,
                ctx.t("We couldn't look at your apps"),
                ctx.t("Nothing was changed. Please try again."),
                Some(widgets::action(
                    p,
                    ButtonKind::Secondary,
                    ctx.t("Try again"),
                    Some(Icon::Refresh),
                    Some(wrap(Msg::Rescan)),
                )),
            )]
            .spacing(theme::GAP);
            c = c.push(technical_details(state, ctx, vec![technical.clone()]));
            c.into()
        }
        Scan::Ready => {
            let present = installed_indices(state);
            if present.is_empty() {
                return status_card(
                    ctx,
                    Icon::CheckCircle,
                    Tone::Good,
                    ctx.t("Nothing to clean up"),
                    ctx.t("None of the apps we know about are on this PC."),
                    None,
                );
            }
            let mut col = column![summary_card(state, ctx)].spacing(theme::GAP);
            for group in Group::ALL {
                let members: Vec<u16> = present
                    .iter()
                    .copied()
                    .filter(|i| app_of(*i).group == group)
                    .collect();
                if !members.is_empty() {
                    col = col.push(group_card(state, ctx, group, &members));
                }
            }
            col.push(option_card(state, ctx)).into()
        }
    }
}

fn summary_card<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let n = state.selected.len();
    let label = count_text(ctx, n, "{n} app selected", "{n} apps selected");
    let can = n > 0 && !ctx.busy;
    let button_label = if n == 0 {
        ctx.t("Remove apps")
    } else {
        count_text(ctx, n, "Remove {n} app", "Remove {n} apps")
    };
    let mut left = column![text(label)
        .size(theme::H2)
        .font(theme::SEMIBOLD)
        .color(p.text)]
    .spacing(2);
    if ctx.busy {
        left = left.push(widgets::small(
            p,
            ctx.t("Please wait until the current task has finished."),
        ));
    } else if n == 0 {
        left = left.push(widgets::small(
            p,
            ctx.t("Tick the apps you want to remove."),
        ));
    } else {
        left = left.push(widgets::small(
            p,
            ctx.t("You will be able to review before anything is removed."),
        ));
    }
    widgets::card(
        p,
        row![
            left,
            Space::new().width(Length::Fill),
            widgets::action(
                p,
                ButtonKind::Primary,
                button_label,
                Some(Icon::Trash),
                can.then(|| wrap(Msg::Review))
            )
        ]
        .align_y(Alignment::Center)
        .spacing(theme::GAP),
    )
    .into()
}

fn group_text(group: Group) -> (&'static str, &'static str, Icon) {
    match group {
        Group::Recommended => (
            "Recommended to remove",
            "Apps most people never use.",
            Icon::Sparkles,
        ),
        Group::Sponsored => (
            "Ads and games",
            "Sponsored apps and games that come pre-installed.",
            Icon::Bell,
        ),
        Group::Promotions => (
            "Microsoft extras",
            "Newer Microsoft features that are pushed to you. Remove them only if you don't use them.",
            Icon::Bot,
        ),
        Group::Utilities => (
            "Everyday tools",
            "Handy tools you may already use. Leave these unticked unless you're sure.",
            Icon::Wrench,
        ),
        Group::Gaming => (
            "Gaming",
            "Xbox and Game Bar.",
            Icon::Gamepad,
        ),
    }
}

fn group_card<'a>(
    state: &'a State,
    ctx: &'a Ctx,
    group: Group,
    members: &[u16],
) -> Element<'a, Message> {
    let p = ctx.palette;
    let (title, subtitle, icon) = group_text(group);
    let chosen = members
        .iter()
        .filter(|i| state.selected.contains(i))
        .count();
    let tick_state = if chosen == 0 {
        Tick::Off
    } else if chosen == members.len() {
        Tick::All
    } else {
        Tick::Some
    };
    let count = format!("{chosen} / {}", members.len());
    let head = row_button(
        p,
        row![
            tick(p, tick_state),
            widgets::icon(icon, 18.0, p.text_muted),
            column![
                text(ctx.t(title))
                    .size(theme::H2)
                    .font(theme::SEMIBOLD)
                    .color(p.text),
                widgets::small(p, ctx.t(subtitle)),
            ]
            .spacing(2),
            Space::new().width(Length::Fill),
            widgets::small(p, count),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        Some(wrap(Msg::ToggleGroup(group))),
    );
    let mut col = column![head].spacing(2);
    if group == Group::Gaming {
        col = col.push(banner(
            p,
            Icon::AlertTriangle,
            Tone::Warn,
            ctx.t("Game Bar recording and Xbox games may stop working"),
        ));
    }
    for (n, &index) in members.iter().enumerate() {
        if n > 0 || group == Group::Gaming {
            col = col.push(divider(p));
        }
        let app = app_of(index);
        let mut line = row![
            tick(
                p,
                if state.selected.contains(&index) {
                    Tick::All
                } else {
                    Tick::Off
                }
            ),
            text(ctx.t(app.name))
                .size(theme::BODY)
                .font(theme::MEDIUM)
                .color(p.text),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if app.store_id.is_none() {
            line = line.push(widgets::pill(
                p,
                ctx.t("Can't be restored automatically"),
                Tone::Warn,
            ));
        }
        col = col.push(row_button(p, line, Some(wrap(Msg::Toggle(index)))));
    }
    widgets::card(p, col).padding(12).into()
}

fn divider<'a>(p: Palette) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.border)),
            ..container::Style::default()
        })
        .into()
}

fn banner<'a>(p: Palette, icon: Icon, tone: Tone, message: String) -> Element<'a, Message> {
    container(
        row![
            widgets::icon(icon, 16.0, p.tone(tone)),
            text(message).size(theme::BODY).color(p.text)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([10, 14])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(p.tint(tone))),
        border: Border {
            radius: theme::RADIUS_SMALL.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn option_card<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let line = row![
        tick(
            p,
            if state.block_again {
                Tick::All
            } else {
                Tick::Off
            }
        ),
        column![
            text(ctx.t("Stop Windows from adding suggested apps again"))
                .size(theme::BODY)
                .font(theme::SEMIBOLD)
                .color(p.text),
            widgets::small(
                p,
                ctx.t("Some editions of Windows may still show a few suggestions.")
            ),
        ]
        .spacing(2),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    widgets::card(p, row_button(p, line, Some(wrap(Msg::ToggleBlock))))
        .padding(12)
        .into()
}

// ---- removed apps tab ---------------------------------------------------

/// Removed (and not yet restored) apps, newest first, one line per app.
fn removed_entries(state: &State) -> Vec<(u16, u64)> {
    let mut out: Vec<(u16, u64)> = Vec::new();
    for batch in state.journal.iter().rev() {
        for r in batch.removed.iter().filter(|r| !r.restored) {
            if (r.index as usize) < debloat::catalog().len()
                && !out.iter().any(|(i, _)| *i == r.index)
            {
                out.push((r.index, batch.t));
            }
        }
    }
    out
}

fn ago(ctx: &Ctx, t: u64) -> String {
    let days = debloat::now().saturating_sub(t) / 86_400;
    match days {
        0 => ctx.t("Removed today"),
        1 => ctx.t("Removed yesterday"),
        n => ctx.t("Removed {n} days ago").replace("{n}", &n.to_string()),
    }
}

fn removed_tab<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let entries = removed_entries(state);
    if entries.is_empty() {
        return status_card(
            ctx,
            Icon::Package,
            Tone::Neutral,
            ctx.t("No removed apps"),
            ctx.t("Apps you remove will show up here so you can bring them back."),
            None,
        );
    }
    let mut col = column![].spacing(2);
    for (n, (index, t)) in entries.into_iter().enumerate() {
        if n > 0 {
            col = col.push(divider(p));
        }
        let app = app_of(index);
        let restoring = state.restoring == Some(index);
        let mut info = column![
            text(ctx.t(app.name))
                .size(theme::BODY)
                .font(theme::MEDIUM)
                .color(p.text),
            widgets::small(p, ago(ctx, t)),
        ]
        .spacing(2);
        let control: Element<'a, Message> = if app.store_id.is_none() {
            info = info.push(widgets::small(
                p,
                ctx.t("You can look for it in the Microsoft Store yourself."),
            ));
            widgets::pill(p, ctx.t("Can't be restored automatically"), Tone::Warn)
        } else {
            let label = if restoring {
                ctx.t("Restoring…")
            } else {
                ctx.t("Restore")
            };
            let enabled = state.restoring.is_none() && !ctx.busy;
            widgets::action(
                p,
                ButtonKind::Secondary,
                label,
                Some(Icon::Undo),
                enabled.then(|| wrap(Msg::Restore(index))),
            )
        };
        col = col.push(
            container(
                row![info, Space::new().width(Length::Fill), control]
                    .spacing(theme::GAP)
                    .align_y(Alignment::Center),
            )
            .padding([10, 8]),
        );
    }
    widgets::card(p, col).padding(12).into()
}

// ---- sheets --------------------------------------------------------------

fn sheet_view<'a>(state: &'a State, sheet: &'a Sheet, ctx: &'a Ctx) -> Element<'a, Message> {
    match sheet {
        Sheet::Review => review_sheet(state, ctx),
        Sheet::Working(items) => working_sheet(items, ctx),
        Sheet::Done(done) => result_sheet(state, done, ctx),
        Sheet::None => Space::new().into(),
    }
}

fn review_sheet<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let indices: Vec<u16> = installed_indices(state)
        .into_iter()
        .filter(|i| state.selected.contains(i))
        .collect();
    let n = indices.len();
    let mut list = column![].spacing(6);
    for &i in &indices {
        list = list.push(
            row![
                widgets::icon(Icon::Package, 14.0, p.text_muted),
                text(ctx.t(app_of(i).name)).size(theme::BODY).color(p.text)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }
    let mut col = column![
        widgets::h1(p, count_text(ctx, n, "Remove {n} app?", "Remove {n} apps?")),
        widgets::muted(
            p,
            ctx.t("These apps will be removed for everyone who uses this PC. Your own files are not touched."),
        ),
        container(scrollable(container(list).padding(4))).max_height(220),
    ]
    .spacing(14);
    let manual: Vec<String> = indices
        .iter()
        .filter(|i| app_of(**i).store_id.is_none())
        .map(|i| ctx.t(app_of(*i).name))
        .collect();
    if manual.is_empty() {
        col = col.push(banner(
            p,
            Icon::Info,
            Tone::Neutral,
            ctx.t("You can reinstall these later from this page."),
        ));
    } else {
        col = col.push(banner(
            p,
            Icon::Info,
            Tone::Neutral,
            ctx.t("You can reinstall most of these later from the Removed apps tab."),
        ));
        col = col.push(banner(
            p,
            Icon::AlertTriangle,
            Tone::Warn,
            format!(
                "{} {}",
                ctx.t("These can't be restored automatically:"),
                manual.join(", ")
            ),
        ));
    }
    if indices.iter().any(|i| app_of(*i).group == Group::Gaming) {
        col = col.push(banner(
            p,
            Icon::AlertTriangle,
            Tone::Warn,
            ctx.t("Game Bar recording and Xbox games may stop working"),
        ));
    }
    if state.block_again {
        col = col.push(widgets::small(
            p,
            ctx.t("We'll also ask Windows not to add suggested apps again."),
        ));
    }
    col.push(
        row![
            Space::new().width(Length::Fill),
            widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t("Cancel"),
                None,
                Some(wrap(Msg::Cancel))
            ),
            widgets::action(
                p,
                ButtonKind::Danger,
                count_text(ctx, n, "Remove {n} app", "Remove {n} apps"),
                Some(Icon::Trash),
                (!ctx.busy).then(|| wrap(Msg::Confirm)),
            ),
        ]
        .spacing(10),
    )
    .into()
}

fn working_sheet<'a>(items: &'a [(u16, Step)], ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut list = column![].spacing(10);
    for (index, step) in items {
        let (icon, tone, note) = match step {
            Step::Waiting => (Icon::Package, Tone::Neutral, ctx.t("Waiting")),
            Step::Working => (Icon::Refresh, Tone::Brand, ctx.t("Removing…")),
            Step::Done(ItemResult::Removed) => (Icon::CheckCircle, Tone::Good, ctx.t("Removed")),
            Step::Done(ItemResult::Protected) => (
                Icon::Info,
                Tone::Neutral,
                ctx.t("Windows protects this app"),
            ),
            Step::Done(ItemResult::Failed(_)) => {
                (Icon::AlertTriangle, Tone::Bad, ctx.t("Couldn't remove"))
            }
        };
        list = list.push(
            row![
                widgets::icon(icon, 18.0, p.tone(tone)),
                text(ctx.t(app_of(*index).name))
                    .size(theme::BODY)
                    .color(p.text),
                Space::new().width(Length::Fill),
                widgets::small(p, note),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );
    }
    column![
        widgets::h1(p, ctx.t("Removing apps…")),
        widgets::muted(
            p,
            ctx.t("Please keep this window open. This can take a few minutes.")
        ),
        container(scrollable(container(list).padding(4))).max_height(300),
    ]
    .spacing(14)
    .into()
}

fn names(ctx: &Ctx, indices: impl Iterator<Item = u16>) -> Vec<String> {
    let mut v: Vec<u16> = indices.collect();
    v.sort_unstable();
    v.dedup();
    v.into_iter().map(|i| ctx.t(app_of(i).name)).collect()
}

fn result_block<'a>(
    p: Palette,
    icon: Icon,
    tone: Tone,
    title: String,
    list: Vec<String>,
) -> Element<'a, Message> {
    column![
        row![
            widgets::icon(icon, 18.0, p.tone(tone)),
            text(title)
                .size(theme::BODY)
                .font(theme::SEMIBOLD)
                .color(p.text)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        widgets::muted(p, list.join(", ")),
    ]
    .spacing(4)
    .into()
}

fn result_sheet<'a>(state: &'a State, done: &'a Finished, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let mut col = column![].spacing(14);
    let mut technical: Vec<String> = Vec::new();
    match (&done.batch, &done.error) {
        (Some(batch), _) => {
            let removed = names(ctx, batch.removed.iter().map(|r| r.index));
            let protected = names(ctx, batch.skipped.iter().copied());
            let failed = names(ctx, batch.failed.iter().map(|f| f.index));
            let title = if batch.removed.is_empty() && failed.is_empty() && protected.is_empty() {
                ctx.t("Nothing needed removing")
            } else if removed.is_empty() {
                ctx.t("No apps were removed")
            } else {
                count_text(ctx, removed.len(), "{n} app removed", "{n} apps removed")
            };
            col = col.push(widgets::h1(p, title));
            if !removed.is_empty() {
                col = col.push(result_block(
                    p,
                    Icon::CheckCircle,
                    Tone::Good,
                    ctx.t("Removed"),
                    removed,
                ));
            }
            if !protected.is_empty() {
                col = col.push(result_block(
                    p,
                    Icon::Info,
                    Tone::Neutral,
                    ctx.t("Windows protects these apps"),
                    protected,
                ));
            }
            if !failed.is_empty() {
                col = col.push(result_block(
                    p,
                    Icon::AlertTriangle,
                    Tone::Bad,
                    ctx.t("Couldn't remove"),
                    failed,
                ));
                col = col.push(widgets::small(
                    p,
                    ctx.t("Restart your PC and try again. Nothing else was changed."),
                ));
            }
            for f in &batch.failed {
                technical.push(format!("{}: {}", app_of(f.index).family, f.reason));
            }
            for r in &batch.removed {
                technical.push(format!("removed {} {}", r.package, r.version));
            }
        }
        (None, error) => {
            col = col.push(widgets::h1(p, ctx.t("We couldn't remove the apps")));
            col = col.push(widgets::muted(
                p,
                ctx.t("Nothing was changed. Please try again."),
            ));
            if let Some(e) = error {
                technical.push(e.clone());
            }
        }
    }
    if done.asked_to_block {
        let (icon, tone, msg) = match (done.policy_ok, done.user_ok) {
            (Some(true), Some(true)) => (
                Icon::ShieldCheck,
                Tone::Good,
                ctx.t("Windows was asked not to add suggested apps again. This works best on Windows Enterprise and Education."),
            ),
            (Some(false), Some(false)) | (Some(false), None) | (None, Some(false)) => (
                Icon::Info,
                Tone::Warn,
                ctx.t("We couldn't change the setting that stops suggested apps."),
            ),
            (None, None) | (Some(true), None) | (None, Some(true)) => (
                Icon::Info,
                Tone::Neutral,
                ctx.t("Asking Windows not to add suggested apps…"),
            ),
            (Some(false), Some(true)) | (Some(true), Some(false)) => (
                Icon::Info,
                Tone::Neutral,
                ctx.t("Windows was asked not to add suggested apps again, but this may not stop every suggestion."),
            ),
        };
        col = col.push(banner(p, icon, tone, msg));
    }
    if !technical.is_empty() {
        col = col.push(technical_details(state, ctx, technical));
    }
    col.push(row![
        Space::new().width(Length::Fill),
        widgets::action(
            p,
            ButtonKind::Primary,
            ctx.t("Done"),
            None,
            Some(wrap(Msg::CloseResult))
        )
    ])
    .into()
}

fn technical_details<'a>(
    state: &'a State,
    ctx: &'a Ctx,
    lines: Vec<String>,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let open = state.details;
    let head = button(
        row![
            widgets::icon(
                if open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                14.0,
                p.text_muted
            ),
            text(ctx.t("Technical details"))
                .size(theme::SMALL)
                .font(theme::MEDIUM)
                .color(p.text_muted),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([4, 0])
    .on_press(wrap(Msg::ToggleDetails))
    .style(|_, _| button::Style::default());
    let mut col = column![head].spacing(6);
    if open {
        let mut block = column![].spacing(4);
        for line in lines {
            block = block.push(text(line).size(theme::SMALL).color(p.text_muted));
        }
        col = col.push(
            container(scrollable(block))
                .max_height(140)
                .padding(10)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(p.surface_alt)),
                    border: Border {
                        radius: theme::RADIUS_SMALL.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
        );
    }
    col.into()
}
