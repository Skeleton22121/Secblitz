//! Tools page: state and update logic. OWNER: tools agent.
//!
//! Everything that touches Windows runs off the UI thread (`blocking`,
//! `blocking_stream`, `ctx.broker_task`). The drawing code is in `view.rs`.
//!
//! Every change to the PC (repair, updates, scans, installs) first opens a
//! review sheet; the sheet's Cancel button is the safe way out.
mod view;

use crate::app::tools::{
    self as logic, Found, InstallEvent, InstallResult, InstallStage, RepairEvent, RepairKind,
    RepairProgress, RepairResult, Secret, TipProfile, TipsReport,
};
use crate::broker;
use crate::gui::theme::Tone;
use crate::gui::widgets::anim::{self, Clock, Tween};
use crate::gui::{blocking, blocking_stream, Ctx, Message};
use iced::{Subscription, Task};
use secblitz::actions;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use view::{modal, view};

/// A review sheet waiting for the person's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    Scan,
    DefenderUpdate,
    Repair(RepairKind),
    InstallUpdates,
    Bitwarden,
}

/// Windows Settings pages reachable from the shortcut list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    WindowsUpdate,
    WindowsSecurity,
    Encryption,
    SignIn,
}

impl Shortcut {
    pub const ALL: [Shortcut; 4] = [
        Self::WindowsUpdate,
        Self::WindowsSecurity,
        Self::Encryption,
        Self::SignIn,
    ];
    fn request(self) -> broker::Request {
        match self {
            Self::WindowsUpdate => broker::Request::OpenWindowsUpdate,
            Self::WindowsSecurity => broker::Request::OpenWindowsSecurity,
            Self::Encryption => broker::Request::OpenEncryption,
            Self::SignIn => broker::Request::OpenSignIn,
        }
    }
}

/// Which "More details" expander is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Scan,
    Defender,
    Repair,
    Updates,
    Tips,
    Bitwarden,
    Sheet,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Ask(Sheet),
    CloseSheet,
    Confirm,
    ScanDone(Result<(), String>),
    DefenderDone(Result<(), String>),
    ClearScan,
    ClearDefender,
    Repair(RepairEvent),
    StopRepair,
    ClearRepair,
    LookForUpdates,
    Found(Result<Found, (String, &'static str)>),
    Install(InstallEvent),
    StopInstall,
    ClearUpdates,
    PickTips(TipProfile),
    Tips(Box<TipsReport>),
    ChooseAnotherTips,
    NewPassword,
    CopyPassword,
    TogglePassword,
    /// The brief "Copied" confirmation on the password card is over.
    CopiedReset,
    /// Animation frame; only delivered while something moves.
    Frame(Instant),
    BitwardenDone(Result<broker::Reply, String>),
    ClearBitwarden,
    Open(Shortcut),
    OpenSecurity,
    Opened(Result<broker::Reply, String>),
    ToggleDetail(Detail),
}

#[derive(Debug)]
pub enum Run<T> {
    Idle,
    Working,
    Done(T),
}

#[allow(clippy::derivable_impls)]
impl<T> Default for Run<T> {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Default)]
pub enum Repair {
    #[default]
    Idle,
    Working {
        kind: RepairKind,
        cancel: Arc<AtomicBool>,
        progress: Option<RepairProgress>,
    },
    Done {
        kind: RepairKind,
        result: RepairResult,
        note: Option<&'static str>,
        technical: String,
    },
}

#[derive(Debug, Default)]
pub enum Updates {
    #[default]
    Idle,
    Looking,
    UpToDate,
    Found(Found),
    Failed {
        technical: String,
        note: &'static str,
    },
    Installing {
        cancel: Arc<AtomicBool>,
        stage: InstallStage,
        elapsed: u64,
        count: usize,
    },
    Done {
        result: InstallResult,
        note: Option<&'static str>,
        technical: String,
    },
}

#[derive(Debug, Default)]
pub enum Tips {
    #[default]
    Pick,
    Running(TipProfile),
    Done(Box<TipsReport>),
}

#[derive(Debug)]
pub struct Password {
    secret: Option<Secret>,
    shown: bool,
    copied: bool,
}

/// Which card a one-shot "finished" animation (check, cross, warning) belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Scan,
    Defender,
    Repair,
    Updates,
    Bitwarden,
    Copy,
}

/// How long the check mark replaces the copy button.
const COPIED_SHOWN: Duration = Duration::from_millis(1600);

