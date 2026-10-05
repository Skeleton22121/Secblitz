//! Protection page: every check grouped, attention rows selectable.
//! OWNER: fixes agent.
//!
//! Layout (docs/DESIGN-SYSTEM.md): one card per group, rows of
//! [badge, title + one plain line, pill, chevron], 48 px minimum, `S1` apart.
//! Row text is translated and built once per check result (`Cache`) so
//! `view()` only assembles widgets.
use crate::advice::{self, Group, NextStep};
use crate::app::flow;
use crate::app::score::{self, Class};
use crate::broker::{Reply, Request};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind, CheckState};
use crate::gui::{Ctx, Message};
use crate::i18n::Lang;
use iced::widget::{column, container, row, space, text, Column};
use iced::{Alignment, Background, Border, Element, Length, Task};
use secblitz::engine::Report;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

/// Size of the small chevron / inline icons inside rows.
const ICON_SMALL: f32 = 16.0;
/// Left inset that lines expanded text up with a row's title:
/// checkbox box + its padding, gap, row padding, badge, gap.
const INDENT: f32 =
    theme::CHECK + theme::S1 * 2.0 + theme::S2 + theme::S4 + theme::CONTROL + theme::S3;
/// Same inset for rows that have no checkbox.
const INDENT_PLAIN: f32 = theme::S4 + theme::CONTROL + theme::S3;

#[derive(Debug, Default)]
pub struct State {
    /// `Ctx::checked_at` the selection below belongs to.
    synced_at: Option<u64>,
    selected: HashSet<String>,
    expanded: HashSet<String>,
    show_protected: bool,
    show_error: bool,
    /// Translated row text for the current report (built lazily, once).
    cache: RefCell<Option<Cached>>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Toggle(String),
    SelectAll,
    SelectNone,
    Expand(String),
    ShowProtected,
    ErrorDetails,
    /// Open a Windows Settings page through the launcher.
    Open(Request),
}

// ---------------------------------------------------------------- row data

#[derive(Debug)]
struct Cached {
    /// Identity of what the rows were built from.
    key: (usize, Lang, usize),
    rows: Rows,
}

#[derive(Debug, Default)]
struct Rows {
    attention: Vec<Att>,
    others: Vec<Other>,
    protected: Vec<Prot>,
}

/// A fixable, selectable row.
#[derive(Debug)]
struct Att {
    id: String,
    name: String,
    line: String,
    why: String,
    tech: String,
    restart: bool,
}

/// A row of the "worth a look / can't check / managed" groups.
#[derive(Debug)]
struct Other {
    key: String,
    name: String,
    line: String,
    status: String,
    step: NextStep,
    tone: Tone,
    bucket: Bucket,
    icon: Icon,
    tech: String,
}

#[derive(Debug)]
struct Prot {
    name: String,
    line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Look,
    Managed,
    Unavailable,
}

fn tech_line(id: &str, status: &str, detail: &str, lang: Lang) -> String {
    format!("{} · {} · {}", id, status, lang.detail(&sanitize(detail)))
}

