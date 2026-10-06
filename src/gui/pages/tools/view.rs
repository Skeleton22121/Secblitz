//! Drawing code for the Tools page. OWNER: tools agent.
//!
//! Calm groups of rows straight on the page: a plain icon, a short title, one
//! line of help and one compact action. Rare actions live in each row's
//! overflow menu. Built only from the shared widgets and theme tokens;
//! `view` does no work beyond building widgets.
use super::{
    repair_ratio, stage_ratio, tools, Detail, Msg, Repair, Run, Sheet, Shortcut, Slot, State, Tips,
    Updates,
};
use crate::app::tools::{
    self as logic, InstallResult, RepairKind, RepairResult, TipProfile, TipState,
};
use crate::broker;
use crate::gui::icons::Icon;
use crate::gui::pages::personal;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, anim, progress, ButtonKind};
use crate::gui::{Ctx, Message, Page};
use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Element, Font, Length};

type El<'a> = Element<'a, Message>;
type MenuEntry = (Icon, String, Message, bool);

/// Size of the spinners and result marks inside rows.
const MARK: f32 = 20.0;

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    page(state, ctx)
}

/// The open review sheet, drawn by the shell above the whole window.
pub fn modal<'a>(state: &'a State, ctx: &'a Ctx) -> Option<El<'a>> {
    state.sheet.map(|sheet| sheet_panel(state, ctx, sheet))
}

fn page<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    column![
        widgets::page_header(
            p,
            ctx.t("Tools"),
            Some(ctx.t("Handy ways to keep your PC safe and healthy.")),
        ),
        widgets::group(
            p,
            ctx.t("Virus protection"),
            None,
            None,
            vec![scan_row(state, ctx), defender_row(state, ctx)],
        ),
        widgets::group(
            p,
            ctx.t("Repair & updates"),
            None,
            None,
            vec![repair_row(state, ctx), updates_row(state, ctx)],
        ),
        widgets::group(
            p,
            ctx.t("PC health tips"),
            None,
            None,
            tips_block(state, ctx)
        ),
        widgets::group(
            p,
            ctx.t("Passwords"),
            None,
            None,
            vec![password_region(state, ctx), manager_row(state, ctx)],
        ),
        personal::view(&state.personal, ctx),
        settings_group(ctx),
    ]
    .spacing(theme::S8)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------

/// Compact secondary button (short label).
fn secondary<'a>(p: Palette, label: String, msg: Option<Msg>) -> El<'a> {
    widgets::action(p, ButtonKind::Secondary, label, None, msg.map(tools))
}

fn entry(icon: Icon, label: String, msg: Msg) -> MenuEntry {
    (icon, label, tools(msg), false)
}

/// Overflow button, or nothing when there is nothing to put in it.
fn more<'a>(p: Palette, items: Vec<MenuEntry>) -> El<'a> {
    if items.is_empty() {
        space::horizontal().width(0).into()
    } else {
        widgets::overflow_menu(p, items)
    }
}

fn trailing<'a>(items: Vec<El<'a>>) -> El<'a> {
    row(items)
        .spacing(theme::S2)
        .align_y(Alignment::Center)
        .into()
}

/// What a running row shows besides its title.
struct Running<'a> {
    icon: Icon,
    title: String,
    sub: String,
    menu: Vec<MenuEntry>,
    bar: El<'a>,
    notes: Vec<El<'a>>,
}

/// A row that is working: the bar sits under its title, inside the row.
fn running<'a>(p: Palette, r: Running<'a>) -> El<'a> {
    let mut below = vec![r.bar];
    below.extend(r.notes);
    widgets::row_item_below(
        p,
        Some(r.icon),
        None,
        r.title,
        Some(r.sub),
        trailing(vec![more(p, r.menu)]),
        below,
        None,
    )
}

/// A working row whose length is unknown: a spinner where its button was,
/// so the row keeps its height and nothing below it moves.
fn busy_row<'a>(state: &State, p: Palette, icon: Icon, title: String, sub: String) -> El<'a> {
    widgets::row_item(
        p,
        Some(icon),
        title,
        Some(sub),
        anim::spinner(MARK, p.text_muted, state.spin_elapsed()),
        None,
    )
}

/// A finished job: the mark draws itself in (check, cross or warning), with
/// the outcome as title and the rest in the row's menu.
struct Outcome {
    slot: Slot,
    icon: Icon,
    tone: Tone,
    title: String,
    sub: Option<String>,
    menu: Vec<MenuEntry>,
    raw: Option<(Detail, String)>,
}

fn finished<'a>(state: &State, ctx: &Ctx, o: Outcome) -> El<'a> {
    finished_with(state, ctx, o, None)
}

