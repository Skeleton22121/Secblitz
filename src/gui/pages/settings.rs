//! Settings page: appearance, language, background protection, updates, about.
//!
//! The shell should call [`on_enter`] when this page opens so the live values
//! (background checks, update status) are read off the UI thread. Until then
//! the page shows calm "Checking…" labels with a Refresh button.
use crate::app::settings::{self as prefs_store, ThemeChoice};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Mode, Palette, Tone};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{blocking, Ctx, Message};
use crate::i18n::Lang;
use iced::widget::{button, column, container, pick_list, row, text, toggler};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

/// A value that is read off the UI thread.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Remote<T> {
    Loading,
    Ready(T),
    Failed,
}

/// Update status in plain words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateView {
    UpToDate,
    Ready,
    Unknown,
    /// This copy has no update source (for example a portable build).
    Off,
}

pub fn update_view(status: &secblitz::updater::UpdateStatus) -> UpdateView {
    use secblitz::updater::UpdateOutcome as O;
    match &status.result {
        O::UpToDate | O::Installed { .. } | O::DeferredRollout { .. } => UpdateView::UpToDate,
        O::WorkerStarted { .. } | O::DeferredBusy => UpdateView::Ready,
        O::Failed { .. } => UpdateView::Unknown,
        O::NotConfigured if status.checked_at == 0 => UpdateView::Off,
        O::NotConfigured => UpdateView::Off,
    }
}

/// A change waiting for the person's yes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confirm {
    Background(bool),
    Tray(bool),
}

#[derive(Debug)]
pub struct State {
    background: Remote<bool>,
    update: Remote<UpdateView>,
    confirm: Option<Confirm>,
    working: bool,
    tray: bool,
    /// Running from the installed location (the only place a logon entry
    /// may point at).
    installed: bool,
    technical: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            background: Remote::Loading,
            update: Remote::Loading,
            confirm: None,
            working: false,
            // Cheap local reads; everything slower goes through `Load`.
            tray: prefs_store::tray_enabled(),
            installed: prefs_store::installed_exe().is_some(),
            technical: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LangItem(Lang);

impl LangItem {
    const ALL: [LangItem; 6] = [
        LangItem(Lang::En),
        LangItem(Lang::Es),
        LangItem(Lang::Fr),
        LangItem(Lang::De),
        LangItem(Lang::Pt),
        LangItem(Lang::It),
    ];
}

impl std::fmt::Display for LangItem {
    /// Each language is named in itself so anyone can find theirs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::Fr => "Français",
            Lang::De => "Deutsch",
            Lang::Pt => "Português",
            Lang::It => "Italiano",
        })
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// (Re)read background protection and update status.
    Load,
    Loaded {
        background: Result<bool, String>,
        update: Option<UpdateView>,
    },
    SetTheme(ThemeChoice),
    SetLang(LangItem),
    Ask(bool),
    AskTray(bool),
    CancelConfirm,
    Confirmed,
    BackgroundDone(bool, Result<(), String>),
    TrayDone(bool, Result<(), String>),
    ToggleTechnical,
}

/// Call when the page opens.
#[allow(dead_code)]
pub fn on_enter(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    update(state, Msg::Load, ctx)
}

fn toast(text: &str, tone: Tone, ctx: &Ctx) -> Task<Message> {
    Task::done(Message::Toast(ctx.t(text), tone))
}

fn save_prefs(ctx: &Ctx) -> Task<Message> {
    match prefs_store::save(&ctx.prefs) {
        Ok(()) => Task::none(),
        Err(_) => toast("We couldn't save that choice.", Tone::Warn, ctx),
    }
}

