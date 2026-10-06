//! "Remove Secblitz": one sheet that asks what should happen to the changes
//! Secblitz made, optionally puts everything back (with progress), and then
//! starts the uninstaller. Opened from the last group on the Settings page.
use crate::broker::{Reply, Request};
use crate::gui::icons::Icon;
use crate::gui::pages::settings;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::hairline::{self, rewind, Plate, Run};
use crate::gui::widgets::{self, anim, progress, ButtonKind};
use crate::gui::{blocking, blocking_stream, Ctx, Message};
use crate::i18n::Lang;
use crate::uninstall::{Left, Plan};
use crate::user_settings::{Op, Setting};
use iced::widget::{column, container, row, scrollable, space};
use iced::{Alignment, Background, Border, Element, Length, Subscription, Task};
use std::path::PathBuf;
use std::time::Instant;

type El<'a> = Element<'a, Message>;


pub const SECTION_TITLE: &str = "Remove Secblitz";
pub const SECTION_ROW: &str = "Remove Secblitz from this PC";
pub const SECTION_HELP: &str = "You choose whether your settings stay as they are or go back.";
const QUESTION: &str = "What should happen to the changes Secblitz made?";
const NOTHING_TO_PUT_BACK: &str = "Nothing Secblitz changed needs to be put back.";
const KEEP_TITLE: &str = "Keep my PC as it is now";
const KEEP_HELP: &str = "Your protection stays on. Apps you removed stay removed; you can reinstall them from the Microsoft Store.";
const PUT_BACK_TITLE: &str = "Put everything back the way it was";
const STAY_NOTE: &str = "Windows updates, virus scans and apps you installed with Secblitz stay.";
const WEB_STOPS: &str = "Web protection stops too, because it is part of Secblitz.";
const KEEP_DELETES_COPIES: &str = "The saved copies of removed apps are deleted to free space, so those apps can then only come back from the Microsoft Store.";
const OWN_ACCOUNT_ONLY: &str =
    "Personal settings are put back for your account only. Other accounts on this PC keep theirs.";
const STORE_NEEDS_INTERNET: &str = "Apps without a saved copy download again from the Microsoft Store, which needs an internet connection. They can take a few minutes to show up in Start.";
const CANCEL: &str = "Cancel";
const LOADING: &str = "Looking at what Secblitz changed…";
const WORKING_TITLE: &str = "Putting everything back…";
const WORKING_HELP: &str = "Please keep this window open. This can take a few minutes.";
const STEP_WAITING: &str = "Waiting";
const STEP_RUNNING: &str = "Putting back…";
const STEP_DONE: &str = "Put back";
const STEP_PARTLY: &str = "Some were left as they are";
const ITEM_PERSONAL: &str = "Your personal settings";
const ITEM_SETTINGS: &str = "Windows settings";
const ITEM_APPS: &str = "Removed apps";
const ITEM_SUGGESTED: &str = "Suggested apps";
const ITEM_WEB: &str = "Web protection";
#[cfg_attr(not(windows), allow(dead_code))]
const WEB_LEFT: &str = "Web protection could not be turned off. Close Secblitz, open it again and try once more. If it still does not work, check for a Secblitz update.";
const RESULT_LEFT_TITLE: &str = "Some things could not be put back";
const RESULT_LEFT_HELP: &str = "These are left as they are. You can remove Secblitz anyway, or keep it and try again later.";
const RESULT_DONE_TITLE: &str = "Everything is back the way it was";
const DELETE_EXE: &str = "You can now delete secblitz.exe.";
const REMOVE_ANYWAY: &str = "Remove Secblitz anyway";
const KEEP_SECBLITZ: &str = "Keep Secblitz";
const CLOSE: &str = "Close";
const LEAVING_TITLE: &str = "Removing Secblitz…";
const LEAVING_HELP: &str = "This window closes by itself.";
const CANNOT_START: &str = "We couldn't start the removal. Secblitz was not removed. Close Secblitz, open it again and try once more. If that does not work, restart your PC.";


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Keep,
    PutBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Personal,
    Settings,
    Apps,
    Suggested,
    Web,
}

impl Item {
    pub const ALL: [Item; 5] = [
        Item::Personal,
        Item::Settings,
        Item::Apps,
        Item::Suggested,
        Item::Web,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|i| *i == self).unwrap_or(0)
    }

    fn label(self) -> &'static str {
        match self {
            Item::Personal => ITEM_PERSONAL,
            Item::Settings => ITEM_SETTINGS,
            Item::Apps => ITEM_APPS,
            Item::Suggested => ITEM_SUGGESTED,
            Item::Web => ITEM_WEB,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Waiting,
    Working,
    Done(bool, Instant),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub changes: usize,
    pub apps: usize,
}

pub fn counts(plan: Option<&Plan>, safe: &[Setting]) -> Option<Counts> {
    let plan = plan?;
    Some(Counts {
        changes: plan.settings
            + safe.len()
            + usize::from(plan.suggested && !safe.contains(&Setting::SuggestedApps)),
        apps: plan.apps_with_copy + plan.apps_store_only,
    })
}

pub fn web_note(plan: Option<&Plan>) -> Option<&'static str> {
    plan.is_some_and(|p| p.web_on).then_some(WEB_STOPS)
}

pub fn can_put_back(counts: Option<Counts>) -> bool {
    counts.is_none_or(|c| c.changes > 0 || c.apps > 0)
}

pub fn choices(can_put_back: bool, installed: bool) -> Vec<Choice> {
    match (installed, can_put_back) {
        (true, true) => vec![Choice::Keep, Choice::PutBack],
        (true, false) => vec![Choice::Keep],
        (false, true) => vec![Choice::PutBack],
        (false, false) => Vec::new(),
    }
}