/// `finished` plus one visible next-step button (Retry, Open Windows Update).
fn finished_with<'a>(
    state: &State,
    ctx: &Ctx,
    o: Outcome,
    button: Option<(String, Icon, Msg)>,
) -> El<'a> {
    let p = ctx.palette;
    let t = state.shot(o.slot);
    let color = p.tone(o.tone);
    let mark: El<'a> = match o.tone {
        Tone::Good => anim::check_draw(MARK, color, t),
        Tone::Bad => anim::cross_draw(MARK, color, t),
        _ => anim::warn_draw(MARK, color, t),
    };
    let mut menu = o.menu;
    let mut below = Vec::new();
    if let Some((which, raw)) = o.raw {
        menu.push(entry(
            Icon::Info,
            ctx.t("More details"),
            Msg::ToggleDetail(which),
        ));
        if state.detail_open(which) {
            below.push(raw_text(ctx, &raw));
        }
    }
    widgets::row_item_below(
        p,
        Some(o.icon),
        None,
        o.title,
        o.sub,
        {
            let mut items = Vec::new();
            if let Some((label, icon, msg)) = button {
                items.push(widgets::action(
                    p,
                    ButtonKind::Secondary,
                    label,
                    Some(icon),
                    Some(tools(msg)),
                ));
            }
            items.push(mark);
            items.push(more(p, menu));
            trailing(items)
        },
        below,
        None,
    )
}

/// Plain-words explanation for the "More details" pane. Never raw
/// developer text: callers pass sentences from `logic::*_why`.
fn raw_text<'a>(ctx: &Ctx, plain: &str) -> El<'a> {
    let p = ctx.palette;
    let shown = if plain.trim().is_empty() {
        ctx.t("No extra details.")
    } else {
        ctx.t(plain.trim())
    };
    text(shown).size(theme::SMALL).color(p.text_muted).into()
}

fn elapsed_phrase(ctx: &Ctx, secs: u64) -> String {
    if secs < 60 {
        ctx.t("{n} sec").replace("{n}", &secs.to_string())
    } else {
        ctx.t("{n} min").replace("{n}", &(secs / 60).to_string())
    }
}

fn running_for(ctx: &Ctx, secs: u64) -> String {
    ctx.t("Running for {time}")
        .replace("{time}", &elapsed_phrase(ctx, secs))
}

/// Collapsed "More details" for people who want the raw evidence (review sheet).
fn details<'a>(state: &State, ctx: &Ctx, which: Detail, raw: &str) -> El<'a> {
    widgets::expander(
        ctx.palette,
        ctx.t("More details"),
        state.detail_open(which),
        tools(Msg::ToggleDetail(which)),
        raw_text(ctx, raw),
    )
}

fn open_security_entry(ctx: &Ctx) -> Option<MenuEntry> {
    ctx.broker.is_some().then(|| {
        entry(
            Icon::ExternalLink,
            ctx.t("Open Windows Security"),
            Msg::OpenSecurity,
        )
    })
}

fn reopen_hint(ctx: &Ctx) -> String {
    ctx.t("Reopen Secblitz from its shortcut to use this.")
}

fn busy_hint(ctx: &Ctx) -> String {
    ctx.t("Another task is running. Please wait for it to finish.")
}

// ---------------------------------------------------------------------------
// Virus protection
// ---------------------------------------------------------------------------

fn scan_row<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    match &state.scan {
        Run::Idle => widgets::row_item(
            p,
            Some(Icon::Bug),
            ctx.t("Scan for viruses"),
            Some(ctx.t("Look for harmful software on your PC.")),
            secondary(p, ctx.t("Scan"), Some(Msg::Ask(Sheet::Scan))),
            None,
        ),
        Run::Working => busy_row(
            state,
            p,
            Icon::Bug,
            ctx.t("Scan for viruses"),
            ctx.t("Starting the scan…"),
        ),
        Run::Done(Ok(())) => finished(
            state,
            ctx,
            Outcome {
                slot: Slot::Scan,
                icon: Icon::Bug,
                tone: Tone::Good,
                title: ctx.t("Scan started"),
                sub: Some(ctx.t("Windows Security will notify you if it finds anything.")),
                menu: open_security_entry(ctx)
                    .into_iter()
                    .chain([entry(Icon::Check, ctx.t("Done"), Msg::ClearScan)])
                    .collect(),
                raw: None,
            },
        ),
        Run::Done(Err(raw)) => finished(
            state,
            ctx,
            Outcome {
                slot: Slot::Scan,
                icon: Icon::Bug,
                tone: Tone::Warn,
                title: ctx.t("We couldn't start the scan"),
                sub: Some(ctx.t("Open Windows Security and start a scan there.")),
                menu: open_security_entry(ctx)
                    .into_iter()
                    .chain([entry(Icon::Refresh, ctx.t("Try again"), Msg::ClearScan)])
                    .collect(),
                raw: Some((Detail::Scan, logic::friendly_why(raw).to_owned())),
            },
        ),
    }
}

