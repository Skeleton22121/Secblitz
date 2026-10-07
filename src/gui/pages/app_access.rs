//! Camera, microphone and location: which apps used them and when, with a switch for each.
use crate::app::app_access::{self, Capability, Entry, Listing, Recency, Target};
use crate::broker::{Reply, Request};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Tone};
use crate::gui::widgets;
use crate::gui::{blocking, Ctx, Helper, Message};
use iced::widget::{column, container, space};
use iced::{Element, Length, Task};

type El<'a> = Element<'a, Message>;

const NAMES_SHOWN: usize = 3;

#[derive(Debug, Clone)]
pub enum Msg {
    Select(Capability),
    Retry,
    Listed(Capability, Result<Reply, String>),
    Read(Capability, Result<Listing, String>),
    Toggle(Target, bool),
    Changed(Capability, Result<Reply, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Load {
    Idle,
    Loading,
    Failed,
}

#[derive(Debug)]
pub struct State {
    capability: Capability,
    listings: [Option<Listing>; 3],
    load: [Load; 3],
    changing: Option<Target>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            capability: Capability::Camera,
            listings: [None, None, None],
            load: [Load::Idle; 3],
            changing: None,
        }
    }
}

fn slot(capability: Capability) -> usize {
    usize::from(capability.to_byte())
}

fn wrap(msg: Msg) -> Message {
    Message::AppAccess(msg)
}

fn toast(text: String) -> Task<Message> {
    Task::done(Message::Toast(text, Tone::Warn))
}

/// Loads the list the first time the panel is shown.
#[allow(dead_code)]
pub fn on_enter(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    if state.load[slot(state.capability)] == Load::Idle {
        return load(state, ctx, state.capability);
    }
    Task::none()
}

fn load(state: &mut State, ctx: &Ctx, capability: Capability) -> Task<Message> {
    if ctx.helper != Helper::Ready || state.load[slot(capability)] == Load::Loading {
        return Task::none();
    }
    state.load[slot(capability)] = Load::Loading;
    ctx.broker_task(Request::AppAccessList(capability), move |r| {
        wrap(Msg::Listed(capability, r))
    })
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Select(capability) => {
            state.capability = capability;
            if state.listings[slot(capability)].is_none() {
                state.load[slot(capability)] = Load::Idle;
                return load(state, ctx, capability);
            }
            Task::none()
        }
        Msg::Retry => {
            state.load[slot(state.capability)] = Load::Idle;
            load(state, ctx, state.capability)
        }
        Msg::Listed(capability, reply) => match reply {
            Ok(Reply::Done) => Task::perform(
                blocking(move || {
                    let path = app_access::handoff_path(capability)
                        .ok_or_else(|| "no per-user data folder".to_owned())?;
                    app_access::take_handoff(&path, capability).map_err(|e| format!("{e:#}"))
                }),
                move |listing| wrap(Msg::Read(capability, listing)),
            ),
            _ => {
                state.load[slot(capability)] = Load::Failed;
                Task::none()
            }
        },
        Msg::Read(capability, listing) => match listing {
            Ok(listing) => {
                state.load[slot(capability)] = Load::Idle;
                state.listings[slot(capability)] = Some(listing);
                Task::none()
            }
            Err(_) => {
                state.load[slot(capability)] = Load::Failed;
                Task::none()
            }
        },
        Msg::Toggle(target, allow) => {
            let capability = state.capability;
            if state.changing.is_some() || state.listings[slot(capability)].is_none() {
                return Task::none();
            }
            state.changing = Some(target);
            ctx.broker_task(
                Request::AppAccessSet {
                    capability,
                    target,
                    allow,
                },
                move |r| wrap(Msg::Changed(capability, r)),
            )
        }
        Msg::Changed(capability, reply) => {
            state.changing = None;
            let note = match reply {
                Ok(Reply::Done) => None,
                Err(e) if e == "unavailable" => Some(helper_text(ctx)),
                _ => Some(ctx.t(
                    "We couldn't change that setting. It was left as it was. Please try again.",
                )),
            };
            state.load[slot(capability)] = Load::Idle;
            let reread = load(state, ctx, capability);
            match note {
                Some(text) => Task::batch([toast(text), reread]),
                None => reread,
            }
        }
    }
}

