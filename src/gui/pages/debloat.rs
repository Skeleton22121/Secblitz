//! Clean up apps page. OWNER: debloat agent.
//!
//! Flow: scan -> pick apps -> review sheet (Cancel is the safe choice) ->
//! working sheet with per-app progress -> result. A second tab lists removed
//! apps with a Restore button.
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind, CheckState};
use crate::gui::widgets::{anim, progress};
use crate::gui::{blocking, blocking_stream, Ctx, Message};
use iced::widget::{column, container, row, scrollable, space};
use iced::{Alignment, Background, Border, Element, Length, Padding, Subscription, Task};
use secblitz::debloat::{self, Batch, Group, Installed, ItemResult, Progress};
use std::collections::BTreeSet;
use std::time::Instant;

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
    Done(ItemResult, Instant),
}

#[derive(Debug, Default)]
enum Sheet {
    #[default]
    None,
    Review,
    Working(Vec<(u16, Step)>),
    Done(Box<Finished>),
}

#[derive(Debug)]
struct Finished {
    batch: Option<Batch>,
    error: Option<String>,
    /// Machine-wide setting written (None = not asked for).
    policy_ok: Option<bool>,
    /// Per-user setting written through the launcher (None = not yet known).
    user_ok: Option<bool>,
    asked_to_block: bool,
    /// When the result appeared (drives the check draw-in).
    at: Instant,
}

impl Default for Finished {
    fn default() -> Self {
        Finished {
            batch: None,
            error: None,
            policy_ok: None,
            user_ok: None,
            asked_to_block: false,
            at: Instant::now(),
        }
    }
}

#[derive(Debug)]
pub struct State {
    /// Generation of the newest scan; results from older ones are ignored.
    scan_gen: u32,
    tab: Tab,
    scan: Scan,
    installed: Vec<Installed>,
    selected: BTreeSet<u16>,
    initialised: bool,
    block_again: bool,
    sheet: Sheet,
    details: bool,
    /// Groups whose list is unfolded (Recommended starts open).
    open: Vec<Group>,
    /// Groups showing every row instead of the first few.
    expanded: Vec<Group>,
    journal: Vec<Batch>,
    restoring: Option<u16>,
    /// App whose restore failed because the PC is offline (shows Retry).
    offline: Option<u16>,
    /// Machine-wide setting result, arrives just before the batch result.
    policy: Option<bool>,
    /// Precomputed in update(): installed catalog indices, per group.
    groups: Vec<(Group, Vec<u16>)>,
    /// Precomputed in update(): removed and not yet restored (index, unix time).
    removed: Vec<(u16, u64)>,
    /// Start of the current wait animation, and the last frame time.
    spin: anim::Clock,
    now: Instant,
}

