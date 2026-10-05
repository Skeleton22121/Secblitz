//! Drawing code for the Tools page. OWNER: tools agent.
//!
//! Built only from the shared design-system widgets (`crate::gui::widgets`)
//! and theme tokens. `view` does no work beyond building widgets; everything
//! it shows was prepared in `update`.
use super::{
    repair_ratio, stage_ratio, tools, Detail, Msg, Repair, Run, Sheet, Shortcut, Slot, State,
    Tips, Updates,
};
use crate::app::tools::{
    self as logic, InstallResult, RepairKind, RepairResult, TipProfile, TipState,
};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, anim, ButtonKind};
use crate::gui::{Ctx, Message};
use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Element, Font, Length};

type El<'a> = Element<'a, Message>;

/// Every card body is at least this tall, so neighbouring cards line up.
const BODY_MIN: f32 = theme::ROW * 2.0;
/// Size of the spinners and result marks inside cards.
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
    let top = column![
        widgets::page_header(
            p,
            ctx.t("Tools"),
            Some(ctx.t("Handy ways to keep your PC safe and healthy.")),
        ),
        space::vertical().height(theme::S6),
        section(
            p,
            ctx.t("Virus protection"),
            pair(scan_card(state, ctx), defender_card(state, ctx)),
        ),
    ]
    .width(Length::Fill);
    column![
        top,
        section(
            p,
            ctx.t("Repair & updates"),
            column![
                pair(repair_card(state, ctx), updates_card(state, ctx)),
                tips_card(state, ctx)
            ]
            .spacing(theme::S4)
            .into(),
        ),
        section(
            p,
            ctx.t("Passwords"),
            pair(password_card(state, ctx), manager_card(state, ctx)),
        ),
        section(p, ctx.t("Windows settings"), settings_card(ctx)),
    ]
    .spacing(theme::S8)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------

fn section<'a>(p: Palette, title: String, body: El<'a>) -> El<'a> {
    column![widgets::section_label(p, title), body]
        .spacing(theme::S3)
        .width(Length::Fill)
        .into()
}

/// Two equal cards side by side.
fn pair<'a>(a: El<'a>, b: El<'a>) -> El<'a> {
    row![a, b].spacing(theme::S4).width(Length::Fill).into()
}

/// Gives a card body a minimum height without letting it stretch.
fn at_least<'a>(body: El<'a>) -> El<'a> {
    row![space::vertical().width(0).height(BODY_MIN), body]
        .width(Length::Fill)
        .into()
}

/// One card: round icon, title and one line of help, then the body.
fn tile<'a>(p: Palette, icon: Icon, title: String, description: String, body: El<'a>) -> El<'a> {
    let head = row![
        widgets::icon_badge(p, icon, Tone::Neutral),
        column![widgets::h2(p, title), widgets::small(p, description)]
            .spacing(theme::S1)
            .width(Length::Fill)
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center);
    widgets::card(
        p,
        column![head, at_least(body)]
            .spacing(theme::S3)
            .width(Length::Fill),
    )
    .into()
}

/// Buttons that wrap onto a second line in a narrow card.
fn buttons<'a>(items: Vec<El<'a>>) -> El<'a> {
    row(items)
        .spacing(theme::S2)
        .wrap()
        .vertical_spacing(theme::S2)
        .into()
}

fn secondary<'a>(p: Palette, label: String, icon: Option<Icon>, msg: Option<Msg>) -> El<'a> {
    widgets::action(p, ButtonKind::Secondary, label, icon, msg.map(tools))
}

fn ghost<'a>(p: Palette, label: String, msg: Msg) -> El<'a> {
    widgets::action(p, ButtonKind::Ghost, label, None, Some(tools(msg)))
}

/// Body-size text in the semibold weight (titles inside a card body).
fn strong<'a>(p: Palette, s: impl Into<String>) -> El<'a> {
    text(s.into())
        .size(theme::BODY)
        .font(theme::SEMIBOLD)
        .color(p.text)
        .into()
}

