//! Tools page: state and update logic. OWNER: tools agent.
//!
//! Everything that touches Windows runs off the UI thread (`blocking`,
//! `blocking_stream`, `ctx.broker_task`). The drawing code is in `view.rs`.
//!
//! Every change to the PC (repair, updates, scans, installs) first opens a
//! review sheet; the sheet's Cancel button is the safe way out.
mod view;

use super::personal;

use crate::app::tools::{
    self as logic, Found, InstallEvent, InstallResult, InstallStage, RepairEvent, RepairKind,
    RepairProgress, RepairResult, Secret, TipProfile, TipsReport,
};
use crate::broker;
use crate::gui::widgets::anim::{self, Clock};
use crate::gui::{blocking, blocking_stream, Ctx, Message, Tone};
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
    /// Restart the PC to finish installing updates.
    Restart,
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
    fn page(self) -> crate::guide::Page {
        use crate::guide::Page;
        match self {
            Self::WindowsUpdate => Page::WindowsUpdate,
            Self::WindowsSecurity => Page::WindowsSecurity,
            Self::Encryption => Page::Encryption,
            Self::SignIn => Page::SignIn,
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
    /// The list of health tips is expanded.
    TipsList,
    /// The collapsed "All good" group of health tips.
    TipsGood,
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
    TipChoice(TipProfile),
    NewPassword,
    CopyPassword,
    TogglePassword,
    /// The brief "Copied" confirmation on the password card is over.
    CopiedReset,
    /// Animation frame; only delivered while something moves.
    Frame(Instant),
    BitwardenDone(Result<broker::Reply, String>),
    /// Whether Bitwarden is already installed, asked once on entering.
    BitwardenKnown(Result<broker::Reply, String>),
    ClearBitwarden,
    Open(Shortcut),
    /// Open the Windows page a health tip points to.
    OpenAction(actions::Action),
    OpenSecurity,
    /// The restart request was answered: an error means nothing was restarted.
    RestartDone(Result<(), String>),
    ToggleDetail(Detail),
    Personal(personal::Msg),
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
        #[allow(dead_code)] // raw evidence, never shown on screen
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
        #[allow(dead_code)] // raw evidence, never shown on screen
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
        #[allow(dead_code)] // raw evidence, never shown on screen
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
    /// Profile picked in the segmented control (checked on "Check").
    tip_choice: TipProfile,
    password: Password,
    bitwarden: Run<Result<(), String>>,
    /// Why the last Bitwarden install failed, when it is a known reason
    /// (`Offline` or `Unavailable` for this account).
    bitwarden_why: Option<broker::Reply>,
    /// Already installed when the page was opened: nothing to offer.
    pub bitwarden_present: bool,
    /// Installation is not possible from this account (checked on page enter).
    /// The Install button is replaced by the "can't install from this account"
    /// presentation when this is true and Bitwarden is not already installed.
    bitwarden_not_here: bool,
    open_details: Vec<Detail>,
    personal: personal::State,
    /// Time of the latest animation frame (never read from the clock in `view`).
    now: Instant,
    /// Zero point for the spinners on rows that are working.
    spin: Instant,
    /// Finished-state draw-ins that are still moving.
    shots: Vec<(Slot, Clock)>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("tools::State").finish_non_exhaustive()
    }
}