fn helper_text(ctx: &Ctx) -> String {
    ctx.t(ctx
        .helper
        .blocker()
        .unwrap_or(crate::gui::REOPEN_TO_DO_THIS))
}

fn capability_label(ctx: &Ctx, capability: Capability) -> String {
    ctx.t(match capability {
        Capability::Camera => "Camera",
        Capability::Microphone => "Microphone",
        Capability::Location => "Location",
    })
}

fn recency_text(ctx: &Ctx, recency: Recency) -> String {
    let count = |one: &str, many: &str, n: u64| {
        ctx.t(if n == 1 { one } else { many })
            .replace("{n}", &n.to_string())
    };
    match recency {
        Recency::InUse => ctx.t("In use now"),
        Recency::JustNow => ctx.t("Used just now"),
        Recency::Minutes(n) => count("Used {n} minute ago", "Used {n} minutes ago", n),
        Recency::Hours(n) => count("Used {n} hour ago", "Used {n} hours ago", n),
        Recency::Days(n) => count("Used {n} day ago", "Used {n} days ago", n),
        Recency::Never => ctx.t("Not used yet"),
    }
}

fn master_texts(ctx: &Ctx, capability: Capability, on: bool) -> (String, String) {
    match (capability, on) {
        (Capability::Camera, true) => (
            ctx.t("Let apps use your camera"),
            ctx.t("On. Switch off any app below that you don't trust."),
        ),
        (Capability::Camera, false) => (
            ctx.t("Let apps use your camera"),
            ctx.t("Off. No app can use your camera."),
        ),
        (Capability::Microphone, true) => (
            ctx.t("Let apps use your microphone"),
            ctx.t("On. Switch off any app below that you don't trust."),
        ),
        (Capability::Microphone, false) => (
            ctx.t("Let apps use your microphone"),
            ctx.t("Off. No app can use your microphone."),
        ),
        (Capability::Location, true) => (
            ctx.t("Let apps use your location"),
            ctx.t("On. Switch off any app below that you don't trust."),
        ),
        (Capability::Location, false) => (
            ctx.t("Let apps use your location"),
            ctx.t("Off. No app can see where you are."),
        ),
    }
}

fn nothing_yet(ctx: &Ctx, capability: Capability) -> String {
    ctx.t(match capability {
        Capability::Camera => "No app has used your camera yet.",
        Capability::Microphone => "No app has used your microphone yet.",
        Capability::Location => "No app has asked for your location yet.",
    })
}

fn info_row<'a>(ctx: &Ctx, icon: Icon, text: String) -> El<'a> {
    widgets::row_item(
        ctx.palette,
        Some(icon),
        text,
        None,
        space::horizontal().width(0),
        None,
    )
}

fn app_row<'a>(
    state: &State,
    ctx: &Ctx,
    index: usize,
    entry: &Entry,
    master: bool,
    now: u64,
) -> El<'a> {
    let target = Target::App(index as u8);
    let toggle = (master && state.changing.is_none())
        .then_some(move |allow: bool| wrap(Msg::Toggle(target, allow)));
    widgets::row_item(
        ctx.palette,
        Some(Icon::Package),
        entry.name.clone(),
        Some(recency_text(ctx, entry.recency(now))),
        widgets::switch(ctx.palette, entry.allowed, toggle),
        None,
    )
}