/// A finished job: the mark draws itself in (check, cross or warning), then
/// a short title and one line of explanation.
fn outcome<'a>(
    state: &State,
    p: Palette,
    slot: Slot,
    tone: Tone,
    title: String,
    detail: Option<String>,
) -> El<'a> {
    let t = state.shot(slot);
    let color = p.tone(tone);
    let mark: El<'a> = match tone {
        Tone::Good => anim::check_draw(MARK, color, t),
        Tone::Bad => anim::cross_draw(MARK, color, t),
        _ => anim::warn_draw(MARK, color, t),
    };
    let mut words = column![strong(p, title)]
        .spacing(theme::S1)
        .width(Length::Fill);
    if let Some(d) = detail {
        words = words.push(widgets::small(p, d));
    }
    row![mark, words]
        .spacing(theme::S3)
        .align_y(Alignment::Start)
        .into()
}

/// Something is running and we cannot say for how long.
fn working<'a>(state: &State, p: Palette, label: String) -> El<'a> {
    row![
        anim::spinner(MARK, p.text_muted, state.spin()),
        widgets::muted(p, label)
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center)
    .into()
}

fn elapsed_phrase(ctx: &Ctx, secs: u64) -> String {
    if secs < 60 {
        format!("{secs} {}", ctx.t("sec"))
    } else {
        format!("{} {}", secs / 60, ctx.t("min"))
    }
}

/// Collapsed "More details" for people who want the raw evidence.
fn details<'a>(state: &State, ctx: &Ctx, which: Detail, raw: &str) -> El<'a> {
    let p = ctx.palette;
    let shown = if raw.trim().is_empty() {
        ctx.t("No extra details.")
    } else {
        raw.trim().to_owned()
    };
    widgets::expander(
        p,
        ctx.t("More details"),
        state.detail_open(which),
        tools(Msg::ToggleDetail(which)),
        text(shown)
            .size(theme::SMALL)
            .font(Font::MONOSPACE)
            .color(p.text_muted),
    )
}

fn open_security_button<'a>(ctx: &Ctx) -> El<'a> {
    secondary(
        ctx.palette,
        ctx.t("Open Windows Security"),
        Some(Icon::ExternalLink),
        ctx.broker.is_some().then_some(Msg::OpenSecurity),
    )
}

fn reopen_hint<'a>(ctx: &Ctx) -> Option<El<'a>> {
    ctx.broker.is_none().then(|| {
        widgets::inline_notice(
            ctx.palette,
            Tone::Neutral,
            ctx.t("Reopen Secblitz from its shortcut to use this."),
        )
    })
}

/// Stack body pieces with the standard gap.
fn stack<'a>(items: Vec<El<'a>>) -> El<'a> {
    column(items).spacing(theme::S3).width(Length::Fill).into()
}

// ---------------------------------------------------------------------------
// Virus protection
// ---------------------------------------------------------------------------

fn scan_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let body: El<'a> = match &state.scan {
        Run::Idle => secondary(
            p,
            ctx.t("Scan now"),
            Some(Icon::Scan),
            Some(Msg::Ask(Sheet::Scan)),
        ),
        Run::Working => working(state, p, ctx.t("Starting the scan…")),
        Run::Done(Ok(())) => {
            let mut items = vec![
                outcome(
                    state,
                    p,
                    Slot::Scan,
                    Tone::Good,
                    ctx.t("Scan started"),
                    Some(ctx.t("Windows Security will notify you if it finds anything.")),
                ),
                buttons(vec![
                    open_security_button(ctx),
                    ghost(p, ctx.t("Done"), Msg::ClearScan),
                ]),
            ];
            items.extend(reopen_hint(ctx));
            stack(items)
        }
        Run::Done(Err(raw)) => stack(vec![
            outcome(
                state,
                p,
                Slot::Scan,
                Tone::Warn,
                ctx.t("We couldn't start the scan"),
                Some(ctx.t("Open Windows Security and start a scan there.")),
            ),
            buttons(vec![
                open_security_button(ctx),
                ghost(p, ctx.t("Try again"), Msg::ClearScan),
            ]),
            details(state, ctx, Detail::Scan, raw),
        ]),
    };
    tile(
        p,
        Icon::Bug,
        ctx.t("Scan for viruses"),
        ctx.t("Look for harmful software on your PC."),
        body,
    )
}