impl State {
    /// Record what the status check found: `Done` means installed,
    /// `Unavailable` that this account can't install it.
    fn bitwarden_known(&mut self, reply: &Result<broker::Reply, String>) {
        self.bitwarden_present = matches!(reply, Ok(broker::Reply::Done));
        self.bitwarden_not_here = matches!(reply, Ok(broker::Reply::Unavailable));
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
            tip_choice: TipProfile::Everyday,
            password: Password {
                secret: Secret::generate().ok(),
                shown: true,
                copied: false,
            },
            bitwarden: Run::Idle,
            bitwarden_why: None,
            bitwarden_present: false,
            bitwarden_not_here: false,
            open_details: Vec::new(),
            personal: Default::default(),
            now: Instant::now(),
            spin: Instant::now(),
            shots: Vec::new(),
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

/// The Tools page was opened: read the account settings once, and whether
/// Bitwarden is already there.
pub fn on_enter(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let settings = personal::on_enter(&mut state.personal, ctx);
    if ctx.broker.is_none() || state.bitwarden_present || !matches!(state.bitwarden, Run::Idle) {
        return settings;
    }
    Task::batch([
        settings,
        ctx.broker_task(broker::Request::BitwardenStatus, |r| {
            tools(Msg::BitwardenKnown(r))
        }),
    ])
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Ask(sheet) => {
            // A restart would cut a running fix, repair or update short.
            let changes_pc = matches!(
                sheet,
                Sheet::Repair(_) | Sheet::InstallUpdates | Sheet::Restart
            );
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
                    *progress = Some(p);
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
            state.close_detail(Detail::TipsList);
            state.tip_choice = profile;
            Task::perform(blocking(move || logic::run_tips(profile)), |r| {
                tools(Msg::Tips(Box::new(r)))
            })
        }
        Msg::Tips(report) => {
            if matches!(state.tips, Tips::Running(_)) {
                state.tips = Tips::Done(report);
                if !state.detail_open(Detail::TipsList) {
                    state.open_details.push(Detail::TipsList);
                }
            }
            Task::none()
        }
        Msg::TipChoice(profile) => {
            state.tip_choice = profile;
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
            state.bitwarden_why = match reply {
                Ok(r @ (broker::Reply::Offline | broker::Reply::Unavailable)) => Some(r),
                _ => None,
            };
            state.bitwarden_present |= matches!(reply, Ok(broker::Reply::Done));
            state.bitwarden = Run::Done(match reply {
                Ok(broker::Reply::Done) => Ok(()),
                Ok(other) => Err(format!("{other:?}")),
                Err(e) => Err(e),
            });
            state.finish(Slot::Bitwarden);
            Task::none()
        }
        Msg::BitwardenKnown(reply) => {
            state.bitwarden_known(&reply);
            Task::none()
        }
        Msg::ClearBitwarden => {
            state.bitwarden = Run::Idle;
            state.close_detail(Detail::Bitwarden);
            Task::none()
        }
        Msg::Open(shortcut) => super::fixes::open_page(ctx, shortcut.page()),
        Msg::OpenAction(action) => match crate::guide::Page::from_action(action) {
            Some(page) => super::fixes::open_page(ctx, page),
            None => Task::none(),
        },
        Msg::OpenSecurity => super::fixes::open_page(ctx, crate::guide::Page::WindowsSecurity),
        Msg::RestartDone(result) => match result {
            Ok(()) => Task::done(Message::Toast(
                ctx.t("Restarting now. Programs with unsaved work will ask you first."),
                Tone::Good,
            )),
            Err(_) => Task::done(Message::Toast(
                ctx.t("We couldn't restart your PC. Restart it from the Start menu instead."),
                Tone::Warn,
            )),
        },
        Msg::Personal(msg) => personal::update(&mut state.personal, msg, ctx),
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
/// Each step owns an equal span of the bar. Within it the bar creeps toward
/// the end without reaching it, so a long system-file check never looks
/// frozen and the next step never jumps backwards.
fn repair_ratio(p: &RepairProgress) -> f32 {
    const PACE: f32 = 180.0;
    let creep = 1.0 - (-(p.step_elapsed as f32) / PACE).exp();
    (p.step.saturating_sub(1) as f32 + 0.9 * creep) / p.total.max(1) as f32
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
    /// True while a draw-in mark is animating. Progress bars ask for their
    /// own redraws, so running jobs need no page-wide frame clock.
    fn needs_frames(&self) -> bool {
        !self.shots.is_empty() || self.spinning()
    }

    /// A row shows the working spinner: a job of unknown length is running.
    fn spinning(&self) -> bool {
        matches!(self.scan, Run::Working)
            || matches!(self.defender, Run::Working)
            || matches!(self.bitwarden, Run::Working)
            || matches!(self.updates, Updates::Looking)
    }

    /// Time on the spinners' clock, read from the last frame.
    pub(super) fn spin_elapsed(&self) -> std::time::Duration {
        self.now.saturating_duration_since(self.spin)
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
        Sheet::Restart => Task::perform(
            blocking(|| actions::restart_for_updates().map_err(plain)),
            |r| tools(Msg::RestartDone(r)),
        ),
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

#[cfg(test)]
mod followup_tests {
    use super::*;

    #[test]
    fn repair_bar_creeps_inside_its_step_and_never_goes_back() {
        let at = |step, secs| {
            repair_ratio(&RepairProgress {
                label: "",
                step,
                total: 2,
                elapsed: secs,
                step_elapsed: secs,
            })
        };
        assert_eq!(at(1, 0), 0.0);
        assert!(at(2, 30) > at(2, 0) && at(2, 600) > at(2, 30));
        assert!(at(1, 100_000) < at(2, 0), "the next step starts ahead");
        assert!(at(2, 100_000) < 1.0, "only Done fills the bar");
    }

    /// bitwarden_not_here starts false and is set correctly from BitwardenKnown replies.
    #[test]
    fn bitwarden_known_sets_not_here_and_present_correctly() {
        let fresh = State::default();
        assert!(!fresh.bitwarden_not_here, "default: not_here is false");
        assert!(!fresh.bitwarden_present, "default: present is false");

        // Unavailable: install is not possible from this account.
        let mut state = State::default();
        state.bitwarden_known(&Ok(broker::Reply::Unavailable));
        assert!(!state.bitwarden_present, "Unavailable: not present");
        assert!(state.bitwarden_not_here, "Unavailable: not_here is set");

        // Done: Bitwarden is installed.
        let mut state = State::default();
        state.bitwarden_known(&Ok(broker::Reply::Done));
        assert!(state.bitwarden_present, "Done: present");
        assert!(!state.bitwarden_not_here, "Done: not_here stays false");

        // NotApplicable: not installed, install is possible.
        let mut state = State::default();
        state.bitwarden_known(&Ok(broker::Reply::NotApplicable));
        assert!(!state.bitwarden_present, "NotApplicable: not present");
        assert!(!state.bitwarden_not_here, "NotApplicable: not_here is false");

        // Unavailable then Done: re-entering with installed clears not_here.
        let mut state = State::default();
        state.bitwarden_known(&Ok(broker::Reply::Unavailable));
        assert!(state.bitwarden_not_here);
        state.bitwarden_known(&Ok(broker::Reply::Done));
        assert!(state.bitwarden_present, "second call Done: present");
        assert!(!state.bitwarden_not_here, "second call Done: not_here cleared");
    }
}