pub fn update(state: &mut State, msg: Msg, ctx: &mut Ctx) -> Task<Message> {
    match msg {
        Msg::Load => {
            state.background = Remote::Loading;
            state.update = Remote::Loading;
            Task::perform(
                blocking(|| {
                    let background = prefs_store::background_on().map_err(|e| format!("{e:#}"));
                    let update = secblitz::updater::status().ok().map(|s| update_view(&s));
                    (background, update)
                }),
                |(background, update)| Message::Settings(Msg::Loaded { background, update }),
            )
        }
        Msg::Loaded { background, update } => {
            state.background = match background {
                Ok(on) => Remote::Ready(on),
                Err(_) => Remote::Failed,
            };
            state.update = match update {
                Some(view) => Remote::Ready(view),
                None => Remote::Failed,
            };
            Task::none()
        }
        Msg::SetTheme(choice) => {
            ctx.prefs.theme = choice;
            ctx.palette = Palette::of(match choice {
                ThemeChoice::Light => Mode::Light,
                ThemeChoice::Dark => Mode::Dark,
            });
            save_prefs(ctx)
        }
        Msg::SetLang(LangItem(lang)) => {
            ctx.lang = lang;
            ctx.prefs.lang = Some(lang.code().to_owned());
            save_prefs(ctx)
        }
        Msg::Ask(on) => {
            if !state.working {
                state.confirm = Some(Confirm::Background(on));
            }
            Task::none()
        }
        Msg::AskTray(on) => {
            if !state.working && state.installed {
                state.confirm = Some(Confirm::Tray(on));
            }
            Task::none()
        }
        Msg::CancelConfirm => {
            state.confirm = None;
            Task::none()
        }
        Msg::Confirmed => {
            let Some(confirm) = state.confirm.take() else {
                return Task::none();
            };
            if state.working {
                return Task::none();
            }
            state.working = true;
            match confirm {
                Confirm::Background(on) => Task::perform(
                    blocking(move || {
                        if on {
                            prefs_store::enable_background()
                        } else {
                            prefs_store::disable_background()
                        }
                        .map_err(|e| format!("{e:#}"))
                    }),
                    move |result| Message::Settings(Msg::BackgroundDone(on, result)),
                ),
                Confirm::Tray(on) => Task::perform(
                    blocking(move || prefs_store::set_tray_enabled(on).map_err(|e| format!("{e:#}"))),
                    move |result| Message::Settings(Msg::TrayDone(on, result)),
                ),
            }
        }
        Msg::BackgroundDone(on, result) => {
            state.working = false;
            match result {
                Ok(()) => {
                    state.background = Remote::Ready(on);
                    toast(
                        if on {
                            "Automatic checks are on."
                        } else {
                            "Automatic checks are off."
                        },
                        Tone::Good,
                        ctx,
                    )
                }
                Err(_) => toast(
                    "We couldn't change that. Nothing was changed on your PC.",
                    Tone::Warn,
                    ctx,
                ),
            }
        }
        Msg::TrayDone(on, result) => {
            state.working = false;
            match result {
                Ok(()) => {
                    state.tray = on;
                    toast(
                        if on {
                            "Secblitz will show next to the clock."
                        } else {
                            "Secblitz will no longer show next to the clock."
                        },
                        Tone::Good,
                        ctx,
                    )
                }
                Err(_) => toast(
                    "We couldn't change that. Nothing was changed on your PC.",
                    Tone::Warn,
                    ctx,
                ),
            }
        }
        Msg::ToggleTechnical => {
            state.technical = !state.technical;
            Task::none()
        }
    }
}

// ----- view -----

fn divider<'a>(p: Palette) -> Element<'a, Message> {
    container(column![])
        .width(Length::Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.border)),
            ..container::Style::default()
        })
        .into()
}

fn item<'a>(
    p: Palette,
    title: String,
    hint: Option<String>,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut label = column![text(title).size(theme::BODY).font(theme::MEDIUM).color(p.text)].spacing(2);
    if let Some(hint) = hint {
        label = label.push(widgets::small(p, hint));
    }
    row![label.width(Length::Fill), control]
        .spacing(theme::GAP)
        .align_y(Alignment::Center)
        .into()
}

fn section<'a>(
    p: Palette,
    title: String,
    rows: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut body = column![widgets::h2(p, title)].spacing(theme::GAP);
    for (i, r) in rows.into_iter().enumerate() {
        if i > 0 {
            body = body.push(divider(p));
        }
        body = body.push(r);
    }
    widgets::card(p, body).into()
}

fn switch<'a>(on: bool, msg: impl Fn(bool) -> Msg + 'a, enabled: bool) -> Element<'a, Message> {
    toggler(on)
        .size(24)
        .on_toggle_maybe(enabled.then_some(move |v| Message::Settings(msg(v))))
        .into()
}

