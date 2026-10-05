//! "Your account" and "App updates": settings that belong to the signed-in
//! person, changed through the unelevated launcher (the broker), and updates
//! for a short list of popular programs. Embedded in the Tools page with one
//! call each to [`view`], [`update`] and [`on_enter`].
//!
//! Nothing here is pre-selected or changed on its own: every switch and every
//! Update button is the person's own choice, and every change can be undone
//! from the same switch (apps cannot be rolled back, and the page says so).
use crate::broker::{Reply, Request};
use crate::explain;
use crate::gui::icons::Icon;
use crate::gui::pages::tools;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, progress, ButtonKind};
use crate::gui::{Ctx, Message};
use crate::user_apps::APPS;
use crate::user_settings::{Op, Setting};
use iced::widget::{column, container, space};
use iced::{Element, Length, Padding, Task};

type El<'a> = Element<'a, Message>;

#[derive(Debug, Clone)]
pub enum Msg {
    /// Read every setting (and nothing else).
    Load,
    Reported(Setting, Result<Reply, String>),
    /// The switch of a setting was flipped (true = turn the protection on).
    Toggle(Setting, bool),
    Changed(Setting, Result<Reply, String>),
    ToggleDetail(Detail),
    ScanApps,
    Scanned(Result<Reply, String>),
    AppQuery(usize, Result<Reply, String>),
    Update(usize),
    Updated(usize, Result<Reply, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Setting(Setting),
    Apps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Idle,
    Loading,
    Known(Reply),
    Working,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    Offline,
    Unavailable,
    Unreadable,
    NoBroker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Apps {
    Idle,
    Scanning,
    /// This many answers are still on their way.
    Reading(usize),
    Ready,
    Failed(Why),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppCell {
    /// Nothing to show (not installed, current, or not readable).
    Hidden,
    Available,
    Updating,
    Updated,
    /// Updated, but the new version could not be confirmed.
    Unconfirmed,
    Failed(Why),
}

#[derive(Debug)]
pub struct State {
    cells: [Cell; Setting::PERSONAL.len()],
    apps: Apps,
    app_cells: [AppCell; APPS.len()],
    open: Vec<Detail>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            cells: [Cell::Idle; Setting::PERSONAL.len()],
            apps: Apps::Idle,
            app_cells: [AppCell::Hidden; APPS.len()],
            open: Vec::new(),
        }
    }
}

fn wrap(msg: Msg) -> Message {
    Message::Tools(tools::Msg::Personal(msg))
}

fn cell(setting: Setting) -> usize {
    usize::from(setting.to_byte())
}

fn toast(text: String, tone: Tone) -> Task<Message> {
    Task::done(Message::Toast(text, tone))
}

// ---------------------------------------------------------------------------
// Logic
// ---------------------------------------------------------------------------

/// Read the settings when the Tools page opens (a cheap, read-only request).
pub fn on_enter(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    if ctx.broker.is_none() || state.cells.iter().any(|c| *c != Cell::Idle) {
        return Task::none();
    }
    update(state, Msg::Load, ctx)
}

fn query(ctx: &Ctx, setting: Setting) -> Task<Message> {
    ctx.broker_task(Request::UserSetting(setting, Op::Query), move |r| {
        wrap(Msg::Reported(setting, r))
    })
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Load => {
            let mut tasks = Vec::new();
            for setting in Setting::PERSONAL {
                if !matches!(state.cells[cell(setting)], Cell::Working | Cell::Loading) {
                    state.cells[cell(setting)] = Cell::Loading;
                    tasks.push(query(ctx, setting));
                }
            }
            Task::batch(tasks)
        }
        Msg::Reported(setting, reply) => {
            state.cells[cell(setting)] = Cell::Known(match reply {
                Ok(
                    r @ (Reply::Safe
                    | Reply::SafeByUs
                    | Reply::NeedsAttention
                    | Reply::NotApplicable),
                ) => r,
                // Anything else, including a broken connection, is "unknown".
                _ => Reply::Unknown,
            });
            Task::none()
        }
        Msg::Toggle(setting, on) => {
            let Cell::Known(now) = state.cells[cell(setting)] else {
                return Task::none();
            };
            let op = match (on, now) {
                (true, Reply::NeedsAttention) => Op::Apply,
                (false, Reply::SafeByUs) => Op::Undo,
                _ => return Task::none(),
            };
            state.cells[cell(setting)] = Cell::Working;
            ctx.broker_task(Request::UserSetting(setting, op), move |r| {
                wrap(Msg::Changed(setting, r))
            })
        }
        Msg::Changed(setting, reply) => {
            let note = match reply {
                Ok(Reply::Done) => None,
                Ok(Reply::Unavailable) => Some(ctx.t("This setting can't be changed on this PC.")),
                _ => Some(ctx.t("We couldn't change that setting. It was left as it was.")),
            };
            // Always read it back: the switch shows what is really set.
            let reread = query(ctx, setting);
            match note {
                Some(text) => Task::batch([toast(text, Tone::Warn), reread]),
                None => reread,
            }
        }
        Msg::ToggleDetail(detail) => {
            if let Some(at) = state.open.iter().position(|d| *d == detail) {
                state.open.remove(at);
            } else {
                state.open.push(detail);
            }
            Task::none()
        }
        Msg::ScanApps => {
            if matches!(state.apps, Apps::Scanning | Apps::Reading(_)) || updating(state) {
                return Task::none();
            }
            if ctx.broker.is_none() {
                state.apps = Apps::Failed(Why::NoBroker);
                return Task::none();
            }
            state.apps = Apps::Scanning;
            state.app_cells = [AppCell::Hidden; APPS.len()];
            ctx.broker_task(Request::AppUpdatesScan, |r| wrap(Msg::Scanned(r)))
        }
        Msg::Scanned(reply) => {
            match reply {
                Ok(Reply::Done) => {
                    state.apps = Apps::Reading(APPS.len());
                    let tasks = (0..APPS.len()).map(|i| {
                        ctx.broker_task(Request::AppUpdateQuery(i as u16), move |r| {
                            wrap(Msg::AppQuery(i, r))
                        })
                    });
                    return Task::batch(tasks);
                }
                Ok(Reply::Offline) => state.apps = Apps::Failed(Why::Offline),
                Ok(Reply::Unavailable) => state.apps = Apps::Failed(Why::Unavailable),
                _ => state.apps = Apps::Failed(Why::Unreadable),
            }
            Task::none()
        }
        Msg::AppQuery(index, reply) => {
            if let Apps::Reading(left) = state.apps {
                state.app_cells[index] = match reply {
                    Ok(Reply::UpdateAvailable) => AppCell::Available,
                    _ => AppCell::Hidden,
                };
                state.apps = if left <= 1 {
                    Apps::Ready
                } else {
                    Apps::Reading(left - 1)
                };
            }
            Task::none()
        }
        Msg::Update(index) => {
            if index >= APPS.len()
                || !matches!(
                    state.app_cells[index],
                    AppCell::Available | AppCell::Failed(_)
                )
                || updating(state)
            {
                return Task::none();
            }
            state.app_cells[index] = AppCell::Updating;
            ctx.broker_task(Request::AppUpdate(index as u16), move |r| {
                wrap(Msg::Updated(index, r))
            })
        }
        Msg::Updated(index, reply) => {
            state.app_cells[index] = match reply {
                Ok(Reply::Done) => AppCell::Updated,
                Ok(Reply::Unknown) => AppCell::Unconfirmed,
                Ok(Reply::Offline) => AppCell::Failed(Why::Offline),
                Ok(Reply::Unavailable) => AppCell::Failed(Why::Unavailable),
                _ => AppCell::Failed(Why::Unreadable),
            };
            Task::none()
        }
    }
}

