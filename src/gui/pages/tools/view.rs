//! Drawing code for the Tools page. OWNER: tools agent.
use super::{tools, Detail, Msg, Repair, Run, Sheet, Shortcut, State, Tips, Updates};
use crate::app::tools::{
    self as logic, InstallResult, InstallStage, RepairKind, RepairResult, TipProfile, TipState,
};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{Ctx, Message};
use iced::widget::{button, column, container, progress_bar, row, text};
use iced::{Alignment, Background, Border, Element, Font, Length};

type El<'a> = Element<'a, Message>;

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
        section(
            p,
            ctx.t("Protection"),
            pair(scan_card(state, ctx), defender_card(state, ctx)),
        ),
        section(
            p,
            ctx.t("Repair & updates"),
            column![
                pair(repair_card(state, ctx), updates_card(state, ctx)),
                tips_card(state, ctx)
            ]
            .spacing(theme::GAP)
            .into(),
        ),
        section(
            p,
            ctx.t("Passwords"),
            pair(password_card(state, ctx), manager_card(state, ctx)),
        ),
        section(p, ctx.t("Windows settings"), settings_card(ctx)),
    ]
    .spacing(28)
    .width(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------

fn section<'a>(p: Palette, title: String, body: El<'a>) -> El<'a> {
    column![
        text(title)
            .size(theme::H2)
            .font(theme::SEMIBOLD)
            .color(p.text),
        body
    ]
    .spacing(12)
    .into()
}

/// Two equal cards side by side (cards stretch to the same height).
fn pair<'a>(a: El<'a>, b: El<'a>) -> El<'a> {
    row![a, b].spacing(theme::GAP).width(Length::Fill).into()
}

/// Icon, title and one-line description at the top of every card.
fn head<'a>(p: Palette, icon: Icon, title: String, description: String) -> El<'a> {
    row![
        widgets::icon_badge(p, icon, Tone::Neutral),
        column![
            text(title).size(16).font(theme::SEMIBOLD).color(p.text),
            widgets::small(p, description)
        ]
        .spacing(2)
        .width(Length::Fill)
    ]
    .spacing(12)
    .align_y(Alignment::Start)
    .into()
}

fn tile<'a>(p: Palette, head: El<'a>, body: El<'a>) -> El<'a> {
    widgets::card(p, column![head, body].spacing(14).width(Length::Fill))
        .height(Length::Fill)
        .into()
}

/// Buttons that wrap onto a second line in a narrow card.
fn buttons<'a>(items: Vec<El<'a>>) -> El<'a> {
    row(items).spacing(8).wrap().vertical_spacing(8).into()
}

fn secondary<'a>(p: Palette, label: String, icon: Option<Icon>, msg: Option<Msg>) -> El<'a> {
    widgets::action(p, ButtonKind::Secondary, label, icon, msg.map(tools))
}

fn ghost<'a>(p: Palette, label: String, msg: Msg) -> El<'a> {
    widgets::action(p, ButtonKind::Ghost, label, None, Some(tools(msg)))
}