fn build(ctx: &Ctx, report: &Report) -> Rows {
    let lang = ctx.lang;
    let mut rows = Rows::default();
    let fixable = candidates(ctx);

    for id in &fixable {
        let Some(r) = report.results.iter().find(|r| r.id == *id) else {
            continue;
        };
        let a = advice::for_outcome(r);
        let impact = advice::control_impact(id);
        let line = if impact.is_empty() {
            ctx.t(a.next)
        } else {
            format!("{} {}", ctx.t(a.impact_prefix()), ctx.t(impact))
        };
        rows.attention.push(Att {
            id: id.clone(),
            name: lang.control(id),
            line,
            why: ctx.t(a.next),
            tech: tech_line(&r.id, &r.status, &r.detail, lang),
            restart: ctx.catalog.restart.iter().any(|x| x == id),
        });
    }

    for r in &report.results {
        if fixable.contains(&r.id) {
            continue;
        }
        let class = score::classify(r);
        let a = advice::for_outcome(r);
        if class == Class::Protected {
            let impact = advice::control_impact(&r.id);
            let line = if impact.is_empty() {
                ctx.t(a.next)
            } else {
                format!("{} {}", ctx.t(a.impact_prefix()), ctx.t(impact))
            };
            rows.protected.push(Prot {
                name: lang.control(&r.id),
                line,
            });
            continue;
        }
        let (bucket, tone) = if a.status == "Managed elsewhere" {
            (Bucket::Managed, Tone::Neutral)
        } else if class == Class::Unknown {
            (Bucket::Unavailable, Tone::Neutral)
        } else {
            (Bucket::Look, Tone::Warn)
        };
        rows.others.push(other(
            ctx,
            rows.others.len(),
            lang.control(&r.id),
            &a,
            bucket,
            tone,
            tech_line(&r.id, &r.status, &r.detail, lang),
        ));
    }
    for f in &report.findings {
        let a = advice::for_finding(&f.title, &f.status, &f.detail);
        if a.group == Group::Protected {
            continue;
        }
        let (bucket, tone) =
            if a.status == "Managed elsewhere" || a.step == NextStep::ReviewWithAdministrator {
                (Bucket::Managed, Tone::Neutral)
            } else if a.step == NextStep::CheckAgain {
                (Bucket::Unavailable, Tone::Neutral)
            } else {
                (
                    Bucket::Look,
                    if a.group == Group::Information {
                        Tone::Neutral
                    } else {
                        Tone::Warn
                    },
                )
            };
        rows.others.push(other(
            ctx,
            rows.others.len(),
            ctx.t(a.label),
            &a,
            bucket,
            tone,
            tech_line(&f.title, &f.status, &f.detail, lang),
        ));
    }
    rows
}

fn other(
    ctx: &Ctx,
    index: usize,
    name: String,
    a: &advice::Advice,
    bucket: Bucket,
    tone: Tone,
    tech: String,
) -> Other {
    let icon = match bucket {
        Bucket::Managed => Icon::Lock,
        Bucket::Unavailable => Icon::Info,
        Bucket::Look if a.group == Group::Information => Icon::Info,
        Bucket::Look => Icon::AlertTriangle,
    };
    Other {
        key: format!("other:{index}"),
        name,
        line: ctx.t(a.next),
        status: ctx.t(a.status),
        step: a.step,
        tone,
        bucket,
        icon,
        tech,
    }
}

/// Build the row cache if the report (or language) changed since last time.
fn ensure(state: &State, ctx: &Ctx, report: &Arc<Report>) {
    let key = (
        Arc::as_ptr(report) as usize,
        ctx.lang,
        ctx.catalog.available.len(),
    );
    let mut cache = state.cache.borrow_mut();
    if cache.as_ref().is_some_and(|c| c.key == key) {
        return;
    }
    *cache = Some(Cached {
        key,
        rows: build(ctx, report),
    });
}

// ------------------------------------------------------------------ update

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
        state.expanded.clear();
    }
}

/// Selected ids in list order, limited to what is currently fixable.
fn selection(state: &State, ctx: &Ctx, all: &[String]) -> Vec<String> {
    if state.synced_at == ctx.checked_at && ctx.checked_at.is_some() {
        all.iter()
            .filter(|id| state.selected.contains(*id))
            .cloned()
            .collect()
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

// ------------------------------------------------------- shared row pieces

/// Row title (medium weight) with one muted line under it. Shared by the
/// Protection, fix-flow and History pages so rows read the same everywhere.
pub fn row_text<'a>(p: Palette, title: String, line: Option<String>) -> Element<'a, Message> {
    let mut c = column![text(title)
        .size(theme::BODY)
        .font(theme::MEDIUM)
        .color(p.text)]
    .spacing(theme::S1)
    .width(Length::Fill);
    if let Some(l) = line {
        c = c.push(widgets::small(p, l));
    }
    c.into()
}

/// Quiet inset box for extra detail (expanded rows, lists inside sheets).
pub fn well<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Container<'a, Message> {
    container(content)
        .padding(theme::S3)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface_alt)),
            border: Border {
                radius: theme::R.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
}

/// Card with a leading badge, a title, one muted line and a trailing action.
pub fn banner<'a>(
    p: Palette,
    icon: Icon,
    tone: Tone,
    title: String,
    body: String,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut r = row![
        widgets::icon_badge(p, icon, tone),
        column![widgets::h2(p, title), widgets::muted(p, body)]
            .spacing(theme::S1)
            .width(Length::Fill),
    ]
    .spacing(theme::S4)
    .align_y(Alignment::Center);
    if let Some(a) = action {
        r = r.push(a);
    }
    widgets::card(p, r).into()
}