fn defender_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let body: El<'a> = match &state.defender {
        Run::Idle => secondary(
            p,
            ctx.t("Update now"),
            Some(Icon::Refresh),
            Some(Msg::Ask(Sheet::DefenderUpdate)),
        ),
        Run::Working => working(state, p, ctx.t("Updating…")),
        Run::Done(Ok(())) => stack(vec![
            outcome(
                state,
                p,
                Slot::Defender,
                Tone::Good,
                ctx.t("Virus protection updated"),
                Some(ctx.t("Windows asked Microsoft for the newest virus information.")),
            ),
            buttons(vec![ghost(p, ctx.t("Done"), Msg::ClearDefender)]),
        ]),
        Run::Done(Err(raw)) => stack(vec![
            outcome(
                state,
                p,
                Slot::Defender,
                Tone::Warn,
                ctx.t("We couldn't update right now"),
                Some(ctx.t("Check your internet connection and try again.")),
            ),
            buttons(vec![ghost(p, ctx.t("Try again"), Msg::ClearDefender)]),
            details(state, ctx, Detail::Defender, raw),
        ]),
    };
    tile(
        p,
        Icon::Download,
        ctx.t("Update virus protection"),
        ctx.t("Get the newest virus information."),
        body,
    )
}

// ---------------------------------------------------------------------------
// Repair & updates
// ---------------------------------------------------------------------------

fn busy_hint<'a>(ctx: &Ctx) -> El<'a> {
    widgets::inline_notice(
        ctx.palette,
        Tone::Neutral,
        ctx.t("Another job is running. Please wait for it to finish."),
    )
}

/// Title with a spinner and a smooth progress bar.
fn job_header<'a>(state: &State, p: Palette, title: String, ratio: f32) -> El<'a> {
    column![
        row![
            anim::spinner(MARK, p.text_muted, state.spin()),
            strong(p, title)
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center),
        widgets::bar(p, state.bar_value(ratio), Tone::Brand),
    ]
    .spacing(theme::S3)
    .into()
}

fn repair_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let free = !ctx.busy;
    let start = |kind: RepairKind, label: String, icon: Icon| {
        secondary(
            p,
            label,
            Some(icon),
            free.then_some(Msg::Ask(Sheet::Repair(kind))),
        )
    };
    let body: El<'a> = match &state.repair {
        Repair::Idle => {
            let mut items = vec![buttons(vec![
                start(RepairKind::Check, ctx.t("Check for problems"), Icon::Scan),
                start(
                    RepairKind::Repair,
                    ctx.t("Repair system files"),
                    Icon::Wrench,
                ),
            ])];
            if !free {
                items.push(busy_hint(ctx));
            }
            stack(items)
        }
        Repair::Working {
            kind,
            cancel,
            progress,
        } => {
            let title = match kind {
                RepairKind::Check => ctx.t("Checking for problems"),
                RepairKind::Repair => ctx.t("Repairing Windows"),
            };
            let stopping = cancel.load(std::sync::atomic::Ordering::SeqCst);
            let (ratio, label, step) = match progress {
                Some(pr) => (
                    repair_ratio(pr),
                    ctx.t(pr.label),
                    format!(
                        "{} {} {} {}  ·  {} {}",
                        ctx.t("Step"),
                        pr.step,
                        ctx.t("of"),
                        pr.total,
                        ctx.t("Running for"),
                        elapsed_phrase(ctx, pr.elapsed)
                    ),
                ),
                None => (0.03, ctx.t("Getting ready…"), String::new()),
            };
            let mut items = vec![job_header(state, p, title, ratio), widgets::muted(p, label)];
            if !step.is_empty() {
                items.push(widgets::small(p, step));
            }
            items.push(widgets::small(
                p,
                ctx.t("You can keep using your PC. Please don't turn it off."),
            ));
            items.push(if stopping {
                widgets::small(p, ctx.t("Stopping after this step…"))
            } else {
                buttons(vec![ghost(
                    p,
                    ctx.t("Stop after this step"),
                    Msg::StopRepair,
                )])
            });
            column(items).spacing(theme::S2).width(Length::Fill).into()
        }
        Repair::Done {
            kind,
            result,
            note,
            technical: raw,
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
            let mut actions = Vec::new();
            if *result == RepairResult::ProblemsFound && *kind == RepairKind::Check {
                actions.push(start(
                    RepairKind::Repair,
                    ctx.t("Repair system files"),
                    Icon::Wrench,
                ));
            }
            actions.push(ghost(p, ctx.t("Done"), Msg::ClearRepair));
            stack(vec![
                outcome(
                    state,
                    p,
                    Slot::Repair,
                    tone,
                    ctx.t(result.title()),
                    Some(detail),
                ),
                buttons(actions),
                details(state, ctx, Detail::Repair, raw),
            ])
        }
    };
    tile(
        p,
        Icon::Wrench,
        ctx.t("Repair Windows"),
        ctx.t("Find and fix problems with Windows itself."),
        body,
    )
}