/// A calm result box: tinted by meaning, with a short title and detail.
fn notice<'a>(p: Palette, tone: Tone, icon: Icon, title: String, detail: Option<String>) -> El<'a> {
    let mut words = column![text(title)
        .size(theme::BODY)
        .font(theme::SEMIBOLD)
        .color(p.text)]
    .spacing(2)
    .width(Length::Fill);
    if let Some(d) = detail {
        words = words.push(widgets::small(p, d));
    }
    let tint = p.tint(tone);
    container(
        row![widgets::icon(icon, 20.0, p.tone(tone)), words]
            .spacing(12)
            .align_y(Alignment::Start),
    )
    .padding(12)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(tint)),
        border: Border {
            radius: theme::RADIUS_SMALL.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn working<'a>(p: Palette, label: String) -> El<'a> {
    row![
        widgets::icon(Icon::Refresh, 16.0, p.text_muted),
        widgets::muted(p, label)
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn bar<'a>(p: Palette, fraction: f32) -> El<'a> {
    progress_bar(0.0..=1.0, fraction.clamp(0.0, 1.0))
        .girth(6)
        .style(move |_| progress_bar::Style {
            background: Background::Color(p.surface_alt),
            bar: Background::Color(p.brand),
            border: Border {
                radius: 3.0.into(),
                ..Border::default()
            },
        })
        .into()
}

fn elapsed_phrase(ctx: &Ctx, secs: u64) -> String {
    if secs < 60 {
        format!("{secs} {}", ctx.t("sec"))
    } else {
        format!("{} {}", secs / 60, ctx.t("min"))
    }
}

/// Collapsed "Technical details" for people who want the raw evidence.
fn technical<'a>(state: &'a State, ctx: &'a Ctx, which: Detail, details: &str) -> El<'a> {
    let p = ctx.palette;
    let open = state.detail_open(which);
    let toggle = button(
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
                .color(p.text_muted)
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([4, 0])
    .on_press(tools(Msg::ToggleDetail(which)))
    .style(|_, _| button::Style {
        background: None,
        ..button::Style::default()
    });
    let mut c = column![toggle].spacing(6);
    if open {
        let shown = if details.trim().is_empty() {
            ctx.t("No extra details.")
        } else {
            details.trim().to_owned()
        };
        c = c.push(
            container(
                text(shown)
                    .size(12)
                    .font(Font::MONOSPACE)
                    .color(p.text_muted),
            )
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
    c.into()
}

fn divider<'a>(p: Palette) -> El<'a> {
    let line = p.border;
    container(iced::widget::space::vertical())
        .width(Length::Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(Background::Color(line)),
            ..container::Style::default()
        })
        .into()
}

fn hint<'a>(p: Palette, s: String) -> El<'a> {
    widgets::small(p, s)
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
        hint(
            ctx.palette,
            ctx.t("Reopen Secblitz from its shortcut to use this."),
        )
    })
}

// ---------------------------------------------------------------------------
// Protection
// ---------------------------------------------------------------------------

fn scan_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let head = head(
        p,
        Icon::Bug,
        ctx.t("Scan for viruses"),
        ctx.t("Look for harmful software on your PC."),
    );
    let body: El<'a> = match &state.scan {
        Run::Idle => secondary(
            p,
            ctx.t("Scan now"),
            Some(Icon::Scan),
            Some(Msg::Ask(Sheet::Scan)),
        ),
        Run::Working => working(p, ctx.t("Starting the scan…")),
        Run::Done(Ok(())) => column![
            notice(
                p,
                Tone::Good,
                Icon::CheckCircle,
                ctx.t("Scan started"),
                Some(ctx.t("Windows Security will notify you if it finds anything.")),
            ),
            buttons(vec![
                open_security_button(ctx),
                ghost(p, ctx.t("Done"), Msg::ClearScan)
            ]),
        ]
        .spacing(10)
        .into(),
        Run::Done(Err(raw)) => column![
            notice(
                p,
                Tone::Warn,
                Icon::AlertTriangle,
                ctx.t("We couldn't start the scan"),
                Some(ctx.t("Open Windows Security and start a scan there.")),
            ),
            buttons(vec![
                open_security_button(ctx),
                ghost(p, ctx.t("Try again"), Msg::ClearScan)
            ]),
            technical(state, ctx, Detail::Scan, raw),
        ]
        .spacing(10)
        .into(),
    };
    let mut c = column![body].spacing(8);
    if let Some(h) = reopen_hint(ctx) {
        if matches!(state.scan, Run::Done(_)) {
            c = c.push(h);
        }
    }
    tile(p, head, c.into())
}

fn defender_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let head = head(
        p,
        Icon::Download,
        ctx.t("Update virus protection"),
        ctx.t("Get the newest virus information."),
    );
    let body: El<'a> = match &state.defender {
        Run::Idle => secondary(
            p,
            ctx.t("Update now"),
            Some(Icon::Refresh),
            Some(Msg::Ask(Sheet::DefenderUpdate)),
        ),
        Run::Working => working(p, ctx.t("Updating…")),
        Run::Done(Ok(())) => column![
            notice(
                p,
                Tone::Good,
                Icon::CheckCircle,
                ctx.t("Virus protection updated"),
                Some(ctx.t("Windows asked Microsoft for the newest virus information.")),
            ),
            buttons(vec![ghost(p, ctx.t("Done"), Msg::ClearDefender)]),
        ]
        .spacing(10)
        .into(),
        Run::Done(Err(raw)) => column![
            notice(
                p,
                Tone::Warn,
                Icon::AlertTriangle,
                ctx.t("We couldn't update right now"),
                Some(ctx.t("Check your internet connection and try again.")),
            ),
            buttons(vec![ghost(p, ctx.t("Try again"), Msg::ClearDefender)]),
            technical(state, ctx, Detail::Defender, raw),
        ]
        .spacing(10)
        .into(),
    };
    tile(p, head, body)
}

