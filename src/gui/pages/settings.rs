//! Settings page: appearance, language, background protection, updates, about.
//!
//! The shell should call [`on_enter`] when this page opens so the live values
//! (background checks, update status) are read off the UI thread. Until then
//! the page shows calm "Checking…" labels with a Refresh button.
use crate::app::settings::{self as prefs_store, ThemeChoice};
use crate::gui::icons::Icon;
use crate::gui::pages::remove;
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
    /// A check is running right now.
    Checking,
    Unknown,
    /// This copy has no update source (for example a portable build).
    Off,
}

pub fn update_view(status: &secblitz::updater::UpdateStatus) -> UpdateView {
    use secblitz::updater::UpdateOutcome as O;
    match &status.result {
        O::UpToDate | O::Installed { .. } | O::DeferredRollout { .. } => UpdateView::UpToDate,
        // `checked_at == 0`: the updater holds its lock (checking now).
        O::DeferredBusy if status.checked_at == 0 => UpdateView::Checking,
        O::WorkerStarted { .. } | O::DeferredBusy => UpdateView::Ready,
        O::Failed { .. } => UpdateView::Unknown,
        O::NotConfigured if status.checked_at == 0 => UpdateView::Off,
        O::NotConfigured => UpdateView::Off,
    }
}

/// What the Updates row shows. Only the installed copy can update itself
/// (the updater refuses any other path), so a copy run from anywhere else
/// says updates aren't set up instead of "couldn't check" forever.
fn shown_update(installed: bool, update: Option<UpdateView>) -> Remote<UpdateView> {
    match update {
        _ if !installed => Remote::Ready(UpdateView::Off),
        Some(view) => Remote::Ready(view),
        None => Remote::Failed,
    }
}

/// Whether the tray switch reads as on. Only the installed copy can show a
/// tray icon, so any other copy shows the switch off whatever the saved choice.
fn shown_tray(installed: bool, tray: bool) -> bool {
    installed && tray
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
    /// Bumped by every load and change; a read from an older one is dropped.
    generation: u32,
    tray: bool,
    /// Running from the installed location (the only place a logon entry
    /// may point at).
    installed: bool,
    /// The About group is open.
    technical: bool,
    /// Drives the spinner; only ticks while something is busy.
    clock: anim::Clock,
    /// The Remove Secblitz sheet.
    remove: remove::State,
}

impl Default for State {
    fn default() -> Self {
        State {
            background: Remote::Loading,
            update: Remote::Loading,
            confirm: None,
            working: false,
            generation: 0,
            // Cheap local reads; everything slower goes through `Load`.
            tray: prefs_store::tray_enabled(),
            installed: prefs_store::installed_exe().is_some(),
            technical: false,
            clock: anim::Clock::new(),
            remove: remove::State::default(),
        }
    }
}

impl State {
    fn busy(&self) -> bool {
        self.working
            || matches!(self.background, Remote::Loading)
            || matches!(self.update, Remote::Loading)
    }
}

/// Frame ticks, only while a spinner is showing (and motion is allowed).
/// The shell batches this into its subscriptions.
pub fn subscription(state: &State) -> Subscription<Message> {
    let spinner = if state.busy() && !anim::reduced() {
        iced::window::frames().map(|_| Message::Settings(Msg::Frame))
    } else {
        Subscription::none()
    };
    Subscription::batch([spinner, remove::subscription(&state.remove)])
}

/// The Remove Secblitz sheet, drawn by the shell above the whole window.
pub fn modal<'a>(state: &'a State, ctx: &'a Ctx) -> Option<Element<'a, Message>> {
    remove::modal(&state.remove, ctx)
}