impl Default for State {
    fn default() -> Self {
        State {
            scan_gen: 0,
            tab: Tab::Apps,
            scan: Scan::Loading,
            installed: Vec::new(),
            selected: BTreeSet::new(),
            initialised: false,
            block_again: false,
            sheet: Sheet::None,
            details: false,
            open: vec![Group::Recommended],
            expanded: Vec::new(),
            journal: Vec::new(),
            restoring: None,
            offline: None,
            policy: None,
            groups: Vec::new(),
            removed: Vec::new(),
            spin: anim::Clock::new(),
            now: Instant::now(),
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
    Scanned(u32, Result<Vec<Installed>, String>),
    JournalLoaded(Vec<Batch>),
    Rescan,
    SetTab(Tab),
    Toggle(u16),
    ToggleGroup(Group),
    ToggleOpen(Group),
    ToggleMore(Group),
    ToggleBlock,
    Review,
    Cancel,
    Confirm,
    Run(Run),
    UserBlocked(bool),
    /// Animation frame (only subscribed while something moves).
    Frame(Instant),
    ToggleDetails,
    CloseResult,
    Restore(u16),
    Restored(u16, Result<crate::broker::Reply, String>),
}

fn wrap(msg: Msg) -> Message {
    Message::Debloat(msg)
}

/// Start an inventory tagged with a fresh generation; an older scan that
/// lands later is dropped.
fn inventory_task(state: &mut State) -> Task<Message> {
    state.scan_gen = state.scan_gen.wrapping_add(1);
    let generation = state.scan_gen;
    Task::perform(
        blocking(|| debloat::inventory().map_err(|e| format!("{e:#}"))),
        move |r| wrap(Msg::Scanned(generation, r)),
    )
}

/// Inventory plus the removal journal.
fn scan_task(state: &mut State) -> Task<Message> {
    Task::batch([
        inventory_task(state),
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
    start_scan(state);
    scan_task(state)
}

fn start_scan(state: &mut State) {
    state.scan = Scan::Loading;
    state.now = Instant::now();
    state.spin = anim::Clock::at(state.now);
}

/// Frame subscription: on only while an animation is actually running.
pub fn subscription(state: &State) -> Subscription<Message> {
    if is_animating(state) {
        iced::window::frames().map(|t| wrap(Msg::Frame(t)))
    } else {
        Subscription::none()
    }
}

fn is_animating(state: &State) -> bool {
    if !anim::animating() {
        return false;
    }
    if matches!(state.scan, Scan::Loading) || state.restoring.is_some() {
        return true;
    }
    match &state.sheet {
        Sheet::Working(_) => true,
        Sheet::Done(done) => {
            !done.at_done(state.now) || (done.asked_to_block && done.user_ok.is_none())
        }
        _ => false,
    }
}

impl Finished {
    fn at_done(&self, now: Instant) -> bool {
        anim::Clock::at(self.at).done(anim::SLOW, now)
    }
}

fn refresh_removed(state: &mut State) {
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
    state.removed = out;
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
        Msg::Scanned(generation, _) if generation != state.scan_gen => Task::none(),
        Msg::Scanned(_, Ok(found)) => {
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
            state.groups = Group::ALL
                .iter()
                .filter_map(|g| {
                    let members: Vec<u16> = present
                        .iter()
                        .copied()
                        .filter(|i| app_of(*i).group == *g)
                        .collect();
                    (!members.is_empty()).then_some((*g, members))
                })
                .collect();
            state.scan = Scan::Ready;
            Task::none()
        }
        Msg::Scanned(_, Err(e)) => {
            state.scan = Scan::Failed(e);
            Task::none()
        }
        Msg::JournalLoaded(j) => {
            state.journal = j;
            refresh_removed(state);
            Task::none()
        }
        Msg::Rescan => {
            start_scan(state);
            scan_task(state)
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
        Msg::ToggleOpen(g) => {
            flip(&mut state.open, g);
            Task::none()
        }
        Msg::ToggleMore(g) => {
            flip(&mut state.expanded, g);
            Task::none()
        }
        Msg::Frame(now) => {
            state.now = now;
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
            state.offline = None;
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
                    refresh_removed(state);
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
                        // The app is installed again: list it under "Apps to
                        // remove" without a loading state (the journal is
                        // reloaded above, after it is marked).
                        inventory_task(state),
                        toast(
                            format!("{name} {}", ctx.t("is back on your PC.")),
                            Tone::Good,
                        ),
                    ])
                }
                Ok(crate::broker::Reply::Offline) => {
                    state.offline = Some(index);
                    Task::none()
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

fn flip(list: &mut Vec<Group>, g: Group) {
    if let Some(pos) = list.iter().position(|x| *x == g) {
        list.remove(pos);
    } else {
        list.push(g);
    }
}

fn pal(ctx: &Ctx) -> Palette {
    Palette::of(ctx.palette.mode)
}

fn confirm(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    if ctx.busy || !matches!(state.sheet, Sheet::Review) {
        return Task::none();
    }
    let indices: Vec<u16> = state.selected.iter().copied().collect();
    if indices.is_empty() {
        state.sheet = Sheet::None;
        return Task::none();
    }
    ctx.busy = true;
    state.details = false;
    state.now = Instant::now();
    state.spin = anim::Clock::at(state.now);
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
        // Task 7 gives saving its own wording; until then it shows as working.
        Run::Step(Progress::Saving(i) | Progress::Started(i)) => {
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
                    item.1 = Step::Done(result, Instant::now());
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
                        refresh_removed(state);
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
            state.now = done.at;
            state.sheet = Sheet::Done(Box::new(done));
            tasks.push(scan_task(state));
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
    let p = pal(ctx);
    let header = widgets::page_header(
        p,
        ctx.t("Clean up apps"),
        Some(ctx.t(
            "Remove apps that came with Windows but you don't need. You can reinstall most of them later.",
        )),
    );
    let removed_label = if state.removed.is_empty() {
        ctx.t("Removed apps")
    } else {
        format!("{} ({})", ctx.t("Removed apps"), state.removed.len())
    };
    let tabs = widgets::segmented(
        p,
        &[
            (Tab::Apps, ctx.t("Apps to remove")),
            (Tab::Removed, removed_label),
        ],
        state.tab,
        |t| wrap(Msg::SetTab(t)),
    );
    let body: Element<'a, Message> = match state.tab {
        Tab::Apps => apps_tab(state, ctx),
        Tab::Removed => removed_tab(state, ctx),
    };
    // Header to content is S6 on every page; tabs sit closer to their list.
    column![header, column![tabs, body].spacing(theme::S4)]
        .spacing(theme::S6)
        .width(Length::Fill)
        .into()
}

/// The open review / working / result sheet, drawn by the shell above the
/// whole window (outside the page's scrollable).
pub fn modal<'a>(state: &'a State, ctx: &'a Ctx) -> Option<Element<'a, Message>> {
    match &state.sheet {
        Sheet::None => None,
        Sheet::Review => Some(review_sheet(state, ctx)),
        Sheet::Working(items) => Some(working_sheet(state, items, ctx)),
        Sheet::Done(done) => Some(result_sheet(state, done, ctx)),
    }
}

fn loading_state<'a>(state: &State, ctx: &Ctx) -> Element<'a, Message> {
    let p = pal(ctx);
    container(
        column![
            anim::spinner(32.0, p.text_muted, state.spin.elapsed_at(state.now)),
            widgets::h2(p, ctx.t("Looking for apps you can remove…")),
            widgets::muted(p, ctx.t("This only takes a moment.")),
        ]
        .spacing(theme::S3)
        .align_x(Alignment::Center),
    )
    .center_x(Length::Fill)
    .padding([theme::S10, theme::S5])
    .into()
}

fn apps_tab<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = pal(ctx);
    match &state.scan {
        Scan::Loading if state.groups.is_empty() => loading_state(state, ctx),
        Scan::Failed(technical) => column![
            widgets::empty_state(
                p,
                Icon::AlertTriangle,
                ctx.t("We couldn't look at your apps"),
                ctx.t("Nothing was changed. Please try again."),
                Some(widgets::action(
                    p,
                    ButtonKind::Secondary,
                    ctx.t("Try again"),
                    Some(Icon::Refresh),
                    Some(wrap(Msg::Rescan)),
                )),
            ),
            details(state, ctx, vec![technical.clone()]),
        ]
        .spacing(theme::S4)
        .into(),
        _ if state.groups.is_empty() => widgets::empty_state(
            p,
            Icon::CheckCircle,
            ctx.t("Nothing to clean up"),
            ctx.t("None of the apps we know about are on this PC."),
            None,
        ),
        _ => {
            let mut col = column![].spacing(theme::S1);
            for (group, members) in &state.groups {
                col = col.push(group_card(state, ctx, *group, members));
            }
            col.into()
        }
    }
}

/// The action bar the shell pins below the scrolling list, so Remove stays in
/// reach however long the list is. Only shown while the app list is on screen.
pub fn footer<'a>(state: &'a State, ctx: &'a Ctx) -> Option<Element<'a, Message>> {
    let listing = state.tab == Tab::Apps
        && !state.groups.is_empty()
        && !matches!(state.scan, Scan::Failed(_));
    listing.then(|| action_bar(state, ctx))
}

/// Selected count plus the one primary action.
fn action_bar<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = pal(ctx);
    let n = state.selected.len();
    let can = n > 0 && !ctx.busy;
    let button_label = if n == 0 {
        ctx.t("Remove apps")
    } else {
        count_text(ctx, n, "Remove {n} app", "Remove {n} apps")
    };
    let hint = if ctx.busy {
        ctx.t("Please wait until the current task has finished.")
    } else if n == 0 {
        ctx.t("Tick the apps you want to remove.")
    } else {
        ctx.t("You will be able to review before anything is removed.")
    };
    let title = if n == 0 {
        ctx.t("No apps selected")
    } else {
        count_text(ctx, n, "{n} app selected", "{n} apps selected")
    };
    row![
        column![widgets::h2(p, title), widgets::small(p, hint)].spacing(theme::S1),
        space::horizontal(),
        widgets::action(
            p,
            ButtonKind::Primary,
            button_label,
            Some(Icon::Trash),
            can.then(|| wrap(Msg::Review))
        )
    ]
    .align_y(Alignment::Center)
    .spacing(theme::S4)
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
        Group::Gaming => ("Gaming", "Xbox and Game Bar.", Icon::Gamepad),
    }
}

/// Rows shown before "Show more" in one group.
const GROUP_ROWS: usize = 8;

fn group_card<'a>(
    state: &'a State,
    ctx: &'a Ctx,
    group: Group,
    members: &'a [u16],
) -> Element<'a, Message> {
    let p = pal(ctx);
    let (title, subtitle, _) = group_text(group);
    let chosen = members
        .iter()
        .filter(|i| state.selected.contains(i))
        .count();
    let check = if chosen == 0 {
        CheckState::Off
    } else if chosen == members.len() {
        CheckState::On
    } else {
        CheckState::Mixed
    };
    let count = ctx
        .t("{a} of {b} selected")
        .replace("{a}", &chosen.to_string())
        .replace("{b}", &members.len().to_string());
    let open = state.open.contains(&group);
    let expanded = state.expanded.contains(&group);
    let mut body = column![].spacing(theme::S1);
    // Lined up with the app names below it.
    body = body.push(
        container(widgets::small(p, ctx.t(subtitle))).padding(Padding {
            top: theme::S1,
            right: theme::S4,
            bottom: theme::S1,
            left: widgets::explain::INDENT,
        }),
    );
    if group == Group::Gaming {
        body = body.push(widgets::inline_notice(
            p,
            Tone::Warn,
            ctx.t("Game Bar recording and Xbox games may stop working"),
        ));
    }
    for &index in widgets::limited(members, GROUP_ROWS, expanded) {
        let app = app_of(index);
        let box_ = widgets::checkbox(
            p,
            state.selected.contains(&index).into(),
            None,
            Some(wrap(Msg::Toggle(index))),
        );
        // A quiet note instead of a loud pill: muted icon plus small text.
        let (note, trailing): (Option<String>, Element<'a, Message>) = if app.store_id.is_none() {
            (
                Some(ctx.t("Can't be restored automatically")),
                row![widgets::icon(Icon::Info, 16.0, p.text_muted), box_]
                    .spacing(theme::S3)
                    .align_y(Alignment::Center)
                    .into(),
            )
        } else {
            (None, box_)
        };
        body = body.push(widgets::row_item(
            p,
            Some(Icon::Package),
            ctx.t(app.name),
            note,
            trailing,
            Some(wrap(Msg::Toggle(index))),
        ));
    }
    if members.len() > GROUP_ROWS {
        let label = if expanded {
            ctx.t("Show less")
        } else {
            ctx.t("Show {n} more")
                .replace("{n}", &(members.len() - GROUP_ROWS).to_string())
        };
        body = body.push(widgets::show_more_button(
            p,
            label,
            wrap(Msg::ToggleMore(group)),
        ));
    }
    row![
        container(widgets::checkbox(
            p,
            check,
            None,
            Some(wrap(Msg::ToggleGroup(group)))
        ))
        .padding([0.0, theme::S2])
        .height(theme::CONTROL + theme::S2)
        .center_y(theme::CONTROL + theme::S2),
        widgets::collapsible(
            p,
            ctx.t(title),
            Some(count),
            open,
            wrap(Msg::ToggleOpen(group)),
            body,
        ),
    ]
    .align_y(Alignment::Start)
    .into()
}

