//! Protection page: every check grouped, attention rows selectable.
//! OWNER: fixes agent.
use crate::advice::{self, Group, NextStep};
use crate::app::flow;
use crate::app::score::{self, Class};
use crate::broker::{Reply, Request};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{Ctx, Message};
use iced::widget::{button, checkbox, column, container, row, space, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};
use secblitz::engine::Report;
use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct State {
    /// `Ctx::checked_at` the selection below belongs to.
    synced_at: Option<u64>,
    selected: HashSet<String>,
    expanded: HashSet<String>,
    technical: HashSet<String>,
    show_protected: bool,
    show_error: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Toggle(String),
    SelectAll,
    SelectNone,
    Expand(String),
    Technical(String),
    ShowProtected,
    ErrorDetails,
    /// Open a Windows Settings page through the launcher.
    Open(Request),
}

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
    }
}

/// Selected ids in list order, limited to what is currently fixable.
fn selection(state: &State, ctx: &Ctx, all: &[String]) -> Vec<String> {
    if state.synced_at == ctx.checked_at && ctx.checked_at.is_some() {
        all.iter().filter(|id| state.selected.contains(*id)).cloned().collect()
    } else {
        all.to_vec()
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
        Msg::Technical(id) => flip(&mut state.technical, id),
        Msg::ShowProtected => state.show_protected = !state.show_protected,
        Msg::ErrorDetails => state.show_error = !state.show_error,
        Msg::Open(request) => return open_settings(ctx, request),
    }
    Task::none()
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

/// Raw evidence made safe to display: control characters removed, bounded.
pub fn sanitize(s: &str) -> String {
    let clean: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() > 300 {
        let mut cut: String = clean.chars().take(300).collect();
        cut.push('…');
        cut
    } else {
        clean
    }
}

pub fn divider<'a>(p: Palette) -> Element<'a, Message> {
    container(space::horizontal())
        .width(Length::Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.border)),
            ..container::Style::default()
        })
        .into()
}

/// Small text button with a trailing chevron-like label (expanders).
pub fn link<'a>(p: Palette, label: String, open: bool, on_press: Message) -> Element<'a, Message> {
    let chevron = if open { Icon::ChevronDown } else { Icon::ChevronRight };
    button(
        row![
            widgets::icon(chevron, 14.0, p.text_muted),
            text(label).size(theme::SMALL).font(theme::MEDIUM).color(p.text_muted)
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([4, 6])
    .on_press(on_press)
    .style(move |_, status| button::Style {
        background: Some(Background::Color(if status == button::Status::Hovered {
            p.surface_alt
        } else {
            Color::TRANSPARENT
        })),
        text_color: p.text_muted,
        border: Border { radius: theme::RADIUS_SMALL.into(), ..Border::default() },
        ..button::Style::default()
    })
    .into()
}

fn section_header<'a>(
    p: Palette,
    title: String,
    count: usize,
    tone: Tone,
    subtitle: Option<String>,
) -> Element<'a, Message> {
    let mut c = column![row![widgets::h2(p, title), widgets::pill(p, count.to_string(), tone)]
        .spacing(10)
        .align_y(Alignment::Center)]
    .spacing(4);
    if let Some(s) = subtitle {
        c = c.push(widgets::muted(p, s));
    }
    c.into()
}

/// One entry of the "needs a look / can't check / managed" sections.
struct Other {
    name: String,
    next: &'static str,
    status: &'static str,
    step: NextStep,
    group: Group,
    tone: Tone,
    bucket: Bucket,
    technical: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Look,
    Managed,
    Unavailable,
}

fn others(ctx: &Ctx, report: &Report, fixable: &[String]) -> Vec<Other> {
    let mut list = Vec::new();
    for r in &report.results {
        if fixable.contains(&r.id) {
            continue;
        }
        let class = score::classify(r);
        if class == Class::Protected {
            continue;
        }
        let a = advice::for_outcome(r);
        let (bucket, tone) = if a.status == "Managed elsewhere" {
            (Bucket::Managed, Tone::Neutral)
        } else if class == Class::Unknown {
            (Bucket::Unavailable, Tone::Neutral)
        } else {
            (Bucket::Look, Tone::Warn)
        };
        list.push(Other {
            name: ctx.lang.control(&r.id),
            next: a.next,
            status: a.status,
            step: a.step,
            group: a.group,
            tone,
            bucket,
            technical: format!("{} · {} · {}", r.id, r.status, sanitize(&r.detail)),
        });
    }
    for f in &report.findings {
        let a = advice::for_finding(&f.title, &f.status, &f.detail);
        if a.group == Group::Protected {
            continue;
        }
        let (bucket, tone) = if a.status == "Managed elsewhere" || a.step == NextStep::ReviewWithAdministrator {
            (Bucket::Managed, Tone::Neutral)
        } else if matches!(a.step, NextStep::CheckAgain) {
            (Bucket::Unavailable, Tone::Neutral)
        } else {
            (Bucket::Look, if a.group == Group::Information { Tone::Neutral } else { Tone::Warn })
        };
        list.push(Other {
            name: ctx.t(a.label),
            next: a.next,
            status: a.status,
            step: a.step,
            group: a.group,
            tone,
            bucket,
            technical: format!("{} · {} · {}", f.title, f.status, sanitize(&f.detail)),
        });
    }
    list
}