// ---------------------------------------------------------------------------
// Repair & updates
// ---------------------------------------------------------------------------

fn busy_hint<'a>(ctx: &Ctx) -> El<'a> {
    hint(
        ctx.palette,
        ctx.t("Another job is running. Please wait for it to finish."),
    )
}

fn repair_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let head = head(
        p,
        Icon::Wrench,
        ctx.t("Repair Windows"),
        ctx.t("Find and fix problems with Windows itself."),
    );
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
            let mut c = column![buttons(vec![
                start(RepairKind::Check, ctx.t("Check for problems"), Icon::Scan),
                start(
                    RepairKind::Repair,
                    ctx.t("Repair system files"),
                    Icon::Wrench
                ),
            ])]
            .spacing(8);
            if !free {
                c = c.push(busy_hint(ctx));
            }
            c.into()
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
            let mut c = column![text(title)
                .size(theme::BODY)
                .font(theme::SEMIBOLD)
                .color(p.text)]
            .spacing(8);
            let stopping = cancel.load(std::sync::atomic::Ordering::SeqCst);
            match progress {
                Some(pr) => {
                    c = c
                        .push(bar(p, (pr.step as f32 - 0.5) / pr.total as f32))
                        .push(widgets::muted(p, ctx.t(pr.label)))
                        .push(hint(
                            p,
                            format!(
                                "{} {} {} {}  ·  {} {}",
                                ctx.t("Step"),
                                pr.step,
                                ctx.t("of"),
                                pr.total,
                                ctx.t("Running for"),
                                elapsed_phrase(ctx, pr.elapsed)
                            ),
                        ));
                }
                None => {
                    c = c
                        .push(bar(p, 0.03))
                        .push(widgets::muted(p, ctx.t("Getting ready…")));
                }
            }
            c = c.push(hint(
                p,
                ctx.t("You can keep using your PC. Please don't turn it off."),
            ));
            if stopping {
                c = c.push(hint(p, ctx.t("Stopping after this step…")));
            } else {
                c = c.push(buttons(vec![ghost(
                    p,
                    ctx.t("Stop after this step"),
                    Msg::StopRepair,
                )]));
            }
            c.into()
        }
        Repair::Done {
            kind,
            result,
            note,
            technical: raw,
        } => {
            let (tone, icon) = match result {
                RepairResult::NoProblems | RepairResult::Repaired => {
                    (Tone::Good, Icon::CheckCircle)
                }
                RepairResult::CouldNotFinish => (Tone::Bad, Icon::ShieldAlert),
                RepairResult::ProblemsFound
                | RepairResult::NeedsRestart
                | RepairResult::Stopped => (Tone::Warn, Icon::AlertTriangle),
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
            column![
                notice(p, tone, icon, ctx.t(result.title()), Some(detail)),
                buttons(actions),
                technical(state, ctx, Detail::Repair, raw),
            ]
            .spacing(10)
            .into()
        }
    };
    tile(p, head, body)
}