fn updates_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let free = !ctx.busy;
    let look = |label: String| {
        secondary(
            p,
            label,
            Some(Icon::Refresh),
            (free || !matches!(state.updates, Updates::Idle)).then_some(Msg::LookForUpdates),
        )
    };
    let body: El<'a> = match &state.updates {
        Updates::Idle => {
            let mut items = vec![
                buttons(vec![secondary(
                    p,
                    ctx.t("Look for updates"),
                    Some(Icon::Refresh),
                    free.then_some(Msg::LookForUpdates),
                )]),
                widgets::small(
                    p,
                    ctx.t("Secblitz asks Windows Update. Nothing is installed yet."),
                ),
            ];
            if !free {
                items.push(busy_hint(ctx));
            }
            stack(items)
        }
        Updates::Looking => working(state, p, ctx.t("Looking for updates…")),
        Updates::UpToDate => stack(vec![
            outcome(
                state,
                p,
                Slot::Updates,
                Tone::Good,
                ctx.t("Your PC is up to date"),
                Some(ctx.t("There are no important updates waiting.")),
            ),
            buttons(vec![look(ctx.t("Check again"))]),
        ]),
        Updates::Found(found) => {
            let size = logic::size_phrase(found.total_bytes());
            let restart = ctx.t("Windows may need to restart afterwards.");
            let tail = if size.is_empty() {
                restart
            } else {
                format!("{size}. {restart}")
            };
            stack(vec![
                widgets::inline_notice(
                    p,
                    Tone::Warn,
                    format!("{} {tail}", count_line(ctx, found.updates.len())),
                ),
                buttons(vec![
                    widgets::action(
                        p,
                        ButtonKind::Primary,
                        ctx.t("Install updates"),
                        Some(Icon::Download),
                        free.then(|| tools(Msg::Ask(Sheet::InstallUpdates))),
                    ),
                    look(ctx.t("Check again")),
                ]),
            ])
        }
        Updates::Failed {
            technical: raw,
            note,
        } => stack(vec![
            outcome(
                state,
                p,
                Slot::Updates,
                Tone::Warn,
                ctx.t("We couldn't check for updates"),
                Some(ctx.t(note)),
            ),
            buttons(vec![look(ctx.t("Try again"))]),
            details(state, ctx, Detail::Updates, raw),
        ]),
        Updates::Installing {
            cancel,
            stage,
            elapsed,
            count,
        } => {
            let stopping = cancel.load(std::sync::atomic::Ordering::SeqCst);
            let mut items = vec![
                job_header(
                    state,
                    p,
                    count_installing(ctx, *count),
                    stage_ratio(*stage),
                ),
                widgets::muted(p, ctx.t(stage.label())),
                widgets::small(
                    p,
                    format!("{} {}", ctx.t("Running for"), elapsed_phrase(ctx, *elapsed)),
                ),
                widgets::small(
                    p,
                    ctx.t("You can keep using your PC. Please don't turn it off."),
                ),
            ];
            items.push(if stopping {
                widgets::small(p, ctx.t("Stopping after this step…"))
            } else {
                buttons(vec![ghost(
                    p,
                    ctx.t("Stop after this step"),
                    Msg::StopInstall,
                )])
            });
            column(items).spacing(theme::S2).width(Length::Fill).into()
        }
        Updates::Done {
            result,
            note,
            technical: raw,
        } => {
            let tone = match result {
                InstallResult::Installed => Tone::Good,
                InstallResult::CouldNotFinish => Tone::Bad,
                _ => Tone::Warn,
            };
            let detail = match (result, note) {
                (InstallResult::CouldNotFinish, Some(n)) => ctx.t(n),
                _ => ctx.t(result.detail()),
            };
            let mut actions = vec![ghost(p, ctx.t("Done"), Msg::ClearUpdates)];
            if *result == InstallResult::NotConfirmed && ctx.broker.is_some() {
                actions.insert(
                    0,
                    secondary(
                        p,
                        ctx.t("Open Windows Update"),
                        Some(Icon::ExternalLink),
                        Some(Msg::Open(Shortcut::WindowsUpdate)),
                    ),
                );
            }
            stack(vec![
                outcome(
                    state,
                    p,
                    Slot::Updates,
                    tone,
                    ctx.t(result.title()),
                    Some(detail),
                ),
                buttons(actions),
                details(state, ctx, Detail::Updates, raw),
            ])
        }
    };
    tile(
        p,
        Icon::Download,
        ctx.t("Windows updates"),
        ctx.t("Install important security updates."),
        body,
    )
}