// ---- removed apps tab ---------------------------------------------------

fn ago(ctx: &Ctx, t: u64) -> String {
    let days = debloat::now().saturating_sub(t) / 86_400;
    match days {
        0 => ctx.t("Removed today"),
        1 => ctx.t("Removed yesterday"),
        n => ctx.t("Removed {n} days ago").replace("{n}", &n.to_string()),
    }
}

fn removed_tab<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = pal(ctx);
    if state.removed.is_empty() {
        return widgets::empty_state(
            p,
            Icon::Package,
            ctx.t("No removed apps"),
            ctx.t("Apps you remove will show up here so you can bring them back."),
            None,
        );
    }
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for &(index, t) in &state.removed {
        let app = app_of(index);
        let restoring = state.restoring == Some(index);
        let (subtitle, trailing): (String, Element<'a, Message>) = if app.store_id.is_none() {
            (
                format!(
                    "{} · {}",
                    ago(ctx, t),
                    ctx.t("You can look for it in the Microsoft Store yourself.")
                ),
                widgets::icon(Icon::Info, 16.0, p.text_muted),
            )
        } else if restoring {
            (
                ctx.t("Restoring…"),
                anim::spinner(16.0, p.text_muted, state.spin.elapsed_at(state.now)),
            )
        } else if state.offline == Some(index) {
            let enabled = state.restoring.is_none() && !ctx.busy;
            (
                ctx.t("You're offline. Connect to the internet and try again."),
                widgets::action(
                    p,
                    widgets::ButtonKind::Secondary,
                    ctx.t("Retry"),
                    Some(Icon::Refresh),
                    enabled.then(|| wrap(Msg::Restore(index))),
                ),
            )
        } else {
            let enabled = state.restoring.is_none() && !ctx.busy;
            let items = if enabled {
                vec![(
                    Icon::Undo,
                    ctx.t("Restore"),
                    wrap(Msg::Restore(index)),
                    false,
                )]
            } else {
                Vec::new()
            };
            (ago(ctx, t), widgets::overflow_menu(p, items))
        };
        rows.push(widgets::row_item(
            p,
            Some(Icon::Package),
            ctx.t(app.name),
            Some(subtitle),
            trailing,
            None,
        ));
    }
    widgets::group(p, ctx.t("Removed apps"), None, None, rows)
}