fn segmented<'a>(p: Palette, ctx: &Ctx) -> Element<'a, Message> {
    let current = ctx.prefs.theme;
    let choice = |label: String, icon: Icon, value: ThemeChoice| {
        let active = current == value;
        let (fg, bg) = if active {
            (p.on_brand, p.brand)
        } else {
            (p.text, Color::TRANSPARENT)
        };
        button(
            row![
                widgets::icon(icon, 15.0, fg),
                text(label).size(theme::BODY).font(theme::MEDIUM)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([7, 16])
        .on_press(Message::Settings(Msg::SetTheme(value)))
        .style(move |_, status| button::Style {
            background: Some(Background::Color(if !active && status == button::Status::Hovered {
                p.surface
            } else {
                bg
            })),
            text_color: fg,
            border: Border {
                radius: theme::RADIUS_SMALL.into(),
                ..Border::default()
            },
            ..button::Style::default()
        })
    };
    container(row![
        choice(ctx.t("Light"), Icon::Sun, ThemeChoice::Light),
        choice(ctx.t("Dark"), Icon::Moon, ThemeChoice::Dark),
    ]
    .spacing(2))
    .padding(3)
    .style(move |_| container::Style {
        background: Some(Background::Color(p.surface_alt)),
        border: Border {
            radius: theme::RADIUS_SMALL.into(),
            width: 1.0,
            color: p.border,
        },
        ..container::Style::default()
    })
    .into()
}

fn confirm_panel<'a>(p: Palette, ctx: &Ctx, confirm: Confirm) -> Element<'a, Message> {
    let (title, text_key, yes) = match confirm {
        Confirm::Background(true) => (
            "Check my PC automatically?",
            "Secblitz checks your PC every 15 minutes in the background. It never changes anything on its own.",
            "Turn on",
        ),
        Confirm::Background(false) => (
            "Stop checking automatically?",
            "Secblitz will stop checking your PC in the background. You can still check any time from Home. Nothing on your PC will change.",
            "Turn off",
        ),
        Confirm::Tray(true) => (
            "Show Secblitz next to the clock?",
            "A small shield will appear next to the clock when you sign in. It shows whether your PC looks safe.",
            "Turn on",
        ),
        Confirm::Tray(false) => (
            "Hide Secblitz next to the clock?",
            "The small shield will no longer appear when you sign in. Nothing else changes.",
            "Turn off",
        ),
    };
    let body = column![
        text(ctx.t(title)).size(theme::BODY).font(theme::SEMIBOLD).color(p.text),
        widgets::muted(p, ctx.t(text_key)),
        row![
            widgets::action(
                p,
                ButtonKind::Secondary,
                ctx.t("Cancel"),
                None,
                Some(Message::Settings(Msg::CancelConfirm))
            ),
            widgets::action(
                p,
                ButtonKind::Primary,
                ctx.t(yes),
                None,
                Some(Message::Settings(Msg::Confirmed))
            ),
        ]
        .spacing(10),
    ]
    .spacing(12);
    container(body)
        .padding(theme::PAD)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface_alt)),
            border: Border {
                radius: theme::RADIUS_SMALL.into(),
                width: 1.0,
                color: p.border,
            },
            ..container::Style::default()
        })
        .into()
}

