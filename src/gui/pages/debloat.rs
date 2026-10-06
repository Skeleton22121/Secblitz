//! Clean up apps page.
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind, CheckState};
use crate::gui::widgets::hairline::start_menu::{
    self, Fate, Filler, Labels, MenuApp, Outcome, StartMenu,
};
use crate::gui::widgets::hairline::{Glyph, Plate};
use crate::gui::widgets::{anim, progress};
use crate::gui::{blocking, blocking_stream, Ctx, Message};
use crate::i18n::Lang;
use iced::widget::image::Handle;
use iced::widget::{column, container, row, scrollable, space};
use iced::{Alignment, Background, Border, Element, Length, Padding, Subscription, Task};
use secblitz::debloat::offline::Restored;
use secblitz::debloat::{self, Batch, Group, Installed, ItemResult, Kept, Progress};
use std::collections::{BTreeMap, BTreeSet};
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
    Saving,
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
    Delete(u16),
}

#[derive(Debug)]
struct Finished {
    batch: Option<Batch>,
    error: Option<String>,
    policy_ok: Option<bool>,
    user_ok: Option<bool>,
    asked_to_block: bool,
    /// Apps left installed because no copy could be saved first.
    kept: Vec<(u16, Kept)>,
    steps: Vec<(u16, Step)>,
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
            kept: Vec::new(),
            steps: Vec::new(),
            at: Instant::now(),
        }
    }
}

#[derive(Debug)]
pub struct State {
    scan_gen: u32,
    tab: Tab,
    scan: Scan,
    installed: Vec<Installed>,
    selected: BTreeSet<u16>,
    initialised: bool,
    block_again: bool,
    sheet: Sheet,
    details: bool,
    open: Vec<Group>,
    expanded: Vec<Group>,
    journal: Vec<Batch>,
    restoring: Option<u16>,
    restoring_copy: bool,
    icons: BTreeMap<u16, Handle>,
    copies: BTreeSet<u16>,
    saved_bytes: u64,
    /// App whose restore failed because the PC is offline (shows Retry).
    offline: Option<u16>,
    policy: Option<bool>,
    groups: Vec<(Group, Vec<u16>)>,
    removed: Vec<(u16, u64)>,
    suggested_machine: bool,
    suggested_user: bool,
    allowing: Option<(u8, bool)>,
    spin: anim::Clock,
    now: Instant,
    run_at: Instant,
    menu_fillers: Vec<u16>,
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
            restoring_copy: false,
            icons: BTreeMap::new(),
            copies: BTreeSet::new(),
            saved_bytes: 0,
            offline: None,
            policy: None,
            groups: Vec::new(),
            removed: Vec::new(),
            suggested_machine: false,
            suggested_user: false,
            allowing: None,
            spin: anim::Clock::new(),
            now: Instant::now(),
            run_at: Instant::now(),
            menu_fillers: Vec::new(),
        }
    }
}

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
    Frame(Instant),
    ToggleDetails,
    CloseResult,
    Restore(u16),
    RestoreStore(u16),
    Restored(u16, Result<crate::broker::Reply, String>),
    RestoredOffline(u16, Result<Restored, String>),
    AskDelete(u16),
    Delete(u16),
    SuggestedMachine(bool),
    SuggestedUser(Result<crate::broker::Reply, String>),
    AllowSuggested,
    SuggestedAllowed(bool),
    Deleted(Result<(), String>),
    Copies(BTreeSet<u16>, u64),
    Icons(BTreeMap<u16, Handle>),
}

fn wrap(msg: Msg) -> Message {
    Message::Debloat(msg)
}

fn inventory_task(state: &mut State) -> Task<Message> {
    state.scan_gen = state.scan_gen.wrapping_add(1);
    let generation = state.scan_gen;
    Task::perform(
        blocking(|| debloat::inventory().map_err(|e| format!("{e:#}"))),
        move |r| wrap(Msg::Scanned(generation, r)),
    )
}

fn icons_task(installed: Vec<Installed>) -> Task<Message> {
    Task::perform(
        blocking(move || {
            debloat::icons::load(&installed)
                .into_iter()
                .map(|(i, img)| (i, Handle::from_rgba(img.width, img.height, img.pixels)))
                .collect::<BTreeMap<u16, Handle>>()
        }),
        |icons| wrap(Msg::Icons(icons)),
    )
}

fn app_glyph<'a>(p: Palette, state: &State, index: u16, size: f32) -> Element<'a, Message> {
    match state.icons.get(&index) {
        Some(handle) => iced::widget::image(handle.clone())
            .width(size)
            .height(size)
            .into(),
        None => widgets::icon(Icon::Package, size, p.text_muted),
    }
}

fn copies_task() -> Task<Message> {
    Task::perform(
        blocking(|| {
            let copies = debloat::catalog()
                .iter()
                .enumerate()
                .map(|(i, _)| i as u16)
                .filter(|i| debloat::offline::has_copy(*i))
                .collect::<BTreeSet<u16>>();
            (copies, debloat::offline::saved_bytes())
        }),
        |(c, b)| wrap(Msg::Copies(c, b)),
    )
}