fn chevron<'a>(p: Palette, open: bool) -> Element<'a, Message> {
    widgets::icon(
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        ICON_SMALL,
        p.text_muted,
    )
}

/// `[lead] [clickable row] [trailing]`, vertically centred, `S2` apart.
fn entry<'a>(
    p: Palette,
    lead: Option<Element<'a, Message>>,
    content: Element<'a, Message>,
    trailing: Option<Element<'a, Message>>,
    on_press: Message,
) -> Element<'a, Message> {
    let mut r = row![].spacing(theme::S2).align_y(Alignment::Center);
    if let Some(l) = lead {
        r = r.push(l);
    }
    r = r.push(widgets::list_button(p, content, on_press));
    if let Some(t) = trailing {
        r = r.push(t);
    }
    r.into()
}

/// Expanded text under a row, lined up with the row's title.
fn expanded<'a>(
    p: Palette,
    indent: f32,
    why: Option<String>,
    label: String,
    tech: String,
) -> Element<'a, Message> {
    let mut c = column![].spacing(theme::S2);
    if let Some(w) = why {
        c = c.push(widgets::body(p, w));
    }
    c = c
        .push(widgets::section_label(p, label))
        .push(widgets::small(p, tech));
    row![space::horizontal().width(indent), well(p, c)].into()
}

fn section_header<'a>(
    p: Palette,
    title: String,
    count: usize,
    tone: Tone,
    subtitle: Option<String>,
) -> Element<'a, Message> {
    let mut c = column![row![
        widgets::h2(p, title),
        widgets::pill(p, count.to_string(), tone)
    ]
    .spacing(theme::S2)
    .align_y(Alignment::Center)]
    .spacing(theme::S1);
    if let Some(s) = subtitle {
        c = c.push(widgets::muted(p, s));
    }
    c.into()
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

// -------------------------------------------------------------------- rows

fn attention_row<'a>(
    state: &State,
    ctx: &Ctx,
    a: &Att,
    checked: bool,
    ready_text: &str,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let open = state.expanded.contains(&a.id);
    let mut content = row![
        widgets::icon_badge(p, Icon::AlertTriangle, Tone::Warn),
        row_text(p, a.name.clone(), Some(a.line.clone())),
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center);
    if a.restart {
        content = content.push(widgets::pill(p, ready_text.to_owned(), Tone::Neutral));
    }
    content = content
        .push(widgets::pill(p, ctx.t("Not protected"), Tone::Warn))
        .push(chevron(p, open));

    let check = widgets::checkbox(
        p,
        CheckState::from(checked),
        None,
        Some(Message::Fixes(Msg::Toggle(a.id.clone()))),
    );
    let head = entry(
        p,
        Some(check),
        content.into(),
        None,
        Message::Fixes(Msg::Expand(a.id.clone())),
    );
    if !open {
        return head;
    }
    column![
        head,
        expanded(
            p,
            INDENT,
            Some(a.why.clone()),
            ctx.t("More details"),
            a.tech.clone()
        )
    ]
    .spacing(theme::S1)
    .into()
}

fn other_row<'a>(state: &State, ctx: &Ctx, o: &Other) -> Element<'a, Message> {
    let p = ctx.palette;
    let open = state.expanded.contains(&o.key);
    let content = row![
        widgets::icon_badge(p, o.icon, o.tone),
        row_text(p, o.name.clone(), Some(o.line.clone())),
        widgets::pill(p, o.status.clone(), o.tone),
        chevron(p, open),
    ]
    .spacing(theme::S3)
    .align_y(Alignment::Center);

    let trailing = if o.step == NextStep::CheckAgain {
        Some(widgets::action(
            p,
            ButtonKind::Secondary,
            ctx.t("Check again"),
            Some(Icon::Refresh),
            (!ctx.busy && ctx.checking.is_none()).then_some(Message::CheckNow),
        ))
    } else {
        settings_button(ctx, o.step)
    };
    let head = entry(
        p,
        None,
        content.into(),
        trailing,
        Message::Fixes(Msg::Expand(o.key.clone())),
    );
    if !open {
        return head;
    }
    column![
        head,
        expanded(p, INDENT_PLAIN, None, ctx.t("More details"), o.tech.clone())
    ]
    .spacing(theme::S1)
    .into()
}