fn count_line(ctx: &Ctx, n: usize) -> String {
    if n == 1 {
        ctx.t("1 important update is ready to install.")
    } else {
        format!("{n} {}", ctx.t("important updates are ready to install."))
    }
}

fn count_installing(ctx: &Ctx, n: usize) -> String {
    if n == 1 {
        ctx.t("Installing 1 update")
    } else {
        format!("{} {n} {}", ctx.t("Installing"), ctx.t("updates"))
    }
}

fn tips_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let body: El<'a> = match &state.tips {
        Tips::Pick => {
            let mut grid = column![widgets::muted(p, ctx.t("What do you use this PC for?"))]
                .spacing(theme::S2)
                .width(Length::Fill);
            for chunk in TipProfile::ALL.chunks(2) {
                grid = grid.push(
                    row(chunk.iter().map(|t| profile_tile(ctx, *t)))
                        .spacing(theme::S2)
                        .width(Length::Fill),
                );
            }
            grid.push(widgets::small(
                p,
                ctx.t("This only looks at your PC. Nothing is changed."),
            ))
            .into()
        }
        Tips::Running(profile) => stack(vec![
            working(state, p, ctx.t("Looking at your PC…")),
            widgets::small(
                p,
                format!(
                    "{}  ·  {}",
                    ctx.t(profile.title()),
                    ctx.t("This can take about a minute.")
                ),
            ),
        ]),
        Tips::Done(report) => {
            let good = report.count(TipState::Good);
            let look = report.count(TipState::Look);
            let mut summary = row![widgets::pill(
                p,
                format!("{good} {}", ctx.t("look good")),
                Tone::Good
            )]
            .spacing(theme::S2)
            .align_y(Alignment::Center);
            if look > 0 {
                summary = summary.push(widgets::pill(
                    p,
                    format!("{look} {}", ctx.t("worth a look")),
                    Tone::Warn,
                ));
            }
            let list = column(report.tips.iter().map(|tip| tip_row(ctx, tip)))
                .spacing(theme::S1)
                .width(Length::Fill);
            stack(vec![
                row![
                    strong(p, ctx.t(report.profile.title())),
                    space::horizontal(),
                    summary
                ]
                .align_y(Alignment::Center)
                .into(),
                list.into(),
                buttons(vec![secondary(
                    p,
                    ctx.t("Choose another"),
                    Some(Icon::Refresh),
                    Some(Msg::ChooseAnotherTips),
                )]),
                details(state, ctx, Detail::Tips, &report.technical),
            ])
        }
    };
    tile(
        p,
        Icon::ShieldCheck,
        ctx.t("PC health tips"),
        ctx.t("See how your PC is doing and what could be better."),
        body,
    )
}