fn updates_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let head = head(
        p,
        Icon::Download,
        ctx.t("Windows updates"),
        ctx.t("Install important security updates."),
    );
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
            let mut c = column![
                buttons(vec![secondary(
                    p,
                    ctx.t("Look for updates"),
                    Some(Icon::Refresh),
                    free.then_some(Msg::LookForUpdates)
                )]),
                hint(
                    p,
                    ctx.t("Secblitz asks Windows Update. Nothing is installed yet.")
                ),
            ]
            .spacing(8);
            if !free {
                c = c.push(busy_hint(ctx));
            }
            c.into()
        }
        Updates::Looking => working(p, ctx.t("Looking for updates…")),
        Updates::UpToDate => column![
            notice(
                p,
                Tone::Good,
                Icon::CheckCircle,
                ctx.t("Your PC is up to date"),
                Some(ctx.t("There are no important updates waiting.")),
            ),
            buttons(vec![look(ctx.t("Check again"))]),
        ]
        .spacing(10)
        .into(),
        Updates::Found(found) => {
            let n = found.updates.len();
            let size = logic::size_phrase(found.total_bytes());
            let detail = if size.is_empty() {
                ctx.t("Windows may need to restart afterwards.")
            } else {
                format!(
                    "{size}. {}",
                    ctx.t("Windows may need to restart afterwards.")
                )
            };
            column![
                notice(
                    p,
                    Tone::Warn,
                    Icon::Download,
                    count_line(ctx, n),
                    Some(detail)
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
            ]
            .spacing(10)
            .into()
        }
        Updates::Failed {
            technical: raw,
            note,
        } => column![
            notice(
                p,
                Tone::Warn,
                Icon::AlertTriangle,
                ctx.t("We couldn't check for updates"),
                Some(ctx.t(note)),
            ),
            buttons(vec![look(ctx.t("Try again"))]),
            technical(state, ctx, Detail::Updates, raw),
        ]
        .spacing(10)
        .into(),
        Updates::Installing {
            cancel,
            stage,
            elapsed,
            count,
        } => {
            let fraction = match stage {
                InstallStage::Preparing => 0.12,
                InstallStage::Installing => 0.55,
                InstallStage::Checking => 0.9,
            };
            let stopping = cancel.load(std::sync::atomic::Ordering::SeqCst);
            let mut c = column![
                text(count_installing(ctx, *count))
                    .size(theme::BODY)
                    .font(theme::SEMIBOLD)
                    .color(p.text),
                bar(p, fraction),
                widgets::muted(p, ctx.t(stage.label())),
                hint(
                    p,
                    format!("{} {}", ctx.t("Running for"), elapsed_phrase(ctx, *elapsed))
                ),
                hint(
                    p,
                    ctx.t("You can keep using your PC. Please don't turn it off.")
                ),
            ]
            .spacing(8);
            if stopping {
                c = c.push(hint(p, ctx.t("Stopping after this step…")));
            } else {
                c = c.push(buttons(vec![ghost(
                    p,
                    ctx.t("Stop after this step"),
                    Msg::StopInstall,
                )]));
            }
            c.into()
        }
        Updates::Done {
            result,
            note,
            technical: raw,
        } => {
            let (tone, icon) = match result {
                InstallResult::Installed => (Tone::Good, Icon::CheckCircle),
                InstallResult::CouldNotFinish => (Tone::Bad, Icon::ShieldAlert),
                _ => (Tone::Warn, Icon::AlertTriangle),
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
            column![
                notice(p, tone, icon, ctx.t(result.title()), Some(detail)),
                buttons(actions),
                technical(state, ctx, Detail::Updates, raw),
            ]
            .spacing(10)
            .into()
        }
    };
    tile(p, head, body)
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
    let head = head(
        p,
        Icon::ShieldCheck,
        ctx.t("PC health tips"),
        ctx.t("See how your PC is doing and what could be better."),
    );
    let body: El<'a> = match &state.tips {
        Tips::Pick => {
            let tiles: Vec<El<'a>> = TipProfile::ALL
                .iter()
                .map(|t| profile_tile(ctx, *t))
                .collect();
            let mut it = tiles.into_iter();
            let mut rows =
                column![widgets::muted(p, ctx.t("What do you use this PC for?"))].spacing(10);
            while let (Some(a), Some(b)) = (it.next(), it.next()) {
                rows = rows.push(row![a, b].spacing(10));
            }
            rows.push(hint(
                p,
                ctx.t("This only looks at your PC. Nothing is changed."),
            ))
            .into()
        }
        Tips::Running(profile) => column![
            working(p, ctx.t("Looking at your PC…")),
            hint(
                p,
                format!(
                    "{}  ·  {}",
                    ctx.t(profile.title()),
                    ctx.t("This can take about a minute.")
                )
            ),
        ]
        .spacing(8)
        .into(),
        Tips::Done(report) => {
            let good = report.count(TipState::Good);
            let look = report.count(TipState::Look);
            let mut summary = row![widgets::pill(
                p,
                format!("{good} {}", ctx.t("look good")),
                Tone::Good
            )]
            .spacing(8)
            .align_y(Alignment::Center);
            if look > 0 {
                summary = summary.push(widgets::pill(
                    p,
                    format!("{look} {}", ctx.t("worth a look")),
                    Tone::Warn,
                ));
            }
            let mut list = column![].spacing(0);
            for (i, tip) in report.tips.iter().enumerate() {
                list = list.push(tip_row(ctx, tip, i > 0));
            }
            column![
                row![
                    text(ctx.t(report.profile.title()))
                        .size(theme::BODY)
                        .font(theme::SEMIBOLD)
                        .color(p.text),
                    iced::widget::space::horizontal(),
                    summary
                ]
                .align_y(Alignment::Center),
                list,
                buttons(vec![secondary(
                    p,
                    ctx.t("Choose another"),
                    Some(Icon::Refresh),
                    Some(Msg::ChooseAnotherTips)
                )]),
                technical(state, ctx, Detail::Tips, &report.technical),
            ]
            .spacing(12)
            .into()
        }
    };
    widgets::card(p, column![head, body].spacing(14).width(Length::Fill)).into()
}

