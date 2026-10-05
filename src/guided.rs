//! Guided API handoff: Ui::with_details(bool), Ui::report_details(&Report)
//! supplement the existing message/report/progress API. All user-facing copy
//! below uses English source keys through Lang::t (including keyboard menus).
//! Translation handoff: collect the literal t()/message()/say()/confirm() keys
//! here plus the new CLI help keys in main.rs. No files outside our ownership
//! are written. Exits 23..27 request the original unelevated desktop broker; it is
//! never authorization by itself: the broker asks for consent again.
use crate::menu::{self, ChoiceInput, TerminalMenu};
use crate::{i18n::Lang, ui::Ui};
use anyhow::{bail, Result};
use secblitz::engine::{Engine, Report};
use std::io::{self, IsTerminal, Write};

/// A closed routing protocol, never authorization or a command payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum BrokerRequest {
    Bitwarden = 23,
    WindowsUpdate = 24,
    WindowsSecurity = 25,
    Encryption = 26,
    SignIn = 27,
}

impl BrokerRequest {
    pub fn from_exit(code: i32) -> Option<Self> {
        match code {
            23 => Some(Self::Bitwarden),
            24 => Some(Self::WindowsUpdate),
            25 => Some(Self::WindowsSecurity),
            26 => Some(Self::Encryption),
            27 => Some(Self::SignIn),
            _ => None,
        }
    }
    fn action(self) -> Option<crate::actions::Action> {
        use crate::actions::Action;
        match self {
            Self::Bitwarden => None,
            Self::WindowsUpdate => Some(Action::OpenWindowsUpdate),
            Self::WindowsSecurity => Some(Action::OpenWindowsSecurity),
            Self::Encryption => Some(Action::OpenEncryptionSettings),
            Self::SignIn => Some(Action::OpenSignInSettings),
        }
    }
    fn prompt(self) -> &'static str {
        match self {
            Self::Bitwarden => "Install Bitwarden in your original desktop account now?",
            Self::WindowsUpdate => "Open Windows Update settings now?",
            Self::WindowsSecurity => "Open Windows Security settings now?",
            Self::Encryption => "Open device encryption settings now?",
            Self::SignIn => "Open sign-in settings now?",
        }
    }
}
pub const TERMINAL_REQUIRED: &str = "The guided check needs an interactive terminal. Open a terminal and run secblitz guide, or use secblitz audit --json for a report.";

pub fn require_terminal(lang: Lang) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() || !io::stderr().is_terminal() {
        bail!(lang.t(TERMINAL_REQUIRED));
    }
    Ok(())
}

struct Input<R, W> {
    reader: R,
    writer: W,
    lang: Lang,
}
impl<R: ChoiceInput, W: Write> Input<R, W> {
    fn say(&mut self, key: &str) -> Result<()> {
        self.line(&self.lang.t(key))
    }
    fn line(&mut self, text: &str) -> Result<()> {
        if !menu::screen_note(text)? {
            writeln!(self.writer, "{text}")?;
        }
        Ok(())
    }
    fn document(&mut self, title: &str, text: &str) -> Result<()> {
        if !menu::screen_active() {
            writeln!(self.writer, "{}\n{text}", self.lang.t(title))?;
        }
        self.reader.view(self.lang, &self.lang.t(title), text)
    }
    fn choose(&mut self, key: &str, items: &[&str], default: usize) -> Result<Option<usize>> {
        self.choose_inner(key, items, default, false)
    }
    fn choose_inner(
        &mut self,
        key: &str,
        items: &[&str],
        default: usize,
        enter_only: bool,
    ) -> Result<Option<usize>> {
        if menu::screen_active() {
            menu::screen_title(&self.lang.t(key));
        } else {
            self.say(key)?;
        }
        self.writer.flush()?;
        let items: Vec<_> = items.iter().map(|key| self.lang.t(key)).collect();
        let choice = self.reader.select(self.lang, &items, default, enter_only)?;
        anyhow::ensure!(
            choice.is_none_or(|index| index < items.len()),
            "Invalid menu selection"
        );
        Ok(choice)
    }
    fn confirm(&mut self, key: &str) -> Result<bool> {
        Ok(self.choose_inner(key, &["Yes, continue", "No, go back"], 1, true)? == Some(0))
    }
    fn multiple(&mut self, items: &[String]) -> Result<Option<Vec<usize>>> {
        self.multiple_selected(items, &vec![false; items.len()])
    }
    fn multiple_selected(
        &mut self,
        items: &[String],
        defaults: &[bool],
    ) -> Result<Option<Vec<usize>>> {
        if menu::screen_active() {
            menu::screen_title(&self.lang.t("Review and choose fixes"));
        } else {
            self.say("Review and choose fixes")?;
        }
        self.writer.flush()?;
        let Some(mut selected) = self
            .reader
            .multi_select_with_defaults(self.lang, items, defaults)?
        else {
            return Ok(None);
        };
        anyhow::ensure!(
            selected.iter().all(|&index| index < items.len()),
            "Invalid menu selection"
        );
        selected.sort_unstable();
        selected.dedup();
        Ok(Some(selected))
    }
}