fn defender_row<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    match &state.defender {
        Run::Idle => widgets::row_item(
            p,
            Some(Icon::Download),
            ctx.t("Update virus protection"),
            Some(ctx.t("Get the newest virus information.")),
            secondary(p, ctx.t("Update"), Some(Msg::Ask(Sheet::DefenderUpdate))),
            None,
        ),
        Run::Working => busy_row(
            state,
            p,
            Icon::Download,
            ctx.t("Update virus protection"),
            ctx.t("Updating…"),
        ),
        Run::Done(Ok(())) => finished(
            state,
            ctx,
            Outcome {
                slot: Slot::Defender,
                icon: Icon::Download,
                tone: Tone::Good,
                title: ctx.t("Virus protection updated"),
                sub: Some(ctx.t("Windows asked Microsoft for the newest virus information.")),
                menu: vec![entry(Icon::Check, ctx.t("Done"), Msg::ClearDefender)],
                raw: None,
            },
        ),
        Run::Done(Err(raw)) => finished(
            state,
            ctx,
            Outcome {
                slot: Slot::Defender,
                icon: Icon::Download,
                tone: Tone::Warn,
                title: ctx.t("We couldn't update right now"),
                sub: Some(ctx.t("Check your internet connection and try again.")),
                menu: vec![entry(Icon::Refresh, ctx.t("Try again"), Msg::ClearDefender)],
                raw: Some((Detail::Defender, logic::friendly_why(raw).to_owned())),
            },
        ),
    }
}

// ---------------------------------------------------------------------------
// Repair & updates
// ---------------------------------------------------------------------------

fn keep_using<'a>(p: Palette, ctx: &Ctx) -> El<'a> {
    widgets::small(
        p,
        ctx.t("You can keep using your PC. Please don't turn it off."),
    )
}

fn repair_row<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let free = !ctx.busy;
    match &state.repair {
        Repair::Idle => {
            let menu = if free {
                vec![entry(
                    Icon::Wrench,
                    ctx.t("Repair system files"),
                    Msg::Ask(Sheet::Repair(RepairKind::Repair)),
                )]
            } else {
                vec![]
            };
            widgets::row_item(
                p,
                Some(Icon::Wrench),
                ctx.t("Repair Windows"),
                Some(if free {
                    ctx.t("Find and fix problems with Windows itself.")
                } else {
                    busy_hint(ctx)
                }),
                trailing(vec![
                    secondary(
                        p,
                        ctx.t("Check"),
                        free.then_some(Msg::Ask(Sheet::Repair(RepairKind::Check))),
                    ),
                    more(p, menu),
                ]),
                None,
            )
        }
        Repair::Working {
            kind,
            cancel,
            progress: pr,
        } => {
            let title = match kind {
                RepairKind::Check => ctx.t("Checking for problems"),
                RepairKind::Repair => ctx.t("Repairing Windows"),
            };
            let stopping = cancel.load(std::sync::atomic::Ordering::SeqCst);
            let (ratio, label) = match pr {
                Some(pr) => (repair_ratio(pr), ctx.t(pr.label)),
                None => (0.03, ctx.t("Getting ready…")),
            };
            let mut notes = Vec::new();
            if let Some(pr) = pr {
                notes.push(widgets::small(
                    p,
                    format!(
                        "{}  ·  {}",
                        ctx.t("Step {a} of {b}")
                            .replace("{a}", &pr.step.to_string())
                            .replace("{b}", &pr.total.to_string()),
                        running_for(ctx, pr.elapsed)
                    ),
                ));
            }
            notes.push(if stopping {
                widgets::small(p, ctx.t("Stopping after this step…"))
            } else {
                keep_using(p, ctx)
            });
            let menu = if stopping {
                vec![]
            } else {
                vec![entry(
                    Icon::X,
                    ctx.t("Stop after this step"),
                    Msg::StopRepair,
                )]
            };
            running(
                p,
                Running {
                    icon: Icon::Wrench,
                    title,
                    sub: label,
                    menu,
                    bar: progress::bar_eased(p, ratio, Tone::Brand),
                    notes,
                },
            )
        }
        Repair::Done {
            kind,
            result,
            note,
            ..
        } => {
            let tone = match result {
                RepairResult::NoProblems | RepairResult::Repaired => Tone::Good,
                RepairResult::CouldNotFinish => Tone::Bad,
                RepairResult::ProblemsFound
                | RepairResult::NeedsRestart
                | RepairResult::Stopped => Tone::Warn,
            };
            let detail = match (result, note) {
                (RepairResult::CouldNotFinish, Some(n)) => ctx.t(n),
                _ => ctx.t(result.detail()),
            };
            let mut menu = Vec::new();
            if *result == RepairResult::ProblemsFound && *kind == RepairKind::Check && free {
                menu.push(entry(
                    Icon::Wrench,
                    ctx.t("Repair system files"),
                    Msg::Ask(Sheet::Repair(RepairKind::Repair)),
                ));
            }
            menu.push(entry(Icon::Check, ctx.t("Done"), Msg::ClearRepair));
            finished(
                state,
                ctx,
                Outcome {
                    slot: Slot::Repair,
                    icon: Icon::Wrench,
                    tone,
                    title: ctx.t(result.title()),
                    sub: Some(detail),
                    menu,
                    raw: Some((Detail::Repair, logic::repair_why(*result, *note).to_owned())),
                },
            )
        }
    }
}