pub struct State {
    sheet: Option<Sheet>,
    scan: Run<Result<(), String>>,
    defender: Run<Result<(), String>>,
    repair: Repair,
    updates: Updates,
    tips: Tips,
    password: Password,
    bitwarden: Run<Result<(), String>>,
    open_details: Vec<Detail>,
    /// Time of the latest animation frame (never read from the clock in `view`).
    now: Instant,
    /// Zero point for the endless spinners.
    epoch: Instant,
    /// Finished-state draw-ins that are still moving.
    shots: Vec<(Slot, Clock)>,
    /// Smooth progress bar of the running repair or update job.
    bar: Option<Tween>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("tools::State").finish_non_exhaustive()
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            sheet: None,
            scan: Run::Idle,
            defender: Run::Idle,
            repair: Repair::Idle,
            updates: Updates::Idle,
            tips: Tips::Pick,
            password: Password {
                secret: Secret::generate().ok(),
                shown: true,
                copied: false,
            },
            bitwarden: Run::Idle,
            open_details: Vec::new(),
            now: Instant::now(),
            epoch: Instant::now(),
            shots: Vec::new(),
            bar: None,
        }
    }
}

fn tools(msg: Msg) -> Message {
    Message::Tools(msg)
}

fn plain(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Escape closes an open review sheet (same as Cancel).
pub fn escape(state: &mut State) {
    if state.sheet.is_some() {
        state.sheet = None;
        state.close_detail(Detail::Sheet);
    }
}

/// Frames for the spinners and draw-ins, only while one is on screen.
/// The shell merges this into its subscriptions (see `home::subscription`).
pub fn subscription(state: &State, _ctx: &Ctx) -> Subscription<Message> {
    if state.needs_frames() && anim::animating() {
        iced::window::frames().map(|at| Message::Tools(Msg::Frame(at)))
    } else {
        Subscription::none()
    }
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Ask(sheet) => {
            let changes_pc = matches!(sheet, Sheet::Repair(_) | Sheet::InstallUpdates);
            let blocked = changes_pc && (ctx.busy || !state.can_start_change());
            if !blocked {
                state.sheet = Some(sheet);
            }
            Task::none()
        }
        Msg::CloseSheet => {
            state.sheet = None;
            state.close_detail(Detail::Sheet);
            Task::none()
        }
        Msg::Confirm => {
            state.close_detail(Detail::Sheet);
            match state.sheet.take() {
                Some(sheet) => confirm(state, sheet, ctx),
                None => Task::none(),
            }
        }
        Msg::ScanDone(r) => {
            state.scan = Run::Done(r);
            state.finish(Slot::Scan);
            Task::none()
        }
        Msg::DefenderDone(r) => {
            state.defender = Run::Done(r);
            state.finish(Slot::Defender);
            Task::none()
        }
        Msg::ClearScan => {
            state.scan = Run::Idle;
            state.close_detail(Detail::Scan);
            Task::none()
        }
        Msg::ClearDefender => {
            state.defender = Run::Idle;
            state.close_detail(Detail::Defender);
            Task::none()
        }
        Msg::Repair(event) => {
            let Repair::Working { kind, progress, .. } = &mut state.repair else {
                return Task::none();
            };
            match event {
                RepairEvent::Preparing => {}
                RepairEvent::Progress(p) => {
                    let ratio = repair_ratio(&p);
                    *progress = Some(p);
                    state.retarget_bar(ratio);
                }
                RepairEvent::Done {
                    result,
                    note,
                    technical,
                } => {
                    let kind = *kind;
                    state.repair = Repair::Done {
                        kind,
                        result,
                        note,
                        technical,
                    };
                    state.bar = None;
                    state.finish(Slot::Repair);
                    ctx.busy = false;
                }
            }
            Task::none()
        }
        Msg::StopRepair => {
            if let Repair::Working { cancel, .. } = &state.repair {
                cancel.store(true, Ordering::SeqCst);
            }
            Task::none()
        }
        Msg::ClearRepair => {
            if matches!(state.repair, Repair::Done { .. }) {
                state.repair = Repair::Idle;
                state.close_detail(Detail::Repair);
            }
            Task::none()
        }
        Msg::LookForUpdates => {
            if !matches!(
                state.updates,
                Updates::Idle
                    | Updates::UpToDate
                    | Updates::Found(_)
                    | Updates::Failed { .. }
                    | Updates::Done { .. }
            ) {
                return Task::none();
            }
            state.updates = Updates::Looking;
            state.close_detail(Detail::Updates);
            Task::perform(blocking(logic::discover_updates), |r| tools(Msg::Found(r)))
        }
        Msg::Found(result) => {
            if matches!(state.updates, Updates::Looking) {
                state.updates = match result {
                    Ok(found) if found.updates.is_empty() => Updates::UpToDate,
                    Ok(found) => Updates::Found(found),
                    Err((technical, note)) => Updates::Failed { technical, note },
                };
                if !matches!(state.updates, Updates::Found(_)) {
                    state.finish(Slot::Updates);
                }
            }
            Task::none()
        }
        Msg::Install(event) => {
            let Updates::Installing { stage, elapsed, .. } = &mut state.updates else {
                return Task::none();
            };
            match event {
                InstallEvent::Stage {
                    stage: s,
                    elapsed: e,
                } => {
                    *stage = s;
                    *elapsed = e;
                    state.retarget_bar(stage_ratio(s));
                }
                InstallEvent::Done {
                    result,
                    note,
                    technical,
                } => {
                    state.updates = Updates::Done {
                        result,
                        note,
                        technical,
                    };
                    state.bar = None;
                    state.finish(Slot::Updates);
                    ctx.busy = false;
                }
            }
            Task::none()
        }
        Msg::StopInstall => {
            if let Updates::Installing { cancel, .. } = &state.updates {
                cancel.store(true, Ordering::SeqCst);
            }
            Task::none()
        }
        Msg::ClearUpdates => {
            if matches!(
                state.updates,
                Updates::Done { .. } | Updates::Failed { .. } | Updates::UpToDate
            ) {
                state.updates = Updates::Idle;
                state.close_detail(Detail::Updates);
            }
            Task::none()
        }
        Msg::PickTips(profile) => {
            if matches!(state.tips, Tips::Running(_)) {
                return Task::none();
            }
            state.tips = Tips::Running(profile);
            state.close_detail(Detail::Tips);
            Task::perform(blocking(move || logic::run_tips(profile)), |r| {
                tools(Msg::Tips(Box::new(r)))
            })
        }
        Msg::Tips(report) => {
            if matches!(state.tips, Tips::Running(_)) {
                state.tips = Tips::Done(report);
            }
            Task::none()
        }
        Msg::ChooseAnotherTips => {
            if matches!(state.tips, Tips::Done(_)) {
                state.tips = Tips::Pick;
                state.close_detail(Detail::Tips);
            }
            Task::none()
        }
        Msg::NewPassword => {
            // The old secret is wiped when it is replaced.
            state.password.secret = Secret::generate().ok();
            state.password.shown = true;
            state.password.copied = false;
            state.shots.retain(|(slot, _)| *slot != Slot::Copy);
            Task::none()
        }
        Msg::CopyPassword => match state
            .password
            .secret
            .as_ref()
            .map(|s| s.reveal().to_owned())
        {
            Some(secret) => {
                state.password.copied = true;
                state.finish(Slot::Copy);
                Task::batch([
                    iced::clipboard::write(secret),
                    Task::perform(blocking(|| std::thread::sleep(COPIED_SHOWN)), |()| {
                        tools(Msg::CopiedReset)
                    }),
                ])
            }
            None => Task::none(),
        },
        Msg::CopiedReset => {
            state.password.copied = false;
            Task::none()
        }
        Msg::Frame(now) => {
            state.now = now;
            state
                .shots
                .retain(|(_, clock)| !clock.done(anim::SLOW, now));
            Task::none()
        }
        Msg::TogglePassword => {
            state.password.shown = !state.password.shown;
            Task::none()
        }
        Msg::BitwardenDone(reply) => {
            state.bitwarden = Run::Done(match reply {
                Ok(broker::Reply::Done) => Ok(()),
                Ok(other) => Err(format!("{other:?}")),
                Err(e) => Err(e),
            });
            state.finish(Slot::Bitwarden);
            Task::none()
        }
        Msg::ClearBitwarden => {
            state.bitwarden = Run::Idle;
            state.close_detail(Detail::Bitwarden);
            Task::none()
        }
        Msg::Open(shortcut) => ctx.broker_task(shortcut.request(), |r| tools(Msg::Opened(r))),
        Msg::OpenSecurity => ctx.broker_task(broker::Request::OpenWindowsSecurity, |r| {
            tools(Msg::Opened(r))
        }),
        Msg::Opened(reply) => match reply {
            Ok(broker::Reply::Done | broker::Reply::OpenedStore) => Task::none(),
            _ => Task::done(Message::Toast(
                ctx.t("We couldn't open that page. Please try again."),
                Tone::Warn,
            )),
        },
        Msg::ToggleDetail(detail) => {
            if state.open_details.contains(&detail) {
                state.close_detail(detail);
            } else {
                state.open_details.push(detail);
            }
            Task::none()
        }
    }
}