fn suggested_task(ctx: &Ctx) -> Task<Message> {
    Task::batch([
        Task::perform(
            blocking(|| {
                debloat::suggested::journal_path()
                    .map(|p| debloat::suggested::recorded(&p))
                    .unwrap_or(false)
            }),
            |on| wrap(Msg::SuggestedMachine(on)),
        ),
        ctx.broker_task(
            crate::broker::Request::UserSetting(
                crate::user_settings::Setting::SuggestedApps,
                crate::user_settings::Op::Query,
            ),
            |r| wrap(Msg::SuggestedUser(r)),
        ),
    ])
}

fn allow_machine() -> bool {
    #[cfg(windows)]
    {
        use debloat::suggested;
        suggested::journal_path()
            .and_then(|p| suggested::undo(&mut suggested::MachinePolicy, &p))
            .is_ok()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

fn scan_task(state: &mut State) -> Task<Message> {
    Task::batch([
        inventory_task(state),
        copies_task(),
        Task::perform(blocking(debloat::journal::load), |j| {
            wrap(Msg::JournalLoaded(j))
        }),
    ])
}

pub fn on_enter(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    if !matches!(state.sheet, Sheet::None) {
        return Task::none();
    }
    start_scan(state);
    Task::batch([scan_task(state), suggested_task(ctx)])
}

fn start_scan(state: &mut State) {
    state.scan = Scan::Loading;
    state.now = Instant::now();
    state.spin = anim::Clock::at(state.now);
}

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
        Sheet::Working(items) => items.iter().any(|(_, step)| match step {
            Step::Done(_, at) => !anim::Clock::at(*at).done(anim::SLOW, state.now),
            _ => false,
        }),
        Sheet::Done(done) => done.asked_to_block && done.user_ok.is_none(),
        _ => false,
    }
}

fn refresh_removed(state: &mut State) {
    state.removed = debloat::journal::still_removed(&state.journal, debloat::catalog().len());
}

pub fn escape(state: &mut State) {
    if matches!(
        state.sheet,
        Sheet::Review | Sheet::Done(_) | Sheet::Delete(_)
    ) {
        state.sheet = Sheet::None;
    }
}

const BLOCKED_NOTE: &str = "Windows was asked not to add suggested apps.";
const ALLOW_AGAIN: &str = "Allow suggested apps again";
const ALLOWED_AGAIN: &str = "Windows can add suggested apps again.";

fn suggested_blocked(state: &State) -> bool {
    state.suggested_machine || state.suggested_user
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
            let icons = icons_task(state.installed.clone());
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
            icons
        }
        Msg::Icons(found) => {
            state.icons.extend(found);
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
            if matches!(state.sheet, Sheet::Review | Sheet::Delete(_)) {
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
            if state.restoring.is_some() || ctx.busy {
                return Task::none();
            }
            if state.copies.contains(&index) {
                state.restoring = Some(index);
                state.restoring_copy = true;
                state.offline = None;
                state.now = Instant::now();
                state.spin = anim::Clock::at(state.now);
                return Task::perform(
                    blocking(move || {
                        debloat::offline::restore_index(index).map_err(|e| format!("{e:#}"))
                    }),
                    move |r| wrap(Msg::RestoredOffline(index, r)),
                );
            }
            store_restore(state, ctx, index)
        }
        Msg::SuggestedMachine(on) => {
            state.suggested_machine = on;
            Task::none()
        }
        Msg::SuggestedUser(reply) => {
            state.suggested_user = matches!(reply, Ok(crate::broker::Reply::SafeByUs));
            Task::none()
        }
        Msg::AllowSuggested => {
            if state.allowing.is_some() || !suggested_blocked(state) {
                return Task::none();
            }
            let user = state.suggested_user;
            state.allowing = Some((1 + u8::from(user), true));
            let mut tasks = vec![Task::perform(blocking(allow_machine), |ok| {
                wrap(Msg::SuggestedAllowed(ok))
            })];
            if user {
                tasks.push(ctx.broker_task(
                    crate::broker::Request::UserSetting(
                        crate::user_settings::Setting::SuggestedApps,
                        crate::user_settings::Op::Undo,
                    ),
                    |r| wrap(Msg::SuggestedAllowed(matches!(r, Ok(crate::broker::Reply::Done)))),
                ));
            }
            Task::batch(tasks)
        }
        Msg::SuggestedAllowed(ok) => {
            let Some((left, all_ok)) = state.allowing else {
                return Task::none();
            };
            let (left, all_ok) = (left - 1, all_ok && ok);
            if left > 0 {
                state.allowing = Some((left, all_ok));
                return Task::none();
            }
            state.allowing = None;
            let text = if all_ok {
                ctx.t(ALLOWED_AGAIN)
            } else {
                ctx.t("We couldn't change that setting. It was left as it was. Please try again, or restart your PC first.")
            };
            Task::batch([
                toast(text, if all_ok { Tone::Good } else { Tone::Warn }),
                suggested_task(ctx),
            ])
        }
        Msg::RestoreStore(index) => {
            if state.restoring.is_some() || ctx.busy {
                return Task::none();
            }
            store_restore(state, ctx, index)
        }
        Msg::RestoredOffline(index, result) => on_restored_offline(state, ctx, index, result),
        Msg::AskDelete(index) => {
            if matches!(state.sheet, Sheet::None)
                && !ctx.busy
                && state.restoring.is_none()
                && state.copies.contains(&index)
            {
                state.sheet = Sheet::Delete(index);
            }
            Task::none()
        }
        Msg::Delete(index) => {
            if !matches!(state.sheet, Sheet::Delete(i) if i == index) {
                return Task::none();
            }
            state.sheet = Sheet::None;
            if ctx.busy || state.restoring.is_some() {
                return Task::none();
            }
            Task::perform(
                blocking(move || {
                    debloat::offline::delete_index(index).map_err(|e| format!("{e:#}"))
                }),
                move |r| wrap(Msg::Deleted(r)),
            )
        }
        Msg::Deleted(result) => match result {
            Ok(()) => Task::batch([
                copies_task(),
                toast(ctx.t("Saved copy deleted."), Tone::Neutral),
            ]),
            Err(_) => Task::batch([
                copies_task(),
                toast(
                    ctx.t("We couldn't delete the saved copy. Please try again. If it keeps failing, restart your PC."),
                    Tone::Bad,
                ),
            ]),
        },
        Msg::Copies(copies, bytes) => {
            state.copies = copies;
            state.saved_bytes = bytes;
            Task::none()
        }
        Msg::Restored(index, result) => {
            state.restoring = None;
            state.restoring_copy = false;
            let name = ctx.t(app_of(index).name);
            match result {
                Ok(crate::broker::Reply::Done) => {
                    let text = format!("{name} {}", ctx.t("is back on your PC."));
                    restored_ok(state, ctx, index, text, Tone::Good)
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
                _ => couldnt_bring_back(ctx, &name),
            }
        }
    }
}