fn updates_row<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let free = !ctx.busy;
    let again = |label: String| entry(Icon::Refresh, label, Msg::LookForUpdates);
    match &state.updates {
        Updates::Idle => widgets::row_item(
            p,
            Some(Icon::Download),
            ctx.t("Windows updates"),
            Some(if free {
                ctx.t("Install important security updates.")
            } else {
                busy_hint(ctx)
            }),
            secondary(
                p,
                ctx.t("Look for updates"),
                free.then_some(Msg::LookForUpdates),
            ),
            None,
        ),
        Updates::Looking => busy_row(
            state,
            p,
            Icon::Download,
            ctx.t("Windows updates"),
            ctx.t("Looking for updates…"),
        ),
        Updates::UpToDate => finished(
            state,
            ctx,
            Outcome {
                slot: Slot::Updates,
                icon: Icon::Download,
                tone: Tone::Good,
                title: ctx.t("Your PC is up to date"),
                sub: Some(ctx.t("There are no important updates waiting.")),
                menu: vec![again(ctx.t("Check again"))],
                raw: None,
            },
        ),
        Updates::Found(found) => {
            let size = logic::size_phrase(found.total_bytes());
            let restart = ctx.t("Windows may need to restart afterwards.");
            let tail = if size.is_empty() {
                restart
            } else {
                format!(
                    "{} {restart}",
                    ctx.t("The download is about {size}.")
                        .replace("{size}", &size)
                )
            };
            widgets::row_item_tinted(
                p,
                Some(Icon::Download),
                Some(Tone::Warn),
                count_line(ctx, found.updates.len()),
                Some(tail),
                trailing(vec![
                    widgets::action(
                        p,
                        ButtonKind::Primary,
                        ctx.t("Install updates"),
                        None,
                        free.then(|| tools(Msg::Ask(Sheet::InstallUpdates))),
                    ),
                    more(p, vec![again(ctx.t("Check again"))]),
                ]),
                None,
            )
        }
        Updates::Failed { note, .. } => {
            let (menu, button) = failure_steps(ctx, note, again(ctx.t("Try again")));
            finished_with(
                state,
                ctx,
                Outcome {
                    slot: Slot::Updates,
                    icon: Icon::Download,
                    tone: Tone::Warn,
                    title: ctx.t("We couldn't check for updates"),
                    sub: Some(ctx.t(note)),
                    menu,
                    raw: Some((Detail::Updates, logic::why_for_note(note).to_owned())),
                },
                button,
            )
        }
        Updates::Installing {
            cancel,
            stage,
            elapsed,
            count,
        } => {
            let stopping = cancel.load(std::sync::atomic::Ordering::SeqCst);
            let notes = vec![
                widgets::small(p, running_for(ctx, *elapsed)),
                if stopping {
                    widgets::small(p, ctx.t("Stopping after this step…"))
                } else {
                    keep_using(p, ctx)
                },
            ];
            let menu = if stopping {
                vec![]
            } else {
                vec![entry(
                    Icon::X,
                    ctx.t("Stop after this step"),
                    Msg::StopInstall,
                )]
            };
            running(
                p,
                Running {
                    icon: Icon::Download,
                    title: count_installing(ctx, *count),
                    sub: ctx.t(stage.label()),
                    menu,
                    bar: progress::bar_eased(p, stage_ratio(*stage), Tone::Brand),
                    notes,
                },
            )
        }
        Updates::Done { result, note, .. } => {
            let tone = match result {
                InstallResult::Installed => Tone::Good,
                InstallResult::CouldNotFinish => Tone::Bad,
                _ => Tone::Warn,
            };
            let detail = match (result, note) {
                (InstallResult::CouldNotFinish, Some(n)) => ctx.t(n),
                _ => ctx.t(result.detail()),
            };
            let mut menu = Vec::new();
            let mut button = None;
            if let (InstallResult::CouldNotFinish, Some(n)) = (result, note) {
                if logic::suggests_windows_update(n) && ctx.broker.is_some() {
                    button = Some(open_update_button(ctx));
                }
            }
            if *result == InstallResult::NotConfirmed && ctx.broker.is_some() {
                menu.push(entry(
                    Icon::ExternalLink,
                    ctx.t("Open Windows Update"),
                    Msg::Open(Shortcut::WindowsUpdate),
                ));
            }
            menu.push(entry(Icon::Check, ctx.t("Done"), Msg::ClearUpdates));
            finished_with(
                state,
                ctx,
                Outcome {
                    slot: Slot::Updates,
                    icon: Icon::Download,
                    tone,
                    title: ctx.t(result.title()),
                    sub: Some(detail),
                    menu,
                    raw: Some((Detail::Updates, logic::install_why(*result, *note).to_owned())),
                },
                button,
            )
        }
    }
}

/// Plain count of what the tips check found.
fn tips_summary(ctx: &Ctx, report: &logic::TipsReport) -> String {
    ctx.t("{good} checks look fine, {look} need a look and {unknown} could not be checked.")
        .replace("{good}", &report.count(logic::TipState::Good).to_string())
        .replace("{look}", &report.count(logic::TipState::Look).to_string())
        .replace(
            "{unknown}",
            &report.count(logic::TipState::Unknown).to_string(),
        )
}

fn open_update_button(ctx: &Ctx) -> (String, Icon, Msg) {
    (
        ctx.t("Open Windows Update"),
        Icon::ExternalLink,
        Msg::Open(Shortcut::WindowsUpdate),
    )
}