fn updating(state: &State) -> bool {
    state.app_cells.contains(&AppCell::Updating)
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    column![account_group(state, ctx), apps_group(state, ctx)]
        .spacing(theme::S8)
        .width(Length::Fill)
        .into()
}

fn secondary<'a>(p: Palette, label: String, msg: Option<Msg>) -> El<'a> {
    widgets::action(p, ButtonKind::Secondary, label, None, msg.map(wrap))
}

/// Extra lines under a row, lined up with its text.
fn under<'a>(items: Vec<El<'a>>) -> El<'a> {
    container(column(items).spacing(theme::S2).width(Length::Fill))
        .padding(Padding {
            top: 0.0,
            right: theme::S4,
            bottom: theme::S2,
            left: theme::S4 + theme::ICON_ROW + theme::S4,
        })
        .width(Length::Fill)
        .into()
}

fn block<'a>(head: El<'a>, extra: Option<El<'a>>) -> El<'a> {
    match extra {
        Some(e) => column![head, e].width(Length::Fill).into(),
        None => head,
    }
}

/// The three plain lines of an explainer behind "More details".
fn explainer<'a>(state: &State, ctx: &Ctx, id: &str, detail: Detail) -> Option<El<'a>> {
    let e = explain::for_check(id)?;
    let p = ctx.palette;
    let lines = column![
        line(p, ctx.t("What it is"), ctx.t(e.what)),
        line(p, ctx.t("If it's off"), ctx.t(e.risk)),
        line(p, ctx.t("If you turn it on"), ctx.t(e.change)),
    ]
    .spacing(theme::S2);
    Some(under(vec![widgets::expander(
        p,
        ctx.t("More details"),
        state.open.contains(&detail),
        wrap(Msg::ToggleDetail(detail)),
        lines,
    )]))
}