fn settings_button<'a>(ctx: &Ctx, step: NextStep) -> Option<Element<'a, Message>> {
    let (label, request) = match step {
        NextStep::OpenWindowsSecurity => ("Open Windows Security", Request::OpenWindowsSecurity),
        NextStep::OpenWindowsUpdate => ("Open Windows Update", Request::OpenWindowsUpdate),
        NextStep::OpenEncryption => ("Open encryption settings", Request::OpenEncryption),
        NextStep::OpenAccounts => ("Open sign-in settings", Request::OpenSignIn),
        _ => return None,
    };
    Some(widgets::action(
        ctx.palette,
        ButtonKind::Secondary,
        ctx.t(label),
        Some(Icon::ExternalLink),
        Some(Message::Fixes(Msg::Open(request))),
    ))
}

fn other_row<'a>(state: &'a State, ctx: &'a Ctx, key: String, o: &Other) -> Element<'a, Message> {
    let p = ctx.palette;
    let icon = match o.bucket {
        Bucket::Managed => Icon::Lock,
        Bucket::Unavailable => Icon::Info,
        Bucket::Look => {
            if o.group == Group::Information {
                Icon::Info
            } else {
                Icon::AlertTriangle
            }
        }
    };
    let mut head = row![
        widgets::icon_badge(p, icon, o.tone),
        column![
            text(o.name.clone()).size(theme::BODY).font(theme::MEDIUM).color(p.text),
            widgets::small(p, ctx.t(o.next)),
        ]
        .spacing(2)
        .width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    head = head.push(widgets::pill(p, ctx.t(o.status), o.tone));
    if o.step == NextStep::CheckAgain {
        head = head.push(widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Check again"),
            Some(Icon::Refresh),
            (!ctx.busy && ctx.checking.is_none()).then_some(Message::CheckNow),
        ));
    } else if let Some(button) = settings_button(ctx, o.step) {
        head = head.push(button);
    }
    let mut c = column![head].spacing(6);
    let open = state.technical.contains(&key);
    c = c.push(
        row![
            space::horizontal().width(48),
            link(p, ctx.t("Technical details"), open, Message::Fixes(Msg::Technical(key.clone())))
        ],
    );
    if open {
        c = c.push(row![
            space::horizontal().width(48),
            widgets::small(p, o.technical.clone())
        ]);
    }
    c.into()
}

fn attention_row<'a>(
    state: &State,
    ctx: &Ctx,
    report: &Report,
    id: &str,
    checked: bool,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let outcome = report.results.iter().find(|r| r.id == id);
    let a = outcome.map(advice::for_outcome);
    let name = ctx.lang.control(id);
    let impact = advice::control_impact(id);
    let one_liner = if !impact.is_empty() {
        format!("{} {}", ctx.t("Risk:"), ctx.t(impact))
    } else {
        a.as_ref().map(|a| ctx.t(a.next)).unwrap_or_default()
    };
    let toggle_id = id.to_owned();
    let boxed = checkbox(checked).on_toggle(move |_| Message::Fixes(Msg::Toggle(toggle_id.clone())));
    let open = state.expanded.contains(id);
    let mut head = row![
        boxed,
        column![
            text(name).size(theme::BODY).font(theme::MEDIUM).color(p.text),
            widgets::small(p, one_liner),
        ]
        .spacing(2)
        .width(Length::Fill),
    ]
    .spacing(14)
    .align_y(Alignment::Center);
    if ctx.catalog.restart.iter().any(|r| r == id) {
        head = head.push(widgets::pill(p, ctx.t("Needs restart"), Tone::Neutral));
    }
    head = head.push(link(
        p,
        ctx.t(if open { "Less" } else { "Details" }),
        open,
        Message::Fixes(Msg::Expand(id.to_owned())),
    ));
    let mut c = column![head].spacing(8);
    if open {
        let mut why = column![widgets::small(p, ctx.t("Why it matters"))].spacing(4);
        if let Some(a) = &a {
            why = why.push(widgets::body(p, ctx.t(a.next)));
        }
        if !impact.is_empty() {
            why = why.push(widgets::muted(
                p,
                format!("{} {}", ctx.t("Protects you from:"), ctx.t(impact)),
            ));
        }
        let tech_open = state.technical.contains(id);
        why = why.push(link(
            p,
            ctx.t("Technical details"),
            tech_open,
            Message::Fixes(Msg::Technical(id.to_owned())),
        ));
        if tech_open {
            if let Some(r) = outcome {
                why = why.push(widgets::small(
                    p,
                    format!("{} · {} · {}", r.id, r.status, sanitize(&r.detail)),
                ));
            }
        }
        c = c.push(row![space::horizontal().width(34), why]);
    }
    c.into()
}