// ---- sheets --------------------------------------------------------------

fn scroll_list<'a>(
    p: theme::Palette,
    list: impl Into<Element<'a, Message>>,
    max: f32,
) -> Element<'a, Message> {
    container(
        scrollable(container(list).padding(theme::S3).width(Length::Fill))
            .direction(widgets::controls::scrollbar())
            .style(widgets::controls::scroll_style(p)),
    )
    .max_height(max)
    .style(move |_| container::Style {
        background: Some(Background::Color(p.surface_alt)),
        border: Border {
            radius: theme::R.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn review_sheet<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = pal(ctx);
    let indices: Vec<u16> = state.selected.iter().copied().collect();
    let n = indices.len();
    let mut list = column![].spacing(theme::S2);
    for &i in &indices {
        list = list.push(
            row![
                widgets::icon(Icon::Package, 16.0, p.text_muted),
                widgets::body(p, ctx.t(app_of(i).name))
            ]
            .spacing(theme::S3)
            .align_y(Alignment::Center),
        );
    }
    let mut col = column![
        widgets::h2(p, count_text(ctx, n, "Remove {n} app?", "Remove {n} apps?")),
        widgets::muted(
            p,
            if n == 1 {
                ctx.t("This app will be removed for everyone who uses this PC. Your own files are not touched.")
            } else {
                ctx.t("These apps will be removed for everyone who uses this PC. Your own files are not touched.")
            },
        ),
        scroll_list(p, list, 220.0),
    ]
    .spacing(theme::S3);
    let manual: Vec<String> = indices
        .iter()
        .filter(|i| app_of(**i).store_id.is_none())
        .map(|i| ctx.t(app_of(*i).name))
        .collect();
    // Where to restore: name the tab whenever something can come back.
    if manual.len() < n {
        col = col.push(widgets::inline_notice(
            p,
            Tone::Neutral,
            if !manual.is_empty() {
                ctx.t("You can bring most of these back later from the Removed apps tab.")
            } else if n == 1 {
                ctx.t("You can bring it back later from the Removed apps tab.")
            } else {
                ctx.t("You can bring these back later from the Removed apps tab.")
            },
        ));
    }
    if !manual.is_empty() {
        col = col.push(widgets::inline_notice(
            p,
            Tone::Warn,
            format!(
                "{} {}",
                if manual.len() == 1 {
                    ctx.t("This can't be restored automatically:")
                } else {
                    ctx.t("These can't be restored automatically:")
                },
                manual.join(", ")
            ),
        ));
    }
    if indices.iter().any(|i| app_of(*i).group == Group::Gaming) {
        col = col.push(widgets::inline_notice(
            p,
            Tone::Warn,
            ctx.t("Game Bar recording and Xbox games may stop working"),
        ));
    }
    // Off by default; one plain sentence says what it does.
    col = col.push(
        column![
            widgets::checkbox(
                p,
                state.block_again.into(),
                Some(ctx.t("Stop Windows from adding suggested apps again")),
                Some(wrap(Msg::ToggleBlock)),
            ),
            container(widgets::small(
                p,
                ctx.t("Windows sometimes installs apps on its own. Tick this to ask it to stop."),
            ))
            // Lined up with the checkbox label.
            .padding(Padding {
                left: theme::S1 + theme::CHECK + theme::S3,
                ..Padding::ZERO
            }),
        ]
        .spacing(theme::S1),
    );
    col.push(space::vertical().height(theme::S1))
        .push(
            row![
                space::horizontal(),
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
            .spacing(theme::S2),
        )
        .into()
}

fn working_sheet<'a>(
    state: &'a State,
    items: &'a [(u16, Step)],
    ctx: &'a Ctx,
) -> Element<'a, Message> {
    let p = pal(ctx);
    let finished = items
        .iter()
        .filter(|(_, s)| matches!(s, Step::Done(..)))
        .count();
    let spin = state.spin.elapsed_at(state.now);
    let mut list = column![].spacing(theme::S3);
    for (index, step) in items {
        let (lead, note): (Element<'a, Message>, String) = match step {
            Step::Waiting => (
                widgets::icon(Icon::Package, 18.0, p.text_muted),
                ctx.t("Waiting"),
            ),
            Step::Working => (anim::spinner(20.0, p.text, spin), ctx.t("Removing…")),
            Step::Done(ItemResult::Removed, at) => (
                anim::check_draw(
                    18.0,
                    p.good,
                    anim::Clock::at(*at).progress_at(anim::SLOW, state.now),
                ),
                ctx.t("Removed"),
            ),
            Step::Done(ItemResult::Protected, _) => (
                widgets::icon(Icon::Info, 18.0, p.text_muted),
                ctx.t("Windows protects this app"),
            ),
            // Task 7 gives "left installed" its own wording.
            Step::Done(ItemResult::Failed(_) | ItemResult::Kept(_), at) => (
                anim::cross_draw(
                    18.0,
                    p.bad,
                    anim::Clock::at(*at).progress_at(anim::SLOW, state.now),
                ),
                ctx.t("Couldn't remove"),
            ),
        };
        list = list.push(
            row![
                container(lead).center(theme::CHECK),
                widgets::body(p, ctx.t(app_of(*index).name)),
                space::horizontal(),
                widgets::small(p, note),
            ]
            .spacing(theme::S3)
            .align_y(Alignment::Center),
        );
    }
    let ratio = if items.is_empty() {
        0.0
    } else {
        finished as f32 / items.len() as f32
    };
    column![
        widgets::h2(p, ctx.t("Removing apps…")),
        widgets::muted(
            p,
            ctx.t("Please keep this window open. This can take a few minutes.")
        ),
        progress::bar_eased(p, ratio, Tone::Brand),
        scroll_list(p, list, 300.0),
    ]
    .spacing(theme::S3)
    .into()
}

fn names(ctx: &Ctx, indices: impl Iterator<Item = u16>) -> Vec<String> {
    let mut v: Vec<u16> = indices.collect();
    v.sort_unstable();
    v.dedup();
    v.into_iter().map(|i| ctx.t(app_of(i).name)).collect()
}

fn result_block<'a>(
    p: theme::Palette,
    icon: Icon,
    tone: Tone,
    title: String,
    list: Vec<String>,
) -> Element<'a, Message> {
    column![
        row![
            widgets::icon(icon, 18.0, p.tone(tone)),
            widgets::body(p, title)
        ]
        .spacing(theme::S2)
        .align_y(Alignment::Center),
        widgets::muted(p, list.join(", ")),
    ]
    .spacing(theme::S1)
    .into()
}

