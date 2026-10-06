//! Per-account settings and updates for the apps the user installed, not Windows apps or Secblitz.
use crate::broker::{Reply, Request};
use crate::explain;
use crate::gui::icons::Icon;
use crate::gui::pages::tools;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets::{self, progress, ButtonKind};
use crate::gui::{blocking, Ctx, Helper, Message};
use crate::user_apps::APPS;
use crate::user_settings::{Op, Setting};
use iced::widget::{column, space};
use iced::{Element, Length, Task};

type El<'a> = Element<'a, Message>;

#[derive(Debug, Clone)]
pub enum Msg {
    Load,
    Reported(Setting, Result<Reply, String>),
    Toggle(Setting, bool),
    Changed(Setting, Result<Reply, String>),
    ToggleDetail(Detail),
    ScanApps,
    Scanned(Result<Reply, String>),
    AppQuery(usize, Result<Reply, String>),
    Update(usize),
    Updated(usize, Result<Reply, String>),
    InstallerChecked(Result<Reply, String>),
    ScanOnline(bool),
    UpdateOnline(usize, bool),
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Apps {
    Unchecked,
    Probing,
    Idle,
    Preparing,
    Scanning,
    Reading(usize),
    Ready,
    Failed(Why),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppCell {
    Hidden,
    Available,
    Preparing,
    Updating,
    Updated,
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
            apps: Apps::Unchecked,
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


pub fn on_enter(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    if ctx.helper != Helper::Ready {
        return Task::none();
    }
    let mut tasks = Vec::new();
    if state.cells.iter().all(|c| *c == Cell::Idle) {
        tasks.push(update(state, Msg::Load, ctx));
    }
    if state.apps == Apps::Unchecked {
        state.apps = Apps::Probing;
        tasks.push(ctx.broker_task(Request::AppInstallerStatus, |r| {
            wrap(Msg::InstallerChecked(r))
        }));
    }
    Task::batch(tasks)
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
                Ok(Reply::Unavailable) => Some(ctx.t("This setting can't be changed on this PC. Your work or school may control it.")),
                Ok(Reply::ChangedSince) => Some(ctx.t("This setting was changed again after Secblitz set it. Whatever you changed was left as it is.")),
                Err(e) if e == "unavailable" => Some(helper_text(ctx)),
                _ => Some(ctx.t("We couldn't change that setting. It was left as it was. Please try again.")),
            };
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
            if ctx.helper != Helper::Ready
                || matches!(
                    state.apps,
                    Apps::Unchecked | Apps::Probing | Apps::Preparing | Apps::Scanning | Apps::Reading(_)
                )
                || updating(state)
            {
                return Task::none();
            }
            state.apps = Apps::Preparing;
            Task::perform(blocking(secblitz::software_install::dns_offline), |offline| {
                wrap(Msg::ScanOnline(!offline))
            })
        }
        Msg::InstallerChecked(reply) => {
            if state.apps == Apps::Probing {
                state.apps = match reply {
                    Ok(Reply::Unavailable) => Apps::Failed(Why::Unavailable),
                    _ => Apps::Idle,
                };
            }
            Task::none()
        }
        Msg::ScanOnline(online) => {
            if state.apps != Apps::Preparing {
                return Task::none();
            }
            if !online {
                state.apps = Apps::Failed(Why::Offline);
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
            state.app_cells[index] = AppCell::Preparing;
            Task::perform(blocking(secblitz::software_install::dns_offline), move |offline| {
                wrap(Msg::UpdateOnline(index, !offline))
            })
        }
        Msg::UpdateOnline(index, online) => {
            if state.app_cells[index] != AppCell::Preparing {
                return Task::none();
            }
            if !online {
                state.app_cells[index] = AppCell::Failed(Why::Offline);
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
    state
        .app_cells
        .iter()
        .any(|c| matches!(c, AppCell::Updating | AppCell::Preparing))
}

fn helper_text(ctx: &Ctx) -> String {
    ctx.t(ctx.helper.blocker().unwrap_or(crate::gui::REOPEN_TO_DO_THIS))
}


pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    column![account_group(state, ctx), apps_group(state, ctx)]
        .spacing(theme::S8)
        .width(Length::Fill)
        .into()
}

fn secondary<'a>(p: Palette, label: String, msg: Option<Msg>) -> El<'a> {
    widgets::action(p, ButtonKind::Secondary, label, None, msg.map(wrap))
}

fn block<'a>(head: El<'a>, extra: Option<El<'a>>) -> El<'a> {
    match extra {
        Some(e) => column![head, e].width(Length::Fill).into(),
        None => head,
    }
}

fn explainer<'a>(state: &State, ctx: &Ctx, id: &str, detail: Detail) -> Option<El<'a>> {
    let e = explain::for_check(id)?;
    let p = ctx.palette;
    let lines = column![
        line(p, ctx.t("What it is"), ctx.t(e.what)),
        line(p, ctx.t("If it's off"), ctx.t(e.risk)),
        line(p, ctx.t("If you turn it on"), ctx.t(e.change)),
    ]
    .spacing(theme::S2);
    Some(widgets::under_row(vec![widgets::expander(
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
        Setting::SuggestedApps => (
            Icon::Package,
            ctx.t("Stop Windows from adding suggested apps again"),
        ),
    }
}

fn is_on(reply: Reply) -> bool {
    matches!(reply, Reply::Safe | Reply::SafeByUs)
}

fn account_group<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let mut rows: Vec<El<'a>> = Vec::new();
    if ctx.helper != Helper::Ready {
        rows.push(widgets::row_item(
            p,
            Some(Icon::Info),
            helper_text(ctx),
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
        Cell::Known(Reply::Unknown) if setting == Setting::NearbySharing => return None,
        Cell::Known(Reply::Unknown) => (
            ctx.t("We couldn't check this. Leave this page and open it again to try once more."),
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
        Why::Offline => ctx.t("You seem to be offline. Connect to the internet, then press Try again."),
        Why::Unavailable => ctx.t("App updates need App Installer from Microsoft, which this PC doesn't have. Install it from the Microsoft Store, then open Secblitz again."),
        Why::Unreadable => ctx.t("We couldn't check for updates. Press Try again. If it keeps failing, try again a little later."),
    }
}

fn apps_group<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let mut rows: Vec<El<'a>> = Vec::new();
    let scan_title = ctx.t("Look for app updates");
    let scan_help =
        ctx.t("Checks your browser, Java, PDF reader and a few other popular programs.");
    if ctx.helper != Helper::Ready {
        rows.push(widgets::row_item(
            p,
            Some(Icon::Info),
            helper_text(ctx),
            None,
            space::horizontal().width(0),
            None,
        ));
    } else {
        match state.apps {
            Apps::Unchecked | Apps::Probing => rows.push(widgets::row_item(
                p,
                Some(Icon::Download),
                scan_title,
                Some(ctx.t("Checking that this PC can do it…")),
                secondary(p, ctx.t("Check"), None),
                None,
            )),
            Apps::Preparing => rows.push(widgets::row_item(
                p,
                Some(Icon::Download),
                scan_title,
                Some(ctx.t("Checking your internet connection…")),
                secondary(p, ctx.t("Check"), None),
                None,
            )),
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
                Some(widgets::under_row(vec![progress::indeterminate(p, Tone::Brand)])),
            )),
            Apps::Failed(why) => rows.push(widgets::row_item_tinted(
                p,
                Some(Icon::AlertTriangle),
                Some(Tone::Warn),
                why_text(ctx, why),
                None,
                if why == Why::Unavailable {
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
            Some(widgets::under_row(vec![progress::indeterminate(p, Tone::Brand)])),
        ),
        AppCell::Preparing => widgets::row_item(
            p,
            Some(Icon::Package),
            name,
            Some(ctx.t("Checking your internet connection…")),
            secondary(p, ctx.t("Update"), None),
            None,
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
                Why::Offline => ctx.t("You seem to be offline. Connect to the internet, then press Try again."),
                _ => ctx.t("We couldn't update it. Close the program, then press Try again."),
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

    #[test]
    fn a_preparing_app_blocks_other_updates() {
        let mut state = State::default();
        state.app_cells[0] = AppCell::Preparing;
        assert!(updating(&state));
    }
}