/// What to offer after a failed update lookup: retrying only when it can help,
/// otherwise a way to finish in Windows Update itself.
fn failure_steps(
    ctx: &Ctx,
    note: &str,
    retry: MenuEntry,
) -> (Vec<MenuEntry>, Option<(String, Icon, Msg)>) {
    if logic::suggests_windows_update(note) {
        let button = ctx.broker.is_some().then(|| open_update_button(ctx));
        (Vec::new(), button)
    } else if logic::is_retryable(note) {
        (vec![retry], None)
    } else {
        (Vec::new(), None)
    }
}

fn count_line(ctx: &Ctx, n: usize) -> String {
    if n == 1 {
        ctx.t("1 important update is ready to install.")
    } else {
        ctx.t("{n} important updates are ready to install.")
            .replace("{n}", &n.to_string())
    }
}

fn count_installing(ctx: &Ctx, n: usize) -> String {
    if n == 1 {
        ctx.t("Installing 1 update")
    } else {
        ctx.t("Installing {n} updates")
            .replace("{n}", &n.to_string())
    }
}

// ---------------------------------------------------------------------------
// PC health tips
// ---------------------------------------------------------------------------

fn tips_block<'a>(state: &'a State, ctx: &'a Ctx) -> Vec<El<'a>> {
    let p = ctx.palette;
    let profiles: Vec<(TipProfile, String)> = TipProfile::ALL
        .iter()
        .map(|t| (*t, ctx.t(t.title())))
        .collect();
    let picker: El<'a> = container(widgets::segmented(p, &profiles, state.tip_choice, |t| {
        tools(Msg::TipChoice(t))
    }))
    .max_width(theme::CONTENT_MAX)
    .into();

    if let Tips::Running(profile) = &state.tips {
        return vec![widgets::row_item_below(
            p,
            Some(Icon::ShieldCheck),
            None,
            ctx.t("Looking at your PC…"),
            Some(format!(
                "{}  ·  {}",
                ctx.t(profile.title()),
                ctx.t("This can take about a minute.")
            )),
            iced::widget::space::horizontal().width(0),
            vec![progress::indeterminate(p, Tone::Brand)],
            None,
        )];
    }

    let done = matches!(state.tips, Tips::Done(_));
    let mut items = vec![secondary(
        p,
        if done {
            ctx.t("Check again")
        } else {
            ctx.t("Check")
        },
        Some(Msg::PickTips(state.tip_choice)),
    )];
    if done {
        items.push(more(
            p,
            vec![entry(
                Icon::Info,
                ctx.t("More details"),
                Msg::ToggleDetail(Detail::Tips),
            )],
        ));
    }
    let blurb = widgets::small(p, ctx.t(state.tip_choice.blurb()));
    let mut below = vec![picker, blurb];
    if let Tips::Done(report) = &state.tips {
        if state.detail_open(Detail::Tips) {
            below.push(raw_text(ctx, &tips_summary(ctx, report)));
        }
    }
    let mut out = vec![widgets::row_item_below(
        p,
        Some(Icon::ShieldCheck),
        None,
        ctx.t("What do you use this PC for?"),
        Some(ctx.t("This only looks at your PC. Nothing is changed.")),
        trailing(items),
        below,
        None,
    )];
    if let Tips::Done(report) = &state.tips {
        let scanning = matches!(state.scan, Run::Working);
        let (needs, fine): (Vec<&logic::Tip>, Vec<&logic::Tip>) = report
            .tips
            .iter()
            .partition(|tip| tip.state != TipState::Good);
        let rows = |tips: &[&logic::Tip]| -> El<'a> {
            column(tips.iter().map(|tip| tip_row(ctx, tip, scanning)))
                .spacing(theme::S1)
                .width(Length::Fill)
                .into()
        };
        if !needs.is_empty() {
            // Open until the person folds it.
            out.push(widgets::collapsible(
                p,
                ctx.t("Needs a look"),
                Some(match report.count(TipState::Look) {
                    0 => ctx.t("{n} items").replace("{n}", &needs.len().to_string()),
                    n => ctx.t("{n} worth a look").replace("{n}", &n.to_string()),
                }),
                !state.detail_open(Detail::TipsList),
                tools(Msg::ToggleDetail(Detail::TipsList)),
                rows(&needs),
            ));
        }
        if !fine.is_empty() {
            out.push(widgets::collapsible(
                p,
                ctx.t("All good"),
                Some(
                    ctx.t("{n} look good")
                        .replace("{n}", &fine.len().to_string()),
                ),
                state.detail_open(Detail::TipsGood),
                tools(Msg::ToggleDetail(Detail::TipsGood)),
                rows(&fine),
            ));
        }
    }
    out
}