fn profile_tile<'a>(ctx: &Ctx, profile: TipProfile) -> El<'a> {
    let p = ctx.palette;
    let icon = match profile {
        TipProfile::Everyday => Icon::Home,
        TipProfile::Gaming => Icon::Gamepad,
        TipProfile::Work => Icon::Package,
        TipProfile::Extra => Icon::Lock,
    };
    button(
        row![
            widgets::icon(icon, 20.0, p.text_muted),
            column![
                text(ctx.t(profile.title()))
                    .size(theme::BODY)
                    .font(theme::SEMIBOLD)
                    .color(p.text),
                widgets::small(p, ctx.t(profile.blurb()))
            ]
            .spacing(2)
            .width(Length::Fill)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(14)
    .on_press(tools(Msg::PickTips(profile)))
    .style(move |_, status| button::Style {
        background: Some(Background::Color(if status == button::Status::Hovered {
            p.surface_alt
        } else {
            p.surface
        })),
        text_color: p.text,
        border: Border {
            radius: theme::RADIUS_SMALL.into(),
            width: 1.0,
            color: p.border,
        },
        ..button::Style::default()
    })
    .into()
}

fn tip_row<'a>(ctx: &Ctx, tip: &logic::Tip, divider_above: bool) -> El<'a> {
    let p = ctx.palette;
    let (tone, icon, words) = match tip.state {
        TipState::Good => (Tone::Good, Icon::CheckCircle, ctx.t("Looks good")),
        TipState::Look => (Tone::Warn, Icon::AlertTriangle, ctx.t(tip.advice)),
        TipState::Unknown => (Tone::Neutral, Icon::Info, ctx.t("We couldn't check this")),
    };
    let content = row![
        widgets::icon(icon, 18.0, p.tone(tone)),
        column![
            text(ctx.t(tip.title))
                .size(theme::BODY)
                .font(theme::MEDIUM)
                .color(p.text),
            widgets::small(p, words)
        ]
        .spacing(2)
        .width(Length::Fill)
    ]
    .spacing(12)
    .align_y(Alignment::Start);
    let row = container(content).padding([10, 0]).width(Length::Fill);
    if divider_above {
        column![divider(p), row].into()
    } else {
        row.into()
    }
}

// ---------------------------------------------------------------------------
// Passwords
// ---------------------------------------------------------------------------

fn password_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let head = head(
        p,
        Icon::Key,
        ctx.t("Password generator"),
        ctx.t("Make a strong password nobody can guess."),
    );
    let shown = match &state.password.secret {
        Some(secret) if state.password.shown => secret.reveal().to_owned(),
        Some(_) => "•".repeat(logic::PASSWORD_LENGTH),
        None => String::new(),
    };
    let body: El<'a> = if state.password.secret.is_none() {
        column![
            notice(
                p,
                Tone::Warn,
                Icon::AlertTriangle,
                ctx.t("We couldn't make a password"),
                Some(ctx.t("Please try again.")),
            ),
            buttons(vec![secondary(
                p,
                ctx.t("Try again"),
                Some(Icon::Refresh),
                Some(Msg::NewPassword)
            )]),
        ]
        .spacing(10)
        .into()
    } else {
        let box_ = container(
            text(shown)
                .size(20)
                .font(Font::MONOSPACE)
                .color(p.text)
                .wrapping(text::Wrapping::Glyph),
        )
        .padding([14, 14])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface_alt)),
            border: Border {
                radius: theme::RADIUS_SMALL.into(),
                width: 1.0,
                color: p.border,
            },
            ..container::Style::default()
        });
        column![
            box_,
            buttons(vec![
                secondary(p, ctx.t("Copy"), None, Some(Msg::CopyPassword)),
                secondary(
                    p,
                    ctx.t("Make another"),
                    Some(Icon::Refresh),
                    Some(Msg::NewPassword)
                ),
                ghost(
                    p,
                    if state.password.shown {
                        ctx.t("Hide")
                    } else {
                        ctx.t("Show")
                    },
                    Msg::TogglePassword
                ),
            ]),
            hint(p, ctx.t("Secblitz never saves your passwords.")),
        ]
        .spacing(10)
        .into()
    };
    tile(p, head, body)
}