fn desktop_row<'a>(state: &State, ctx: &Ctx, listing: &Listing, now: u64) -> El<'a> {
    let p = ctx.palette;
    let names: Vec<&str> = listing
        .desktop
        .iter()
        .take(NAMES_SHOWN)
        .map(|e| e.name.as_str())
        .collect();
    let more = listing.desktop.len().saturating_sub(NAMES_SHOWN);
    let joined = names.join(", ");
    let names = if more == 0 {
        joined
    } else {
        ctx.t("{names} and {n} more")
            .replace("{names}", &joined)
            .replace("{n}", &more.to_string())
    };
    let latest = listing
        .desktop
        .first()
        .map_or(Recency::Never, |e| e.recency(now));
    let toggle = (listing.master && state.changing.is_none())
        .then_some(|allow: bool| wrap(Msg::Toggle(Target::DesktopApps, allow)));
    widgets::row_item_below(
        p,
        Some(Icon::Apps),
        ctx.t("Desktop apps"),
        Some(format!("{} · {names}", recency_text(ctx, latest))),
        widgets::switch(p, listing.desktop_allowed, toggle),
        vec![widgets::small(
            p,
            ctx.t("Windows can only switch desktop apps off all together."),
        )],
        None,
    )
}

fn listing_rows<'a>(state: &State, ctx: &Ctx, listing: &Listing) -> Vec<El<'a>> {
    let p = ctx.palette;
    let now = crate::app::history::now();
    let capability = listing.capability;
    let (title, sub) = master_texts(ctx, capability, listing.master);
    let toggle = state
        .changing
        .is_none()
        .then_some(|allow: bool| wrap(Msg::Toggle(Target::Master, allow)));
    let mut rows: Vec<El<'a>> = vec![widgets::row_item(
        p,
        Some(Icon::Lock),
        title,
        Some(sub),
        widgets::switch(p, listing.master, toggle),
        None,
    )];
    if listing.is_empty() {
        rows.push(info_row(ctx, Icon::Info, nothing_yet(ctx, capability)));
        return rows;
    }
    if !listing.apps.is_empty() {
        rows.push(
            container(widgets::section_label(
                p,
                ctx.t("Apps from the Microsoft Store"),
            ))
            .padding([theme::S2, theme::S4])
            .into(),
        );
        for (index, entry) in listing.apps.iter().enumerate() {
            rows.push(app_row(state, ctx, index, entry, listing.master, now));
        }
    }
    if !listing.desktop.is_empty() {
        rows.push(desktop_row(state, ctx, listing, now));
    }
    rows
}

#[allow(dead_code)]
pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> El<'a> {
    let p = ctx.palette;
    let capability = state.capability;
    let tabs = container(widgets::segmented(
        p,
        &Capability::ALL.map(|c| (c, capability_label(ctx, c))),
        capability,
        |c| wrap(Msg::Select(c)),
    ))
    .padding([0.0, theme::S4]);
    let mut rows: Vec<El<'a>> = vec![tabs.into()];
    if ctx.helper != Helper::Ready {
        rows.push(info_row(ctx, Icon::Info, helper_text(ctx)));
    } else {
        match (
            &state.listings[slot(capability)],
            state.load[slot(capability)],
        ) {
            (Some(listing), _) => rows.extend(listing_rows(state, ctx, listing)),
            (None, Load::Failed) => rows.push(widgets::row_item(
                p,
                Some(Icon::AlertTriangle),
                ctx.t("We couldn't read which apps used this. Please try again."),
                None,
                widgets::action(
                    p,
                    widgets::ButtonKind::Secondary,
                    ctx.t("Try again"),
                    None,
                    Some(wrap(Msg::Retry)),
                ),
                None,
            )),
            (None, _) => rows.push(info_row(ctx, Icon::Refresh, ctx.t("Checking…"))),
        }
    }
    column![widgets::group(
        p,
        ctx.t("Camera, microphone and location"),
        Some(ctx.t("See which apps used them and when. Switch off any you don't trust.")),
        None,
        rows,
    )]
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
pub fn shown_as(state: &State, capability: Capability) -> &'static str {
    match (
        &state.listings[slot(capability)],
        state.load[slot(capability)],
    ) {
        (Some(_), Load::Loading) => "reloading",
        (Some(_), _) => "listed",
        (None, Load::Failed) => "failed",
        (None, Load::Loading) => "loading",
        (None, Load::Idle) => "idle",
    }
}

#[cfg(test)]
pub fn is_changing(state: &State) -> bool {
    state.changing.is_some()
}