fn line<'a>(p: Palette, label: String, text: String) -> El<'a> {
    column![widgets::small(p, label), widgets::body(p, text)]
        .spacing(2)
        .into()
}

fn label(ctx: &Ctx, setting: Setting) -> (Icon, String) {
    match setting {
        Setting::StoreAppsWebCheck => (Icon::Package, ctx.t("Web check for Store apps")),
        Setting::ShowExtensions => (Icon::Eye, ctx.t("Show file endings like .pdf")),
        Setting::NearbySharing => (Icon::Globe, ctx.t("Nearby sharing with your devices only")),
        Setting::TailoredExperiences => (
            Icon::EyeOff,
            ctx.t("Keep your PC habits out of tips and ads"),
        ),
        Setting::OfficeMacros => (
            Icon::Lock,
            ctx.t("Block macros in Office files from the internet"),
        ),
        // Not listed on this page; kept so the match stays exhaustive.
        Setting::SuggestedApps => (
            Icon::Package,
            ctx.t("Stop Windows from adding suggested apps again"),
        ),
    }
}

/// What the switch means: on = the safer choice.
fn is_on(reply: Reply) -> bool {
    matches!(reply, Reply::Safe | Reply::SafeByUs)
}

/// Every switch is worded so that ON means the protection is on.
fn account_group<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let mut rows: Vec<El<'a>> = Vec::new();
    if ctx.broker.is_none() {
        rows.push(widgets::row_item(
            p,
            Some(Icon::Info),
            ctx.t("Reopen Secblitz from its shortcut to use this."),
            None,
            space::horizontal().width(0),
            None,
        ));
    } else if state.cells.iter().all(|c| *c == Cell::Idle) {
        rows.push(widgets::row_item(
            p,
            Some(Icon::Scan),
            ctx.t("Check my account settings"),
            None,
            secondary(p, ctx.t("Check"), Some(Msg::Load)),
            None,
        ));
    } else {
        for setting in Setting::PERSONAL {
            if let Some(r) = setting_row(state, ctx, setting) {
                rows.push(r);
            }
        }
        if rows.is_empty() {
            rows.push(widgets::row_item(
                p,
                Some(Icon::CheckCircle),
                ctx.t("Nothing to change here."),
                None,
                space::horizontal().width(0),
                None,
            ));
        }
    }
    widgets::group(
        p,
        ctx.t("Your account"),
        Some(ctx.t("Settings that belong to the person signed in to this PC.")),
        None,
        rows,
    )
}

fn setting_row<'a>(state: &'a State, ctx: &'a Ctx, setting: Setting) -> Option<El<'a>> {
    let p = ctx.palette;
    let (icon, title) = label(ctx, setting);
    let cell = state.cells[cell(setting)];
    let toggle = move |on: bool| wrap(Msg::Toggle(setting, on));
    let (sub, control): (String, El<'a>) = match cell {
        Cell::Idle => return None,
        Cell::Loading => (ctx.t("Checking…"), space::horizontal().width(0).into()),
        Cell::Working => (
            ctx.t("Changing…"),
            widgets::switch(p, false, None::<fn(bool) -> Message>),
        ),
        Cell::Known(Reply::NotApplicable) => return None,
        // Not set on this PC: nothing reliable to show or change.
        Cell::Known(Reply::Unknown) if setting == Setting::NearbySharing => return None,
        Cell::Known(Reply::Unknown) => (
            ctx.t("We couldn't check this."),
            space::horizontal().width(0).into(),
        ),
        Cell::Known(Reply::NeedsAttention) => {
            (ctx.t("Off"), widgets::switch(p, false, Some(toggle)))
        }
        Cell::Known(Reply::SafeByUs) => (
            ctx.t("On. You can switch it back."),
            widgets::switch(p, true, Some(toggle)),
        ),
        Cell::Known(reply) => (
            ctx.t("On"),
            widgets::switch(p, is_on(reply), None::<fn(bool) -> Message>),
        ),
    };
    let head = widgets::row_item(p, Some(icon), title, Some(sub), control, None);
    Some(block(
        head,
        explainer(state, ctx, setting.id(), Detail::Setting(setting)),
    ))
}

fn why_text(ctx: &Ctx, why: Why) -> String {
    match why {
        Why::Offline => ctx.t("Check your internet connection and try again."),
        Why::Unavailable => ctx.t("App updates aren't available on this PC."),
        Why::Unreadable => ctx.t("We couldn't check for updates. Please try again later."),
        Why::NoBroker => ctx.t("Reopen Secblitz from its shortcut to use this."),
    }
}

