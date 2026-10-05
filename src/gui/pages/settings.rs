//! Settings page: appearance, language, background protection, updates, about.
//!
//! The shell should call [`on_enter`] when this page opens so the live values
//! (background checks, update status) are read off the UI thread. Until then
//! the page shows calm "Checking…" labels with a Refresh button.
use crate::app::settings::{self as prefs_store, ThemeChoice};
use crate::gui::icons::Icon;
use crate::gui::theme::{self, Mode, Palette, Tone};
use crate::gui::widgets::{self, anim, ButtonKind};
use crate::gui::{blocking, Ctx, Message};
use crate::i18n::Lang;
use iced::widget::{column, container, row, space};
use iced::{Alignment, Background, Border, Element, Length, Subscription, Task};

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
    /// Drives the spinner; only ticks while something is busy.
    clock: anim::Clock,
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
            clock: anim::Clock::new(),
        }
    }
}

impl State {
    #[allow(dead_code)] // used by `subscription` once the shell batches it
    fn busy(&self) -> bool {
        self.working
            || matches!(self.background, Remote::Loading)
            || matches!(self.update, Remote::Loading)
    }
}

/// Frame ticks, only while a spinner is showing (and motion is allowed).
/// The shell batches this into its subscriptions.
#[allow(dead_code)] // wired by the shell (src/gui/mod.rs, not this file)
pub fn subscription(state: &State) -> Subscription<Message> {
    if state.busy() && !anim::reduced() {
        iced::window::frames().map(|_| Message::Settings(Msg::Frame))
    } else {
        Subscription::none()
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
    /// Animation frame; the redraw is the whole job.
    #[allow(dead_code)]
    Frame,
}

/// Call when the page opens.
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
        Msg::Frame => Task::none(),
        Msg::Load => {
            state.clock.restart();
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
            state.clock.restart();
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
                    blocking(move || {
                        prefs_store::set_tray_enabled(on).map_err(|e| format!("{e:#}"))
                    }),
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
                            "Secblitz now shows in the system tray."
                        } else {
                            "Secblitz no longer shows in the system tray."
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

/// Row height: ROW plus S2, so single and two line rows align everywhere.
const SETTING_ROW: f32 = theme::ROW + theme::S2;
const CONTROL_WIDTH: f32 = 200.0;

fn divider<'a>(p: Palette) -> Element<'a, Message> {
    container(space::vertical())
        .width(Length::Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.border)),
            ..container::Style::default()
        })
        .into()
}

/// One 56 px settings row: text on the left, control on the right.
fn item<'a>(
    p: Palette,
    title: String,
    hint: Option<String>,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut label = column![widgets::body(p, title)].spacing(theme::S1);
    if let Some(hint) = hint {
        label = label.push(widgets::small(p, hint));
    }
    container(
        row![label.width(Length::Fill), control]
            .spacing(theme::S4)
            .align_y(Alignment::Center),
    )
    .height(Length::Fixed(SETTING_ROW))
    .align_y(Alignment::Center)
    .into()
}

/// Section caption above a card whose rows are separated by dividers.
fn section<'a>(p: Palette, title: String, rows: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut body = column![];
    for (i, r) in rows.into_iter().enumerate() {
        if i > 0 {
            body = body.push(divider(p));
        }
        body = body.push(r);
    }
    column![
        widgets::section_label(p, title),
        container(body)
            .padding([0.0, theme::S6])
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.surface)),
                border: Border {
                    radius: theme::R_LARGE.into(),
                    width: 1.0,
                    color: p.border,
                },
                text_color: Some(p.text),
                ..container::Style::default()
            }),
    ]
    .spacing(theme::S3)
    .into()
}

/// Spinner plus a short word, for anything with an unknown wait.
fn busy<'a>(p: Palette, state: &State, label: String) -> Element<'a, Message> {
    row![
        anim::spinner(18.0, p.text_muted, state.clock.elapsed()),
        widgets::muted(p, label),
    ]
    .spacing(theme::S2)
    .align_y(Alignment::Center)
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
            "Show Secblitz in the system tray?",
            "A small shield will appear in the bottom-right corner of your screen when you sign in. It shows whether your PC looks safe.",
            "Turn on",
        ),
        Confirm::Tray(false) => (
            "Hide Secblitz from the system tray?",
            "The small shield will no longer appear when you sign in. Nothing else changes.",
            "Turn off",
        ),
    };
    let body = column![
        widgets::body(p, ctx.t(title)),
        widgets::muted(p, ctx.t(text_key)),
        row![
            space::horizontal(),
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
        .spacing(theme::S2),
    ]
    .spacing(theme::S3);
    container(body)
        .padding(theme::S4)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.surface_alt)),
            border: Border {
                radius: theme::R.into(),
                width: 1.0,
                color: p.border,
            },
            ..container::Style::default()
        })
        .into()
}