/// Escape closes the Remove Secblitz sheet when no work is running.
pub fn escape(state: &mut State) {
    remove::escape(&mut state.remove);
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
        generation: u32,
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
    Frame,
    Remove(remove::Msg),
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
        Msg::Remove(m) => remove::update(&mut state.remove, m, ctx),
        Msg::Load => {
            state.clock.restart();
            state.background = Remote::Loading;
            state.update = Remote::Loading;
            load_task(state)
        }
        Msg::Loaded {
            generation,
            background,
            update,
        } => {
            // A read that started before a change (or lands mid-change) is stale.
            if generation != state.generation || state.working {
                return Task::none();
            }
            state.background = match background {
                Ok(on) => Remote::Ready(on),
                Err(_) => Remote::Failed,
            };
            state.update = shown_update(state.installed, update);
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
            state.generation = state.generation.wrapping_add(1);
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
            state.generation = state.generation.wrapping_add(1);
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
            state.generation = state.generation.wrapping_add(1);
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

const CONTROL_WIDTH: f32 = 168.0;

/// Spinner plus a short word, for anything with an unknown wait.
fn busy<'a>(p: Palette, state: &State, label: String) -> Element<'a, Message> {
    row![
        anim::spinner(16.0, p.text_muted, state.clock.elapsed()),
        widgets::muted(p, label),
    ]
    .spacing(theme::S2)
    .align_y(Alignment::Center)
    .into()
}

fn confirm_text(confirm: Confirm) -> (&'static str, &'static str, &'static str) {
    match confirm {
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
    }
}

/// A confirmation sits between rows as a quiet tonal block.
fn confirm_row<'a>(p: Palette, ctx: &Ctx, confirm: Confirm) -> Element<'a, Message> {
    let (title, text_key, yes) = confirm_text(confirm);
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
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// Icon-only "more" menu with a single "Try again" / "Check now" entry.
fn reload_menu<'a>(p: Palette, label: String) -> Element<'a, Message> {
    widgets::overflow_menu(
        p,
        vec![(Icon::Refresh, label, Message::Settings(Msg::Load), false)],
    )
}

pub fn view<'a>(state: &'a State, ctx: &'a Ctx) -> Element<'a, Message> {
    // `ctx.palette` is a faded copy during the page entrance.
    let p = Palette::of(ctx.palette.mode);
    let t = |s: &str| ctx.t(s);

    // Appearance
    let theme_options = [
        (ThemeChoice::Light, t("Light")),
        (ThemeChoice::Dark, t("Dark")),
    ];
    let selected_lang = LangItem::ALL.iter().find(|l| l.0 == ctx.lang);
    let appearance = widgets::group(
        p,
        t("Appearance"),
        None,
        None,
        vec![
            widgets::row_item(
                p,
                Some(Icon::Sun),
                t("Theme"),
                None,
                widgets::segmented(p, &theme_options, ctx.prefs.theme, |v| {
                    Message::Settings(Msg::SetTheme(v))
                }),
                None,
            ),
            widgets::row_item(
                p,
                Some(Icon::Globe),
                t("Language"),
                None,
                container(widgets::dropdown(
                    p,
                    &LangItem::ALL[..],
                    selected_lang,
                    t("Choose a language"),
                    |l| Message::Settings(Msg::SetLang(l)),
                ))
                .width(CONTROL_WIDTH),
                None,
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
                reload_menu(p, t("Try again")),
            ]
            .spacing(theme::S2)
            .align_y(Alignment::Center)
            .into(),
            Remote::Ready(on) => widgets::switch(p, *on, Some(|v| Message::Settings(Msg::Ask(v)))),
        }
    };
    let mut rows = vec![widgets::row_item(
        p,
        Some(Icon::Scan),
        t("Check my PC automatically"),
        Some(t(
            "Looks for problems in the background and tells you if something changes.",
        )),
        background_control,
        None,
    )];
    if let Some(c @ Confirm::Background(_)) = state.confirm {
        rows.push(confirm_row(p, ctx, c));
    }
    let tray_toggle =
        (state.installed && !state.working).then_some(|v| Message::Settings(Msg::AskTray(v)));
    rows.push(widgets::row_item(
        p,
        Some(Icon::Bell),
        t("System tray icon"),
        Some(if state.installed {
            t("A small shield tells you at a glance if your PC looks safe.")
        } else {
            t("Available once Secblitz is installed.")
        }),
        widgets::switch(p, shown_tray(state.installed, state.tray), tray_toggle),
        None,
    ));
    if let Some(c @ Confirm::Tray(_)) = state.confirm {
        rows.push(confirm_row(p, ctx, c));
    }
    let protection = widgets::group(p, t("Background protection"), None, None, rows);

    // Updates: one compact row, details in the menu.
    let (status, line): (Element<'a, Message>, String) = match &state.update {
        Remote::Loading | Remote::Ready(UpdateView::Checking) => {
            (busy(p, state, t("Checking…")), t("Looking for updates."))
        }
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
    let mut trailing = row![status].spacing(theme::S2).align_y(Alignment::Center);
    if !matches!(
        state.update,
        Remote::Loading | Remote::Ready(UpdateView::Off | UpdateView::Checking)
    ) {
        trailing = trailing.push(reload_menu(p, t("Check now")));
    }
    let updates = widgets::group(
        p,
        t("Updates"),
        None,
        None,
        vec![widgets::row_item(
            p,
            Some(Icon::Download),
            line,
            None,
            trailing,
            None,
        )],
    );

    // About: collapsed shows only the version.
    let mut details = column![
        widgets::small(p, t("A safer PC. Without headaches.")),
        widgets::small(p, t("Fonts: IBM Plex Sans (SIL Open Font License).")),
        widgets::small(p, t("Icons: Fluent UI System Icons (MIT license).")),
        widgets::small(
            p,
            t("App clean-up lists draw on the Win11Debloat and WinUtil projects (MIT license).")
        ),
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
    .spacing(theme::S2);
    if let Some(error) = &ctx.engine_error {
        details = details.push(widgets::small(p, format!("{}: {}", t("Last problem"), t(crate::app::flow::plain_failure(error)))));
    }
    let about = widgets::collapsible(
        p,
        "Secblitz",
        Some(format!("{} {}", t("Version"), env!("CARGO_PKG_VERSION"))),
        state.technical,
        Message::Settings(Msg::ToggleTechnical),
        container(details).padding([theme::S2, theme::S4 + theme::ICON_ROW + theme::S4]),
    );

    // Remove Secblitz: one button, the choices are in the sheet.
    let removal = widgets::group(
        p,
        t(remove::SECTION_TITLE),
        None,
        None,
        vec![widgets::row_item(
            p,
            Some(Icon::Trash),
            t(remove::SECTION_ROW),
            Some(t(remove::SECTION_HELP)),
            widgets::action(
                p,
                ButtonKind::Danger,
                t(remove::SECTION_TITLE),
                None,
                (!ctx.busy).then(|| Message::Settings(Msg::Remove(remove::Msg::Open))),
            ),
            None,
        )],
    );

    column![
        widgets::page_header(p, t("Settings"), Some(t("Make Secblitz work your way."))),
        appearance,
        protection,
        updates,
        removal,
        about,
    ]
    .spacing(theme::S8)
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
        // The updater's lock is held: a check is running, nothing is ready.
        assert_eq!(
            update_view(&status(O::DeferredBusy, 0)),
            UpdateView::Checking
        );
        assert_eq!(
            update_view(&status(O::Failed { reason: "x".into() }, 5)),
            UpdateView::Unknown
        );
        assert_eq!(update_view(&status(O::NotConfigured, 0)), UpdateView::Off);
    }

    #[test]
    fn a_copy_that_is_not_installed_says_updates_are_not_set_up() {
        assert!(matches!(
            shown_update(false, Some(UpdateView::Unknown)),
            Remote::Ready(UpdateView::Off)
        ));
        assert!(matches!(
            shown_update(false, None),
            Remote::Ready(UpdateView::Off)
        ));
        assert!(matches!(
            shown_update(true, Some(UpdateView::Unknown)),
            Remote::Ready(UpdateView::Unknown)
        ));
        assert!(matches!(shown_update(true, None), Remote::Failed));
    }

    #[test]
    fn a_copy_that_is_not_installed_shows_the_tray_switch_off() {
        assert!(!shown_tray(false, true));
        assert!(!shown_tray(false, false));
        assert!(shown_tray(true, true));
        assert!(!shown_tray(true, false));
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

/// Warm the page in the background: the two slow reads run off the UI thread
/// and the page keeps whatever it already shows until they land.
#[allow(clippy::items_after_test_module)]
pub fn preload(state: &mut State, ctx: &mut Ctx) -> Task<Message> {
    let _ = ctx;
    if state.working {
        return Task::none();
    }
    load_task(state)
}

/// Start a read tagged with a fresh generation; older reads are dropped.
fn load_task(state: &mut State) -> Task<Message> {
    state.generation = state.generation.wrapping_add(1);
    let generation = state.generation;
    Task::perform(
        blocking(|| {
            let background = prefs_store::background_on().map_err(|e| format!("{e:#}"));
            let update = secblitz::updater::status().ok().map(|s| update_view(&s));
            (background, update)
        }),
        move |(background, update)| {
            Message::Settings(Msg::Loaded {
                generation,
                background,
                update,
            })
        },
    )
}