fn tip_row<'a>(ctx: &Ctx, tip: &logic::Tip, scanning: bool) -> El<'a> {
    let p = ctx.palette;
    // A fix is only promised when the latest Protection check offers it. A
    // Not offered control says why on Protection; otherwise the manual steps.
    let fix = logic::tip_fix(tip, ctx.report.as_deref(), &ctx.catalog.available);
    let (advice, guide) = match fix {
        logic::TipFix::Offered(_) => (tip.fix_advice, None),
        logic::TipFix::NotOffered { control, reason } => (
            tip.advice,
            crate::guide::guide_not_offered(control, reason),
        ),
        logic::TipFix::Manual => (tip.advice, tip.guide),
    };
    let (tone, icon, words) = match tip.state {
        TipState::Good => (Tone::Good, Icon::CheckCircle, ctx.t("Looks good")),
        TipState::Look => (Tone::Warn, Icon::AlertTriangle, ctx.t(advice)),
        TipState::Unknown => (Tone::Neutral, Icon::Info, ctx.t("We couldn't check this")),
    };
    // One compact action: the fix review, the usual scan (after its own
    // confirmation), or the Windows page that helps. Nothing starts without
    // the person's say-so.
    let action: El<'a> = match (fix, tip.open) {
        _ if tip.state != TipState::Look => space::horizontal().width(0).into(),
        // Opens the same review sheet as Protection; nothing changes until
        // the person agrees there.
        (logic::TipFix::Offered(id), _) => widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Review fix"),
            Some(Icon::ShieldCheck),
            (!ctx.busy).then_some(Message::ReviewFixes(vec![id.to_owned()])),
        ),
        (logic::TipFix::NotOffered { .. }, _) => widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("See why"),
            None,
            Some(Message::Navigate(Page::Fixes)),
        ),
        // A restart that finishes updates, after its own confirmation.
        _ if tip.restart => secondary(p, ctx.t("Restart now"), Some(Msg::Ask(Sheet::Restart))),
        // The in-app scan stays reachable even when steps are shown below.
        _ if tip.scan => secondary(
            p,
            ctx.t("Scan now"),
            (!scanning).then_some(Msg::Ask(Sheet::Scan)),
        ),
        // A guide shows its own button under the steps.
        _ if guide.is_some() => space::horizontal().width(0).into(),
        (_, Some(open)) if ctx.broker.is_some() => {
            // The button is named after the page it opens.
            let label = crate::guide::Page::from_action(open)
                .map_or("Open", crate::guide::Page::button);
            widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t(label),
                Some(Icon::ExternalLink),
                Some(tools(Msg::OpenAction(open))),
            )
        }
        _ => space::horizontal().width(0).into(),
    };
    let head = widgets::row_item_tinted(
        p,
        Some(icon),
        Some(tone),
        ctx.t(tip.title),
        Some(words),
        action,
        None,
    );
    // The steps need no launcher; only the buttons that open pages do.
    let head = match (guide, tip.state) {
        (Some(g), TipState::Look) => column![
            head,
            crate::gui::pages::fixes::guide_block(
                ctx,
                g,
                widgets::explain::INDENT,
                ctx.broker.is_some()
            )
        ]
        .spacing(theme::S1)
        .into(),
        _ => head,
    };
    match &tip.explain {
        // Tips only report: the third line says what the person can do.
        Some(id) => {
            widgets::explain::with_disclosure(ctx, "tips", id, true, widgets::explain::INDENT, head)
        }
        None => head,
    }
}

// ---------------------------------------------------------------------------
// Passwords
// ---------------------------------------------------------------------------

fn password_region<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let Some(secret) = &state.password.secret else {
        return widgets::row_item_tinted(
            p,
            Some(Icon::Key),
            Some(Tone::Warn),
            ctx.t("We couldn't make a password"),
            Some(ctx.t("Please try again.")),
            secondary(p, ctx.t("Try again"), Some(Msg::NewPassword)),
            None,
        );
    };
    let shown = if state.password.shown {
        secret.reveal().to_owned()
    } else {
        "•".repeat(logic::PASSWORD_LENGTH)
    };
    let field = container(
        text(shown)
            .size(theme::H2)
            .font(Font::MONOSPACE)
            .color(p.text)
            .wrapping(text::Wrapping::Glyph),
    )
    .padding([theme::S2, theme::S3])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(p.surface_alt)),
        border: iced::Border {
            radius: theme::R.into(),
            ..iced::Border::default()
        },
        ..container::Style::default()
    });
    let eye = widgets::icon_button(
        p,
        ButtonKind::Ghost,
        if state.password.shown {
            Icon::EyeOff
        } else {
            Icon::Eye
        },
        Some(tools(Msg::TogglePassword)),
    );
    let copy: El<'a> = if state.password.copied {
        container(anim::check_draw(MARK, p.good, state.shot(Slot::Copy)))
            .center(theme::CONTROL)
            .into()
    } else {
        widgets::icon_button(
            p,
            ButtonKind::Ghost,
            Icon::Copy,
            Some(tools(Msg::CopyPassword)),
        )
    };
    let again = widgets::icon_button(
        p,
        ButtonKind::Ghost,
        Icon::Refresh,
        Some(tools(Msg::NewPassword)),
    );
    let caption = if state.password.copied {
        ctx.t("Copied. Paste it where you need it.")
    } else {
        ctx.t("Secblitz never saves your passwords.")
    };
    // A plain row like the rest of the page, the password under its title.
    widgets::row_item_below(
        p,
        Some(Icon::Key),
        None,
        ctx.t("Password generator"),
        Some(caption),
        space::horizontal().width(0),
        vec![row![field, eye, copy, again]
            .spacing(theme::S1)
            .align_y(Alignment::Center)
            .into()],
        None,
    )
}