/// A confirmation sits inside its section, spaced like the rows.
fn confirm_row<'a>(p: Palette, ctx: &Ctx, confirm: Confirm) -> Element<'a, Message> {
    container(confirm_panel(p, ctx, confirm))
        .padding([theme::S3, 0.0])
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

    // Appearance: theme and language
    let theme_options = [
        (ThemeChoice::Light, t("Light")),
        (ThemeChoice::Dark, t("Dark")),
    ];
    let selected_lang = LangItem::ALL.iter().find(|l| l.0 == ctx.lang);
    let appearance = section(
        p,
        t("Appearance"),
        vec![
            item(
                p,
                t("Theme"),
                Some(t("Light or dark, whichever is easier on your eyes.")),
                widgets::segmented(p, &theme_options, ctx.prefs.theme, |v| {
                    Message::Settings(Msg::SetTheme(v))
                }),
            ),
            item(
                p,
                t("Language"),
                Some(t("Changes right away.")),
                container(widgets::dropdown(
                    p,
                    &LangItem::ALL[..],
                    selected_lang,
                    t("Choose a language"),
                    |l| Message::Settings(Msg::SetLang(l)),
                ))
                .width(CONTROL_WIDTH)
                .into(),
            ),
        ],
    );

    // Background protection
    let background_control: Element<'a, Message> = if state.working {
        busy(p, state, t("Working"))
    } else {
        match &state.background {
            Remote::Loading => busy(p, state, t("Checking…")),
            Remote::Failed => row![
                widgets::pill(p, t("Couldn't check"), Tone::Warn),
                refresh(p, ctx)
            ]
            .spacing(theme::S2)
            .align_y(Alignment::Center)
            .into(),
            Remote::Ready(on) => widgets::switch(p, *on, Some(|v| Message::Settings(Msg::Ask(v)))),
        }
    };
    let mut rows = vec![item(
        p,
        t("Check my PC automatically"),
        Some(t(
            "Looks for problems in the background and tells you if something changes.",
        )),
        background_control,
    )];
    if let Some(c @ Confirm::Background(_)) = state.confirm {
        rows.push(confirm_row(p, ctx, c));
    }
    let tray_toggle = (state.installed && !state.working)
        .then_some(|v| Message::Settings(Msg::AskTray(v)));
    rows.push(item(
        p,
        t("Show Secblitz in the system tray (bottom-right corner)"),
        Some(if state.installed {
            t("A small shield tells you at a glance if your PC looks safe.")
        } else {
            t("Available once Secblitz is installed.")
        }),
        widgets::switch(p, state.tray, tray_toggle),
    ));
    if let Some(c @ Confirm::Tray(_)) = state.confirm {
        rows.push(confirm_row(p, ctx, c));
    }
    let protection = section(p, t("Background protection"), rows);

    // Updates
    let (status, line): (Element<'a, Message>, String) = match &state.update {
        Remote::Loading => (busy(p, state, t("Checking…")), t("Looking for updates.")),
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
        vec![item(
            p,
            line,
            Some(t("Secblitz updates itself. You don't need to do anything.")),
            status,
        )],
    );

    // About
    let mut about = column![
        row![
            widgets::icon_filled(Icon::ShieldCheck, 28.0, p.text),
            column![
                widgets::h2(p, "Secblitz"),
                widgets::small(p, format!("{} {}", t("Version"), env!("CARGO_PKG_VERSION"))),
            ]
            .spacing(theme::S1)
        ]
        .spacing(theme::S3)
        .align_y(Alignment::Center),
        widgets::muted(p, t("A safer PC. Without headaches.")),
        widgets::small(p, t("Fonts: IBM Plex Sans (SIL Open Font License).")),
        widgets::small(p, t("Icons: Fluent UI System Icons (MIT licence).")),
        widgets::small(
            p,
            t("App clean-up lists draw on the Win11Debloat and WinUtil projects (MIT licence).")
        ),
    ]
    .spacing(theme::S2);
    let mut details = column![
        widgets::small(
            p,
            format!(
                "{}: {}",
                t("Data folder"),
                ctx.state_dir
                    .as_ref()
                    .map(|d| d.display().to_string())
                    .unwrap_or_else(|| "-".into())
            )
        ),
        widgets::small(
            p,
            format!(
                "{}: {}",
                t("Protection checks available"),
                ctx.catalog.available.len()
            )
        ),
    ]
    .spacing(theme::S1);
    if let Some(error) = &ctx.engine_error {
        details = details.push(widgets::small(p, format!("{}: {error}", t("Last problem"))));
    }
    about = about.push(widgets::expander(
        p,
        t("Technical details"),
        state.technical,
        Message::Settings(Msg::ToggleTechnical),
        details,
    ));
    let about = column![
        widgets::section_label(p, t("About")),
        widgets::card(p, about),
    ]
    .spacing(theme::S3);

    column![
        widgets::page_header(p, t("Settings"), Some(t("Make Secblitz work your way."))),
        space::vertical().height(theme::S2),
        appearance,
        protection,
        updates,
        about,
    ]
    .spacing(theme::S6)
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
            update_view(&status(
                O::Installed {
                    version: "1.0.0".into()
                },
                5
            )),
            UpdateView::UpToDate
        );
        assert_eq!(
            update_view(&status(
                O::WorkerStarted {
                    version: "1.0.0".into()
                },
                5
            )),
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
        assert_eq!(
            names,
            [
                "English",
                "Español",
                "Français",
                "Deutsch",
                "Português",
                "Italiano"
            ]
        );
        let codes: Vec<_> = LangItem::ALL.iter().map(|l| l.0.code()).collect();
        assert_eq!(codes, ["en", "es", "fr", "de", "pt", "it"]);
    }
}