fn protected_row<'a>(p: Palette, r: &Prot, label: &str) -> Element<'a, Message> {
    container(
        row![
            widgets::icon_badge(p, Icon::Check, Tone::Good),
            row_text(p, r.name.clone(), Some(r.line.clone())),
            widgets::pill(p, label.to_owned(), Tone::Good),
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center),
    )
    .padding([theme::S3, theme::S4])
    .width(Length::Fill)
    .into()
}

// -------------------------------------------------------------------- view

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let header = widgets::page_header(
        p,
        ctx.t("Protection"),
        Some(ctx.t("Everything we check on your PC, in plain words.")),
    );
    let page = |body: Column<'a, Message>| -> Element<'a, Message> {
        column![header, space::vertical().height(theme::S6), body].into()
    };
    let mut body = column![].spacing(theme::S4);

    // Could not start.
    if let Some(error) = &ctx.engine_error {
        let details = widgets::expander(
            p,
            ctx.t("More details"),
            state.show_error,
            Message::Fixes(Msg::ErrorDetails),
            widgets::small(p, sanitize(error)),
        );
        return page(body.push(widgets::card(
            p,
            widgets::empty_state(
                p,
                Icon::ShieldAlert,
                ctx.t("We couldn't start the protection check"),
                ctx.t(
                    "Close Secblitz and open it again. If this keeps happening, restart your PC.",
                ),
                Some(details),
            ),
        )));
    }

    // First check not finished yet, or it failed.
    let Some(report) = ctx.report.as_ref() else {
        if let Some(error) = &ctx.check_error {
            let details = widgets::expander(
                p,
                ctx.t("More details"),
                state.show_error,
                Message::Fixes(Msg::ErrorDetails),
                widgets::small(p, sanitize(error)),
            );
            let again = widgets::action(
                p,
                ButtonKind::Primary,
                ctx.t("Check again"),
                Some(Icon::Refresh),
                (ctx.checking.is_none() && !ctx.busy).then_some(Message::CheckNow),
            );
            return page(
                body.push(widgets::card(
                    p,
                    widgets::empty_state(
                        p,
                        Icon::ShieldAlert,
                        ctx.t("We couldn't finish checking"),
                        ctx.t("Nothing was changed. Please check again in a moment."),
                        Some(
                            column![again, details]
                                .spacing(theme::S3)
                                .align_x(Alignment::Center)
                                .into(),
                        ),
                    ),
                )),
            );
        }
        let done = ctx.checking.as_ref().map_or(0, |c| c.items.len());
        let total = ctx.catalog.available.len().max(done).max(1);
        let progress = if done > 0 {
            ctx.t("{a} of {b} checked")
                .replace("{a}", &done.to_string())
                .replace("{b}", &total.to_string())
        } else {
            ctx.t("This takes about a minute. Nothing is changed.")
        };
        return page(
            body.push(widgets::card(
                p,
                column![
                    widgets::h2(p, ctx.t("Checking your PC")),
                    widgets::muted(p, progress),
                    widgets::bar(p, done as f32 / total as f32, Tone::Neutral),
                ]
                .spacing(theme::S3),
            )),
        );
    };

    ensure(state, ctx, report);
    let cache = state.cache.borrow();
    let rows = &cache.as_ref().expect("filled by ensure").rows;

    if ctx.checking.is_some() {
        body = body.push(widgets::inline_notice(
            p,
            Tone::Neutral,
            ctx.t("Checking your PC again. This list updates when it's done."),
        ));
    } else if ctx.check_error.is_some() {
        body = body.push(widgets::inline_notice(
            p,
            Tone::Warn,
            ctx.t("We couldn't refresh the check. You're seeing the last result we could confirm."),
        ));
    }
    if report.results.iter().any(|r| r.status == "pending")
        || report.findings.iter().any(|f| f.status == "pending")
    {
        body = body.push(banner(
            p,
            Icon::Undo,
            Tone::Warn,
            ctx.t("An earlier change isn't finished"),
            ctx.t("Undo your last fixes before making new ones."),
            Some(widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t("Undo your last fixes"),
                Some(Icon::Undo),
                (!ctx.busy).then_some(Message::ReviewUndo),
            )),
        ));
    }

    // Needs your attention (selectable).
    if rows.attention.is_empty() {
        body = body.push(widgets::card(
            p,
            widgets::empty_state(
                p,
                Icon::ShieldCheck,
                ctx.t("Nothing needs fixing right now"),
                ctx.t("We'll tell you if anything changes."),
                None,
            ),
        ));
    } else {
        let all: Vec<String> = rows.attention.iter().map(|a| a.id.clone()).collect();
        let chosen = selection(state, ctx, &all);
        let n = chosen.len();
        let restart_label = ctx.t("Needs restart");
        let mut list = column![].spacing(theme::S1);
        for a in &rows.attention {
            list = list.push(attention_row(
                state,
                ctx,
                a,
                chosen.contains(&a.id),
                &restart_label,
            ));
        }
        let count = match n {
            0 => ctx.t("Nothing selected"),
            1 => ctx.t("1 selected"),
            _ => ctx.t("{n} selected").replace("{n}", &n.to_string()),
        };
        let ready = !ctx.busy && ctx.checking.is_none() && ctx.check_error.is_none();
        let (toggle_label, toggle_msg) = if n == all.len() {
            (ctx.t("Select none"), Msg::SelectNone)
        } else {
            (ctx.t("Select all"), Msg::SelectAll)
        };
        let footer = row![
            widgets::muted(p, count),
            space::horizontal(),
            widgets::action(
                p,
                ButtonKind::Ghost,
                toggle_label,
                None,
                Some(Message::Fixes(toggle_msg))
            ),
            widgets::action(
                p,
                ButtonKind::Primary,
                ctx.t("Fix selected"),
                Some(Icon::Wrench),
                (ready && n > 0).then(|| Message::ReviewFixes(chosen.clone())),
            ),
        ]
        .spacing(theme::S2)
        .align_y(Alignment::Center);
        body =
            body.push(widgets::card(
                p,
                column![
                    section_header(
                        p,
                        ctx.t("Needs your attention"),
                        rows.attention.len(),
                        Tone::Warn,
                        Some(ctx.t(
                            "We've ticked what we recommend. Nothing changes until you review it."
                        )),
                    ),
                    space::vertical().height(theme::S1),
                    list,
                    space::vertical().height(theme::S1),
                    footer,
                ]
                .spacing(theme::S3),
            ));
    }

    // Everything else that isn't protected.
    for (bucket, title, subtitle, tone) in [
        (
            Bucket::Look,
            "Worth a look",
            "These need a decision from you. We can't safely change them for you.",
            Tone::Warn,
        ),
        (
            Bucket::Unavailable,
            "Can't check right now",
            "Windows didn't give us an answer. Check again in a moment.",
            Tone::Neutral,
        ),
        (
            Bucket::Managed,
            "Managed by someone else",
            "This PC's owner controls these settings, so we leave them alone.",
            Tone::Neutral,
        ),
    ] {
        let items: Vec<&Other> = rows.others.iter().filter(|o| o.bucket == bucket).collect();
        if items.is_empty() {
            continue;
        }
        let mut list = column![].spacing(theme::S1);
        for o in &items {
            list = list.push(other_row(state, ctx, o));
        }
        body = body.push(widgets::card(
            p,
            column![
                section_header(p, ctx.t(title), items.len(), tone, Some(ctx.t(subtitle))),
                space::vertical().height(theme::S1),
                list,
            ]
            .spacing(theme::S3),
        ));
    }

    // Protected (collapsed by default).
    if !rows.protected.is_empty() {
        let mut c = column![row![
            section_header(
                p,
                ctx.t("Protected"),
                rows.protected.len(),
                Tone::Good,
                None
            ),
            space::horizontal(),
            widgets::action(
                p,
                ButtonKind::Ghost,
                ctx.t(if state.show_protected { "Hide" } else { "Show" }),
                Some(if state.show_protected {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                }),
                Some(Message::Fixes(Msg::ShowProtected)),
            ),
        ]
        .align_y(Alignment::Center)]
        .spacing(theme::S3);
        if state.show_protected {
            let label = ctx.t("Protected");
            let mut list = column![].spacing(theme::S1);
            for r in &rows.protected {
                list = list.push(protected_row(p, r, &label));
            }
            c = c.push(list);
        }
        body = body.push(widgets::card(p, c));
    }
    page(body)
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