/// Bar position for a repair step (the current step counts as half done).
fn repair_ratio(p: &RepairProgress) -> f32 {
    (p.step as f32 - 0.5) / p.total.max(1) as f32
}

fn stage_ratio(stage: InstallStage) -> f32 {
    match stage {
        InstallStage::Preparing => 0.12,
        InstallStage::Installing => 0.55,
        InstallStage::Checking => 0.9,
    }
}

impl State {
    /// A job just ended: start its draw-in (check, cross or warning).
    fn finish(&mut self, slot: Slot) {
        self.shots.retain(|(s, _)| *s != slot);
        self.shots.push((slot, Clock::new()));
    }
    /// 0..1 draw-in progress of a finished state; 1 when it is not moving.
    fn shot(&self, slot: Slot) -> f32 {
        self.shots
            .iter()
            .find(|(s, _)| *s == slot)
            .map_or(1.0, |(_, clock)| clock.progress_at(anim::SLOW, self.now))
    }
    /// Time since the spinners' zero point.
    fn spin(&self) -> Duration {
        self.now.saturating_duration_since(self.epoch)
    }
    fn start_bar(&mut self, at: f32) {
        self.bar = Some(Tween::new(at, at, anim::SLOW));
    }
    fn retarget_bar(&mut self, to: f32) {
        let now = Instant::now();
        if let Some(bar) = &mut self.bar {
            bar.retarget(now, to);
        }
    }
    fn bar_value(&self, fallback: f32) -> f32 {
        self.bar.map_or(fallback, |b| b.value(self.now))
    }
    /// True while any spinner or draw-in is on screen.
    fn needs_frames(&self) -> bool {
        !self.shots.is_empty()
            || matches!(self.scan, Run::Working)
            || matches!(self.defender, Run::Working)
            || matches!(self.bitwarden, Run::Working)
            || matches!(self.repair, Repair::Working { .. })
            || matches!(self.updates, Updates::Looking | Updates::Installing { .. })
            || matches!(self.tips, Tips::Running(_))
    }