fn refresh<'a>(p: Palette, ctx: &Ctx) -> Element<'a, Message> {
    widgets::action(
        p,
        ButtonKind::Ghost,
        ctx.t("Try again"),
        Some(Icon::Refresh),
        Some(Message::Settings(Msg::Load)),
    )
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    let p = ctx.palette;
    let t = |s: &str| ctx.t(s);

    // Appearance
    let appearance = section(
        p,
        t("Appearance"),
        vec![item(p, t("Theme"), Some(t("Light or dark, whichever is easier on your eyes.")), segmented(p, ctx))],
    );

    // Language
    let selected = LangItem(ctx.lang);
    let language = section(
        p,
        t("Language"),
        vec![item(
            p,
            t("Language"),
            Some(t("Changes right away.")),
            pick_list(&LangItem::ALL[..], Some(selected), |l| {
                Message::Settings(Msg::SetLang(l))
            })
            .width(200)
            .text_size(theme::BODY)
            .into(),
        )],
    );

    // Background protection
    let background_control: Element<'a, Message> = if state.working {
        widgets::pill(p, t("Working…"), Tone::Neutral)
    } else {
        match &state.background {
            Remote::Loading => widgets::pill(p, t("Checking…"), Tone::Neutral),
            Remote::Failed => row![widgets::pill(p, t("Couldn't check"), Tone::Warn), refresh(p, ctx)]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            Remote::Ready(on) => switch(*on, Msg::Ask, true),
        }
    };
    let mut rows = vec![item(
        p,
        t("Check my PC automatically"),
        Some(t("Looks for problems in the background and tells you if something changes.")),
        background_control,
    )];
    if let Some(Confirm::Background(_)) = state.confirm {
        rows.push(confirm_panel(p, ctx, state.confirm.unwrap_or(Confirm::Background(true))));
    }
    let tray_control = switch(state.tray, Msg::AskTray, state.installed && !state.working);
    rows.push(item(
        p,
        t("Show Secblitz next to the clock"),
        Some(if state.installed {
            t("A small shield tells you at a glance if your PC looks safe.")
        } else {
            t("Available once Secblitz is installed.")
        }),
        tray_control,
    ));
    if let Some(Confirm::Tray(_)) = state.confirm {
        rows.push(confirm_panel(p, ctx, state.confirm.unwrap_or(Confirm::Tray(true))));
    }
    let protection = section(p, t("Background protection"), rows);

    // Updates
    let (pill, line) = match &state.update {
        Remote::Loading => (widgets::pill(p, t("Checking…"), Tone::Neutral), t("Looking for updates.")),
        Remote::Failed | Remote::Ready(UpdateView::Unknown) => (
            widgets::pill(p, t("Couldn't check"), Tone::Warn),
            t("We couldn't check for updates. We'll try again later."),
        ),
        Remote::Ready(UpdateView::UpToDate) => (
            widgets::pill(p, t("Up to date"), Tone::Good),
            t("You have the latest version of Secblitz."),
        ),
        Remote::Ready(UpdateView::Ready) => (
            widgets::pill(p, t("Update ready"), Tone::Warn),
            t("An update is ready. It installs when Secblitz is closed."),
        ),
        Remote::Ready(UpdateView::Off) => (
            widgets::pill(p, t("Not set up"), Tone::Neutral),
            t("Automatic updates aren't turned on for this copy of Secblitz."),
        ),
    };
    let updates = section(
        p,
        t("Updates"),
        vec![item(p, line, Some(t("Secblitz updates itself. You don't need to do anything.")), pill)],
    );

    // About
    let chevron = if state.technical { Icon::ChevronDown } else { Icon::ChevronRight };
    let mut about = column![
        row![
            widgets::icon(Icon::ShieldCheck, 28.0, p.brand),
            column![
                text("Secblitz").size(theme::H2).font(theme::BOLD).color(p.text),
                widgets::small(p, format!("{} {}", t("Version"), env!("CARGO_PKG_VERSION"))),
            ]
            .spacing(2)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        widgets::muted(p, t("A safer PC. Without headaches.")),
        widgets::small(
            p,
            t("Secblitz uses the Inter font (SIL Open Font License) and Lucide icons (ISC licence). Its app clean-up lists draw on the Win11Debloat and WinUtil projects (MIT licence).")
        ),
        button(
            row![
                widgets::icon(chevron, 14.0, p.text_muted),
                text(t("Technical details")).size(theme::SMALL).font(theme::MEDIUM).color(p.text_muted)
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        )
        .padding([4, 0])
        .on_press(Message::Settings(Msg::ToggleTechnical))
        .style(|_, _| button::Style::default()),
    ]
    .spacing(10);
    if state.technical {
        let folder = ctx
            .state_dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| "-".into());
        let mut details = column![
            widgets::small(p, format!("{}: {folder}", t("Data folder"))),
            widgets::small(
                p,
                format!("{}: {}", t("Protection checks available"), ctx.catalog.available.len())
            ),
            widgets::small(p, format!("{}: {}", t("Version"), env!("CARGO_PKG_VERSION"))),
        ]
        .spacing(4);
        if let Some(error) = &ctx.engine_error {
            details = details.push(widgets::small(p, format!("{}: {error}", t("Last problem"))));
        }
        about = about.push(
            container(details)
                .padding(12)
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
    let about = widgets::card(p, column![widgets::h2(p, t("About")), about].spacing(theme::GAP));

    column![
        widgets::page_header(p, t("Settings"), Some(t("Make Secblitz work your way."))),
        appearance,
        language,
        protection,
        updates,
        about,
    ]
    .spacing(theme::GAP)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use secblitz::updater::{UpdateOutcome as O, UpdateStatus};

    fn status(result: O, checked_at: u64) -> UpdateStatus {
        UpdateStatus { checked_at, result }
    }

    #[test]
    fn update_status_is_shown_in_three_plain_states() {
        assert_eq!(update_view(&status(O::UpToDate, 5)), UpdateView::UpToDate);
        assert_eq!(
            update_view(&status(O::Installed { version: "1.0.0".into() }, 5)),
            UpdateView::UpToDate
        );
        assert_eq!(
            update_view(&status(O::WorkerStarted { version: "1.0.0".into() }, 5)),
            UpdateView::Ready
        );
        assert_eq!(update_view(&status(O::DeferredBusy, 5)), UpdateView::Ready);
        assert_eq!(
            update_view(&status(O::Failed { reason: "x".into() }, 5)),
            UpdateView::Unknown
        );
        assert_eq!(update_view(&status(O::NotConfigured, 0)), UpdateView::Off);
    }

    #[test]
    fn all_six_languages_are_offered_with_native_names() {
        let names: Vec<String> = LangItem::ALL.iter().map(ToString::to_string).collect();
        assert_eq!(names, ["English", "Español", "Français", "Deutsch", "Português", "Italiano"]);
        let codes: Vec<_> = LangItem::ALL.iter().map(|l| l.0.code()).collect();
        assert_eq!(codes, ["en", "es", "fr", "de", "pt", "it"]);
    }
}