fn manager_row<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let has_broker = ctx.broker.is_some();
    match &state.bitwarden {
        Run::Idle if state.bitwarden_present => widgets::row_item(
            p,
            Some(Icon::Lock),
            ctx.t("Bitwarden is installed"),
            Some(ctx.t("Find it in your Start menu.")),
            anim::check_draw(MARK, p.tone(Tone::Good), 1.0),
            None,
        ),
        // Show the "can't install" state upfront when the account check says so,
        // using the same copy and mark as the post-attempt refused state.
        Run::Idle if state.bitwarden_not_here => widgets::row_item(
            p,
            Some(Icon::Lock),
            ctx.t("Bitwarden can't be installed from this account"),
            Some(ctx.t("You can get it from bitwarden.com instead.")),
            anim::warn_draw(MARK, p.tone(Tone::Warn), 1.0),
            None,
        ),
        Run::Idle => widgets::row_item(
            p,
            Some(Icon::Lock),
            ctx.t("Password manager"),
            Some(if has_broker {
                ctx.t("Keep all your passwords safe in one place.")
            } else {
                reopen_hint(ctx)
            }),
            secondary(
                p,
                ctx.t("Install"),
                has_broker.then_some(Msg::Ask(Sheet::Bitwarden)),
            ),
            None,
        ),
        Run::Working => busy_row(
            state,
            p,
            Icon::Lock,
            ctx.t("Password manager"),
            ctx.t("Installing Bitwarden. This can take a few minutes."),
        ),
        Run::Done(Ok(())) => finished(
            state,
            ctx,
            Outcome {
                slot: Slot::Bitwarden,
                icon: Icon::Lock,
                tone: Tone::Good,
                title: ctx.t("Bitwarden is installed"),
                sub: Some(ctx.t("Find it in your Start menu.")),
                menu: vec![entry(Icon::Check, ctx.t("Done"), Msg::ClearBitwarden)],
                raw: None,
            },
        ),
        Run::Done(Err(_)) if state.bitwarden_why == Some(broker::Reply::Unavailable) => finished(
            state,
            ctx,
            Outcome {
                slot: Slot::Bitwarden,
                icon: Icon::Lock,
                tone: Tone::Warn,
                title: ctx.t("Bitwarden can't be installed from this account"),
                sub: Some(ctx.t("You can get it from bitwarden.com instead.")),
                menu: vec![entry(Icon::Check, ctx.t("Done"), Msg::ClearBitwarden)],
                raw: Some((Detail::Bitwarden, logic::WHY_BITWARDEN_UNAVAILABLE.to_owned())),
            },
        ),
        Run::Done(Err(_)) if state.bitwarden_why == Some(broker::Reply::Offline) => finished_with(
            state,
            ctx,
            Outcome {
                slot: Slot::Bitwarden,
                icon: Icon::Lock,
                tone: Tone::Warn,
                title: ctx.t("We couldn't install Bitwarden"),
                sub: Some(ctx.t("You're offline. Connect to the internet and try again.")),
                menu: vec![entry(Icon::X, ctx.t("Done"), Msg::ClearBitwarden)],
                raw: Some((Detail::Bitwarden, logic::WHY_BITWARDEN_OFFLINE.to_owned())),
            },
            Some((ctx.t("Retry"), Icon::Refresh, Msg::Ask(Sheet::Bitwarden))),
        ),
        Run::Done(Err(raw)) => finished(
            state,
            ctx,
            Outcome {
                slot: Slot::Bitwarden,
                icon: Icon::Lock,
                tone: Tone::Bad,
                title: ctx.t("We couldn't install Bitwarden"),
                sub: Some(ctx.t("Check your internet connection and try again. You can also get it from bitwarden.com.")),
                menu: vec![entry(
                    Icon::Refresh,
                    ctx.t("Try again"),
                    Msg::ClearBitwarden,
                )],
                raw: Some((Detail::Bitwarden, logic::bitwarden_why(raw).to_owned())),
            },
        ),
    }
}

// ---------------------------------------------------------------------------
// Windows settings
// ---------------------------------------------------------------------------

fn settings_group<'a>(ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let available = ctx.broker.is_some();
    let rows = Shortcut::ALL
        .iter()
        .map(|&shortcut| {
            let (icon, title, desc) = match shortcut {
                Shortcut::WindowsUpdate => (
                    Icon::Download,
                    "Windows Update",
                    "See and install Windows updates",
                ),
                Shortcut::WindowsSecurity => (
                    Icon::Shield,
                    "Windows Security",
                    "Virus protection and firewall",
                ),
                Shortcut::Encryption => (
                    Icon::Lock,
                    "Device encryption",
                    "Keep your files private if your PC is lost",
                ),
                Shortcut::SignIn => (
                    Icon::Key,
                    "Sign-in options",
                    "PIN, fingerprint and password",
                ),
            };
            widgets::row_item(
                p,
                Some(icon),
                ctx.t(title),
                Some(ctx.t(desc)),
                widgets::icon(Icon::ExternalLink, 16.0, p.text_muted),
                available.then_some(tools(Msg::Open(shortcut))),
            )
        })
        .collect();
    widgets::group(
        p,
        ctx.t("Windows settings"),
        (!available).then(|| reopen_hint(ctx)),
        None,
        rows,
    )
}