    fn close_detail(&mut self, detail: Detail) {
        self.open_details.retain(|d| *d != detail);
    }
    fn detail_open(&self, detail: Detail) -> bool {
        self.open_details.contains(&detail)
    }
    /// Repair and update jobs never overlap each other.
    fn can_start_change(&self) -> bool {
        !matches!(self.repair, Repair::Working { .. })
            && !matches!(self.updates, Updates::Installing { .. } | Updates::Looking)
    }
}

fn confirm(state: &mut State, sheet: Sheet, ctx: &mut Ctx) -> Task<Message> {
    match sheet {
        Sheet::Scan => {
            state.scan = Run::Working;
            Task::perform(
                blocking(|| {
                    actions::run(actions::Action::QuickScan)
                        .map(|_| ())
                        .map_err(plain)
                }),
                |r| tools(Msg::ScanDone(r)),
            )
        }
        Sheet::DefenderUpdate => {
            state.defender = Run::Working;
            Task::perform(
                blocking(|| {
                    actions::run(actions::Action::UpdateDefender)
                        .map(|_| ())
                        .map_err(plain)
                }),
                |r| tools(Msg::DefenderDone(r)),
            )
        }
        Sheet::Repair(kind) => {
            if ctx.busy || !state.can_start_change() {
                return Task::none();
            }
            ctx.busy = true;
            let cancel = Arc::new(AtomicBool::new(false));
            state.repair = Repair::Working {
                kind,
                cancel: cancel.clone(),
                progress: None,
            };
            state.start_bar(0.03);
            state.close_detail(Detail::Repair);
            Task::run(
                blocking_stream(move |emit| logic::run_repair(kind, cancel, emit)),
                |event| tools(Msg::Repair(event)),
            )
        }
        Sheet::InstallUpdates => {
            let Updates::Found(found) = &state.updates else {
                return Task::none();
            };
            if ctx.busy || !state.can_start_change() {
                return Task::none();
            }
            let reviewed = found.identities();
            let count = reviewed.len();
            ctx.busy = true;
            let cancel = Arc::new(AtomicBool::new(false));
            state.updates = Updates::Installing {
                cancel: cancel.clone(),
                stage: InstallStage::Preparing,
                elapsed: 0,
                count,
            };
            state.start_bar(stage_ratio(InstallStage::Preparing));
            state.close_detail(Detail::Updates);
            Task::run(
                blocking_stream(move |emit| logic::run_install(reviewed, cancel, emit)),
                |event| tools(Msg::Install(event)),
            )
        }
        Sheet::Bitwarden => {
            state.bitwarden = Run::Working;
            ctx.broker_task(broker::Request::InstallBitwarden, |r| {
                tools(Msg::BitwardenDone(r))
            })
        }
    }
}