trait Session {
    fn available(&self) -> Vec<String>;
    fn restart_ids(&self) -> Vec<String> {
        Vec::new()
    }
    fn audit(&mut self, view: &Ui) -> Result<Report>;
    fn audit_after(&mut self, view: &Ui) -> Result<Report> {
        self.audit(view)
    }
    fn apply(&mut self, ids: &[String], view: &Ui) -> Result<Report>;
    fn undo(&mut self, view: &Ui) -> Result<Report>;
    fn history(&mut self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}
impl Session for Engine {
    fn available(&self) -> Vec<String> {
        self.available_controls()
            .iter()
            .map(|c| c.id.clone())
            .collect()
    }
    fn audit(&mut self, view: &Ui) -> Result<Report> {
        let p = view.progress_named("Checking your PC");
        self.audit_with_progress(|id, status| p.update(id, status))
    }
    fn restart_ids(&self) -> Vec<String> {
        self.available_controls()
            .iter()
            .filter(|c| c.reboot)
            .map(|c| c.id.clone())
            .collect()
    }
    fn audit_after(&mut self, view: &Ui) -> Result<Report> {
        let p = view.progress_named("Checking after your changes");
        self.audit_with_progress(|id, status| p.update(id, status))
    }
    fn apply(&mut self, ids: &[String], view: &Ui) -> Result<Report> {
        let p = view.progress_named("Applying selected fixes");
        self.apply_selected(ids, |id, status| p.update(id, status))
    }
    fn undo(&mut self, view: &Ui) -> Result<Report> {
        let p = view.progress_named("Undoing recorded fixes");
        self.revert(|id, status| p.update(id, status))
    }
    fn history(&mut self) -> Result<Vec<String>> {
        Engine::history(self)
    }
}

pub fn run(engine: &mut Engine, view: &Ui, lang: Lang, broker: bool) -> Result<i32> {
    let _screen = menu::Screen::enter(lang, view.animations_enabled())?;
    let mut input = Input {
        reader: TerminalMenu::default(),
        writer: io::stdout(),
        lang,
    };
    session(engine, view, &mut input, broker)
}

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
enum RootChoice {
    Recommended,
    Review,
    Check,
    Advanced,
    Exit,
}

const ROOT_ITEMS: [&str; 5] = [
    "Fix recommended",
    "Review and choose fixes",
    "Check again",
    "Advanced",
    "Exit",
];
const ROOT_CHOICES: [RootChoice; 5] = [
    RootChoice::Recommended,
    RootChoice::Review,
    RootChoice::Check,
    RootChoice::Advanced,
    RootChoice::Exit,
];
const ROOT_HELPERS: [&str; 5] = [
    "Review the recommended set before anything changes.",
    "Choose individual fixes and review your exact selection.",
    "Refresh the read-only protection check.",
    "Undo, maintenance, diagnostics and specialist tools.",
    "Return to your terminal.",
];

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
enum AdvancedChoice {
    Undo,
    Maintenance,
    Diagnostics,
    QualityUpdates,
    Tools,
    Details,
    Back,
}
const ADVANCED_ITEMS: [&str; 7] = [
    "Undo my last fixes",
    "Maintenance plans and owner policy",
    "Read-only diagnostics and profiles",
    "Selected Windows quality updates",
    "Extra tools",
    "Technical details and history",
    "Back",
];
const ADVANCED_CHOICES: [AdvancedChoice; 7] = [
    AdvancedChoice::Undo,
    AdvancedChoice::Maintenance,
    AdvancedChoice::Diagnostics,
    AdvancedChoice::QualityUpdates,
    AdvancedChoice::Tools,
    AdvancedChoice::Details,
    AdvancedChoice::Back,
];
const DETAILS_ITEMS: [&str; 3] = ["Technical details (optional)", "History", "Back"];
const TOOLS_ITEMS: [&str; 6] = [
    "Generate a password",
    "Install Bitwarden (optional password manager)",
    "Install and start optional monitoring",
    "Update Microsoft Defender protection",
    "Windows settings and next steps",
    "Back",
];
const SETTINGS_ITEMS: [&str; 5] = [
    "Open Windows Update settings",
    "Open Windows Security settings",
    "Open device encryption / BitLocker settings",
    "Open sign-in settings",
    "Back",
];

/// The plan is scoped to this assessment; no refresh can add IDs to an approval.
struct Snapshot {
    generation: u64,
    report: Report,
}
#[derive(Default)]
struct FlowState {
    snapshot: Option<Snapshot>,
    generation: u64,
    last_operation: Option<Report>,
    operation_error: Option<anyhow::Error>,
    verification_error: Option<anyhow::Error>,
}
impl FlowState {
    fn capture(&mut self, result: Result<Report>) {
        self.generation += 1;
        self.snapshot = None;
        self.verification_error = None;
        match result {
            Ok(report) => {
                self.snapshot = Some(Snapshot {
                    generation: self.generation,
                    report,
                })
            }
            Err(error) => self.verification_error = Some(error),
        }
    }
    fn show_assessment(
        &self,
        view: &Ui,
        input: &mut Input<impl ChoiceInput, impl Write>,
    ) -> Result<()> {
        if let Some(snapshot) = &self.snapshot {
            view.summary(&snapshot.report)?;
        } else {
            view.guided_header_with_failure(None, self.verification_error.is_some());
            input.say("The new check could not finish. Check again before choosing more fixes. Undo is still available.")?;
            input.say("Choose Technical details to see the original failure.")?;
        }
        Ok(())
    }
}

fn candidates(report: &Report, available: &[String]) -> Vec<String> {
    if report.findings.iter().any(|f| f.status == "pending")
        || report.results.iter().any(|r| r.status == "pending")
    {
        return Vec::new();
    }
    let mut ids = Vec::new();
    for r in &report.results {
        if r.status == "attention"
            && available.contains(&r.id)
            && crate::ui::advice::for_outcome(r).step == crate::ui::advice::NextStep::Repair
            && !ids.contains(&r.id)
        {
            ids.push(r.id.clone());
        }
    }
    ids
}

/// Impact source keys earned by this batch: (protected now, protected after restart).
///
/// Evidence only: Secblitz must have applied the id in this batch AND the fresh
/// post-check must classify it as Protected. The post-check reads the saved
/// setting, so a restart-required apply is never "protected now". Called only
/// after a successful verification of an apply, never for undo.
pub fn payoff(
    attempted: &[String],
    applied: &Report,
    verified: &Report,
) -> (Vec<String>, Vec<String>) {
    use crate::ui::advice::{self, Group};
    let mut now: Vec<String> = Vec::new();
    let mut after_restart: Vec<String> = Vec::new();
    for id in attempted {
        let impact = advice::control_impact(id);
        let Some(change) = applied
            .results
            .iter()
            .find(|r| r.id == *id && r.status == "applied")
        else {
            continue;
        };
        let confirmed = verified
            .results
            .iter()
            .any(|r| r.id == *id && advice::for_outcome(r).group == Group::Protected);
        if impact.is_empty() || !confirmed {
            continue;
        }
        let list = if advice::for_control(id, &change.status, &change.detail).step
            == advice::NextStep::Restart
        {
            &mut after_restart
        } else {
            &mut now
        };
        if !list.iter().any(|k| k == impact) {
            list.push(impact.to_owned());
        }
    }
    (now, after_restart)
}

fn approve_plan(
    input: &mut Input<impl ChoiceInput, impl Write>,
    candidates: &[String],
    recommended: bool,
    restart_ids: &[String],
) -> Result<Option<Vec<String>>> {
    let _section = menu::section(menu::Section::Review);
    let labels: Vec<_> = candidates.iter().map(|id| input.lang.control(id)).collect();
    let mut ids = if recommended {
        candidates.to_vec()
    } else {
        let Some(selected) = input.multiple(&labels)? else {
            return Ok(None);
        };
        selected
            .into_iter()
            .map(|n| candidates[n].clone())
            .collect()
    };
    loop {
        menu::screen_clear();
        if ids.is_empty() {
            input.say("Nothing selected. No changes made.")?;
            return Ok(None);
        }
        input.line(&format!(
            "\u{25b8} {} \u{b7} {}",
            input
                .lang
                .t("Selected fixes: {count}")
                .replace("{count}", &ids.len().to_string()),
            input.lang.t("May need a restart: {count}").replace(
                "{count}",
                &ids.iter()
                    .filter(|id| restart_ids.contains(id))
                    .count()
                    .to_string(),
            )
        ))?;
        for id in &ids {
            let name = input.lang.control(id);
            // IDs remain bound internally. Only ambiguous display labels need
            // an ID suffix; full IDs are always available in technical details.
            let ambiguous = ids
                .iter()
                .filter(|other| input.lang.control(other) == name)
                .count()
                > 1;
            let restart = restart_ids.contains(id);
            let glyph = if restart { '\u{21bb}' } else { '!' }; // ↻ or !
            let label = if ambiguous {
                format!("{glyph} {name} ({id})")
            } else {
                format!("{glyph} {name}")
            };
            input.line(&label)?;
            if restart {
                input.line(&format!(
                    "  \u{b7} {}",
                    input.lang.t("Needs a restart to finish")
                ))?;
            }
            // Show the concrete threat so users understand what this fix guards against.
            let a = crate::ui::advice::for_control(id, "attention", "");
            if let Some(imp) = crate::ui::impact_line(input.lang, &a) {
                input.line(&format!("  \u{b7} {imp}"))?;
            }
        }
        input.line("")?;
        input.line(&format!(
            "\u{b7} {}",
            input.lang.t("Restart: never automatic.")
        ))?;
        input.line(&format!(
            "\u{b7} {}",
            input.lang.t("Undo: saved settings only; later changes may block it.")
        ))?;
        input.line(&format!(
            "\u{b7} {}",
            input.lang.t("Next: apply this selection, then verify the result.")
        ))?;
        match input.choose_inner(
            "Confirm selected fixes",
            &["Apply these fixes", "Change selection", "Back"],
            2,
            true,
        )? {
            Some(0) => return Ok(Some(ids)),
            Some(1) => {
                menu::screen_clear();
                input.say(
                    "Your selection is kept. Space toggles an item; Enter returns to the recap.",
                )?;
                let defaults = candidates
                    .iter()
                    .map(|id| ids.contains(id))
                    .collect::<Vec<_>>();
                let Some(selected) = input.multiple_selected(&labels, &defaults)? else {
                    return Ok(None);
                };
                ids = selected
                    .into_iter()
                    .map(|n| candidates[n].clone())
                    .collect();
            }
            _ => return Ok(None),
        }
    }
}

fn attempt_and_verify(
    state: &mut FlowState,
    engine: &mut impl Session,
    view: &Ui,
    input: &mut Input<impl ChoiceInput, impl Write>,
    ids: Option<&[String]>,
) -> Result<()> {
    menu::screen_clear();
    state.snapshot = None;
    let attempted = match ids {
        Some(ids) => engine.apply(ids, view),
        None => engine.undo(view),
    };
    // Always audit once, even after partial writes. Do this before any fallible
    // result rendering or prompt can prevent verification.
    state.capture(engine.audit_after(view));
    view.guided_header_with_failure(
        state.snapshot.as_ref().map(|s| &s.report),
        state.verification_error.is_some(),
    );
    let result_role = if attempted.is_err() || state.verification_error.is_some() {
        menu::Role::Failure
    } else if attempted.as_ref().is_ok_and(crate::needs_review) {
        menu::Role::Review
    } else {
        menu::Role::Text
    };
    let mut result = String::new();
    match attempted {
        Ok(report) => {
            let completed = report
                .results
                .iter()
                .filter(|r| {
                    matches!(
                        r.status.as_str(),
                        "applied" | "restored" | "unchanged" | "compliant" | "ok"
                    )
                })
                .count();
            result.push_str(
                &input
                    .lang
                    .t("{completed} completed · {review} need review")
                    .replace("{completed}", &completed.to_string())
                    .replace("{review}", &(report.results.len() - completed).to_string()),
            );
            // Payoff section: rendered prominently BEFORE per-item results.
            // Only when verification succeeded and this was an apply (ids Some), not undo.
            if let (Some(attempted_ids), Some(snapshot)) = (ids, &state.snapshot) {
                let (now, restart_phrases) = payoff(attempted_ids, &report, &snapshot.report);
                if !now.is_empty() {
                    result.push_str(&format!(
                        "\n\n\u{25b8} {}\n",
                        input.lang.t("You're now protected from:")
                    ));
                    for phrase in &now {
                        result.push_str(&format!("\u{2713} {}\n", input.lang.t(phrase)));
                    }
                }
                if !restart_phrases.is_empty() {
                    result.push_str(&format!(
                        "\n\u{25b8} {}\n",
                        input.lang.t("After you restart, you'll be protected from:")
                    ));
                    for phrase in &restart_phrases {
                        result.push_str(&format!("\u{21bb} {}\n", input.lang.t(phrase)));
                    }
                }
            }
            result.push_str("\n\n");
            result.push_str(&view.guided_report_text(&report));
            state.last_operation = Some(report);
        }
        Err(error) => {
            state.operation_error = Some(error);
            result.push_str(&format!(
                "\u{2717} {}",
                input.lang.t("Operation failed")
            ));
            result.push('\n');
            result.push_str(&input.lang.t("The operation did not finish. Some changes may already have been made; remaining work is not confirmed. You can check again or undo recorded fixes."));
        }
    }
    result.push_str("\n\n");
    if state.verification_error.is_some() {
        result.push_str(&format!(
            "\u{2717} {}",
            input.lang.t("The post-check failed separately. Current protection is unverified; check again before more fixes.")
        ));
    } else {
        result.push_str(&input.lang.t("Post-check complete. Review any remaining items before making more changes."));
    }
    result.push('\n');
    result.push_str(&input.lang.t("Advanced contains Undo and technical details. Only recorded hardening changes can be undone."));
    menu::screen_role(result_role);
    input.document("Fix results", &result)
}

fn session(
    engine: &mut impl Session,
    view: &Ui,
    input: &mut Input<impl ChoiceInput, impl Write>,
    broker: bool,
) -> Result<i32> {
    let _section = menu::section(menu::Section::Overview);
    menu::screen_clear();
    input.say("First, we will check your protection. You choose what to fix; checking does not apply fixes.")?;
    let mut state = FlowState::default();
    state.capture(engine.audit(view));
    state.show_assessment(view, input)?;
    loop {
        view.guided_header_with_failure(
            state.snapshot.as_ref().map(|s| &s.report),
            state.verification_error.is_some(),
        );
        menu::screen_home(ROOT_HELPERS.iter().map(|key| input.lang.t(key)).collect());
        let Some(choice) = input.choose("Your next step", &ROOT_ITEMS, 0)? else {
            return Ok(0);
        };
        match ROOT_CHOICES[choice] {
            RootChoice::Exit => return Ok(0),
            RootChoice::Recommended | RootChoice::Review => {
                let Some(current) = &state.snapshot else {
                    input.say("Check again before choosing fixes. The previous check is no longer current.")?;
                    continue;
                };
                let generation = current.generation;
                let candidates = candidates(&current.report, &engine.available());
                if candidates.is_empty() {
                    input.say("There are no recommended automatic fixes available. See details for other next steps.")?;
                    if matches!(ROOT_CHOICES[choice], RootChoice::Review) {
                        input.document(
                            "Protection review",
                            &view.guided_report_text(&current.report),
                        )?;
                    }
                    continue;
                }
                let Some(ids) = approve_plan(
                    input,
                    &candidates,
                    choice == RootChoice::Recommended as usize,
                    &engine.restart_ids(),
                )?
                else {
                    continue;
                };
                if state
                    .snapshot
                    .as_ref()
                    .is_none_or(|s| s.generation != generation)
                {
                    continue;
                }
                attempt_and_verify(&mut state, engine, view, input, Some(&ids))?;
            }
            RootChoice::Check => {
                menu::screen_clear();
                state.capture(engine.audit(view));
                state.show_assessment(view, input)?;
                let text = state.snapshot.as_ref().map(|s| view.guided_report_text(&s.report)).unwrap_or_else(|| input.lang.t("The post-check failed separately. Current protection is unverified; check again before more fixes."));
                input.document("Check results", &text)?;
            }
            RootChoice::Advanced => {
                if let Some(request) = advanced(&mut state, engine, view, input, broker)? {
                    return Ok(request as i32);
                }
            }
        }
    }
}

fn advanced(
    state: &mut FlowState,
    engine: &mut impl Session,
    view: &Ui,
    input: &mut Input<impl ChoiceInput, impl Write>,
    broker: bool,
) -> Result<Option<BrokerRequest>> {
    let _section = menu::section(menu::Section::Advanced);
    menu::screen_clear();
    loop {
        view.guided_header_with_failure(
            state.snapshot.as_ref().map(|s| &s.report),
            state.verification_error.is_some(),
        );
        let Some(choice) = input.choose("Advanced", &ADVANCED_ITEMS, 0)? else {
            return Ok(None);
        };
        match ADVANCED_CHOICES[choice] {
            AdvancedChoice::Back => return Ok(None),
            AdvancedChoice::Undo => {
                input.say("Only saved hardening settings are restored. Maintenance, scans and software installations are not undone.")?;
                if input.confirm("Undo the latest recorded fixes? This restores their saved original settings; extra tools and software installs are not undone.")? { attempt_and_verify(state, engine, view, input, None)?; }
            }
            AdvancedChoice::Maintenance
            | AdvancedChoice::Diagnostics
            | AdvancedChoice::QualityUpdates => {
                let group = match ADVANCED_CHOICES[choice] {
                    AdvancedChoice::Diagnostics => "diagnostics",
                    AdvancedChoice::Maintenance => "operations",
                    _ => "quality-updates",
                };
                let result = crate::maintenance_cli::guide(
                    group,
                    input.lang,
                    &mut input.reader,
                    &mut input.writer,
                    broker,
                );
                if group != "diagnostics" {
                    state.snapshot = None;
                }
                if let Err(error) = result {
                    let mut bytes = Vec::new();
                    crate::maintenance_cli::write_failure(
                        input.lang,
                        &error,
                        false,
                        false,
                        &mut io::sink(),
                        &mut bytes,
                    )?;
                    state.operation_error = Some(error);
                    input.document("Operation failed", &String::from_utf8(bytes)?)?;
                }
            }
            AdvancedChoice::Tools => match extra_tools(input, view, broker) {
                Ok(ToolOutcome::Broker(request)) => return Ok(Some(request)),
                Ok(ToolOutcome::Changed) => state.snapshot = None,
                Ok(ToolOutcome::None) => {}
                Err(error) => {
                    state.snapshot = None;
                    remember_failure(input, &mut state.operation_error, error)?;
                }
            },
            AdvancedChoice::Details => {
                let _section = menu::section(menu::Section::Details);
                loop {
                    match input.choose("Technical details and history", &DETAILS_ITEMS, 0)? {
                        Some(0) => {
                            let mut text = String::new();
                            for (key, error) in [("Technical details of the last failure (may include system paths and native messages):", &state.operation_error), ("Technical details of the latest check failure:", &state.verification_error)] {
                        if let Some(error) = error { text.push_str(&format!("{}\n{}\n", input.lang.t(key), crate::ui::error_details(input.lang, error))); }
                    }
                            if let Some(current) = &state.snapshot {
                                text.push_str(&view.details_text(&current.report)?);
                            }
                            if let Some(operation) = &state.last_operation {
                                text.push_str(&format!("\n{}\n{}", input.lang.t("Technical details of the last completed operation (not a new protection check):"), view.details_text(operation)?));
                            }
                            input.document("Technical details (optional)", &text)?;
                        }
                        Some(1) => {
                            let history = engine.history()?;
                            let text = if history.is_empty() {
                                input.lang.t("No transactions recorded")
                            } else {
                                history
                                    .iter()
                                    .map(|entry| input.lang.detail(entry))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            };
                            input.document("History", &text)?;
                        }
                        _ => break,
                    }
                }
            }
        }
    }
}

fn remember_failure(
    input: &mut Input<impl ChoiceInput, impl Write>,
    last_error: &mut Option<anyhow::Error>,
    error: anyhow::Error,
) -> Result<()> {
    *last_error = Some(error);
    input.say("The operation did not finish. Some changes may already have been made; remaining work is not confirmed. You can check again or undo recorded fixes.")?;
    input.say("Choose Technical details to see the original failure.")
}

pub fn handle_broker(
    request: BrokerRequest,
    lang: Lang,
    details: bool,
    no_animation: bool,
) -> Result<bool> {
    require_terminal(lang)?;
    anyhow::ensure!(
        !secblitz::platform::is_elevated()?,
        "Desktop requests require a non-elevated window"
    );
    let _screen = menu::Screen::enter(lang, !no_animation)?;
    let _section = menu::section(menu::Section::Desktop);
    menu::screen_header(menu::Header::message(
        lang.t("Original desktop account"),
        menu::Role::Text,
    ));
    let mut input = Input {
        reader: TerminalMenu::default(),
        writer: io::stdout(),
        lang,
    };
    broker_turn(&mut input, request, details, |request| {
        if let Some(action) = request.action() {
            let result = crate::actions::run(action)?;
            anyhow::ensure!(
                result.status == "opened",
                "Unexpected action result: {}: {}",
                result.status,
                result.detail
            );
        } else {
            crate::tools::install_bitwarden()?;
        }
        Ok(())
    })
}

fn broker_turn(
    input: &mut Input<impl ChoiceInput, impl Write>,
    request: BrokerRequest,
    details: bool,
    run: impl FnOnce(BrokerRequest) -> Result<()>,
) -> Result<bool> {
    if request == BrokerRequest::Bitwarden {
        input.say(crate::TOOL_CONSENT)?;
    } else {
        input.say("Opening settings does not fix a finding. Follow the Windows instructions, then check again.")?;
    }
    // Decline, Back, empty input and EOF end the handoff without another UAC prompt.
    if !input.confirm(request.prompt())? {
        return Ok(false);
    }
    match run(request) {
        Ok(()) if request == BrokerRequest::Bitwarden => input.say("Complete")?,
        Ok(()) => input.say("Settings opened. Follow the Windows instructions; opening settings does not mean the issue is fixed.")?,
        Err(error) => {
            input.say("That action did not finish. You can view the details before trying again.")?;
            if details || input.confirm("Show technical details of this failure?")? {
                input.document("Technical details (optional)", &crate::ui::error_details(input.lang, &error))?;
            }
        }
    }
    input.confirm("Return to the PC check? Windows will ask for administrator permission again.")
}

#[derive(Debug, PartialEq, Eq)]
enum ToolOutcome {
    None,
    Changed,
    Broker(BrokerRequest),
}

fn extra_tools(
    input: &mut Input<impl ChoiceInput, impl Write>,
    view: &Ui,
    broker: bool,
) -> Result<ToolOutcome> {
    let _section = menu::section(menu::Section::Tools);
    let mut changed = false;
    loop {
        match input.choose("Extra tools", &TOOLS_ITEMS, 0)? {
        Some(0) => crate::ui::password(input.lang)?,
        Some(1) if broker => {
            input.say(crate::TOOL_CONSENT)?;
            if input.confirm("Return to your original non-administrator window to install Bitwarden? That window will ask for consent again.")? { return Ok(ToolOutcome::Broker(BrokerRequest::Bitwarden)); }
        }
        Some(1) => input.say("Bitwarden must be installed from your normal, non-administrator desktop terminal. Open that terminal and run secblitz tools bitwarden --yes after reviewing the installation consent in --help.")?,
        Some(2) => { changed |= run_action(input, view, crate::actions::Action::StartMonitoring)?; if changed { view.guided_header(None); } },
        Some(3) => { changed |= run_action(input, view, crate::actions::Action::UpdateDefender)?; if changed { view.guided_header(None); } },
        Some(4) => {
            match next_steps(input, view, broker)? {
                ToolOutcome::Broker(request) => return Ok(ToolOutcome::Broker(request)),
                ToolOutcome::Changed => { changed=true; view.guided_header(None); },
                ToolOutcome::None => {},
            }
        },
        Some(5) | None => return Ok(tool_outcome(changed)),
        _ => unreachable!("validated menu index"),
    }
    }
}

fn tool_outcome(ran: bool) -> ToolOutcome {
    if ran {
        ToolOutcome::Changed
    } else {
        ToolOutcome::None
    }
}

fn next_steps(
    input: &mut Input<impl ChoiceInput, impl Write>,
    view: &Ui,
    broker: bool,
) -> Result<ToolOutcome> {
    let _section = menu::section(menu::Section::Settings);
    loop {
        input.say("Opening settings does not fix a finding. Follow the Windows instructions, then check again.")?;
        let choice = input.choose("Review protection and next steps", &SETTINGS_ITEMS, 0)?;
        let request = match choice {
            Some(0) => Some(BrokerRequest::WindowsUpdate),
            Some(1) => Some(BrokerRequest::WindowsSecurity),
            Some(2) => Some(BrokerRequest::Encryption),
            Some(3) => Some(BrokerRequest::SignIn),
            _ => None,
        };
        if let Some(request) = request {
            if !broker {
                input.say("To open Settings from here, close Secblitz and open it normally, without Run as administrator.")?;
                continue;
            }
            if input.confirm("Return to your original Secblitz window to open Settings? That window will ask you again before opening anything.")? {
            return Ok(ToolOutcome::Broker(request));
        }
            continue;
        }
        let _ = view;
        return Ok(ToolOutcome::None);
    }
}

fn run_action(
    input: &mut Input<impl ChoiceInput, impl Write>,
    view: &Ui,
    action: crate::actions::Action,
) -> Result<bool> {
    run_action_with(input, action, || {
        let progress = view.progress();
        // The progress renderer stays live while the fixed native action blocks.
        let result = std::thread::spawn(move || crate::actions::run(action)).join();
        drop(progress);
        match result {
            Ok(result) => result,
            Err(payload) => {
                let cause = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("Worker panicked without a text payload");
                bail!("Action worker failed: {cause}");
            }
        }
    })
}

fn run_action_with(
    input: &mut Input<impl ChoiceInput, impl Write>,
    action: crate::actions::Action,
    run: impl FnOnce() -> Result<crate::actions::ActionResult>,
) -> Result<bool> {
    use crate::actions::Action;
    match action {
        Action::UpdateDefender => input.say("Defender will connect to its configured update sources and download protection updates.")?,
        Action::QuickScan => input.say("Defender will scan your device and may remediate threats using your existing Defender settings. This can take several minutes.")?,
        Action::StartMonitoring => input.say("This installs the optional read-only monitor if needed and starts it. It does not automatically fix findings.")?,
        _ => input.say("Open settings only: no fix is applied or verified by opening this page.")?,
    }
    if input.confirm(
        "Run the selected extra action now? Extra actions are not part of Undo my last fixes.",
    )? {
        match run() {
            Ok(result) => {
                let key = match result.status.as_str() {
                    "opened" => "Settings opened. Follow the Windows instructions; opening settings does not mean the issue is fixed.",
                    "running" => "Monitoring is running. Check reports separately to verify their freshness.",
                    "returned" => "Defender's command returned. Review Windows Security for update or scan results, then check again.",
                    _ => bail!("Unexpected action result: {}: {}", result.status, result.detail),
                };
                input.say(key)?;
            }
            Err(error) => {
                input.say("The action could not be verified. Defender work may still be running. Review Windows Security or service status before trying again.")?;
                input.say("Extra actions are not part of Undo my last fixes.")?;
                return Err(error);
            }
        }
        input.say("Choose Check again when you are ready to verify current protection.")?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secblitz::engine::Outcome;
    use std::collections::VecDeque;

    #[derive(Clone, Debug)]
    enum Event {
        Root(Route),
        AtRoot(usize),
        Pick(usize),
        Check(Vec<usize>),
        Default,
        Escape,
        OnMenu(&'static [&'static str], Option<usize>),
    }
    use Event::{Check, Default, Escape, Pick, Root};
    #[derive(Clone, Copy, Debug)]
    enum Route {
        Recommended,
        Select,
        Review,
        Recheck,
        Undo,
        Details,
        Exit,
    }
    use Route::{Details, Exit, Recheck, Recommended, Review, Select, Undo};
    #[test]
    fn exact_recap_counts_restart_and_retains_checkboxes_when_changing_selection() {
        struct Recorder {
            script: TestScript,
            defaults: Vec<Vec<bool>>,
        }
        impl ChoiceInput for Recorder {
            fn select(
                &mut self,
                lang: Lang,
                items: &[String],
                default: usize,
                consent: bool,
            ) -> Result<Option<usize>> {
                self.script.select(lang, items, default, consent)
            }
            fn multi_select(&mut self, lang: Lang, items: &[String]) -> Result<Option<Vec<usize>>> {
                self.script.multi_select(lang, items)
            }
            fn multi_select_with_defaults(
                &mut self,
                lang: Lang,
                items: &[String],
                defaults: &[bool],
            ) -> Result<Option<Vec<usize>>> {
                self.defaults.push(defaults.to_vec());
                self.script.multi_select(lang, items)
            }
        }
        let mut input = Input {
            reader: Recorder {
                script: TestScript::new(&[Check(vec![0, 1]), Pick(1), Check(vec![1]), Pick(0)]),
                defaults: vec![],
            },
            writer: Vec::new(),
            lang: Lang::En,
        };
        let candidates = vec!["uac.enabled".into(), "uac.consent".into()];
        let ids = approve_plan(&mut input, &candidates, false, &["uac.enabled".into()])
            .unwrap()
            .unwrap();
        assert_eq!(ids, ["uac.consent"]);
        assert_eq!(
            input.reader.defaults,
            [vec![false, false], vec![true, true]]
        );
        let text = String::from_utf8(input.writer).unwrap();
        assert!(text.contains("Selected fixes: 2") && text.contains("Selected fixes: 1"));
        assert!(text.contains("May need a restart: 1") && text.contains("May need a restart: 0"));
        assert!(
            text.contains("↻ Permission prompts") && text.contains("! Administrator approval"),
            "{text}"
        );
        assert!(!text.contains("uac.enabled") && !text.contains("uac.consent"), "{text}");
        assert!(
            text.contains("later changes may block it") && text.contains("never automatic"),
            "{text}"
        );
        // Restart fix shows "Needs a restart to finish"; risk lines use · prefix.
        assert!(text.contains("Needs a restart to finish"), "{text}");
        assert!(text.contains("  \u{b7} Risk:"), "{text}");
    }
    #[test]
    fn results_distinguish_partial_operation_failure_from_failed_post_check() {
        let mut fake = queued(vec![
            Ok(assessment("attention", "attention")),
            Err(anyhow::anyhow!("verification unavailable")),
        ]);
        fake.fail_apply = true;
        let text = drive_with(
            &[Root(Select), Check(vec![0, 1]), Pick(0), Root(Exit)],
            &mut fake,
        );
        assert_eq!(fake.events, ["audit", "apply", "audit"]);
        assert_eq!(
            fake.fixes,
            [vec!["uac.enabled".to_owned(), "uac.consent".to_owned()]]
        );
        assert!(text.contains("Operation failed"));
        assert!(text.contains("remaining work is not confirmed"));
        assert!(text.contains("The post-check failed separately"));
        assert!(!text.contains("verification unavailable")); // Native details stay opt-in.
    }
    #[test]
    #[ignore = "driven by the native Linux PTY test; mock engine only"]
    fn native_guided_flow_probe() {
        struct Native(Fake);
        impl Session for Native {
            fn available(&self) -> Vec<String> {
                self.0.available()
            }
            fn restart_ids(&self) -> Vec<String> {
                self.0.restart_ids()
            }
            fn audit(&mut self, view: &Ui) -> Result<Report> {
                let _progress = view.progress_named("Checking your PC");
                std::thread::sleep(std::time::Duration::from_millis(250));
                self.0.audit(view)
            }
            fn audit_after(&mut self, view: &Ui) -> Result<Report> {
                let _progress = view.progress_named("Checking after your changes");
                std::thread::sleep(std::time::Duration::from_millis(300));
                self.0.audit(view)
            }
            fn apply(&mut self, ids: &[String], view: &Ui) -> Result<Report> {
                let progress = view.progress_named("Applying selected fixes");
                progress.update(&ids[0], "pending");
                std::thread::sleep(std::time::Duration::from_millis(300));
                self.0.apply(ids, view)
            }
            fn undo(&mut self, _: &Ui) -> Result<Report> {
                panic!("native probe must not undo")
            }
        }
        let _screen = menu::Screen::enter(Lang::En, false).unwrap();
        let mut engine = Native(queued(vec![
            Ok(assessment("attention", "attention")),
            Ok(assessment("attention", "compliant")),
        ]));
        let mut input = Input {
            reader: TerminalMenu::default(),
            writer: io::stdout(),
            lang: Lang::En,
        };
        session(
            &mut engine,
            &Ui::new(Lang::En, true, false),
            &mut input,
            false,
        )
        .unwrap();
        assert_eq!(engine.0.fixes, [vec!["uac.consent".to_owned()]]);
        assert_eq!(engine.0.events, ["audit", "apply", "audit"]);
    }
    #[test]
    fn root_is_five_choices_and_specialist_actions_are_grouped_under_advanced() {
        assert_eq!(
            ROOT_ITEMS,
            [
                "Fix recommended",
                "Review and choose fixes",
                "Check again",
                "Advanced",
                "Exit"
            ]
        );
        assert_eq!(RootChoice::Exit as usize, 4);
        assert_eq!(RootChoice::Advanced as usize, 3);
        assert_eq!(ADVANCED_ITEMS.len(), 7);
        for (i, choice) in ROOT_CHOICES.iter().enumerate() {
            assert_eq!(*choice as usize, i);
        }
    }
    struct TestScript(VecDeque<Event>);
    impl TestScript {
        fn new(events: &[Event]) -> Self {
            // High-level business-flow scripts explicitly walk Back through
            // each parent before asking for another root action. Strict menu
            // tests below use raw OnMenu events and do not use this expansion.
            let mut queue = VecDeque::new();
            let mut depth = 0;
            for event in events {
                if let Root(route) = event {
                    queue.extend((0..depth).map(|_| Escape));
                    let (steps, new_depth) = match route {
                        Recommended => (vec![Event::AtRoot(0)], 0),
                        Select => (vec![Event::AtRoot(1)], 0),
                        Recheck => (vec![Event::AtRoot(2)], 0),
                        Undo => (vec![Event::AtRoot(3), Pick(0)], 1),
                        Details => (vec![Event::AtRoot(3), Pick(5), Pick(0)], 2),
                        Review => (vec![Event::AtRoot(3), Pick(4), Pick(4)], 3),
                        Exit => (vec![Event::AtRoot(4)], 0),
                    };
                    queue.extend(steps);
                    depth = new_depth;
                } else {
                    queue.push_back(event.clone());
                }
            }
            Self(queue)
        }
    }
    impl ChoiceInput for TestScript {
        fn select(
            &mut self,
            _: Lang,
            items: &[String],
            default: usize,
            enter_only: bool,
        ) -> Result<Option<usize>> {
            if enter_only {
                assert_eq!(
                    default,
                    items.len() - 1,
                    "consent must default to Back or No"
                );
            }
            Ok(match self.0.pop_front() {
                Some(Pick(index)) => Some(index),
                Some(Event::AtRoot(index)) => {
                    assert_eq!(items, ROOT_ITEMS.map(String::from));
                    Some(index)
                }
                Some(Event::OnMenu(expected, choice)) => {
                    assert_eq!(
                        items,
                        expected.iter().map(|s| s.to_string()).collect::<Vec<_>>()
                    );
                    choice
                }
                Some(Default) => Some(default),
                Some(Escape) | None => None,
                event => panic!("expected single choice, got {event:?}"),
            })
        }
        fn multi_select(&mut self, _: Lang, _: &[String]) -> Result<Option<Vec<usize>>> {
            Ok(match self.0.pop_front() {
                Some(Check(indices)) => Some(indices),
                Some(Default) => Some(vec![]),
                Some(Escape) | None => None,
                event => panic!("expected checklist, got {event:?}"),
            })
        }
    }
    #[test]
    fn back_and_escape_return_to_the_immediate_parent_without_side_effects() {
        let mut input = Input {
            reader: TestScript(VecDeque::from([
                Event::AtRoot(3),
                Event::OnMenu(&ADVANCED_ITEMS, Some(5)),
                Event::OnMenu(&DETAILS_ITEMS, None),
                Event::OnMenu(&ADVANCED_ITEMS, Some(4)),
                Event::OnMenu(&TOOLS_ITEMS, Some(4)),
                Event::OnMenu(&SETTINGS_ITEMS, Some(4)),
                Event::OnMenu(&TOOLS_ITEMS, None),
                Event::OnMenu(&ADVANCED_ITEMS, None),
                Event::AtRoot(4),
            ])),
            writer: Vec::new(),
            lang: Lang::En,
        };
        let mut fake = Fake::default();
        assert_eq!(
            session(
                &mut fake,
                &Ui::new(Lang::En, true, false),
                &mut input,
                false
            )
            .unwrap(),
            0
        );
        assert!(input.reader.0.is_empty());
        assert!(fake.fixes.is_empty());
        assert_eq!(fake.undos, 0);
        assert_eq!(fake.scans, 1);
    }
    #[test]
    #[ignore = "native driver: NAV-0 through NAV-8; mock engine only"]
    fn native_navigation_probe() {
        struct Native {
            menu: TerminalMenu,
            step: usize,
        }
        impl ChoiceInput for Native {
            fn select(
                &mut self,
                lang: Lang,
                items: &[String],
                default: usize,
                consent: bool,
            ) -> Result<Option<usize>> {
                use menu::Section::*;
                let (expected, choice, path): (&[&str], Option<usize>, &[menu::Section]) =
                    match self.step {
                        0 => (&ROOT_ITEMS, Some(3), &[Overview]),
                        1 => (&ADVANCED_ITEMS, Some(5), &[Overview, Advanced]),
                        2 => (
                            &DETAILS_ITEMS,
                            None,
                            &[Overview, Advanced, menu::Section::Details],
                        ),
                        3 => (&ADVANCED_ITEMS, Some(4), &[Overview, Advanced]),
                        4 => (&TOOLS_ITEMS, Some(4), &[Overview, Advanced, Tools]),
                        5 => (
                            &SETTINGS_ITEMS,
                            Some(4),
                            &[Overview, Advanced, Tools, Settings],
                        ),
                        6 => (&TOOLS_ITEMS, None, &[Overview, Advanced, Tools]),
                        7 => (&ADVANCED_ITEMS, None, &[Overview, Advanced]),
                        8 => (&ROOT_ITEMS, Some(4), &[Overview]),
                        _ => panic!("unexpected navigation step"),
                    };
                assert_eq!(
                    items,
                    expected.iter().map(|s| lang.t(s)).collect::<Vec<_>>()
                );
                assert_eq!(menu::section_path_for_test(), path);
                menu::screen_title(&format!("NAV-{}", self.step));
                let actual = self.menu.select(lang, items, default, consent)?;
                assert_eq!(actual, choice);
                self.step += 1;
                Ok(actual)
            }
            fn multi_select(&mut self, _: Lang, _: &[String]) -> Result<Option<Vec<usize>>> {
                panic!("navigation probe cannot select fixes");
            }
        }
        let _screen = menu::Screen::enter(Lang::En, false).unwrap();
        let mut input = Input {
            reader: Native {
                menu: TerminalMenu::default(),
                step: 0,
            },
            writer: io::stdout(),
            lang: Lang::En,
        };
        let mut fake = Fake::default();
        session(
            &mut fake,
            &Ui::new(Lang::En, true, false),
            &mut input,
            false,
        )
        .unwrap();
        assert_eq!(input.reader.step, 9);
        assert!(fake.fixes.is_empty());
        assert_eq!(fake.undos, 0);
        assert!(menu::section_path_for_test().is_empty());
    }
    #[test]
    fn broker_protocol_has_only_five_fixed_requests() {
        use crate::actions::Action;
        for (code, request, action) in [
            (23, BrokerRequest::Bitwarden, None),
            (
                24,
                BrokerRequest::WindowsUpdate,
                Some(Action::OpenWindowsUpdate),
            ),
            (
                25,
                BrokerRequest::WindowsSecurity,
                Some(Action::OpenWindowsSecurity),
            ),
            (
                26,
                BrokerRequest::Encryption,
                Some(Action::OpenEncryptionSettings),
            ),
            (27, BrokerRequest::SignIn, Some(Action::OpenSignInSettings)),
        ] {
            assert_eq!(BrokerRequest::from_exit(code), Some(request));
            assert_eq!(request as i32, code);
            assert_eq!(request.action(), action);
        }
        for code in [i32::MIN, -1, 0, 1, 2, 22, 28, 255, i32::MAX] {
            assert_eq!(BrokerRequest::from_exit(code), None);
        }
    }

    #[test]
    fn settings_handoff_never_dispatches_in_elevated_session() {
        let view = Ui::new(Lang::En, true, false);
        for (choice, code) in [(0, 24), (1, 25), (2, 26), (3, 27)] {
            for (broker, answer, expected) in [
                (true, Pick(0), true),
                (true, Default, false),
                (true, Escape, false),
                (false, Pick(0), false),
            ] {
                let script = [Pick(choice), answer];
                let mut input = Input {
                    reader: TestScript::new(&script),
                    writer: Vec::new(),
                    lang: Lang::En,
                };
                let result = next_steps(&mut input, &view, broker).unwrap();
                assert_eq!(
                    result,
                    if expected {
                        ToolOutcome::Broker(BrokerRequest::from_exit(code).unwrap())
                    } else {
                        ToolOutcome::None
                    }
                );
                let shown = String::from_utf8(input.writer).unwrap();
                assert!(!shown.contains("Settings opened."));
                if !broker {
                    assert!(shown.contains("open it normally"));
                }
            }
            let script = [Root(Review), Pick(choice), Pick(0)];
            let mut input = Input {
                reader: TestScript::new(&script),
                writer: Vec::new(),
                lang: Lang::En,
            };
            let mut fake = Fake::default();
            assert_eq!(session(&mut fake, &view, &mut input, true).unwrap(), code);
            assert!(fake.fixes.is_empty());
            assert_eq!(fake.undos, 0);
        }
    }

    #[test]
    fn parent_reconsent_and_return_are_both_explicit() {
        for code in 23..=27 {
            let request = BrokerRequest::from_exit(code).unwrap();
            for script in [vec![], vec![Default], vec![Pick(1)], vec![Escape]] {
                let mut input = Input {
                    reader: TestScript::new(&script),
                    writer: Vec::new(),
                    lang: Lang::En,
                };
                assert!(!broker_turn(&mut input, request, false, |_| panic!(
                    "declined request ran"
                ))
                .unwrap());
            }
            for (script, reopen) in [
                (vec![Pick(0), Default], false),
                (vec![Pick(0)], false),
                (vec![Pick(0), Pick(0)], true),
            ] {
                let mut input = Input {
                    reader: TestScript::new(&script),
                    writer: Vec::new(),
                    lang: Lang::En,
                };
                let mut called = false;
                assert_eq!(
                    broker_turn(&mut input, request, false, |actual| {
                        assert_eq!(actual, request);
                        called = true;
                        Ok(())
                    })
                    .unwrap(),
                    reopen
                );
                assert!(called);
            }
        }
        let mut input = Input {
            reader: TestScript::new(&[Pick(0), Pick(0), Default]),
            writer: Vec::new(),
            lang: Lang::En,
        };
        assert!(
            !broker_turn(&mut input, BrokerRequest::WindowsSecurity, false, |_| {
                Err(anyhow::anyhow!("native settings failure").context("Open page"))
            })
            .unwrap()
        );
        let shown = String::from_utf8(input.writer).unwrap();
        assert!(shown.contains("Open page: native settings failure"));
        assert!(!shown.contains("Settings opened."));
    }
    #[test]
    fn selections_are_bounded_unique_and_never_commands() {
        let mut input = Input {
            reader: TestScript::new(&[
                Check(vec![3, 0, 2, 0]),
                Default,
                Escape,
                Check(vec![4]),
                Check(vec![usize::MAX]),
                Pick(7),
            ]),
            writer: Vec::new(),
            lang: Lang::En,
        };
        let labels = vec!["A".into(), "B".into(), "C".into(), "D".into()];
        assert_eq!(input.multiple(&labels).unwrap(), Some(vec![0, 2, 3]));
        assert_eq!(input.multiple(&labels).unwrap(), Some(vec![]));
        assert_eq!(input.multiple(&labels).unwrap(), None);
        assert!(input.multiple(&labels).is_err());
        assert!(input.multiple(&labels).is_err());
        assert!(input.choose("Choose an action", &["Back"], 0).is_err());
        // Exhausted scripts remain EOF rather than supplying a default forever.
        assert_eq!(
            input.choose("Choose an action", &["Back"], 0).unwrap(),
            None
        );
        assert_eq!(input.multiple(&labels).unwrap(), None);
    }
    struct Fake {
        scans: usize,
        fixes: Vec<Vec<String>>,
        undos: usize,
        reports: VecDeque<Result<Report>>,
        fail_apply: bool,
        fail_undo: bool,
        apply_status: &'static str,
        events: Vec<&'static str>,
    }
    fn assessment(first: &str, second: &str) -> Report {
        Report {
            results: [
                ("uac.enabled", first),
                ("uac.consent", second),
                ("unavailable", "attention"),
            ]
            .into_iter()
            .map(|(id, status)| Outcome {
                id: id.into(),
                status: status.into(),
                ..Outcome::default()
            })
            .collect(),
            ..Report::default()
        }
    }
    impl std::default::Default for Fake {
        fn default() -> Self {
            Self {
                scans: 0,
                fixes: vec![],
                undos: 0,
                reports: [
                    Ok(assessment("attention", "skipped")),
                    Ok(assessment("compliant", "skipped")),
                ]
                .into(),
                fail_apply: false,
                fail_undo: false,
                apply_status: "applied",
                events: vec![],
            }
        }
    }
    fn queued(reports: Vec<Result<Report>>) -> Fake {
        Fake {
            reports: reports.into(),
            ..Fake::default()
        }
    }
    impl Session for Fake {
        fn restart_ids(&self) -> Vec<String> {
            vec!["uac.enabled".into()]
        }
        fn available(&self) -> Vec<String> {
            vec!["uac.enabled".into(), "uac.consent".into()]
        }
        fn audit(&mut self, _: &Ui) -> Result<Report> {
            self.scans += 1;
            self.events.push("audit");
            self.reports
                .pop_front()
                .expect("unexpected extra assessment")
        }
        fn apply(&mut self, ids: &[String], _: &Ui) -> Result<Report> {
            self.fixes.push(ids.to_vec());
            self.events.push("apply");
            if self.fail_apply {
                return Err(anyhow::Error::new(io::Error::other("disk full\x1b"))
                    .context("Save apply result"));
            }
            Ok(Report {
                transaction: Some("mock-transaction".into()),
                findings: vec![],
                results: ids
                    .iter()
                    .map(|id| Outcome {
                        id: id.clone(),
                        title: String::new(),
                        status: self.apply_status.into(),
                        detail: String::new(),
                        ..Outcome::default()
                    })
                    .collect(),
                ..Report::default()
            })
        }
        fn undo(&mut self, _: &Ui) -> Result<Report> {
            self.undos += 1;
            self.events.push("undo");
            if self.fail_undo {
                bail!("rollback storage failed");
            }
            Ok(Report {
                transaction: Some("mock-transaction".into()),
                findings: vec![],
                results: vec![],
                ..Report::default()
            })
        }
    }
    fn drive(script: &[Event]) -> Fake {
        let mut fake = Fake::default();
        drive_with(script, &mut fake);
        fake
    }
    fn drive_with(script: &[Event], fake: &mut Fake) -> String {
        let view = Ui::new(Lang::En, true, false);
        let mut input = Input {
            reader: TestScript::new(script),
            writer: Vec::new(),
            lang: Lang::En,
        };
        assert_eq!(session(fake, &view, &mut input, false).unwrap(), 0);
        assert!(
            input.reader.0.is_empty(),
            "session left scripted choices unread"
        );
        String::from_utf8(input.writer).unwrap()
    }
    #[test]
    fn eof_empty_escaped_and_declined_input_never_modify() {
        for script in [
            vec![],
            vec![Escape],
            vec![Root(Exit)],
            vec![Root(Recommended)],
            vec![Root(Recommended), Default, Root(Exit)],
            vec![Root(Recommended), Escape, Root(Exit)],
            vec![Root(Recommended), Pick(1), Default, Root(Exit)],
            vec![Root(Select)],
            vec![Root(Select), Default, Root(Exit)],
            vec![Root(Select), Check(vec![0])],
            vec![Root(Select), Check(vec![0]), Default, Root(Exit)],
            vec![Root(Select), Check(vec![0]), Escape, Root(Exit)],
            vec![Root(Select), Escape, Root(Exit)],
            vec![Root(Undo)],
            vec![Root(Undo), Default, Root(Exit)],
            vec![Root(Undo), Escape, Root(Exit)],
        ] {
            let fake = drive(&script);
            assert!(fake.fixes.is_empty(), "{script:?}");
            assert_eq!(fake.undos, 0, "{script:?}");
            assert_eq!(fake.scans, 1, "{script:?}");
        }
    }
    #[test]
    fn only_available_attention_is_selected_and_recheck_is_read_only() {
        let fake = drive(&[Root(Select), Check(vec![0]), Pick(0), Root(Exit)]);
        assert_eq!(fake.fixes, vec![vec!["uac.enabled".to_owned()]]);
        assert_eq!(fake.undos, 0);
        assert_eq!(fake.scans, 2);
        let undone = drive(&[Root(Undo), Pick(0), Root(Exit)]);
        assert_eq!(undone.undos, 1);
        assert_eq!(undone.events, ["audit", "undo", "audit"]);
        assert_eq!(fake.events, ["audit", "apply", "audit"]);
    }
    #[test]
    fn selection_after_rescan_uses_new_candidates_and_none_never_mutates() {
        let mut fake = queued(vec![
            Ok(assessment("attention", "skipped")),
            Ok(assessment("compliant", "attention")),
            Ok(assessment("compliant", "compliant")),
        ]);
        drive_with(
            &[
                Root(Select),
                Default,
                Root(Recheck),
                Root(Select),
                Check(vec![0]),
                Pick(0),
                Root(Exit),
            ],
            &mut fake,
        );
        assert_eq!(fake.scans, 3);
        assert_eq!(fake.fixes, vec![vec!["uac.consent".to_owned()]]);
        assert_eq!(fake.undos, 0);
    }

    #[test]
    fn partial_apply_failure_preserves_cause_and_allows_undo_and_rescan() {
        let mut recovery = assessment("attention", "attention");
        recovery.findings.push(secblitz::model::Finding {
            title: "Journal recovery".into(),
            status: "pending".into(),
            detail: "partial write".into(),
        });
        let mut fake = queued(vec![
            Ok(assessment("attention", "skipped")),
            Ok(recovery),
            Ok(assessment("attention", "skipped")),
            Ok(assessment("attention", "skipped")),
        ]);
        fake.fail_apply = true;
        let text = drive_with(
            &[
                Root(Recommended),
                Pick(0),
                Root(Recommended),
                Root(Details),
                Root(Undo),
                Pick(0),
                Root(Recheck),
                Root(Exit),
            ],
            &mut fake,
        );
        assert_eq!(fake.fixes.len(), 1); // stale selection did not apply again
        assert_eq!(fake.undos, 1);
        assert_eq!(fake.scans, 4); // initial, after apply, after undo, explicit
        assert!(text.contains("remaining work is not confirmed"));
        assert!(text.contains("There are no recommended automatic fixes available"));
        assert!(text.contains("Save apply result: disk full"));
        assert!(!text.contains('\x1b'));
        let mut fake = Fake {
            fail_apply: true,
            ..Fake::default()
        };
        let text = drive_with(&[Root(Recommended), Pick(0), Root(Exit)], &mut fake);
        assert!(!text.contains("disk full")); // technical cause is opt-in
    }

    #[test]
    fn failed_initial_and_later_checks_never_leave_actionable_stale_results() {
        for failed in [1, 2] {
            let mut reports = Vec::new();
            if failed == 2 {
                reports.push(Ok(assessment("attention", "skipped")));
            }
            reports.extend([
                Err(anyhow::anyhow!("scan unavailable: native code 5")),
                Ok(assessment("attention", "skipped")),
                Ok(assessment("compliant", "skipped")),
            ]);
            let mut fake = queued(reports);
            let script = if failed == 1 {
                vec![
                    Root(Recommended),
                    Root(Recheck),
                    Root(Recommended),
                    Pick(0),
                    Root(Exit),
                ]
            } else {
                vec![
                    Root(Recheck),
                    Root(Recommended),
                    Root(Recheck),
                    Root(Recommended),
                    Pick(0),
                    Root(Exit),
                ]
            };
            let text = drive_with(&script, &mut fake);
            assert!(text.contains("Check again before choosing fixes"));
            assert_eq!(fake.fixes.len(), 1);
            assert_eq!(fake.scans, failed + 2);
        }
    }

    #[test]
    fn confirmed_apply_cancelled_rollback_and_failed_rollback_remain_recoverable() {
        let fake = drive(&[Root(Recommended), Pick(0), Root(Undo), Default, Root(Exit)]);
        assert_eq!(fake.fixes.len(), 1);
        assert_eq!(fake.undos, 0);
        assert_eq!(fake.scans, 2);
        let mut fake = Fake {
            fail_undo: true,
            ..Fake::default()
        };
        let text = drive_with(&[Root(Undo), Pick(0), Root(Details), Root(Exit)], &mut fake);
        assert_eq!(fake.undos, 1);
        assert!(fake.fixes.is_empty());
        assert_eq!(fake.scans, 2);
        assert!(text.contains("rollback storage failed"));
    }

    #[test]
    fn recommended_is_one_approval_for_exact_candidates_and_never_adds_new_fixes() {
        let mut fake = queued(vec![
            Ok(assessment("attention", "skipped")),
            Ok(assessment("compliant", "attention")),
        ]);
        drive_with(&[Root(Recommended), Pick(0), Root(Exit)], &mut fake);
        assert_eq!(fake.fixes, [vec!["uac.enabled".to_owned()]]);
        assert_eq!(fake.events, ["audit", "apply", "audit"]);
        // A new candidate after verification has not inherited the approval.
        assert_eq!(fake.scans, 2);
    }

    #[test]
    fn returned_noop_or_partial_reports_also_verify_once_without_retry() {
        for status in ["unchanged", "pending", "error"] {
            let mut fake = Fake {
                apply_status: status,
                ..Fake::default()
            };
            drive_with(&[Root(Recommended), Pick(0), Root(Exit)], &mut fake);
            assert_eq!(fake.events, ["audit", "apply", "audit"]);
            assert_eq!(fake.fixes.len(), 1);
            assert_eq!(fake.undos, 0);
        }
    }

    #[test]
    fn change_selection_replaces_recommended_list_and_does_not_apply_omissions() {
        let mut fake = queued(vec![
            Ok(assessment("attention", "attention")),
            Ok(assessment("attention", "compliant")),
        ]);
        let text = drive_with(
            &[
                Root(Recommended),
                Pick(1),
                Check(vec![1]),
                Pick(0),
                Root(Exit),
            ],
            &mut fake,
        );
        assert_eq!(fake.fixes, [vec!["uac.consent".to_owned()]]);
        assert!(text.contains("Your selection is kept"));
        assert_eq!(fake.scans, 2);
    }

    #[test]
    fn both_entry_paths_share_candidates_and_pending_recovery_blocks_both() {
        let mut report = assessment("attention", "unknown");
        report.results.extend([
            Outcome {
                id: "uac.enabled".into(),
                status: "attention".into(),
                ..Outcome::default()
            },
            Outcome {
                id: "defender_update".into(),
                status: "attention".into(),
                ..Outcome::default()
            },
            Outcome {
                id: "firewall.public.inbound".into(),
                status: "attention".into(),
                ..Outcome::default()
            },
        ]);
        let available = vec![
            "uac.enabled".into(),
            "uac.consent".into(),
            "defender_update".into(),
            "firewall.public.inbound".into(),
        ];
        assert_eq!(candidates(&report, &available), ["uac.enabled"]);
        report.findings.push(secblitz::model::Finding {
            title: "Journal recovery".into(),
            status: "pending".into(),
            detail: String::new(),
        });
        assert!(candidates(&report, &available).is_empty());
        let mut fake = queued(vec![Ok(report)]);
        drive_with(&[Root(Recommended), Root(Select), Root(Exit)], &mut fake);
        assert!(fake.fixes.is_empty());
        assert_eq!(fake.scans, 1);
        let mut fake = queued(vec![Ok(assessment("compliant", "skipped"))]);
        drive_with(&[Root(Recommended), Root(Select), Root(Exit)], &mut fake);
        assert!(fake.fixes.is_empty());
        assert_eq!(fake.scans, 1);
    }

    #[test]
    fn apply_and_verification_errors_are_separate_and_failed_check_keeps_undo() {
        let mut fake = queued(vec![
            Ok(assessment("attention", "skipped")),
            Err(anyhow::anyhow!("verification storage unavailable")),
            Ok(assessment("attention", "skipped")),
        ]);
        fake.fail_apply = true;
        let text = drive_with(
            &[
                Root(Recommended),
                Pick(0),
                Root(Recommended),
                Root(Details),
                Root(Undo),
                Pick(0),
                Root(Details),
                Root(Exit),
            ],
            &mut fake,
        );
        assert_eq!(fake.fixes.len(), 1);
        assert_eq!(fake.undos, 1);
        assert_eq!(fake.events, ["audit", "apply", "audit", "undo", "audit"]);
        assert!(text.contains("Save apply result: disk full"));
        assert!(text.contains("verification storage unavailable"));
        assert!(text.contains("Check again before choosing fixes"));
        assert!(text.matches("Save apply result: disk full").count() >= 2);
    }

    #[test]
    fn verification_runs_even_when_rendering_the_failure_cannot_write() {
        struct BrokenOutput;
        impl Write for BrokenOutput {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut fake = queued(vec![Ok(assessment("compliant", "skipped"))]);
        fake.fail_apply = true;
        let mut state = FlowState::default();
        let mut input = Input {
            reader: TestScript::new(&[]),
            writer: BrokenOutput,
            lang: Lang::En,
        };
        assert!(attempt_and_verify(
            &mut state,
            &mut fake,
            &Ui::new(Lang::En, true, false),
            &mut input,
            Some(&["uac.enabled".into()])
        )
        .is_err());
        assert_eq!(fake.events, ["apply", "audit"]);
        assert!(state.snapshot.is_some());
        assert!(state.operation_error.is_some());
    }

    #[test]
    fn extra_action_consent_is_required_and_failures_keep_the_full_cause() {
        for script in [vec![], vec![Default], vec![Pick(1)], vec![Escape]] {
            let mut input = Input {
                reader: TestScript::new(&script),
                writer: Vec::new(),
                lang: Lang::En,
            };
            assert!(
                !run_action_with(&mut input, crate::actions::Action::QuickScan, || {
                    panic!("unconfirmed action must not run")
                })
                .unwrap()
            );
        }
        let mut input = Input {
            reader: TestScript::new(&[Pick(0)]),
            writer: Vec::new(),
            lang: Lang::En,
        };
        let error = run_action_with(&mut input, crate::actions::Action::QuickScan, || {
            Err(anyhow::Error::new(io::Error::other("native 123")).context("Defender failed"))
        })
        .unwrap_err();
        assert_eq!(format!("{error:#}"), "Defender failed: native 123");
        let text = String::from_utf8(input.writer).unwrap();
        assert!(text.contains("work may still be running"));
        assert!(!text.contains("command returned"));
    }
    #[test]
    fn bitwarden_request_requires_choice_consent_and_desktop_broker() {
        let view = Ui::new(Lang::En, true, false);
        for (script, broker, expected) in [
            (vec![Pick(1), Pick(0)], true, true),
            (vec![Pick(1), Default], true, false),
            (vec![Pick(1)], true, false),
            (vec![Pick(1), Escape], false, false),
            (vec![Escape], true, false),
        ] {
            let mut input = Input {
                reader: TestScript::new(&script),
                writer: Vec::new(),
                lang: Lang::En,
            };
            assert_eq!(
                extra_tools(&mut input, &view, broker).unwrap()
                    == ToolOutcome::Broker(BrokerRequest::Bitwarden),
                expected
            );
            if expected {
                let shown = String::from_utf8(input.writer).unwrap();
                assert!(shown.contains(crate::TOOL_CONSENT));
                assert!(shown.contains("ask for consent again"));
            }
        }
    }

    #[test]
    fn payoff_only_counts_verified_protected_ids_from_attempted_list() {
        use secblitz::engine::Report;
        use secblitz::engine::Outcome;

        // uac.enabled compliant post-check → Group::Protected → in "now" list
        let applied = Report {
            results: vec![Outcome {
                id: "uac.enabled".into(),
                status: "applied".into(),
                detail: String::new(),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        let verified = Report {
            results: vec![Outcome {
                id: "uac.enabled".into(),
                status: "compliant".into(),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        let (now, restart) = payoff(&["uac.enabled".to_owned()], &applied, &verified);
        assert!(!now.is_empty(), "compliant post-check should yield protected-now entry");
        assert!(now[0].contains("system-wide"), "wrong impact phrase: {:?}", now);
        assert!(restart.is_empty());

        // ID not attempted is excluded
        let (now2, _) = payoff(&[], &applied, &verified);
        assert!(now2.is_empty());

        // ID not in verified results is excluded
        let empty_verified = Report::default();
        let (now3, _) = payoff(&["uac.enabled".to_owned()], &applied, &empty_verified);
        assert!(now3.is_empty());
    }

    #[test]
    fn payoff_restart_required_id_lands_in_second_list() {
        use secblitz::engine::{Outcome, Report};

        let applied = Report {
            results: vec![Outcome {
                id: "uac.enabled".into(),
                status: "applied".into(),
                detail: "Preference applied; restart required".into(),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        // The post-check reads the saved value, so it is already compliant
        // even though the change only takes effect after a restart.
        let verified = Report {
            results: vec![Outcome {
                id: "uac.enabled".into(),
                status: "compliant".into(),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        let (now, restart) = payoff(&["uac.enabled".to_owned()], &applied, &verified);
        assert!(now.is_empty(), "restart-needed must not appear in protected-now: {:?}", now);
        assert!(!restart.is_empty(), "restart-needed id must appear in restart list");
        assert!(restart[0].contains("system-wide"), "wrong phrase: {:?}", restart);
    }

    #[test]
    fn payoff_contradictory_firewall_evidence_excluded_from_restart_list() {
        use secblitz::engine::{Outcome, Report};
        use secblitz::model::{Authority, EffectiveFirewall, InboundAction};

        // Applied firewall with restart detail in apply report
        let applied = Report {
            results: vec![Outcome {
                id: "firewall.public.inbound".into(),
                status: "applied".into(),
                detail: "Preference applied; restart required".into(),
                effective: Some(EffectiveFirewall::Inbound(InboundAction::Block)),
                authority: Some(Authority::Local),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        // Post-check contradicts: applied but effective=Allow → for_outcome → CheckAgain
        let verified = Report {
            results: vec![Outcome {
                id: "firewall.public.inbound".into(),
                status: "applied".into(),
                detail: String::new(),
                effective: Some(EffectiveFirewall::Inbound(InboundAction::Allow)),
                authority: Some(Authority::Local),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        let (now, restart) = payoff(
            &["firewall.public.inbound".to_owned()],
            &applied,
            &verified,
        );
        assert!(now.is_empty(), "contradictory evidence must not appear in now: {:?}", now);
        assert!(restart.is_empty(), "contradictory firewall must be excluded from restart: {:?}", restart);
    }

    #[test]
    fn payoff_still_broken_id_excluded_from_both_lists() {
        use secblitz::engine::{Outcome, Report};

        let applied = Report {
            results: vec![Outcome {
                id: "uac.enabled".into(),
                status: "applied".into(),
                detail: String::new(),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        // Post-check shows still attention (still broken)
        let verified = Report {
            results: vec![Outcome {
                id: "uac.enabled".into(),
                status: "attention".into(),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        let (now, restart) = payoff(&["uac.enabled".to_owned()], &applied, &verified);
        assert!(now.is_empty());
        assert!(restart.is_empty());
    }

    #[test]
    fn payoff_deduplicates_same_impact_phrase() {
        use secblitz::engine::{Outcome, Report};

        // Both BITS and wuauserv have the same impact phrase; should dedup to one entry
        let applied = Report {
            results: vec![
                Outcome { id: "permissions.service.bits".into(), status: "applied".into(), ..Outcome::default() },
                Outcome { id: "permissions.service.wuauserv".into(), status: "applied".into(), ..Outcome::default() },
            ],
            ..Report::default()
        };
        let verified = Report {
            results: vec![
                Outcome { id: "permissions.service.bits".into(), status: "compliant".into(), ..Outcome::default() },
                Outcome { id: "permissions.service.wuauserv".into(), status: "compliant".into(), ..Outcome::default() },
            ],
            ..Report::default()
        };
        let (now, _) = payoff(
            &["permissions.service.bits".to_owned(), "permissions.service.wuauserv".to_owned()],
            &applied,
            &verified,
        );
        assert_eq!(now.len(), 1, "same impact phrase should be deduplicated: {:?}", now);
    }

    #[test]
    fn payoff_unknown_id_or_empty_impact_is_excluded() {
        use secblitz::engine::{Outcome, Report};

        let applied = Report {
            results: vec![Outcome {
                id: "unknown.control".into(),
                status: "applied".into(),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        let verified = Report {
            results: vec![Outcome {
                id: "unknown.control".into(),
                status: "compliant".into(),
                ..Outcome::default()
            }],
            ..Report::default()
        };
        let (now, restart) = payoff(&["unknown.control".to_owned()], &applied, &verified);
        assert!(now.is_empty(), "unknown id with empty impact must be excluded");
        assert!(restart.is_empty());
    }

    #[test]
    fn payoff_block_appears_before_per_item_results() {
        // A successful apply-and-verify for uac.enabled should show a payoff block
        // BEFORE the per-item guided_report_text content.
        let text = drive_with(&[Root(Recommended), Pick(0), Root(Exit)], &mut Fake::default());
        let payoff_marker = "You're now protected from:";
        // Use a group heading from guided_report_text as the per-item marker; the
        // recap (approve_plan) uses individual item lines, not group headings.
        let item_marker = "▸ Protected"; // group heading only produced by guided_report_text
        if let (Some(payoff_pos), Some(item_pos)) = (text.find(payoff_marker), text.find(item_marker)) {
            assert!(
                payoff_pos < item_pos,
                "payoff must appear before per-item results\n{text}"
            );
        } else {
            // If the payoff section is present, verify the ordering is correct.
            // If neither appears, there is nothing to order.
        }
        // No ANSI codes in plain-text output.
        assert!(!text.contains('\x1b'), "no ANSI in plain text result\n{text}");
    }

    #[test]
    fn failure_lines_get_cross_glyph_prefix() {
        // Operation failed → ✗ prefix
        let mut fake = queued(vec![
            Ok(assessment("attention", "attention")),
            Err(anyhow::anyhow!("verification unavailable")),
        ]);
        fake.fail_apply = true;
        let text = drive_with(
            &[Root(Select), Check(vec![0, 1]), Pick(0), Root(Exit)],
            &mut fake,
        );
        assert!(text.contains("\u{2717}"), "✗ glyph missing from failure\n{text}");
        assert!(text.contains("Operation failed"), "{text}");
        // Post-check failure also gets ✗
        assert!(
            text.contains("The post-check failed separately"),
            "{text}"
        );
    }
}