fn result_sheet<'a>(state: &'a State, done: &'a Finished, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = pal(ctx);
    let t = anim::Clock::at(done.at).progress_at(anim::SLOW, state.now);
    let mut col = column![].spacing(theme::S3);
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
            let lead: Element<'a, Message> = if failed.is_empty() && !removed.is_empty() {
                anim::check_draw(40.0, p.good, t)
            } else if failed.is_empty() {
                widgets::icon(Icon::Info, 32.0, p.text_muted)
            } else {
                anim::warn_draw(40.0, p.warn, t)
            };
            col = col.push(lead).push(widgets::h2(p, title));
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
            col = col
                .push(anim::cross_draw(40.0, p.bad, t))
                .push(widgets::h2(p, ctx.t("We couldn't remove the apps")))
                .push(widgets::muted(
                    p,
                    ctx.t("Nothing was changed. Please try again."),
                ));
            if let Some(e) = error {
                technical.push(e.clone());
            }
        }
    }
    if done.asked_to_block {
        let note: Element<'a, Message> = match (done.policy_ok, done.user_ok) {
            (_, None) => row![
                anim::spinner(16.0, p.text_muted, state.spin.elapsed_at(state.now)),
                widgets::muted(p, ctx.t("Asking Windows not to add suggested apps…"))
            ]
            .spacing(theme::S3)
            .align_y(Alignment::Center)
            .into(),
            (Some(true), Some(true)) => widgets::inline_notice(
                p,
                Tone::Good,
                ctx.t("Windows was asked not to add suggested apps again. This works best on Windows Enterprise and Education."),
            ),
            (Some(false), Some(false)) | (None, Some(false)) => widgets::inline_notice(
                p,
                Tone::Warn,
                ctx.t("We couldn't change the setting that stops suggested apps."),
            ),
            _ => widgets::inline_notice(
                p,
                Tone::Neutral,
                ctx.t("Windows was asked not to add suggested apps again, but this may not stop every suggestion."),
            ),
        };
        col = col.push(note);
    }
    if !technical.is_empty() {
        col = col.push(details(state, ctx, technical));
    }
    col.push(space::vertical().height(theme::S1))
        .push(row![
            space::horizontal(),
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

/// "More details" expander with the raw lines (closed by default).
fn details<'a>(state: &'a State, ctx: &'a Ctx, lines: Vec<String>) -> Element<'a, Message> {
    let p = pal(ctx);
    let mut block = column![].spacing(theme::S1);
    for line in lines {
        block = block.push(widgets::small(p, line));
    }
    widgets::expander(
        p,
        ctx.t("More details"),
        state.details,
        wrap(Msg::ToggleDetails),
        container(
            scrollable(block)
                .direction(widgets::controls::scrollbar())
                .style(widgets::controls::scroll_style(p)),
        )
        .max_height(theme::DETAILS_MAX),
    )
}

/// Whether a finished scan is the newest one started.
pub fn is_current_scan(state: &State, generation: u32) -> bool {
    state.scan_gen == generation
}

/// The app list must not be reloaded under an open sheet.
pub fn is_busy(state: &State) -> bool {
    !matches!(state.sheet, Sheet::None)
}

/// Warm the page in the background. A list already on screen is refreshed
/// silently; only a page without data shows its loading state.
#[allow(clippy::items_after_test_module)]
pub fn preload(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let _ = ctx;
    if is_busy(state) {
        return Task::none();
    }
    if !matches!(state.scan, Scan::Ready) {
        start_scan(state);
    }
    scan_task(state)
}