// ---------------------------------------------------------------------------
// Review sheets
// ---------------------------------------------------------------------------

fn sheet_panel<'a>(state: &'a State, ctx: &'a Ctx, sheet: Sheet) -> El<'a> {
    let p = ctx.palette;
    let (icon, title, lines, confirm_label): (Icon, String, Vec<String>, String) = match sheet {
        Sheet::Scan => (
            Icon::Bug,
            ctx.t("Scan for viruses?"),
            vec![
                ctx.t("Windows Security will check your PC for harmful software."),
                ctx.t("If it finds something, it removes it or puts it somewhere safe."),
                ctx.t("You can keep using your PC."),
            ],
            ctx.t("Start scan"),
        ),
        Sheet::DefenderUpdate => (
            Icon::Download,
            ctx.t("Update virus protection?"),
            vec![ctx.t("Windows will download the newest virus information from Microsoft.")],
            ctx.t("Update now"),
        ),
        Sheet::Repair(RepairKind::Check) => (
            Icon::Wrench,
            ctx.t("Check for problems?"),
            vec![
                ctx.t("Secblitz will look at Windows for damage. Nothing is changed."),
                ctx.t("This can take a few minutes. You can keep using your PC."),
            ],
            ctx.t("Start check"),
        ),
        Sheet::Repair(RepairKind::Repair) => (
            Icon::Wrench,
            ctx.t("Repair system files?"),
            vec![
                ctx.t("Secblitz will look for damaged Windows files and replace them with good copies."),
                ctx.t("This can take 15–45 minutes. You can keep using your PC."),
                ctx.t("Your own files and apps are not touched. This can't be undone automatically, but it only fixes files that belong to Windows."),
            ],
            ctx.t("Repair now"),
        ),
        Sheet::InstallUpdates => {
            let n = match &state.updates {
                Updates::Found(f) => f.updates.len(),
                _ => 0,
            };
            (
                Icon::Download,
                if n == 1 {
                    ctx.t("Install 1 update?")
                } else {
                    ctx.t("Install {n} updates?").replace("{n}", &n.to_string())
                },
                vec![
                    ctx.t("These updates come from Microsoft and protect your PC."),
                    ctx.t("This can take a while. You can keep using your PC, but save your work first because Windows may need to restart."),
                    ctx.t("Updates can't be undone automatically. By continuing you accept Microsoft's license terms for them."),
                ],
                ctx.t("Install now"),
            )
        }
        Sheet::Restart => (
            Icon::Restart,
            ctx.t("Restart your PC now?"),
            vec![
                ctx.t("Your PC restarts to finish installing updates."),
                ctx.t("Save your work first. Programs with unsaved work will ask you before they close."),
            ],
            ctx.t("Restart now"),
        ),
        Sheet::Bitwarden => (
            Icon::Lock,
            ctx.t("Install Bitwarden?"),
            vec![
                ctx.t("Bitwarden is a free password manager."),
                ctx.t("Secblitz will download it from its official source and install it for you."),
                ctx.t("You can remove it later in Windows Settings."),
            ],
            ctx.t("Install"),
        ),
    };
    let mut content = column![row![
        widgets::icon(icon, theme::ICON_ROW, p.text_muted),
        widgets::h2(p, title)
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center)]
    .spacing(theme::S3)
    .width(Length::Fill);
    for line in lines {
        content = content.push(widgets::body(p, line));
    }
    if sheet == Sheet::InstallUpdates {
        if let Updates::Found(found) = &state.updates {
            let mut list = column![].spacing(theme::S2);
            for u in found.updates.iter().take(5) {
                list = list.push(
                    row![
                        widgets::icon(Icon::Check, 14.0, p.good),
                        text(u.title.clone())
                            .size(theme::SMALL)
                            .color(p.text)
                            .width(Length::Fill)
                    ]
                    .spacing(theme::S2)
                    .align_y(Alignment::Center),
                );
            }
            if found.updates.len() > 5 {
                list = list.push(widgets::small(
                    p,
                    ctx.t("and {n} more")
                        .replace("{n}", &(found.updates.len() - 5).to_string()),
                ));
            }
            content = content.push(list);
            let mut raw = ctx.t("These updates come from Microsoft through Windows Update.");
            let mut seen: Vec<&str> = Vec::new();
            for u in &found.updates {
                if !u.license.is_empty() && !seen.contains(&u.license.as_str()) {
                    seen.push(&u.license);
                    raw.push('\n');
                    raw.push_str(&u.license);
                }
            }
            if raw.len() > 4000 {
                raw.truncate(raw.floor_char_boundary(4000));
            }
            content = content.push(details(state, ctx, Detail::Sheet, &raw));
        }
    }
    let footer = row![
        space::horizontal(),
        widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Cancel"),
            None,
            Some(tools(Msg::CloseSheet)),
        ),
        widgets::action(
            p,
            ButtonKind::Primary,
            confirm_label,
            None,
            Some(tools(Msg::Confirm)),
        ),
    ]
    .spacing(theme::S2);
    column![content, footer]
        .spacing(theme::S6)
        .width(Length::Fill)
        .into()
}