pub fn put_back_key(counts: Option<Counts>) -> &'static str {
    let Some(Counts { changes, apps }) = counts else {
        return "Secblitz undoes the changes it made and brings back the apps you removed first. This can take a few minutes.";
    };
    match (changes, apps) {
        (0, 0) => "",
        (0, 1) => "Secblitz brings back {apps} removed app first. This can take a few minutes.",
        (0, _) => "Secblitz brings back {apps} removed apps first. This can take a few minutes.",
        (1, 0) => "Secblitz undoes its {n} change first. This can take a few minutes.",
        (_, 0) => "Secblitz undoes its {n} changes first. This can take a few minutes.",
        (1, 1) => "Secblitz undoes its {n} change and brings back {apps} removed app first. This can take a few minutes.",
        (1, _) => "Secblitz undoes its {n} change and brings back {apps} removed apps first. This can take a few minutes.",
        (_, 1) => "Secblitz undoes its {n} changes and brings back {apps} removed app first. This can take a few minutes.",
        (_, _) => "Secblitz undoes its {n} changes and brings back {apps} removed apps first. This can take a few minutes.",
    }
}

fn put_back_text(lang: Lang, counts: Option<Counts>) -> String {
    let c = counts.unwrap_or(Counts {
        changes: 0,
        apps: 0,
    });
    lang.t(put_back_key(counts))
        .replace("{n}", &c.changes.to_string())
        .replace("{apps}", &c.apps.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    PutBack,
    Remove,
    Nothing,
}

pub fn start(choice: Choice, installed: bool) -> Start {
    match (choice, installed) {
        (Choice::PutBack, _) => Start::PutBack,
        (Choice::Keep, true) => Start::Remove,
        (Choice::Keep, false) => Start::Nothing,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    Remove,
    ShowResult,
}

pub fn finish(lines: &[String], installed: bool) -> Finish {
    if lines.is_empty() && installed {
        Finish::Remove
    } else {
        Finish::ShowResult
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultAction {
    RemoveAnyway,
    Keep,
    Close,
}

pub fn result_actions(installed: bool) -> Vec<ResultAction> {
    if installed {
        vec![ResultAction::Keep, ResultAction::RemoveAnyway]
    } else {
        vec![ResultAction::Close]
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn prune_apps(left: Vec<Left>, still_removed: &[String]) -> Vec<Left> {
    left.into_iter()
        .filter(|l| match l {
            Left::App { name } | Left::AppNeedsStore { name } => still_removed.contains(name),
            _ => true,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
enum Sheet {
    Closed,
    Loading {
        plan: Option<Option<Plan>>,
        waiting: usize,
        safe: Vec<Setting>,
    },
    Choose {
        plan: Option<Plan>,
        safe: Vec<Setting>,
        choice: Option<Choice>,
    },
    Working {
        steps: [StepState; 5],
    },
    Result {
        lines: Vec<String>,
        at: Instant,
    },
    Leaving,
}

#[derive(Debug)]
pub struct State {
    sheet: Sheet,
    installed: bool,
    generation: u32,
    spin: anim::Clock,
    now: Instant,
    since: Instant,
}

impl Default for State {
    fn default() -> Self {
        State {
            sheet: Sheet::Closed,
            installed: false,
            generation: 0,
            spin: anim::Clock::new(),
            now: Instant::now(),
            since: Instant::now(),
        }
    }
}

impl State {
    fn settle(&mut self) {
        if let Sheet::Loading {
            plan: Some(plan),
            waiting: 0,
            safe,
        } = &mut self.sheet
        {
            let choices = choices(can_put_back(counts(plan.as_ref(), safe)), self.installed);
            self.sheet = Sheet::Choose {
                plan: plan.take(),
                safe: std::mem::take(safe),
                choice: (choices.len() == 1).then(|| choices[0]),
            };
        }
    }

    fn on_planned(&mut self, generation: u32, plan: Result<Plan, String>) {
        if generation != self.generation {
            return;
        }
        if let Sheet::Loading { plan: slot, .. } = &mut self.sheet {
            *slot = Some(plan.ok());
        }
        self.settle();
    }

    fn on_queried(&mut self, generation: u32, setting: Setting, reply: Result<Reply, String>) {
        if generation != self.generation {
            return;
        }
        if let Sheet::Loading { waiting, safe, .. } = &mut self.sheet {
            *waiting = waiting.saturating_sub(1);
            if matches!(reply, Ok(Reply::SafeByUs)) {
                safe.push(setting);
            }
        }
        self.settle();
    }

    fn pick(&mut self, picked: Choice) {
        if let Sheet::Choose { plan, safe, choice } = &mut self.sheet {
            if choices(can_put_back(counts(plan.as_ref(), safe)), self.installed).contains(&picked)
            {
                *choice = Some(picked);
            }
        }
    }

    fn on_event(&mut self, event: Event, now: Instant) {
        self.now = now;
        match event {
            Event::Started(item) => {
                if let Sheet::Working { steps } = &mut self.sheet {
                    steps[item.index()] = StepState::Working;
                }
            }
            Event::Finished(item, ok) => {
                if let Sheet::Working { steps } = &mut self.sheet {
                    steps[item.index()] = StepState::Done(ok, now);
                }
            }
            Event::Done(_) => {}
        }
    }

    fn finished_steps(&self) -> usize {
        match &self.sheet {
            Sheet::Working { steps } => steps
                .iter()
                .filter(|s| matches!(s, StepState::Done(..)))
                .count(),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Started(Item),
    Finished(Item, bool),
    Done(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Open,
    Cancel,
    Planned(u32, Result<Plan, String>),
    Queried(u32, Setting, Result<Reply, String>),
    Pick(Choice),
    Confirm,
    Event(Event),
    RemoveAnyway,
    Spawned(bool),
    Frame(Instant),
}

fn wrap(msg: Msg) -> Message {
    Message::Settings(settings::Msg::Remove(msg))
}


pub fn uninstaller() -> Option<PathBuf> {
    let exe = crate::app::settings::installed_exe()?;
    let path = exe.parent()?.join("unins000.exe");
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return None;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & REPARSE_POINT != 0 || !trusted_owner(&path) {
            return None;
        }
    }
    Some(path)
}

#[cfg(windows)]
fn trusted_owner(path: &std::path::Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    unsafe {
        let mut owner: PSID = null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        let status = GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        );
        if status != 0 || owner.is_null() {
            if !sd.is_null() {
                LocalFree(sd);
            }
            return false;
        }
        let mut text: *mut u16 = null_mut();
        let trusted = ConvertSidToStringSidW(owner, &mut text) != 0 && !text.is_null() && {
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            matches!(sid.as_str(), "S-1-5-18" | "S-1-5-32-544")
        };
        if !text.is_null() {
            LocalFree(text.cast());
        }
        LocalFree(sd);
        trusted
    }
}

#[cfg(windows)]
fn load_plan() -> Result<Plan, String> {
    crate::uninstall::plan().map_err(|e| format!("{e:#}"))
}

#[cfg(not(windows))]
fn load_plan() -> Result<Plan, String> {
    Ok(Plan::default())
}

fn launch_uninstaller() -> bool {
    let Some(path) = uninstaller() else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new(path)
            .args([
                "/VERYSILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/SECBLITZDONE",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

#[cfg(windows)]
fn still_removed_names() -> Vec<String> {
    use secblitz::debloat::{self, journal};
    journal::still_removed(&journal::load(), debloat::catalog().len())
        .into_iter()
        .filter_map(|(i, _)| {
            debloat::catalog()
                .get(usize::from(i))
                .map(|a| a.name.to_owned())
        })
        .collect()
}

#[cfg(windows)]
const STORE_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(windows)]
fn reinstall_from_store(client: Option<&crate::broker::Client>) -> Vec<String> {
    use secblitz::debloat::{self, journal};
    use std::time::Duration;
    let Some(client) = client else {
        return Vec::new();
    };
    let mut pending: Vec<u16> = journal::still_removed(&journal::load(), debloat::catalog().len())
        .into_iter()
        .map(|(index, _)| index)
        .filter(|index| debloat::catalog()[usize::from(*index)].store_id.is_some())
        .filter(|index| matches!(client.send(Request::StartStoreApp(*index)), Ok(Reply::Done)))
        .collect();
    let deadline = Instant::now() + STORE_WAIT;
    while !pending.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        pending.retain(|index| match client.send(Request::StoreAppStatus(*index)) {
            Ok(Reply::Working) => true,
            Ok(Reply::Done) => {
                let _ = journal::mark_restored(*index);
                false
            }
            _ => false,
        });
    }
    pending
        .into_iter()
        .filter_map(|index| {
            debloat::catalog()
                .get(usize::from(index))
                .map(|a| a.name.to_owned())
        })
        .collect()
}

#[cfg(windows)]
fn web_protection_off() -> bool {
    use secblitz::filter::config::{config_path, load_config, Config};
    let Ok(path) = config_path() else {
        return true;
    };
    if !load_config(&path).any_on() {
        return true;
    }
    secblitz::filter::control::apply_switches(Config::default()).is_ok()
}

#[cfg(any(windows, test))]
fn put_back_left(setting: Setting, was_ours: bool, reply: Option<Option<&Reply>>) -> Option<Left> {
    match reply {
        Some(Some(Reply::Done)) => None,
        Some(Some(Reply::ChangedSince)) => Some(Left::Setting {
            title: crate::uninstall::personal_title(setting.id()).to_owned(),
            reason: crate::uninstall::LeftReason::ChangedSince,
        }),
        Some(Some(Reply::Unavailable)) if !was_ours => None,
        None if !was_ours => None,
        _ => Some(Left::Personal { id: setting.id() }),
    }
}

#[cfg(windows)]
fn run_put_back(
    client: Option<std::sync::Arc<crate::broker::Client>>,
    safe: Vec<Setting>,
    lang: Lang,
    emit: &dyn Fn(Event),
) {
    use crate::uninstall::{left_line, revert_machine, Step};
    let mut left: Vec<Left> = Vec::new();

    emit(Event::Started(Item::Personal));
    for setting in Setting::ALL {
        let reply = client
            .as_ref()
            .map(|c| c.send(Request::UserSetting(setting, Op::Undo)));
        let reply = reply.as_ref().map(|r| r.as_ref().ok());
        if let Some(l) = put_back_left(setting, safe.contains(&setting), reply) {
            left.push(l);
        }
    }
    emit(Event::Finished(Item::Personal, left.is_empty()));

    emit(Event::Started(Item::Settings));
    let downloading = std::cell::RefCell::new(Vec::new());
    let still_removed = || -> Vec<String> {
        let downloading = downloading.borrow();
        still_removed_names()
            .into_iter()
            .filter(|name| !downloading.contains(name))
            .collect()
    };
    let summary = revert_machine(&|step, ok| match step {
        Step::Settings => {
            emit(Event::Finished(Item::Settings, ok));
            emit(Event::Started(Item::Apps));
        }
        Step::Apps => {
            *downloading.borrow_mut() = reinstall_from_store(client.as_deref());
            emit(Event::Finished(Item::Apps, still_removed().is_empty()));
            emit(Event::Started(Item::Suggested));
        }
        Step::Suggested => emit(Event::Finished(Item::Suggested, ok)),
    });
    left.extend(summary.left);
    let left = prune_apps(left, &still_removed());

    emit(Event::Started(Item::Web));
    let web = web_protection_off();
    emit(Event::Finished(Item::Web, web));

    let mut lines: Vec<String> = left.iter().map(|l| left_line(l, lang)).collect();
    if !web {
        lines.push(lang.t(WEB_LEFT));
    }
    emit(Event::Done(lines));
}

#[cfg(not(windows))]
fn run_put_back(
    _client: Option<std::sync::Arc<crate::broker::Client>>,
    _safe: Vec<Setting>,
    _lang: Lang,
    emit: &dyn Fn(Event),
) {
    for item in Item::ALL {
        emit(Event::Started(item));
        emit(Event::Finished(item, true));
    }
    emit(Event::Done(Vec::new()));
}


pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Open => {
            if ctx.busy || !matches!(state.sheet, Sheet::Closed) {
                return Task::none();
            }
            state.installed = uninstaller().is_some();
            state.generation = state.generation.wrapping_add(1);
            state.now = Instant::now();
            state.spin = anim::Clock::at(state.now);
            let generation = state.generation;
            let ask = if ctx.broker.is_some() {
                Setting::ALL.to_vec()
            } else {
                Vec::new()
            };
            state.sheet = Sheet::Loading {
                plan: None,
                waiting: ask.len(),
                safe: Vec::new(),
            };
            let mut tasks = vec![Task::perform(blocking(load_plan), move |r| {
                wrap(Msg::Planned(generation, r))
            })];
            for setting in ask {
                tasks.push(
                    ctx.broker_task(Request::UserSetting(setting, Op::Query), move |r| {
                        wrap(Msg::Queried(generation, setting, r))
                    }),
                );
            }
            Task::batch(tasks)
        }
        Msg::Cancel => {
            close(state);
            Task::none()
        }
        Msg::Planned(generation, plan) => {
            state.on_planned(generation, plan);
            Task::none()
        }
        Msg::Queried(generation, setting, reply) => {
            state.on_queried(generation, setting, reply);
            Task::none()
        }
        Msg::Pick(choice) => {
            state.pick(choice);
            Task::none()
        }
        Msg::Confirm => confirm(state, ctx),
        Msg::Event(Event::Done(lines)) => {
            ctx.busy = false;
            state.now = Instant::now();
            match finish(&lines, state.installed) {
                Finish::Remove => remove(state),
                Finish::ShowResult => {
                    state.sheet = Sheet::Result {
                        lines,
                        at: state.now,
                    };
                    Task::done(Message::CheckNow)
                }
            }
        }
        Msg::Event(event) => {
            state.on_event(event, Instant::now());
            Task::none()
        }
        Msg::RemoveAnyway => {
            if matches!(state.sheet, Sheet::Result { .. }) && state.installed {
                remove(state)
            } else {
                Task::none()
            }
        }
        Msg::Spawned(true) => iced::exit(),
        Msg::Spawned(false) => {
            state.sheet = Sheet::Closed;
            Task::done(Message::Toast(ctx.t(CANNOT_START), Tone::Warn))
        }
        Msg::Frame(now) => {
            state.now = now;
            Task::none()
        }
    }
}

fn close(state: &mut State) {
    if matches!(
        state.sheet,
        Sheet::Loading { .. } | Sheet::Choose { .. } | Sheet::Result { .. }
    ) {
        state.generation = state.generation.wrapping_add(1);
        state.sheet = Sheet::Closed;
    }
}

pub fn escape(state: &mut State) -> bool {
    let open = !matches!(state.sheet, Sheet::Closed);
    close(state);
    open
}

fn confirm(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let Sheet::Choose {
        safe,
        choice: Some(choice),
        ..
    } = &state.sheet
    else {
        return Task::none();
    };
    match start(*choice, state.installed) {
        Start::Nothing => {
            state.sheet = Sheet::Closed;
            Task::none()
        }
        Start::Remove => remove(state),
        Start::PutBack => {
            if ctx.busy {
                return Task::none();
            }
            let safe = safe.clone();
            let client = ctx.broker.clone();
            let lang = ctx.lang;
            ctx.busy = true;
            state.now = Instant::now();
            state.spin = anim::Clock::at(state.now);
            state.since = state.now;
            state.sheet = Sheet::Working {
                steps: [StepState::Waiting; 5],
            };
            let stream = blocking_stream(move |emit: &dyn Fn(Event)| {
                run_put_back(client, safe, lang, emit);
            });
            Task::run(stream, |e| wrap(Msg::Event(e)))
        }
    }
}

fn remove(state: &mut State) -> Task<Message> {
    state.now = Instant::now();
    state.spin = anim::Clock::at(state.now);
    state.sheet = Sheet::Leaving;
    Task::perform(blocking(launch_uninstaller), |ok| wrap(Msg::Spawned(ok)))
}

pub fn subscription(state: &State) -> Subscription<Message> {
    let moving = match &state.sheet {
        Sheet::Loading { .. } | Sheet::Working { .. } | Sheet::Leaving => true,
        // The result's drawing asks for its own frames.
        _ => false,
    };
    if moving && anim::animating() {
        iced::window::frames().map(|t| wrap(Msg::Frame(t)))
    } else {
        Subscription::none()
    }
}


fn pal(ctx: &Ctx) -> Palette {
    Palette::of(ctx.palette.mode)
}

pub fn modal<'a>(state: &'a State, ctx: &'a Ctx) -> Option<El<'a>> {
    let p = pal(ctx);
    Some(match &state.sheet {
        Sheet::Closed => return None,
        Sheet::Loading { .. } => loading_sheet(state, ctx, p),
        Sheet::Choose { plan, safe, choice } => choose_sheet(state, ctx, p, plan, safe, *choice),
        Sheet::Working { steps } => working_sheet(state, ctx, p, steps),
        Sheet::Result { lines, at } => result_sheet(state, ctx, p, lines, *at),
        Sheet::Leaving => centered(
            p,
            anim::spinner(32.0, p.text_muted, state.spin.elapsed_at(state.now)),
            ctx.t(LEAVING_TITLE),
            ctx.t(LEAVING_HELP),
        ),
    })
}

fn centered<'a>(p: Palette, lead: El<'a>, title: String, help: String) -> El<'a> {
    container(
        column![lead, widgets::h2(p, title), widgets::muted(p, help)]
            .spacing(theme::S3)
            .align_x(Alignment::Center),
    )
    .center_x(Length::Fill)
    .padding([theme::S6, theme::S5])
    .into()
}

fn loading_sheet<'a>(state: &State, ctx: &Ctx, p: Palette) -> El<'a> {
    centered(
        p,
        anim::spinner(32.0, p.text_muted, state.spin.elapsed_at(state.now)),
        ctx.t(LOADING),
        ctx.t("This only takes a moment."),
    )
}

fn option_row<'a>(
    p: Palette,
    selected: bool,
    glyph: Icon,
    title: String,
    help: String,
    on_press: Option<Message>,
) -> El<'a> {
    let mark: El<'a> = if selected {
        widgets::icon(Icon::CheckCircle, theme::ICON_ROW, p.brand)
    } else {
        space::horizontal().width(theme::ICON_ROW).into()
    };
    container(widgets::row_item(
        p,
        Some(glyph),
        title,
        Some(help),
        mark,
        on_press,
    ))
    .style(move |_| container::Style {
        background: selected.then_some(Background::Color(p.selected)),
        border: Border {
            radius: theme::R.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn choose_sheet<'a>(
    state: &State,
    ctx: &Ctx,
    p: Palette,
    plan: &Option<Plan>,
    safe: &[Setting],
    choice: Option<Choice>,
) -> El<'a> {
    let n = counts(plan.as_ref(), safe);
    let offered = choices(can_put_back(n), state.installed);
    let mut col = column![widgets::h2(p, ctx.t(SECTION_TITLE))].spacing(theme::S3);
    if offered.is_empty() {
        col = col.push(widgets::muted(p, ctx.t(NOTHING_TO_PUT_BACK)));
        col = col.push(widgets::muted(p, ctx.t(DELETE_EXE)));
        return col
            .push(row![
                space::horizontal(),
                widgets::action(
                    p,
                    ButtonKind::Primary,
                    ctx.t(CLOSE),
                    None,
                    Some(wrap(Msg::Cancel))
                )
            ])
            .into();
    }
    col = col.push(widgets::muted(
        p,
        ctx.t(if can_put_back(n) {
            QUESTION
        } else {
            NOTHING_TO_PUT_BACK
        }),
    ));
    let mut options = column![].spacing(theme::S1);
    let single = offered.len() == 1;
    for option in &offered {
        let on = (!single).then(|| wrap(Msg::Pick(*option)));
        options = options.push(match option {
            Choice::Keep => option_row(
                p,
                choice == Some(Choice::Keep),
                Icon::Shield,
                ctx.t(KEEP_TITLE),
                ctx.t(KEEP_HELP),
                on,
            ),
            Choice::PutBack => option_row(
                p,
                choice == Some(Choice::PutBack),
                Icon::Undo,
                ctx.t(PUT_BACK_TITLE),
                put_back_text(ctx.lang, n),
                on,
            ),
        });
    }
    col = col.push(options);
    col = col.push(widgets::small(p, ctx.t(STAY_NOTE)));
    if let Some(note) = web_note(plan.as_ref()) {
        col = col.push(widgets::small(p, ctx.t(note)));
    }
    let copies = plan.as_ref().is_some_and(|pl| pl.apps_with_copy > 0);
    let store_only = plan.as_ref().is_some_and(|pl| pl.apps_store_only > 0);
    match choice {
        Some(Choice::Keep) if copies => {
            col = col.push(widgets::small(p, ctx.t(KEEP_DELETES_COPIES)))
        }
        Some(Choice::PutBack) => {
            col = col.push(widgets::small(p, ctx.t(OWN_ACCOUNT_ONLY)));
            if store_only {
                col = col.push(widgets::small(p, ctx.t(STORE_NEEDS_INTERNET)));
            }
        }
        _ => {}
    }
    let label = match (choice, state.installed) {
        (_, false) => ctx.t(PUT_BACK_TITLE),
        _ => ctx.t(SECTION_TITLE),
    };
    let go = widgets::action(
        p,
        ButtonKind::Danger,
        label,
        Some(Icon::Trash),
        (choice.is_some() && !ctx.busy).then(|| wrap(Msg::Confirm)),
    );
    col.push(space::vertical().height(theme::S1))
        .push(
            row![
                space::horizontal(),
                widgets::action(
                    p,
                    ButtonKind::Secondary,
                    ctx.t(CANCEL),
                    None,
                    Some(wrap(Msg::Cancel))
                ),
                go,
            ]
            .spacing(theme::S2),
        )
        .into()
}

fn rewind_art<'a>(
    ctx: &Ctx,
    p: Palette,
    run: Run,
    progress: Option<f32>,
    changed: Instant,
    now: Instant,
) -> El<'a> {
    let drawing = hairline::Rewind {
        p,
        plate: Plate::Surface,
        run,
        progress,
        changed,
        now: now.max(changed),
        label: ctx.t(rewind::label_key(rewind::Undo::Everything, run)),
    }
    .view();
    container(drawing).center_x(Length::Fill).into()
}

fn result_run(lines: &[String]) -> Run {
    if lines.is_empty() {
        Run::Done
    } else {
        Run::Partial
    }
}

fn working_sheet<'a>(state: &State, ctx: &Ctx, p: Palette, steps: &[StepState; 5]) -> El<'a> {
    let spin = state.spin.elapsed_at(state.now);
    let mut list = column![].spacing(theme::S3);
    for item in Item::ALL {
        let (lead, note): (El<'a>, String) = match steps[item.index()] {
            StepState::Waiting => (
                space::horizontal().width(theme::CHECK).into(),
                ctx.t(STEP_WAITING),
            ),
            StepState::Working => (anim::spinner(20.0, p.text, spin), ctx.t(STEP_RUNNING)),
            StepState::Done(true, at) => (
                anim::check_draw(
                    18.0,
                    p.good,
                    anim::Clock::at(at).progress_at(anim::SLOW, state.now),
                ),
                ctx.t(STEP_DONE),
            ),
            StepState::Done(false, at) => (
                anim::warn_draw(
                    18.0,
                    p.warn,
                    anim::Clock::at(at).progress_at(anim::SLOW, state.now),
                ),
                ctx.t(STEP_PARTLY),
            ),
        };
        list = list.push(
            row![
                container(lead).center(theme::CHECK),
                widgets::body(p, ctx.t(item.label())),
                space::horizontal(),
                widgets::small(p, note),
            ]
            .spacing(theme::S3)
            .align_y(Alignment::Center),
        );
    }
    let ratio = state.finished_steps() as f32 / Item::ALL.len() as f32;
    column![
        rewind_art(ctx, p, Run::Working, Some(ratio), state.since, state.now),
        widgets::h2_centred(p, ctx.t(WORKING_TITLE)),
        widgets::muted_centred(p, ctx.t(WORKING_HELP)),
        progress::bar_eased(p, ratio, Tone::Brand),
        container(list)
            .padding(theme::S3)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.surface_alt)),
                border: Border {
                    radius: theme::R.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
    ]
    .spacing(theme::S3)
    .height(Length::Fixed(widgets::SHEET_FIT_HEIGHT))
    .into()
}

fn result_sheet<'a>(state: &State, ctx: &Ctx, p: Palette, lines: &[String], at: Instant) -> El<'a> {
    let mut col = column![rewind_art(ctx, p, result_run(lines), None, at, state.now)]
        .spacing(theme::S3);
    if lines.is_empty() {
        col = col
            .push(widgets::h2_centred(p, ctx.t(RESULT_DONE_TITLE)))
            .push(widgets::muted_centred(p, ctx.t(DELETE_EXE)));
    } else {
        let mut list = column![].spacing(theme::S2);
        for line in lines {
            list = list.push(widgets::body(p, line.clone()));
        }
        col = col
            .push(widgets::h2_centred(p, ctx.t(RESULT_LEFT_TITLE)))
            .push(widgets::muted_centred(
                p,
                ctx.t(if state.installed {
                    RESULT_LEFT_HELP
                } else {
                    DELETE_EXE
                }),
            ))
            .push(
                container(
                    scrollable(container(list).padding(theme::S3).width(Length::Fill))
                        .direction(widgets::controls::scrollbar())
                        .style(widgets::controls::scroll_style(p)),
                )
                .max_height(168.0)
                .style(move |_| container::Style {
                    background: Some(Background::Color(p.surface_alt)),
                    border: Border {
                        radius: theme::R.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
            );
    }
    let mut buttons = row![space::horizontal()].spacing(theme::S2);
    for action in result_actions(state.installed) {
        buttons = buttons.push(match action {
            ResultAction::Keep => widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t(KEEP_SECBLITZ),
                None,
                Some(wrap(Msg::Cancel)),
            ),
            ResultAction::RemoveAnyway => widgets::action(
                p,
                ButtonKind::Danger,
                ctx.t(REMOVE_ANYWAY),
                Some(Icon::Trash),
                Some(wrap(Msg::RemoveAnyway)),
            ),
            ResultAction::Close => widgets::action(
                p,
                ButtonKind::Primary,
                ctx.t(CLOSE),
                None,
                Some(wrap(Msg::Cancel)),
            ),
        });
    }
    col.push(space::vertical().height(Length::Fill))
        .push(buttons)
        .height(Length::Fixed(widgets::SHEET_FIT_HEIGHT))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LANGS: [Lang; 6] = [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It];

    fn plan(settings: usize, copy: usize, store: usize, suggested: bool) -> Plan {
        Plan {
            settings,
            apps_with_copy: copy,
            apps_store_only: store,
            suggested,
            web_on: false,
        }
    }

    #[test]
    fn web_note_only_while_web_protection_is_on() {
        assert_eq!(web_note(None), None);
        assert_eq!(web_note(Some(&plan(3, 0, 0, false))), None);
        let on = Plan {
            web_on: true,
            ..plan(0, 0, 0, false)
        };
        assert_eq!(web_note(Some(&on)), Some(WEB_STOPS));
    }

    fn loading(installed: bool) -> State {
        State {
            installed,
            sheet: Sheet::Loading {
                plan: None,
                waiting: 0,
                safe: Vec::new(),
            },
            ..State::default()
        }
    }

    #[test]
    fn sheet_hides_put_back_when_nothing_to_undo() {
        let none = counts(Some(&plan(0, 0, 0, false)), &[]);
        assert!(!can_put_back(none));
        assert_eq!(choices(can_put_back(none), true), vec![Choice::Keep]);
        let some = counts(Some(&plan(2, 0, 0, false)), &[]);
        assert_eq!(
            choices(can_put_back(some), true),
            vec![Choice::Keep, Choice::PutBack]
        );
        let personal = counts(Some(&plan(0, 0, 0, false)), &[Setting::OfficeMacros]);
        assert!(can_put_back(personal));
        assert!(can_put_back(None));
        let mut state = loading(true);
        state.on_planned(0, Ok(plan(0, 0, 0, false)));
        assert!(matches!(
            state.sheet,
            Sheet::Choose {
                choice: Some(Choice::Keep),
                ..
            }
        ));
    }

    #[test]
    fn put_back_text_counts() {
        let c = counts(Some(&plan(7, 2, 1, false)), &[Setting::ShowExtensions; 5]);
        assert_eq!(
            c,
            Some(Counts {
                changes: 12,
                apps: 3
            })
        );
        assert_eq!(
            put_back_text(Lang::En, c),
            "Secblitz undoes its 12 changes and brings back 3 removed apps first. This can take a few minutes."
        );
        assert_eq!(
            put_back_text(
                Lang::En,
                Some(Counts {
                    changes: 1,
                    apps: 0
                })
            ),
            "Secblitz undoes its 1 change first. This can take a few minutes."
        );
        assert_eq!(
            put_back_text(
                Lang::En,
                Some(Counts {
                    changes: 0,
                    apps: 1
                })
            ),
            "Secblitz brings back 1 removed app first. This can take a few minutes."
        );
        let both = counts(
            Some(&plan(0, 0, 0, true)),
            &[Setting::SuggestedApps, Setting::OfficeMacros],
        );
        assert_eq!(both.map(|c| c.changes), Some(2));
        let machine_only = counts(Some(&plan(1, 0, 0, true)), &[]);
        assert_eq!(machine_only.map(|c| c.changes), Some(2));
        for (changes, apps) in [
            (0, 1),
            (0, 4),
            (1, 0),
            (4, 0),
            (1, 1),
            (1, 4),
            (4, 1),
            (4, 4),
        ] {
            for lang in LANGS {
                let text = put_back_text(lang, Some(Counts { changes, apps }));
                assert!(!text.contains('{') && !text.contains('}'), "{text}");
            }
        }
        assert!(!put_back_text(Lang::En, None).contains('{'));
    }

    #[test]
    fn portable_sheet_offers_put_back_only() {
        assert_eq!(choices(true, false), vec![Choice::PutBack]);
        assert!(choices(false, false).is_empty());
        let mut state = loading(false);
        state.on_planned(0, Ok(plan(3, 0, 0, false)));
        assert!(matches!(
            state.sheet,
            Sheet::Choose {
                choice: Some(Choice::PutBack),
                ..
            }
        ));
        assert_eq!(start(Choice::PutBack, false), Start::PutBack);
        assert_eq!(start(Choice::Keep, false), Start::Nothing);
        assert_eq!(finish(&[], false), Finish::ShowResult);
        assert_eq!(result_actions(false), vec![ResultAction::Close]);
        state.pick(Choice::Keep);
        assert!(matches!(
            state.sheet,
            Sheet::Choose {
                choice: Some(Choice::PutBack),
                ..
            }
        ));
    }

    #[test]
    fn failure_lists_left_lines_and_two_buttons() {
        let left = [
            Left::Setting {
                title: "Firewall at home".into(),
                reason: crate::uninstall::LeftReason::ChangedSince,
            },
            Left::Personal {
                id: "net.nearby_sharing",
            },
        ];
        let lines: Vec<String> = left
            .iter()
            .map(|l| crate::uninstall::left_line(l, Lang::En))
            .collect();
        assert_eq!(
            lines[0],
            "Firewall at home: it has changed since Secblitz set it, so it was left as it is. No action is needed."
        );
        assert_eq!(finish(&lines, true), Finish::ShowResult);
        assert_eq!(
            result_actions(true),
            vec![ResultAction::Keep, ResultAction::RemoveAnyway]
        );
        assert_eq!(finish(&[], true), Finish::Remove);
    }

    #[test]
    fn keep_skips_put_back() {
        assert_eq!(start(Choice::Keep, true), Start::Remove);
        assert_eq!(start(Choice::PutBack, true), Start::PutBack);
    }

    #[test]
    fn a_name_that_is_back_is_not_listed() {
        let left = vec![
            Left::AppNeedsStore { name: "A".into() },
            Left::App { name: "B".into() },
            Left::SuggestedOlderVersion,
        ];
        let kept = prune_apps(left, &["B".to_owned()]);
        assert_eq!(
            kept,
            vec![Left::App { name: "B".into() }, Left::SuggestedOlderVersion]
        );
    }

    #[test]
    fn put_back_reports_each_personal_reply_plainly() {
        let s = Setting::OfficeMacros;
        assert_eq!(put_back_left(s, true, Some(Some(&Reply::Done))), None);
        assert!(matches!(
            put_back_left(s, false, Some(Some(&Reply::ChangedSince))),
            Some(Left::Setting { reason: crate::uninstall::LeftReason::ChangedSince, .. })
        ));
        assert_eq!(put_back_left(s, false, Some(Some(&Reply::Unavailable))), None);
        assert_eq!(put_back_left(s, false, None), None);
        assert!(matches!(put_back_left(s, true, None), Some(Left::Personal { .. })));
        assert!(matches!(
            put_back_left(s, false, Some(Some(&Reply::Failed))),
            Some(Left::Personal { .. })
        ));
    }

    #[test]
    fn loading_waits_for_the_plan_and_every_answer() {
        let mut state = State {
            installed: true,
            generation: 4,
            sheet: Sheet::Loading {
                plan: None,
                waiting: 2,
                safe: Vec::new(),
            },
            ..State::default()
        };
        state.on_planned(4, Ok(plan(1, 0, 0, false)));
        assert!(matches!(state.sheet, Sheet::Loading { .. }));
        state.on_queried(3, Setting::OfficeMacros, Ok(Reply::SafeByUs));
        assert!(matches!(state.sheet, Sheet::Loading { waiting: 2, .. }));
        state.on_queried(4, Setting::OfficeMacros, Ok(Reply::SafeByUs));
        state.on_queried(4, Setting::ShowExtensions, Ok(Reply::Safe));
        match &state.sheet {
            Sheet::Choose { safe, choice, .. } => {
                assert_eq!(safe, &vec![Setting::OfficeMacros]);
                assert_eq!(*choice, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unreadable_counts_still_offer_both_choices() {
        let mut state = loading(true);
        state.on_planned(0, Err("x".into()));
        match &state.sheet {
            Sheet::Choose { plan, .. } => assert!(plan.is_none()),
            other => panic!("{other:?}"),
        }
        state.pick(Choice::PutBack);
        assert!(matches!(
            state.sheet,
            Sheet::Choose {
                choice: Some(Choice::PutBack),
                ..
            }
        ));
    }

    #[test]
    fn progress_ticks_each_line_in_order() {
        let mut state = State {
            sheet: Sheet::Working {
                steps: [StepState::Waiting; 5],
            },
            ..State::default()
        };
        let now = Instant::now();
        state.on_event(Event::Started(Item::Personal), now);
        state.on_event(Event::Finished(Item::Personal, true), now);
        state.on_event(Event::Started(Item::Settings), now);
        state.on_event(Event::Finished(Item::Settings, false), now);
        assert_eq!(state.finished_steps(), 2);
        match &state.sheet {
            Sheet::Working { steps } => {
                assert_eq!(steps[0], StepState::Done(true, now));
                assert_eq!(steps[1], StepState::Done(false, now));
                assert_eq!(steps[2], StepState::Waiting);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rewind_ends_done_or_partly_done() {
        assert_eq!(result_run(&[]), Run::Done);
        assert_eq!(result_run(&["Web protection".into()]), Run::Partial);
    }

    #[test]
    fn escape_closes_only_when_no_work_runs() {
        let mut state = loading(true);
        assert!(escape(&mut state));
        assert!(matches!(state.sheet, Sheet::Closed));
        state.sheet = Sheet::Working {
            steps: [StepState::Waiting; 5],
        };
        escape(&mut state);
        assert!(matches!(state.sheet, Sheet::Working { .. }));
        state.sheet = Sheet::Leaving;
        escape(&mut state);
        assert!(matches!(state.sheet, Sheet::Leaving));
    }

    #[test]
    fn uninstaller_is_none_for_a_copy_that_is_not_installed() {
        assert!(uninstaller().is_none());
    }

    #[test]
    fn every_text_is_plain_and_translated() {
        let sources = [
            SECTION_TITLE,
            SECTION_ROW,
            SECTION_HELP,
            QUESTION,
            NOTHING_TO_PUT_BACK,
            KEEP_TITLE,
            KEEP_HELP,
            PUT_BACK_TITLE,
            STAY_NOTE,
            WEB_STOPS,
            KEEP_DELETES_COPIES,
            OWN_ACCOUNT_ONLY,
            STORE_NEEDS_INTERNET,
            LOADING,
            WORKING_TITLE,
            WORKING_HELP,
            STEP_RUNNING,
            STEP_DONE,
            STEP_PARTLY,
            ITEM_PERSONAL,
            ITEM_SUGGESTED,
            ITEM_WEB,
            WEB_LEFT,
            RESULT_LEFT_TITLE,
            RESULT_LEFT_HELP,
            RESULT_DONE_TITLE,
            DELETE_EXE,
            REMOVE_ANYWAY,
            KEEP_SECBLITZ,
            CLOSE,
            LEAVING_TITLE,
            LEAVING_HELP,
            CANNOT_START,
            "Secblitz undoes the changes it made and brings back the apps you removed first. This can take a few minutes.",
            "Secblitz brings back {apps} removed app first. This can take a few minutes.",
            "Secblitz brings back {apps} removed apps first. This can take a few minutes.",
            "Secblitz undoes its {n} change first. This can take a few minutes.",
            "Secblitz undoes its {n} changes first. This can take a few minutes.",
            "Secblitz undoes its {n} change and brings back {apps} removed app first. This can take a few minutes.",
            "Secblitz undoes its {n} change and brings back {apps} removed apps first. This can take a few minutes.",
            "Secblitz undoes its {n} changes and brings back {apps} removed app first. This can take a few minutes.",
            "Secblitz undoes its {n} changes and brings back {apps} removed apps first. This can take a few minutes.",
        ];
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/i18n-pending/a6.tsv");
        let pending = std::fs::read_to_string(path).ok();
        for source in sources {
            assert!(!source.contains('\u{2014}'), "{source}");
            for jargon in ["DNS", "NRPT", "registry", "service"] {
                assert!(!source.contains(jargon), "{source}");
            }
            let row = pending.as_deref().and_then(|t| {
                t.lines()
                    .map(|l| l.split('\t').collect::<Vec<_>>())
                    .find(|c| c[0] == source)
            });
            if let Some(cols) = row {
                assert_eq!(cols.len(), 6, "{source}");
                assert!(cols
                    .iter()
                    .all(|c| !c.trim().is_empty() && !c.contains('\u{2014}')));
                for placeholder in ["{n}", "{apps}"] {
                    for c in &cols {
                        assert_eq!(
                            source.contains(placeholder),
                            c.contains(placeholder),
                            "{source}"
                        );
                    }
                }
            } else {
                for lang in &LANGS[1..] {
                    assert_ne!(lang.t(source), source, "{source}");
                }
            }
        }
    }
}