fn apps_group<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let mut rows: Vec<El<'a>> = Vec::new();
    let scan_title = ctx.t("Look for app updates");
    let scan_help =
        ctx.t("Checks your browser, Java, PDF reader and a few other popular programs.");
    match state.apps {
        Apps::Idle => rows.push(widgets::row_item(
            p,
            Some(Icon::Download),
            scan_title,
            Some(scan_help),
            secondary(p, ctx.t("Check"), Some(Msg::ScanApps)),
            None,
        )),
        Apps::Scanning | Apps::Reading(_) => rows.push(block(
            widgets::row_item(
                p,
                Some(Icon::Download),
                ctx.t("Looking for app updates…"),
                Some(ctx.t("This can take a minute.")),
                space::horizontal().width(0),
                None,
            ),
            Some(under(vec![progress::indeterminate(p, Tone::Brand)])),
        )),
        Apps::Failed(why) => rows.push(widgets::row_item_tinted(
            p,
            Some(Icon::AlertTriangle),
            Some(Tone::Warn),
            why_text(ctx, why),
            None,
            if why == Why::NoBroker || why == Why::Unavailable {
                space::horizontal().width(0).into()
            } else {
                secondary(p, ctx.t("Try again"), Some(Msg::ScanApps))
            },
            None,
        )),
        Apps::Ready => {
            let shown: Vec<usize> = (0..APPS.len())
                .filter(|i| state.app_cells[*i] != AppCell::Hidden)
                .collect();
            let again = secondary(p, ctx.t("Check again"), Some(Msg::ScanApps));
            if shown.is_empty() {
                rows.push(widgets::row_item_tinted(
                    p,
                    Some(Icon::CheckCircle),
                    Some(Tone::Good),
                    ctx.t("Your popular programs are up to date."),
                    None,
                    again,
                    None,
                ));
            } else {
                rows.push(widgets::row_item(
                    p,
                    Some(Icon::Download),
                    ctx.t("Newer versions are available"),
                    Some(ctx.t("Updates can't be undone. A program may close while it updates.")),
                    again,
                    None,
                ));
                let busy = updating(state);
                for i in shown {
                    rows.push(app_row(state, ctx, i, busy));
                }
            }
        }
    }
    if let Some(e) = explainer(state, ctx, "software.outdated_winget", Detail::Apps) {
        rows.push(e);
    }
    widgets::group(
        p,
        ctx.t("App updates"),
        Some(ctx.t("Newer versions of popular programs fix security problems.")),
        None,
        rows,
    )
}

fn app_row<'a>(state: &'a State, ctx: &'a Ctx, i: usize, busy: bool) -> El<'a> {
    let p = ctx.palette;
    let name = APPS[i].name.to_owned();
    match state.app_cells[i] {
        AppCell::Updating => block(
            widgets::row_item(
                p,
                Some(Icon::Package),
                name,
                Some(ctx.t("Updating… The program may close for a moment.")),
                space::horizontal().width(0),
                None,
            ),
            Some(under(vec![progress::indeterminate(p, Tone::Brand)])),
        ),
        AppCell::Updated => widgets::row_item_tinted(
            p,
            Some(Icon::CheckCircle),
            Some(Tone::Good),
            name,
            Some(ctx.t("Updated.")),
            space::horizontal().width(0),
            None,
        ),
        AppCell::Unconfirmed => widgets::row_item_tinted(
            p,
            Some(Icon::AlertTriangle),
            Some(Tone::Warn),
            name,
            Some(ctx.t("Updated, but we couldn't confirm it. Check again.")),
            space::horizontal().width(0),
            None,
        ),
        AppCell::Failed(why) => {
            let help = match why {
                Why::Offline => ctx.t("Check your internet connection and try again."),
                _ => ctx.t("We couldn't update it. Close the program and try again."),
            };
            widgets::row_item_tinted(
                p,
                Some(Icon::AlertTriangle),
                Some(Tone::Warn),
                name,
                Some(help),
                secondary(p, ctx.t("Try again"), (!busy).then_some(Msg::Update(i))),
                None,
            )
        }
        AppCell::Available | AppCell::Hidden => widgets::row_item(
            p,
            Some(Icon::Package),
            name,
            Some(ctx.t("A newer version is available.")),
            secondary(p, ctx.t("Update"), (!busy).then_some(Msg::Update(i))),
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_has_a_cell_and_a_label_and_an_explainer() {
        let state = State::default();
        assert_eq!(state.cells.len(), Setting::PERSONAL.len());
        for (i, setting) in Setting::PERSONAL.iter().enumerate() {
            assert_eq!(cell(*setting), i);
            assert!(explain::for_check(setting.id()).is_some());
        }
    }

    #[test]
    fn a_failed_app_can_be_retried_but_not_while_another_updates() {
        let mut state = State::default();
        state.app_cells[0] = AppCell::Failed(Why::Unreadable);
        state.app_cells[1] = AppCell::Updating;
        assert!(updating(&state));
        state.app_cells[1] = AppCell::Updated;
        assert!(!updating(&state));
    }
}