fn profile_tile<'a>(ctx: &Ctx, profile: TipProfile) -> El<'a> {
    let p = ctx.palette;
    let icon = match profile {
        TipProfile::Everyday => Icon::Home,
        TipProfile::Gaming => Icon::Gamepad,
        TipProfile::Work => Icon::Package,
        TipProfile::Extra => Icon::Lock,
    };
    widgets::list_button(
        p,
        row![
            widgets::icon_badge(p, icon, Tone::Neutral),
            column![
                widgets::body(p, ctx.t(profile.title())),
                widgets::small(p, ctx.t(profile.blurb()))
            ]
            .spacing(theme::S1)
            .width(Length::Fill)
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center),
        tools(Msg::PickTips(profile)),
    )
}

fn tip_row<'a>(ctx: &Ctx, tip: &logic::Tip) -> El<'a> {
    let p = ctx.palette;
    let (tone, icon, words) = match tip.state {
        TipState::Good => (Tone::Good, Icon::CheckCircle, ctx.t("Looks good")),
        TipState::Look => (Tone::Warn, Icon::AlertTriangle, ctx.t(tip.advice)),
        TipState::Unknown => (Tone::Neutral, Icon::Info, ctx.t("We couldn't check this")),
    };
    container(
        row![
            widgets::icon_badge(p, icon, tone),
            column![
                widgets::body(p, ctx.t(tip.title)),
                widgets::small(p, words)
            ]
            .spacing(theme::S1)
            .width(Length::Fill)
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center),
    )
    .padding([theme::S2, theme::S4])
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Passwords
// ---------------------------------------------------------------------------

fn password_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let body: El<'a> = if state.password.secret.is_none() {
        stack(vec![
            widgets::inline_notice(
                p,
                Tone::Warn,
                format!(
                    "{}. {}",
                    ctx.t("We couldn't make a password"),
                    ctx.t("Please try again.")
                ),
            ),
            buttons(vec![secondary(
                p,
                ctx.t("Try again"),
                Some(Icon::Refresh),
                Some(Msg::NewPassword),
            )]),
        ])
    } else {
        let shown = match &state.password.secret {
            Some(secret) if state.password.shown => secret.reveal().to_owned(),
            Some(_) => "•".repeat(logic::PASSWORD_LENGTH),
            None => String::new(),
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
                width: 1.0,
                color: p.border,
            },
            ..container::Style::default()
        });
        let eye = widgets::icon_button(
            p,
            ButtonKind::Secondary,
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
                ButtonKind::Secondary,
                Icon::Copy,
                Some(tools(Msg::CopyPassword)),
            )
        };
        stack(vec![
            row![field, eye, copy]
                .spacing(theme::S2)
                .align_y(Alignment::Center)
                .into(),
            buttons(vec![secondary(
                p,
                ctx.t("Make another"),
                Some(Icon::Refresh),
                Some(Msg::NewPassword),
            )]),
            widgets::small(
                p,
                if state.password.copied {
                    ctx.t("Copied. Paste it where you need it.")
                } else {
                    ctx.t("Secblitz never saves your passwords.")
                },
            ),
        ])
    };
    tile(
        p,
        Icon::Key,
        ctx.t("Password generator"),
        ctx.t("Make a strong password nobody can guess."),
        body,
    )
}