fn notice<'a>(p: Palette, icon: Icon, tone: Tone, title: String, body: String) -> Element<'a, Message> {
    widgets::card(
        p,
        row![
            widgets::icon_badge(p, icon, tone),
            column![widgets::h2(p, title), widgets::muted(p, body)].spacing(4).width(Length::Fill)
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .into()
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let header = widgets::page_header(
        p,
        ctx.t("Protection"),
        Some(ctx.t("Everything we check on your PC, in plain words.")),
    );
    let mut page = column![header].spacing(theme::GAP);

    if let Some(error) = &ctx.engine_error {
        let mut c = column![
            notice(
                p,
                Icon::ShieldAlert,
                Tone::Bad,
                ctx.t("We couldn't start the protection check"),
                ctx.t("Close Secblitz and open it again. If this keeps happening, restart your PC."),
            ),
            link(p, ctx.t("Technical details"), state.show_error, Message::Fixes(Msg::ErrorDetails)),
        ]
        .spacing(8);
        if state.show_error {
            c = c.push(widgets::small(p, sanitize(error)));
        }
        return page.push(c).into();
    }

    let Some(report) = ctx.report.as_deref() else {
        let card = if let Some(error) = &ctx.check_error {
            let mut c = column![
                notice(
                    p,
                    Icon::ShieldAlert,
                    Tone::Warn,
                    ctx.t("We couldn't finish checking"),
                    ctx.t("Nothing was changed. Please check again in a moment."),
                ),
                row![
                    widgets::action(
                        p,
                        ButtonKind::Primary,
                        ctx.t("Check again"),
                        Some(Icon::Refresh),
                        (ctx.checking.is_none() && !ctx.busy).then_some(Message::CheckNow)
                    ),
                    link(p, ctx.t("Technical details"), state.show_error, Message::Fixes(Msg::ErrorDetails)),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(12);
            if state.show_error {
                c = c.push(widgets::small(p, sanitize(error)));
            }
            c.into()
        } else {
            let done = ctx.checking.as_ref().map_or(0, |c| c.items.len());
            let body = if done > 0 {
                format!("{} {}", ctx.t("Checked so far:"), done)
            } else {
                ctx.t("This takes about a minute. Nothing is changed.")
            };
            notice(p, Icon::Scan, Tone::Neutral, ctx.t("Checking your PC"), body)
        };
        return page.push(card).into();
    };

    if ctx.check_error.is_some() {
        page = page.push(notice(
            p,
            Icon::AlertTriangle,
            Tone::Warn,
            ctx.t("We couldn't refresh the check"),
            ctx.t("You're seeing the last result we could confirm."),
        ));
    }
    if report.results.iter().any(|r| r.status == "pending")
        || report.findings.iter().any(|f| f.status == "pending")
    {
        page = page.push(widgets::card(
            p,
            row![
                widgets::icon_badge(p, Icon::Undo, Tone::Warn),
                column![
                    widgets::h2(p, ctx.t("An earlier change isn't finished")),
                    widgets::muted(
                        p,
                        ctx.t("Undo your last fixes before making new ones."),
                    ),
                ]
                .spacing(4)
                .width(Length::Fill),
                widgets::action(
                    p,
                    ButtonKind::Secondary,
                    ctx.t("Undo your last fixes"),
                    Some(Icon::Undo),
                    (!ctx.busy).then_some(Message::ReviewUndo),
                ),
            ]
            .spacing(14)
            .align_y(Alignment::Center),
        ));
    }

    // Needs your attention (selectable).
    let fixable = candidates(ctx);
    if fixable.is_empty() {
        page = page.push(notice(
            p,
            Icon::ShieldCheck,
            Tone::Good,
            ctx.t("Nothing needs fixing right now"),
            ctx.t("We'll tell you if anything changes."),
        ));
    } else {
        let chosen = selection(state, ctx, &fixable);
        let n = chosen.len();
        let selected_label = if n == 1 {
            ctx.t("1 selected")
        } else {
            ctx.t("{n} selected").replace("{n}", &n.to_string())
        };
        let ready = !ctx.busy && ctx.checking.is_none() && ctx.check_error.is_none();
        let mut bar = row![
            widgets::body(p, selected_label),
            widgets::action(p, ButtonKind::Ghost, ctx.t("Select all"), None, Some(Message::Fixes(Msg::SelectAll))),
            widgets::action(p, ButtonKind::Ghost, ctx.t("Select none"), None, Some(Message::Fixes(Msg::SelectNone))),
            space::horizontal(),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        bar = bar.push(widgets::action(
            p,
            ButtonKind::Primary,
            ctx.t("Fix selected"),
            Some(Icon::Wrench),
            (ready && n > 0).then(|| Message::ReviewFixes(chosen.clone())),
        ));
        let mut list = column![].spacing(12);
        for (i, id) in fixable.iter().enumerate() {
            if i > 0 {
                list = list.push(divider(p));
            }
            list = list.push(attention_row(state, ctx, report, id, chosen.contains(id)));
        }
        page = page.push(
            widgets::card(
                p,
                column![
                    section_header(
                        p,
                        ctx.t("Needs your attention"),
                        fixable.len(),
                        Tone::Warn,
                        Some(ctx.t("We've ticked what we recommend. Nothing changes until you review it.")),
                    ),
                    bar,
                    divider(p),
                    list,
                ]
                .spacing(14),
            ),
        );
    }

    // Everything else that isn't protected.
    let rest = others(ctx, report, &fixable);
    for (bucket, title, subtitle) in [
        (
            Bucket::Look,
            "Worth a look",
            "These need a decision from you. We can't safely change them for you.",
        ),
        (
            Bucket::Unavailable,
            "Can't check right now",
            "Windows didn't give us an answer. Check again in a moment.",
        ),
        (
            Bucket::Managed,
            "Managed by your organization",
            "Someone else controls these settings, so we leave them alone.",
        ),
    ] {
        let items: Vec<(usize, &Other)> = rest
            .iter()
            .enumerate()
            .filter(|(_, o)| o.bucket == bucket)
            .collect();
        if items.is_empty() {
            continue;
        }
        let mut list = column![].spacing(12);
        for (i, (index, o)) in items.iter().enumerate() {
            if i > 0 {
                list = list.push(divider(p));
            }
            list = list.push(other_row(state, ctx, format!("other:{index}"), o));
        }
        page = page.push(widgets::card(
            p,
            column![
                section_header(p, ctx.t(title), items.len(), Tone::Neutral, Some(ctx.t(subtitle))),
                list
            ]
            .spacing(14),
        ));
    }

    // Protected (collapsed by default).
    let protected: Vec<&str> = report
        .results
        .iter()
        .filter(|r| score::classify(r) == Class::Protected)
        .map(|r| r.id.as_str())
        .collect();
    if !protected.is_empty() {
        let mut c = column![row![
            section_header(p, ctx.t("Protected"), protected.len(), Tone::Good, None),
            space::horizontal(),
            widgets::action(
                p,
                ButtonKind::Ghost,
                ctx.t(if state.show_protected { "Hide" } else { "Show" }),
                None,
                Some(Message::Fixes(Msg::ShowProtected)),
            ),
        ]
        .align_y(Alignment::Center)]
        .spacing(14);
        if state.show_protected {
            c = c.push(divider(p));
            let mut list = column![].spacing(12);
            for id in protected {
                let impact = advice::control_impact(id);
                let mut text_col = column![
                    text(ctx.lang.control(id)).size(theme::BODY).font(theme::MEDIUM).color(p.text)
                ]
                .spacing(2)
                .width(Length::Fill);
                if !impact.is_empty() {
                    text_col = text_col.push(widgets::small(
                        p,
                        format!("{} {}", ctx.t("Protects you from:"), ctx.t(impact)),
                    ));
                }
                list = list.push(
                    row![widgets::icon_badge(p, Icon::Check, Tone::Good), text_col]
                        .spacing(12)
                        .align_y(Alignment::Center),
                );
            }
            c = c.push(list);
        }
        page = page.push(widgets::card(p, c));
    }
    page.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_controls_and_bounds_length() {
        assert_eq!(sanitize("a\u{1b}[31m\nb\t c"), "a [31m b c");
        let long = "x".repeat(1000);
        let s = sanitize(&long);
        assert_eq!(s.chars().count(), 301);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn flip_toggles_membership() {
        let mut set = HashSet::new();
        flip(&mut set, "a".into());
        assert!(set.contains("a"));
        flip(&mut set, "a".into());
        assert!(set.is_empty());
    }
}