fn couldnt_bring_back(ctx: &Ctx, name: &str) -> Task<Message> {
    toast(
        format!(
            "{} {name}. {}",
            ctx.t("We couldn't bring back"),
            ctx.t("Restart your PC and try again.")
        ),
        Tone::Bad,
    )
}

fn store_restore(state: &mut State, ctx: &mut Ctx, index: u16) -> Task<Message> {
    if app_of(index).store_id.is_none() {
        return Task::none();
    }
    state.restoring = Some(index);
    state.restoring_copy = false;
    state.offline = None;
    state.now = Instant::now();
    state.spin = anim::Clock::at(state.now);
    ctx.broker_task(crate::broker::Request::ReinstallStoreApp(index), move |r| {
        wrap(Msg::Restored(index, r))
    })
}

fn restored_ok(
    state: &mut State,
    ctx: &mut Ctx,
    index: u16,
    text: String,
    tone: Tone,
) -> Task<Message> {
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
        inventory_task(state),
        Task::perform(blocking(move || debloat::finish_restore(index)), |_| ())
            .then(|_| copies_task()),
        toast(text, tone),
    ])
}

fn on_restored_offline(
    state: &mut State,
    ctx: &mut Ctx,
    index: u16,
    result: Result<Restored, String>,
) -> Task<Message> {
    state.restoring = None;
    state.restoring_copy = false;
    let name = ctx.t(app_of(index).name);
    let can_use_store = app_of(index).store_id.is_some();
    match result {
        Ok(Restored::Back) => {
            let text = format!("{name} {}", ctx.t("is back on your PC."));
            restored_ok(state, ctx, index, text, Tone::Good)
        }
        Ok(Restored::BackWithoutSomeData) => {
            let text = ctx
                .t("{name} is back. Some of its saved data couldn't be put back.")
                .replace("{name}", &name);
            restored_ok(state, ctx, index, text, Tone::Neutral)
        }
        Ok(Restored::AlreadyThere) => {
            let text = ctx
                .t("{name} is already on your PC.")
                .replace("{name}", &name);
            restored_ok(state, ctx, index, text, Tone::Neutral)
        }
        Ok(Restored::Damaged) => {
            let text = ctx
                .t(if can_use_store {
                    "The saved copy of {name} is damaged, so Secblitz is getting it from the Microsoft Store instead."
                } else {
                    "The saved copy of {name} is damaged, so it can't be brought back from Secblitz."
                })
                .replace("{name}", &name);
            let mut tasks = vec![toast(text, Tone::Warn), copies_task()];
            if can_use_store {
                tasks.push(store_restore(state, ctx, index));
            }
            Task::batch(tasks)
        }
        Ok(Restored::NoCopy) => {
            if can_use_store {
                Task::batch([copies_task(), store_restore(state, ctx, index)])
            } else {
                Task::batch([copies_task(), couldnt_bring_back(ctx, &name)])
            }
        }
        Err(_) => couldnt_bring_back(ctx, &name),
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
    state.run_at = state.now;
    state.menu_fillers = installed_indices(state)
        .into_iter()
        .filter(|i| !state.selected.contains(i))
        .collect();
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
        Run::Step(Progress::Saving(i)) => {
            set_step(state, i, Step::Saving);
            Task::none()
        }
        Run::Step(Progress::Started(i)) => {
            set_step(state, i, Step::Working);
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
            let steps = match &state.sheet {
                Sheet::Working(items) => items.clone(),
                _ => Vec::new(),
            };
            let kept = steps
                .iter()
                .filter_map(|(i, step)| match step {
                    Step::Done(ItemResult::Kept(k), _) => Some((*i, k.clone())),
                    _ => None,
                })
                .collect();
            let mut done = Finished {
                asked_to_block: asked,
                kept,
                steps,
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

fn set_step(state: &mut State, i: u16, step: Step) {
    if let Sheet::Working(items) = &mut state.sheet {
        if let Some(item) = items.iter_mut().find(|(n, _)| *n == i) {
            item.1 = step;
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
            "Remove apps that came with Windows but you don't need. Secblitz keeps a copy, so you can bring them back any time.",
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
    let mut page = column![header, column![tabs, body].spacing(theme::S4)]
        .spacing(theme::S6)
        .width(Length::Fill);
    if suggested_blocked(state) {
        page = page.push(
            row![
                widgets::muted(p, ctx.t(BLOCKED_NOTE)),
                space::horizontal(),
                widgets::link(
                    p,
                    ctx.t(ALLOW_AGAIN),
                    wrap(Msg::AllowSuggested),
                ),
            ]
            .spacing(theme::S3)
            .align_y(Alignment::Center),
        );
    }
    page.into()
}

pub fn modal<'a>(state: &'a State, ctx: &'a Ctx) -> Option<Element<'a, Message>> {
    match &state.sheet {
        Sheet::None => None,
        Sheet::Review => Some(review_sheet(state, ctx)),
        Sheet::Working(items) => Some(working_sheet(state, items, ctx)),
        Sheet::Done(done) => Some(result_sheet(state, done, ctx)),
        Sheet::Delete(index) => Some(delete_sheet(*index, ctx)),
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
                ctx.t(debloat::friendly::run_failure(technical)),
                Some(widgets::action(
                    p,
                    ButtonKind::Secondary,
                    ctx.t("Try again"),
                    Some(Icon::Refresh),
                    Some(wrap(Msg::Rescan)),
                )),
            ),
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

pub fn footer<'a>(state: &'a State, ctx: &'a Ctx) -> Option<Element<'a, Message>> {
    let listing = state.tab == Tab::Apps
        && !state.groups.is_empty()
        && !matches!(state.scan, Scan::Failed(_));
    listing.then(|| action_bar(state, ctx))
}

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
        body = body.push(widgets::row_item_lead(
            p,
            Some(app_glyph(p, state, index, theme::ICON_ROW)),
            ctx.t(app.name),
            None,
            box_,
            Vec::new(),
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
        let has_copy = state.copies.contains(&index);
        let enabled = state.restoring.is_none() && !ctx.busy;
        let (subtitle, trailing): (String, Element<'a, Message>) = if state.restoring == Some(index)
        {
            let text = if state.restoring_copy {
                ctx.t("Bringing back {name}…")
                    .replace("{name}", &ctx.t(app.name))
            } else {
                ctx.t("Restoring…")
            };
            (
                text,
                anim::spinner(16.0, p.text_muted, state.spin.elapsed_at(state.now)),
            )
        } else if state.offline == Some(index) {
            (
                ctx.t("You're offline. Connect to the internet, then press Retry."),
                widgets::action(
                    p,
                    widgets::ButtonKind::Secondary,
                    ctx.t("Retry"),
                    Some(Icon::Refresh),
                    enabled.then(|| wrap(Msg::RestoreStore(index))),
                ),
            )
        } else if has_copy {
            (
                format!(
                    "{} · {}",
                    ago(ctx, t),
                    ctx.t("Can be brought back without internet")
                ),
                actions_view(p, ctx, row_actions(state, index, enabled)),
            )
        } else if app.store_id.is_none() {
            (
                format!(
                    "{} · {}",
                    ago(ctx, t),
                    ctx.t("You can look for it in the Microsoft Store yourself.")
                ),
                widgets::icon(Icon::Info, 16.0, p.text_muted),
            )
        } else {
            (
                ago(ctx, t),
                actions_view(p, ctx, row_actions(state, index, enabled)),
            )
        };
        rows.push(widgets::row_item_lead(
            p,
            Some(app_glyph(p, state, index, theme::ICON_ROW)),
            ctx.t(app.name),
            Some(subtitle),
            trailing,
            Vec::new(),
            None,
        ));
    }
    let summary = (state.saved_bytes > 0).then(|| {
        ctx.t("Saved copies use about {size}.")
            .replace("{size}", &crate::app::tools::size_phrase(state.saved_bytes))
    });
    widgets::group(p, ctx.t("Removed apps"), summary, None, rows)
}

struct RowActions {
    primary: Option<Msg>,
    menu: Vec<(Icon, &'static str, Msg, bool)>,
}

fn row_actions(state: &State, index: u16, enabled: bool) -> RowActions {
    let mut actions = RowActions {
        primary: None,
        menu: Vec::new(),
    };
    if !enabled {
        return actions;
    }
    if state.copies.contains(&index) {
        actions.primary = Some(Msg::Restore(index));
        if app_of(index).store_id.is_some() {
            actions.menu.push((
                Icon::Undo,
                "Get it from the Microsoft Store",
                Msg::RestoreStore(index),
                false,
            ));
        }
        actions.menu.push((
            Icon::Trash,
            "Delete saved copy",
            Msg::AskDelete(index),
            true,
        ));
    } else if app_of(index).store_id.is_some() {
        actions.primary = Some(Msg::Restore(index));
    }
    actions
}

fn actions_view<'a>(p: Palette, ctx: &Ctx, actions: RowActions) -> Element<'a, Message> {
    let mut trailing = row![].spacing(theme::S2).align_y(Alignment::Center);
    if let Some(m) = actions.primary {
        trailing = trailing.push(widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Restore"),
            None,
            Some(wrap(m)),
        ));
    }
    let items = actions
        .menu
        .into_iter()
        .map(|(icon, label, m, danger)| (icon, ctx.t(label), wrap(m), danger))
        .collect();
    trailing.push(widgets::overflow_menu(p, items)).into()
}

fn delete_sheet<'a>(index: u16, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = pal(ctx);
    let name = ctx.t(app_of(index).name);
    let body = if app_of(index).store_id.is_some() {
        ctx.t("{name} can then only come back from the Microsoft Store.")
    } else {
        ctx.t("{name} can't come back after this.")
    }
    .replace("{name}", &name);
    column![
        widgets::h2(p, ctx.t("Delete the saved copy?")),
        widgets::muted(p, body),
        space::vertical().height(theme::S1),
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
                ctx.t("Delete"),
                Some(Icon::Trash),
                Some(wrap(Msg::Delete(index))),
            ),
        ]
        .spacing(theme::S2),
    ]
    .spacing(theme::S3)
    .into()
}

const WORKING_LIST_MAX: f32 = 200.0;
const RESULT_BODY_MAX: f32 = 200.0;

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
                app_glyph(p, state, i, 16.0),
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
    col = col.push(widgets::inline_notice(
        p,
        Tone::Neutral,
        if n == 1 {
            ctx.t("Secblitz keeps a copy, so you can bring it back any time from the Removed apps tab, even without internet.")
        } else {
            ctx.t("Secblitz keeps a copy, so you can bring them back any time from the Removed apps tab, even without internet.")
        },
    ));
    if indices.iter().any(|i| app_of(*i).group == Group::Gaming) {
        col = col.push(widgets::inline_notice(
            p,
            Tone::Warn,
            ctx.t("Game Bar recording and Xbox games may stop working"),
        ));
    }
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
    let mut list = column![].spacing(theme::S3);
    for (index, step) in items {
        let blank = || -> Element<'a, Message> {
            space::horizontal().width(theme::CHECK).into()
        };
        let (lead, note): (Element<'a, Message>, String) = match step {
            Step::Waiting => (blank(), ctx.t("Waiting")),
            Step::Saving => (blank(), ctx.t("Saving a copy…")),
            Step::Working => (blank(), ctx.t("Removing…")),
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
            Step::Done(ItemResult::Kept(kept), _) => (
                widgets::icon(Icon::Info, 18.0, p.text_muted),
                ctx.t(kept_text(kept)),
            ),
            Step::Done(ItemResult::Failed(_), at) => (
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
                app_glyph(p, state, *index, theme::CHECK),
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
    let title = ctx.t("Removing apps…");
    let drawing = removal_menu(
        state,
        ctx,
        items.iter().map(|(i, step)| (*i, fate_of(step))),
        Outcome::Working,
        state.run_at,
        title.clone(),
    );
    column![
        drawing,
        widgets::h2(p, title),
        widgets::muted(
            p,
            ctx.t("Please keep this window open. This can take a few minutes.")
        ),
        progress::bar_eased(p, ratio, Tone::Brand),
        scroll_list(p, list, WORKING_LIST_MAX),
    ]
    .spacing(theme::S3)
    .into()
}

fn fate_of(step: &Step) -> Fate {
    match step {
        Step::Waiting => Fate::Waiting,
        Step::Saving => Fate::Saving,
        Step::Working => Fate::Busy,
        Step::Done(ItemResult::Removed, at) => Fate::Removed(*at),
        Step::Done(ItemResult::Protected, at) => Fate::Stays(*at),
        Step::Done(ItemResult::Kept(_), at) => Fate::Kept(*at),
        Step::Done(ItemResult::Failed(_), at) => Fate::Refused(*at),
    }
}

fn final_fate(index: u16, step: &Step, done: &Finished) -> Fate {
    if matches!(step, Step::Done(..)) {
        return fate_of(step);
    }
    let at = done.at;
    let Some(batch) = &done.batch else {
        return Fate::Refused(at);
    };
    if batch.removed.iter().any(|r| r.index == index) {
        Fate::Removed(at)
    } else if batch.failed.iter().any(|f| f.index == index) {
        Fate::Refused(at)
    } else if batch.kept.contains(&index) || done.kept.iter().any(|(i, _)| *i == index) {
        Fate::Kept(at)
    } else if batch.skipped.contains(&index) {
        Fate::Stays(at)
    } else {
        Fate::Absent(at)
    }
}

fn menu_fillers(state: &State, lang: Lang) -> Vec<Filler> {
    [
        (Glyph::Gear, lang.t("Settings")),
        (Glyph::Folder, lang.t("File Explorer")),
        (Glyph::Shield, lang.t("Windows Security")),
        (Glyph::Cart, lang.t("Microsoft Store")),
        (Glyph::Doc, lang.t("Notepad")),
    ]
    .into_iter()
    .map(|(glyph, name)| Filler { glyph, name })
    .chain(state.menu_fillers.iter().map(|i| Filler {
        glyph: start_menu::glyph_for(app_of(*i).family),
        name: lang.t(app_of(*i).name),
    }))
    .collect()
}

fn menu_labels(lang: Lang, result: String) -> Labels {
    Labels {
        waiting: lang.t("Waiting to remove {name}"),
        saving: lang.t("Saving a copy of {name}"),
        removing: lang.t("Removing {name}"),
        refused: lang.t("Couldn't remove {name}"),
        kept: lang.t("{name} stays on your PC"),
        protected: lang.t("Windows protects {name}"),
        more_one: lang.t("1 more app"),
        more_many: lang.t("{n} more apps"),
        result,
    }
}

fn removal_menu<'a>(
    state: &State,
    ctx: &Ctx,
    apps: impl Iterator<Item = (u16, Fate)>,
    outcome: Outcome,
    changed: Instant,
    result: String,
) -> Element<'a, Message> {
    start_menu::start_menu(menu_model(
        state,
        pal(ctx),
        ctx.lang,
        apps,
        outcome,
        changed,
        result,
    ))
}

fn menu_model(
    state: &State,
    palette: theme::Palette,
    lang: Lang,
    apps: impl Iterator<Item = (u16, Fate)>,
    outcome: Outcome,
    changed: Instant,
    result: String,
) -> StartMenu {
    StartMenu {
        palette,
        plate: Plate::Surface,
        outcome,
        changed,
        now: state.now,
        apps: apps
            .map(|(i, fate)| MenuApp {
                glyph: start_menu::glyph_for(app_of(i).family),
                name: lang.t(app_of(i).name),
                fate,
            })
            .collect(),
        fillers: menu_fillers(state, lang),
        labels: menu_labels(lang, result),
    }
}

fn kept_text(kept: &Kept) -> &'static str {
    match kept {
        Kept::NoSpace => "Kept: not enough free space to save a copy. Free up some space and try again.",
        Kept::NoCopy(_) => "Kept: couldn't save a copy first. Restart your PC and try again.",
    }
}

fn names(ctx: &Ctx, indices: impl Iterator<Item = u16>) -> Vec<(u16, String)> {
    let mut v: Vec<u16> = indices.collect();
    v.sort_unstable();
    v.dedup();
    v.into_iter().map(|i| (i, ctx.t(app_of(i).name))).collect()
}

fn result_block<'a>(
    p: theme::Palette,
    state: &State,
    icon: Icon,
    tone: Tone,
    title: String,
    list: Vec<(u16, String)>,
) -> Element<'a, Message> {
    let head = row![
        widgets::icon(icon, 18.0, p.tone(tone)),
        widgets::body(p, title)
    ]
    .spacing(theme::S2)
    .align_y(Alignment::Center);
    if !list.iter().any(|(i, _)| state.icons.contains_key(i)) {
        let line = list.into_iter().map(|(_, n)| n).collect::<Vec<_>>();
        return column![head, widgets::muted(p, line.join(", "))]
            .spacing(theme::S1)
            .into();
    }
    let mut col = column![head].spacing(theme::S1);
    for (i, name) in list {
        col = col.push(
            row![app_glyph(p, state, i, 16.0), widgets::muted(p, name)]
                .spacing(theme::S2)
                .align_y(Alignment::Center),
        );
    }
    col.into()
}

fn result_sheet<'a>(state: &'a State, done: &'a Finished, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = pal(ctx);
    let mut head = column![].spacing(theme::S3);
    let mut body: Vec<Element<'a, Message>> = Vec::new();
    let fates = done
        .steps
        .iter()
        .map(|(i, step)| (*i, final_fate(*i, step, done)))
        .collect::<Vec<_>>();
    let menu = |outcome: Outcome, title: &str| {
        removal_menu(
            state,
            ctx,
            fates.iter().copied(),
            outcome,
            done.at,
            title.to_string(),
        )
    };
    let mut technical: Vec<String> = Vec::new();
    match (&done.batch, &done.error) {
        (Some(batch), _) => {
            let removed = names(ctx, batch.removed.iter().map(|r| r.index));
            let protected = names(ctx, batch.skipped.iter().copied());
            let failed = names(ctx, batch.failed.iter().map(|f| f.index));
            let title = if batch.removed.is_empty()
                && failed.is_empty()
                && protected.is_empty()
                && done.kept.is_empty()
            {
                ctx.t("Nothing needed removing")
            } else if removed.is_empty() {
                ctx.t("No apps were removed")
            } else {
                count_text(ctx, removed.len(), "{n} app removed", "{n} apps removed")
            };
            let outcome = if failed.is_empty() && done.kept.is_empty() && !removed.is_empty() {
                Outcome::Removed
            } else if failed.is_empty() && done.kept.is_empty() {
                Outcome::Unchanged
            } else {
                Outcome::Partly
            };
            head = head.push(menu(outcome, &title)).push(widgets::h2(p, title));
            if !removed.is_empty() {
                body.push(result_block(
                    p,
                    state,
                    Icon::CheckCircle,
                    Tone::Good,
                    ctx.t("Removed"),
                    removed,
                ));
            }
            if !protected.is_empty() {
                body.push(result_block(
                    p,
                    state,
                    Icon::Info,
                    Tone::Neutral,
                    ctx.t("Windows protects these apps"),
                    protected,
                ));
            }
            for reason in [Kept::NoSpace, Kept::NoCopy(String::new())] {
                let list = names(
                    ctx,
                    done.kept
                        .iter()
                        .filter(|(_, k)| {
                            std::mem::discriminant(k) == std::mem::discriminant(&reason)
                        })
                        .map(|(i, _)| *i),
                );
                if !list.is_empty() {
                    body.push(result_block(
                        p,
                        state,
                        Icon::Info,
                        Tone::Neutral,
                        ctx.t(kept_text(&reason)),
                        list,
                    ));
                }
            }
            if !failed.is_empty() {
                body.push(result_block(
                    p,
                    state,
                    Icon::AlertTriangle,
                    Tone::Bad,
                    ctx.t("Couldn't remove"),
                    failed,
                ));
                body.push(widgets::small(
                    p,
                    ctx.t("Restart your PC and try again. Nothing else was changed."),
                ));
            }
            for f in &batch.failed {
                let line = format!(
                    "{}: {}",
                    ctx.t(app_of(f.index).name),
                    ctx.t(debloat::friendly::removal_failure(&f.reason))
                );
                if !technical.contains(&line) {
                    technical.push(line);
                }
            }
            for (i, k) in &done.kept {
                if let Kept::NoCopy(reason) = k {
                    let line = format!(
                        "{}: {}",
                        ctx.t(app_of(*i).name),
                        ctx.t(debloat::friendly::no_copy(reason))
                    );
                    if !technical.contains(&line) {
                        technical.push(line);
                    }
                }
            }
        }
        (None, error) => {
            let title = ctx.t("We couldn't remove the apps");
            head = head
                .push(menu(Outcome::Failed, &title))
                .push(widgets::h2(p, title));
            body.push(widgets::muted(
                p,
                ctx.t("Nothing was changed. Please try again."),
            ));
            if let Some(e) = error {
                technical.push(ctx.t(debloat::friendly::removal_run_failure(e)));
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
        body.push(note);
    }
    if !technical.is_empty() {
        body.push(details(state, ctx, technical));
    }
    let mut sheet = column![head].spacing(theme::S3);
    if !body.is_empty() {
        sheet = sheet.push(
            container(
                scrollable(
                    container(column(body).spacing(theme::S3))
                        .padding(Padding::ZERO.right(theme::S3))
                        .width(Length::Fill),
                )
                .direction(widgets::controls::scrollbar())
                .style(widgets::controls::scroll_style(p)),
            )
            .max_height(RESULT_BODY_MAX),
        );
    }
    sheet
        .push(space::vertical().height(theme::S1))
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

pub fn is_current_scan(state: &State, generation: u32) -> bool {
    state.scan_gen == generation
}

/// The app list must not be reloaded under an open sheet.
pub fn is_busy(state: &State) -> bool {
    !matches!(state.sheet, Sheet::None)
}

pub fn preload(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    if is_busy(state) {
        return Task::none();
    }
    if !matches!(state.scan, Scan::Ready) {
        start_scan(state);
    }
    Task::batch([scan_task(state), suggested_task(ctx)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use secblitz::debloat::{Failure, Removed};
    use std::time::Duration;

    #[test]
    fn the_start_menu_follows_each_apps_real_step() {
        let t = Instant::now();
        let t2 = t + Duration::from_secs(2);
        assert_eq!(fate_of(&Step::Waiting), Fate::Waiting);
        assert_eq!(fate_of(&Step::Saving), Fate::Saving);
        assert_eq!(fate_of(&Step::Working), Fate::Busy);
        assert_eq!(fate_of(&Step::Done(ItemResult::Removed, t)), Fate::Removed(t));
        assert_eq!(
            fate_of(&Step::Done(ItemResult::Failed("x".into()), t)),
            Fate::Refused(t)
        );
        assert_eq!(fate_of(&Step::Done(ItemResult::Protected, t)), Fate::Stays(t));
        assert_eq!(
            fate_of(&Step::Done(ItemResult::Kept(Kept::NoSpace), t)),
            Fate::Kept(t)
        );
        let done = Finished {
            batch: Some(Batch {
                removed: vec![Removed {
                    index: 1,
                    package: "p".into(),
                    version: "1".into(),
                    restored: false,
                }],
                failed: vec![Failure {
                    index: 2,
                    reason: "r".into(),
                }],
                skipped: vec![3],
                ..Batch::default()
            }),
            at: t2,
            ..Finished::default()
        };
        assert_eq!(
            final_fate(1, &Step::Done(ItemResult::Removed, t), &done),
            Fate::Removed(t)
        );
        assert_eq!(final_fate(1, &Step::Working, &done), Fate::Removed(t2));
        assert_eq!(final_fate(2, &Step::Waiting, &done), Fate::Refused(t2));
        assert_eq!(final_fate(3, &Step::Waiting, &done), Fate::Stays(t2));
        assert_eq!(final_fate(4, &Step::Waiting, &done), Fate::Absent(t2));
        let failed = Finished {
            at: t2,
            ..Finished::default()
        };
        assert_eq!(final_fate(1, &Step::Working, &failed), Fate::Refused(t2));
        let steps = [
            (1, Step::Done(ItemResult::Removed, t)),
            (2, Step::Working),
            (4, Step::Waiting),
        ];
        let fates = steps.iter().map(|(i, s)| (*i, final_fate(*i, s, &done)));
        let state = State::default();
        let m = menu_model(
            &state,
            theme::LIGHT,
            Lang::En,
            fates,
            Outcome::Partly,
            t2,
            "x".into(),
        );
        let sc = m.scene(t2 + Duration::from_millis(300));
        assert_eq!(m.label(start_menu::Part::Tile(6), &sc), app_of(4).name);
        assert_eq!(
            m.label(start_menu::Part::Tile(5), &sc),
            format!("Couldn't remove {}", app_of(2).name)
        );
        let sc = m.scene(t2 + Duration::from_secs(2));
        use start_menu::Place::{Gone, Slot};
        assert_eq!(sc.hidden, 0);
        assert_eq!(
            sc.places,
            vec![Slot(0), Gone, Slot(1), Slot(2), Slot(3), Slot(4), Gone, Slot(5)]
        );
    }

    #[test]
    fn start_menu_labels_and_fillers_are_translated() {
        let l = menu_labels(Lang::Fr, "x".into());
        assert_eq!(l.removing, "Suppression de {name}");
        assert_eq!(l.more_many, "{n} applications de plus");
        assert_eq!(l.refused, "Impossible de supprimer {name}");
        let state = State {
            menu_fillers: vec![debloat::catalog::owner("Microsoft.BingWeather").unwrap()],
            ..State::default()
        };
        let fillers = menu_fillers(&state, Lang::De);
        assert_eq!(fillers[0].name, "Einstellungen");
        assert_eq!(fillers[1].name, "Datei-Explorer");
        assert_eq!(fillers.last().unwrap().name, Lang::De.t("Weather"));
        assert_eq!(fillers.len(), 6);
    }

    fn debug(m: &Option<Msg>) -> Option<String> {
        m.as_ref().map(|m| format!("{m:?}"))
    }

    #[test]
    fn saved_copy_rows_offer_restore_and_delete() {
        let index = debloat::catalog::owner("Microsoft.BingWeather").unwrap();
        let mut state = State::default();
        state.copies.insert(index);
        let actions = row_actions(&state, index, true);
        assert_eq!(
            debug(&actions.primary),
            Some(format!("{:?}", Msg::Restore(index)))
        );
        let menu: Vec<String> = actions.menu.iter().map(|m| format!("{:?}", m.2)).collect();
        assert!(menu.iter().any(|m| m.contains("AskDelete")));
        assert!(menu.iter().any(|m| m.contains("RestoreStore")));
        state.copies.clear();
        let actions = row_actions(&state, index, true);
        assert!(!actions
            .menu
            .iter()
            .any(|m| format!("{:?}", m.2).contains("AskDelete")));
        assert_eq!(
            debug(&actions.primary),
            Some(format!("{:?}", Msg::Restore(index)))
        );
    }

    #[test]
    fn app_glyph_falls_back_and_uses_a_known_icon() {
        let index = debloat::catalog::owner("Microsoft.BingWeather").unwrap();
        let mut state = State::default();
        // No icon known: builds the generic glyph (and does not panic).
        let _ = app_glyph(theme::LIGHT, &state, index, theme::ICON_ROW);
        assert!(!state.icons.contains_key(&index));
        state
            .icons
            .insert(index, Handle::from_rgba(1, 1, vec![1, 2, 3, 255]));
        let _ = app_glyph(theme::LIGHT, &state, index, theme::ICON_ROW);
    }

    #[test]
    fn rows_offer_nothing_while_busy() {
        let index = debloat::catalog::owner("Microsoft.BingWeather").unwrap();
        let mut state = State::default();
        state.copies.insert(index);
        let actions = row_actions(&state, index, false);
        assert!(actions.primary.is_none() && actions.menu.is_empty());
    }
}