fn manager_card<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let head = head(
        p,
        Icon::Lock,
        ctx.t("Password manager"),
        ctx.t("Keep all your passwords safe in one place."),
    );
    let body: El<'a> = match &state.bitwarden {
        Run::Idle => {
            let mut c = column![
                buttons(vec![secondary(
                    p,
                    ctx.t("Install Bitwarden"),
                    Some(Icon::Download),
                    ctx.broker.is_some().then_some(Msg::Ask(Sheet::Bitwarden))
                )]),
                hint(
                    p,
                    ctx.t("Bitwarden is free and trusted by millions of people.")
                ),
            ]
            .spacing(8);
            if let Some(h) = reopen_hint(ctx) {
                c = c.push(h);
            }
            c.into()
        }
        Run::Working => working(p, ctx.t("Installing Bitwarden. This can take a minute…")),
        Run::Done(Ok(())) => column![
            notice(
                p,
                Tone::Good,
                Icon::CheckCircle,
                ctx.t("Bitwarden is installed"),
                Some(ctx.t("Find it in your Start menu.")),
            ),
            buttons(vec![ghost(p, ctx.t("Done"), Msg::ClearBitwarden)]),
        ]
        .spacing(10)
        .into(),
        Run::Done(Err(raw)) => column![
            notice(
                p,
                Tone::Bad,
                Icon::ShieldAlert,
                ctx.t("We couldn't install Bitwarden"),
                Some(ctx.t("Check your internet connection and try again.")),
            ),
            buttons(vec![ghost(p, ctx.t("Try again"), Msg::ClearBitwarden)]),
            technical(state, ctx, Detail::Bitwarden, raw),
        ]
        .spacing(10)
        .into(),
    };
    tile(p, head, body)
}

// ---------------------------------------------------------------------------
// Windows settings
// ---------------------------------------------------------------------------

fn settings_card<'a>(ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let mut list = column![].spacing(0);
    for (i, shortcut) in Shortcut::ALL.iter().enumerate() {
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
        let item = row![
            widgets::icon_badge(p, icon, Tone::Neutral),
            column![
                text(ctx.t(title))
                    .size(theme::BODY)
                    .font(theme::MEDIUM)
                    .color(p.text),
                widgets::small(p, ctx.t(desc))
            ]
            .spacing(2)
            .width(Length::Fill),
            secondary(
                p,
                ctx.t("Open"),
                Some(Icon::ExternalLink),
                ctx.broker.is_some().then_some(Msg::Open(*shortcut))
            )
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        list = list.push(container(item).padding([10, 0]).width(Length::Fill));
        if i + 1 < Shortcut::ALL.len() {
            list = list.push(divider(p));
        }
    }
    let mut c = column![].spacing(8);
    if let Some(h) = reopen_hint(ctx) {
        c = c.push(h);
    }
    c = c.push(list);
    widgets::card(p, c).into()
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
                ctx.t("Secblitz will allow this one job to run now. Nothing else changes."),
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
                ctx.t("Secblitz will allow this one job to run now. Nothing else changes."),
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
    let mut c = column![row![
        widgets::icon_badge(p, icon, Tone::Neutral),
        text(title)
            .size(theme::H2)
            .font(theme::SEMIBOLD)
            .color(p.text)
    ]
    .spacing(12)
    .align_y(Alignment::Center),]
    .spacing(14)
    .width(Length::Fill);
    for line in lines {
        c = c.push(widgets::body(p, line));
    }
    if sheet == Sheet::InstallUpdates {
        if let Updates::Found(found) = &state.updates {
            let mut list = column![].spacing(6);
            for u in found.updates.iter().take(5) {
                list = list.push(
                    row![
                        widgets::icon(Icon::Check, 14.0, p.good),
                        text(u.title.clone())
                            .size(theme::SMALL)
                            .color(p.text)
                            .width(Length::Fill)
                    ]
                    .spacing(8)
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
            c = c.push(list);
            let mut details = found.technical.clone();
            let mut seen: Vec<&str> = Vec::new();
            for u in &found.updates {
                if !u.license.is_empty() && !seen.contains(&u.license.as_str()) {
                    seen.push(&u.license);
                    details.push('\n');
                    details.push_str(&u.license);
                }
            }
            if details.len() > 4000 {
                details.truncate(details.floor_char_boundary(4000));
            }
            c = c.push(technical(state, ctx, Detail::Sheet, &details));
        }
    }
    let actions = row![
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
            Some(tools(Msg::Confirm))
        ),
    ]
    .spacing(10);
    c.push(actions).into()
}