fn manager_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let body: El<'a> = match &state.bitwarden {
        Run::Idle => {
            let mut items = vec![
                buttons(vec![secondary(
                    p,
                    ctx.t("Install Bitwarden"),
                    Some(Icon::Download),
                    ctx.broker.is_some().then_some(Msg::Ask(Sheet::Bitwarden)),
                )]),
                widgets::small(
                    p,
                    ctx.t("Bitwarden is free and trusted by millions of people."),
                ),
            ];
            items.extend(reopen_hint(ctx));
            stack(items)
        }
        Run::Working => working(
            state,
            p,
            ctx.t("Installing Bitwarden. This can take a minute…"),
        ),
        Run::Done(Ok(())) => stack(vec![
            outcome(
                state,
                p,
                Slot::Bitwarden,
                Tone::Good,
                ctx.t("Bitwarden is installed"),
                Some(ctx.t("Find it in your Start menu.")),
            ),
            buttons(vec![ghost(p, ctx.t("Done"), Msg::ClearBitwarden)]),
        ]),
        Run::Done(Err(raw)) => stack(vec![
            outcome(
                state,
                p,
                Slot::Bitwarden,
                Tone::Bad,
                ctx.t("We couldn't install Bitwarden"),
                Some(ctx.t("Check your internet connection and try again.")),
            ),
            buttons(vec![ghost(p, ctx.t("Try again"), Msg::ClearBitwarden)]),
            details(state, ctx, Detail::Bitwarden, raw),
        ]),
    };
    tile(
        p,
        Icon::Lock,
        ctx.t("Password manager"),
        ctx.t("Keep all your passwords safe in one place."),
        body,
    )
}

// ---------------------------------------------------------------------------
// Windows settings
// ---------------------------------------------------------------------------

fn settings_card<'a>(ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let available = ctx.broker.is_some();
    let mut list = column![].spacing(theme::S1).width(Length::Fill);
    for shortcut in Shortcut::ALL {
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
        let content = row![
            widgets::icon_badge(p, icon, Tone::Neutral),
            column![
                widgets::body(p, ctx.t(title)),
                widgets::small(p, ctx.t(desc))
            ]
            .spacing(theme::S1)
            .width(Length::Fill),
            widgets::icon(Icon::ExternalLink, 16.0, p.text_muted),
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center);
        list = list.push(if available {
            widgets::list_button(p, content, tools(Msg::Open(shortcut)))
        } else {
            container(content)
                .padding([theme::S3, theme::S4])
                .width(Length::Fill)
                .into()
        });
    }
    let mut c = column![].spacing(theme::S3).width(Length::Fill);
    if let Some(h) = reopen_hint(ctx) {
        c = c.push(h);
    }
    widgets::card(p, c.push(list)).into()
}

// ---------------------------------------------------------------------------
// Review sheets
// ---------------------------------------------------------------------------

fn sheet_panel<'a>(state: &'a State, ctx: &'a Ctx, sheet: Sheet) -> El<'a> {
    let p = ctx.palette;
    let (icon, title, lines, confirm_label): (Icon, String, Vec<String>, String) = match sheet {
        Sheet::Scan => (
            Icon::Scan,
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
            Icon::Scan,
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
                    format!("{} {n} {}", ctx.t("Install"), ctx.t("updates?"))
                },
                vec![
                    ctx.t("These updates come from Microsoft and protect your PC."),
                    ctx.t("This can take a while. You can keep using your PC, but save your work first because Windows may need to restart."),
                    ctx.t("Updates can't be undone automatically. By continuing you accept Microsoft's license terms for them."),
                ],
                ctx.t("Install now"),
            )
        }
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
        widgets::icon_badge(p, icon, Tone::Neutral),
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
                    format!(
                        "{} {} {}",
                        ctx.t("and"),
                        found.updates.len() - 5,
                        ctx.t("more")
                    ),
                ));
            }
            content = content.push(list);
            let mut raw = found.technical.clone();
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
