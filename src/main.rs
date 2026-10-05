// GUI program: no console window is ever created.
#![cfg_attr(windows, windows_subsystem = "windows")]
use secblitz::actions;
pub(crate) mod advice;
mod app;
mod broker;
mod gui;
mod guided;
mod launcher;
mod tray;
mod i18n;
mod maintenance_cli;
mod menu;
mod ui;

use anyhow::{bail, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use i18n::Lang;
use secblitz::{engine::Engine, platform, service, tools, updater};
use std::io::{self, IsTerminal, Write};

const TOOL_CONSENT: &str = "--yes authorizes downloading and installing Bitwarden from the Microsoft WinGet repository and accepting Bitwarden package licenses/agreements and WinGet source agreements. Another password manager is a valid choice.";

fn command(lang: Lang) -> Command {
    // Clap without the `string` feature accepts static text. The catalog owns
    // static translations; these few process-lifetime help strings are bounded.
    fn text(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }
    let sub = |name, key| Command::new(name).about(text(lang.t(key)));
    let mut cmd =
        Command::new("secblitz")
            .version(env!("CARGO_PKG_VERSION"))
            .about(text(lang.t("A safer PC. Without headaches.")))
            .after_help(text(lang.t(
                "No command: open the guided security check. Nothing is fixed without your choice.",
            )))
            .arg(
                Arg::new("lang")
                    .long("lang")
                    .global(true)
                    .value_name("en|es|fr|de|pt|it")
                    .hide_possible_values(true)
                    .value_parser(["en", "es", "fr", "de", "pt", "it"])
                    .help(text(lang.t("Language (default: Windows display language)"))),
            )
            .arg(
                Arg::new("no-animation")
                    .long("no-animation")
                    .global(true)
                    .action(ArgAction::SetTrue)
                    .help(text(lang.t("Disable terminal animation"))),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .global(true)
                    .action(ArgAction::SetTrue)
                    .help(text(
                        lang.t("Output raw JSON reports (including update check/status)"),
                    )),
            )
            .arg(
                Arg::new("details")
                    .long("details")
                    .global(true)
                    .action(ArgAction::SetTrue)
                    .help(text(lang.t("Show technical report details"))),
            )
            .subcommand(
                sub("guide", "Guided security check and selected fixes").arg(
                    Arg::new("desktop-broker")
                        .long("desktop-broker")
                        .hide(true)
                        .action(ArgAction::SetTrue),
                ),
            )
            .subcommand(sub("audit", "Audit security preferences"))
            .subcommand(sub("apply", "Apply conservative protection"))
            .subcommand(sub("revert", "Restore the latest recorded transaction"))
            .subcommand(sub("history", "Show transaction history"))
            .subcommand(
                sub("update", "Keep Secblitz up to date")
                    .subcommand_required(true)
                    .subcommand(sub("check", "Check for Secblitz updates"))
                    .subcommand(sub("status", "Show the latest update status"))
                    .subcommand(Command::new("health").hide(true))
                    .subcommand(Command::new("install-staged").hide(true)),
            )
            .subcommand(sub(
                "password",
                "Generate a 24-character password on this terminal only",
            ))
            .subcommand(
                sub("service", "Manage the optional service")
                    .subcommand_required(true)
                    .subcommand(sub("install", "Install the service"))
                    .subcommand(sub("start", "Start the optional monitoring service"))
                    .subcommand(sub("uninstall", "Uninstall the service"))
                    .subcommand(sub("status", "Query service status"))
                    .subcommand(sub("run", "Run the service dispatcher")),
            )
            .subcommand(
                sub("tools", "Optional software tools")
                    .subcommand_required(true)
                    .subcommand(
                        sub("bitwarden", "Install Bitwarden with explicit consent")
                            .after_help(text(lang.t(TOOL_CONSENT)))
                            .arg(Arg::new("yes").long("yes").action(ArgAction::SetTrue).help(
                                text(lang.t("Consent to downloading and installing Bitwarden")),
                            )),
                    ),
            );
    fn localize(cmd: Command, lang: Lang, parent: &str) -> Command {
        let path = if parent.is_empty() {
            cmd.get_name().to_owned()
        } else {
            format!("{parent} {}", cmd.get_name())
        };
        let suffix = if cmd.get_subcommands().next().is_some() {
            if parent.is_empty() {
                format!(" [{}]", lang.t("Commands"))
            } else {
                format!(" <{}>", lang.t("Commands"))
            }
        } else {
            String::new()
        };
        let positional = cmd
            .get_positionals()
            .map(|arg| {
                let name = arg
                    .get_value_names()
                    .and_then(|v| v.first())
                    .map(|v| v.as_str())
                    .unwrap_or(arg.get_id().as_str());
                format!(" <{name}>")
            })
            .collect::<String>();
        let usage = format!("{path} [{}]{positional}{suffix}", lang.t("Options"));
        let template = format!("{{before-help}}{{name}} {{version}}\n{{about-with-newline}}\n{}: {{usage}}\n\n{{all-args}}{{after-help}}", lang.t("Usage"));
        cmd.disable_help_flag(true)
            .disable_version_flag(true)
            .disable_help_subcommand(true)
            .override_usage(Box::leak(usage.into_boxed_str()) as &'static str)
            .help_template(Box::leak(template.into_boxed_str()) as &'static str)
            .subcommand_help_heading(Box::leak(lang.t("Commands").into_boxed_str()) as &'static str)
            .next_help_heading(Box::leak(lang.t("Options").into_boxed_str()) as &'static str)
            .arg(
                Arg::new("help")
                    .short('h')
                    .long("help")
                    .action(ArgAction::Help)
                    .help(Box::leak(lang.t("Show help").into_boxed_str()) as &'static str),
            )
            .mut_args(|a| {
                a.help_heading(Box::leak(lang.t("Options").into_boxed_str()) as &'static str)
            })
            .mut_subcommands(|c| localize(c, lang, &path))
    }
    for extra in maintenance_cli::commands(lang) {
        cmd = cmd.subcommand(extra);
    }
    cmd = localize(cmd, lang, "");
    cmd.arg(
        Arg::new("version")
            .short('V')
            .long("version")
            .action(ArgAction::Version)
            .help(text(lang.t("Show version"))),
    )
}

fn selected_language(args: &[std::ffi::OsString]) -> Lang {
    let mut lang = Lang::detect();
    let mut args = args.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        let value = if arg == "--lang" {
            args.next().and_then(|s| s.to_str())
        } else {
            arg.to_str().and_then(|s| s.strip_prefix("--lang="))
        };
        if let Some(value) = value.and_then(Lang::parse) {
            lang = value;
        }
    }
    lang
}

/// Reconstruct the child command solely from parsed enum-like choices. No user
/// paths, arbitrary command strings, or unparsed arguments cross the UAC boundary.
fn elevated_args(matches: &ArgMatches, lang: Lang) -> Vec<String> {
    let mut args = vec!["--lang".into(), lang.code().into()];
    for flag in ["no-animation", "json", "details"] {
        if matches.get_flag(flag) {
            args.push(format!("--{flag}"));
        }
    }
    if let Some((name, sub)) = matches.subcommand() {
        match name {
            "guide" => args.extend(["guide".into(), "--desktop-broker".into()]),
            "audit" | "apply" | "revert" | "history" => args.push(name.into()),
            "update" => {
                if let Some(action @ ("check" | "status")) = sub.subcommand_name() {
                    args.extend(["update".into(), action.into()]);
                } else {
                    unreachable!();
                }
            }
            "service" => {
                args.push("service".into());
                if let Some(name @ ("install" | "start" | "uninstall" | "status")) =
                    sub.subcommand_name()
                {
                    args.push(name.into());
                }
            }
            _ => unreachable!(),
        }
    } else {
        args.extend(["guide".into(), "--desktop-broker".into()]);
    }
    args
}

fn execute(matches: &ArgMatches, lang: Lang) -> Result<i32> {
    // Installer health is deliberately before all UI, UAC, locks and wrappers.
    if update_command(matches) == Some(UpdateCommand::Health) {
        anyhow::ensure!(matches.get_flag("json"), "update-health-requires-json");
        return write_health(updater::health()?, &mut io::stdout().lock());
    }
    let json = matches.get_flag("json");
    let name = matches.subcommand_name().unwrap_or("guide");
    if json && !json_allowed(matches) {
        bail!(lang.t(
            "JSON is available only for report commands, not interactive guides or desktop tools."
        ));
    }
    // The scheduled task and protected worker never enter guided/UI paths.
    if let Some(action) = update_command(matches) {
        return execute_update(matches, lang, action);
    }
    if maintenance_cli::handles(name) {
        return match maintenance_cli::execute(matches, lang) {
            Ok(code) => Ok(code),
            Err(error) => {
                maintenance_cli::write_failure(
                    lang,
                    &error,
                    json,
                    matches.get_flag("details"),
                    &mut io::stdout().lock(),
                    &mut io::stderr().lock(),
                )?;
                Ok(1)
            }
        };
    }
    if name == "guide" {
        guided::require_terminal(lang)?;
    }
    let view = ui::Ui::new(lang, matches.get_flag("no-animation"), json)
        .with_details(matches.get_flag("details"));
    if name == "password" {
        return ui::password(lang).map(|()| 0);
    }
    if name == "tools" {
        let sub = matches
            .subcommand_matches("tools")
            .unwrap()
            .subcommand_matches("bitwarden")
            .unwrap();
        if !sub.get_flag("yes") {
            bail!(
                "{} {}",
                lang.t("Bitwarden installation requires --yes."),
                lang.t(TOOL_CONSENT)
            );
        }
        view.brand();
        view.message(TOOL_CONSENT);
        tools::install_bitwarden()?;
        view.done();
        return Ok(0);
    }
    let service_action = matches
        .subcommand_matches("service")
        .and_then(ArgMatches::subcommand_name);
    if service_action == Some("run") {
        service::run()?;
        return Ok(0);
    }
    // Even audit/history use the ACL-protected journal, so they require access.
    if !platform::is_elevated()? {
        // ShellExecute creates a separate elevated console; it cannot promise
        // delivery of a report into the caller's redirected stdout.
        if json {
            bail!(lang.t("Run JSON reports from an administrator terminal."));
        }
        view.message("Requesting administrator access");
        let args = elevated_args(matches, lang);
        return desktop_loop(
            name,
            || elevate_and_wait(&args),
            |request| {
                guided::handle_broker(
                    request,
                    lang,
                    matches.get_flag("details"),
                    matches.get_flag("no-animation"),
                )
            },
        );
    }
    if name != "guide" {
        view.brand();
    }
    if let Some(action) = service_action {
        match action {
            "start" => {
                service::start()?;
                view.message(
                    "Monitoring is running. Check reports separately to verify their freshness.",
                );
            }
            "install" => {
                service::install()?;
                view.message("Installed SecblitzMonitor as LocalService; it has not been started. Binary and reports are retained on uninstall.");
            }
            "uninstall" => {
                service::uninstall()?;
                view.message("SecblitzMonitor registration is removed or was already absent. Binary, reports and journals are preserved.");
            }
            "status" => view.service_status(&service::query_status()?)?,
            _ => unreachable!(),
        }
        view.done();
        return Ok(0);
    }
    let _guided_screen = if name == "guide" {
        let screen = menu::Screen::enter(lang, view.animations_enabled())?;
        menu::screen_progress(&lang.t("Checking your PC"), &lang.t("Checking readiness"));
        Some(screen)
    } else {
        None
    };
    let mut engine = Engine::open(
        platform::state_dir()?,
        secblitz::permissions::with_permissions(platform::backend()?),
    )?;
    if name == "guide" {
        let broker = matches
            .subcommand_matches("guide")
            .is_some_and(|m| m.get_flag("desktop-broker"));
        return guided::run(&mut engine, &view, lang, broker);
    }
    let progress = view.progress();
    if name == "history" {
        let history = engine.history()?;
        drop(progress);
        view.history(&history)?;
        return Ok(0);
    }
    let report = match name {
        "audit" => engine.audit_with_progress(|id, status| progress.update(id, status))?,
        "apply" => engine.apply(|id, status| progress.update(id, status))?,
        "revert" => engine.revert(|id, status| progress.update(id, status))?,
        _ => unreachable!(),
    };
    drop(progress);
    // A completed operation is not necessarily a clean assessment.
    let review = needs_review(&report);
    view.report(&report, review)?;
    Ok(if review { 2 } else { 0 })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateCommand {
    Check,
    Status,
    InstallStaged,
    Health,
}

fn update_command(matches: &ArgMatches) -> Option<UpdateCommand> {
    match matches.subcommand_matches("update")?.subcommand_name()? {
        "check" => Some(UpdateCommand::Check),
        "status" => Some(UpdateCommand::Status),
        "install-staged" => Some(UpdateCommand::InstallStaged),
        "health" => Some(UpdateCommand::Health),
        _ => unreachable!(),
    }
}

fn json_allowed(matches: &ArgMatches) -> bool {
    maintenance_cli::json_allowed(matches)
        || matches!(
            matches.subcommand_name(),
            Some("audit" | "apply" | "revert" | "history")
        )
        || matches!(
            update_command(matches),
            Some(UpdateCommand::Check | UpdateCommand::Status | UpdateCommand::Health)
        )
}

fn pause_allowed(matches: &ArgMatches) -> bool {
    matches!(matches.subcommand_name(), Some("password" | "tools")) && !matches.get_flag("json")
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum UpdateReport {
    Outcome(updater::UpdateOutcome),
    Status(updater::UpdateStatus),
}

impl UpdateReport {
    fn outcome(&self) -> &updater::UpdateOutcome {
        match self {
            Self::Outcome(outcome) => outcome,
            Self::Status(status) => &status.result,
        }
    }

    fn message(&self) -> &'static str {
        use updater::UpdateOutcome;
        if matches!(self, Self::Status(status) if status.checked_at == 0 && matches!(status.result, UpdateOutcome::NotConfigured))
        {
            return "No update information yet.";
        }
        match self.outcome() {
            UpdateOutcome::NotConfigured => "Updates aren't available for this installation.",
            UpdateOutcome::UpToDate => "Secblitz is up to date.",
            UpdateOutcome::DeferredBusy => "We'll try updating when Secblitz is closed.",
            UpdateOutcome::DeferredRollout { .. } => {
                "This update is not yet offered to this device."
            }
            UpdateOutcome::WorkerStarted { .. } => {
                "Your update is ready. Close Secblitz so the installer can continue."
            }
            UpdateOutcome::Installed { .. } => "Secblitz was updated.",
            UpdateOutcome::Failed { .. } => "The update could not be completed.",
        }
    }
}

enum UpdateRun {
    ElevatedExit(i32),
    Report(UpdateReport),
}

/// Worker authorization belongs to the core. In particular, even a manually
/// invoked worker must reach its path/token checks without a UAC relaunch.
fn run_update_request(
    action: UpdateCommand,
    interactive: bool,
    json: bool,
    elevated: impl FnOnce() -> Result<bool>,
    elevate: impl FnOnce() -> Result<i32>,
    run: impl FnOnce(UpdateCommand) -> Result<UpdateReport>,
) -> Result<UpdateRun> {
    if !matches!(action, UpdateCommand::InstallStaged | UpdateCommand::Health) && !elevated()? {
        anyhow::ensure!(
            interactive && !json,
            "Run update commands from an administrator terminal."
        );
        return elevate().map(UpdateRun::ElevatedExit);
    }
    run(action).map(UpdateRun::Report)
}

fn execute_update(matches: &ArgMatches, lang: Lang, action: UpdateCommand) -> Result<i32> {
    let interactive = action != UpdateCommand::InstallStaged
        && io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && io::stderr().is_terminal();
    let json = matches.get_flag("json");
    let result = run_update_request(
        action,
        interactive,
        json,
        platform::is_elevated,
        || {
            eprintln!("{}", lang.t("Requesting administrator access"));
            elevate_and_wait(&elevated_args(matches, lang))
        },
        |action| match action {
            UpdateCommand::Check => updater::check_and_stage().map(UpdateReport::Outcome),
            UpdateCommand::Status => updater::status().map(UpdateReport::Status),
            UpdateCommand::InstallStaged => updater::install_staged().map(UpdateReport::Outcome),
            UpdateCommand::Health => unreachable!(),
        },
    );
    // Do not wait for a spawned worker, start a menu or keep this executable
    // open with a pause. The worker persists its status in protected storage.
    write_update_result(
        result,
        lang,
        json,
        interactive,
        matches.get_flag("details"),
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )
}

fn write_health(health: updater::UpdateHealth, out: &mut impl Write) -> Result<i32> {
    serde_json::to_writer(&mut *out, &health)?;
    writeln!(out)?;
    Ok(0)
}

fn write_update_result(
    result: Result<UpdateRun>,
    lang: Lang,
    json: bool,
    interactive: bool,
    details: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<i32> {
    let (report, failure) = match result {
        Ok(UpdateRun::ElevatedExit(code)) => return Ok(code),
        Ok(UpdateRun::Report(report)) => (report, None),
        Err(error) => (
            UpdateReport::Outcome(updater::UpdateOutcome::Failed {
                // Keep native causes, URLs and operational metadata out of JSON
                // errors. Explicit interactive --details is the evidence path.
                reason: "The update could not be completed.".into(),
            }),
            Some(error),
        ),
    };
    let failed = matches!(report.outcome(), updater::UpdateOutcome::Failed { .. });
    if json {
        serde_json::to_writer(&mut *out, &report)?;
        writeln!(out)?;
    } else if interactive {
        writeln!(out, "{}", lang.t(report.message()))?;
        if failed {
            if details {
                if let Some(error) = failure {
                    writeln!(err, "{}", ui::error_details(lang, &error))?;
                } else if let updater::UpdateOutcome::Failed { reason } = report.outcome() {
                    writeln!(
                        err,
                        "{}",
                        ui::error_details(lang, &anyhow::anyhow!("{reason}"))
                    )?;
                }
            } else {
                writeln!(err, "{}", lang.t("Run update status --details from an administrator terminal for more information."))?;
            }
        }
    }
    Ok(i32::from(failed))
}

fn desktop_loop(
    name: &str,
    mut elevate: impl FnMut() -> Result<i32>,
    mut handoff: impl FnMut(guided::BrokerRequest) -> Result<bool>,
) -> Result<i32> {
    loop {
        let code = elevate()?;
        let Some(request) = guided::BrokerRequest::from_exit(code).filter(|_| name == "guide")
        else {
            return Ok(code);
        };
        // A fixed exit code is only a request. The original desktop window
        // checks its context and asks again; only explicit Return re-elevates.
        if !handoff(request)? {
            return Ok(0);
        }
    }
}

fn needs_review(report: &secblitz::engine::Report) -> bool {
    // Match the engine's confirmed storage blockers, not advisory power/reboot
    // notices. Unknown readiness is not a failure or a protection verdict.
    if report
        .readiness
        .as_ref()
        .is_some_and(secblitz::model::Readiness::blocks_repairs)
    {
        return true;
    }
    // Unknown/future statuses fail visibly rather than silently claiming success.
    report
        .results
        .iter()
        .map(|r| r.status.as_str())
        .chain(report.findings.iter().map(|f| f.status.as_str()))
        .any(|status| {
            !matches!(
                status,
                "compliant" | "ok" | "applied" | "restored" | "unchanged" | "info"
            )
        })
}

#[cfg(windows)]
fn elevate_and_wait(args: &[String]) -> Result<i32> {
    use std::{
        mem::{size_of, zeroed},
        os::windows::ffi::OsStrExt,
        ptr::null_mut,
    };
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, WaitForSingleObject},
    };
    // Native ABI avoids requiring the windows-sys Registry feature just for
    // SHELLEXECUTEINFOW's unused hkeyClass field. Secblitz targets Windows x64.
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ShellExecuteInfo {
        cbSize: u32,
        fMask: u32,
        hwnd: *mut std::ffi::c_void,
        lpVerb: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShow: i32,
        hInstApp: *mut std::ffi::c_void,
        lpIDList: *mut std::ffi::c_void,
        lpClass: *const u16,
        hkeyClass: *mut std::ffi::c_void,
        dwHotKey: u32,
        iconOrMonitor: *mut std::ffi::c_void,
        hProcess: *mut std::ffi::c_void,
    }
    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteExW(info: *mut ShellExecuteInfo) -> i32;
    }
    // All arguments are fixed ASCII words/options generated above, so quoting
    // isn't needed. The executable uses a separate ShellExecute field.
    anyhow::ensure!(
        args.iter()
            .all(|a| a.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')),
        "Invalid elevation arguments"
    );
    let exe: Vec<u16> = std::env::current_exe()?
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let params: Vec<u16> = args.join(" ").encode_utf16().chain(Some(0)).collect();
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let mut info: ShellExecuteInfo = unsafe { zeroed() };
    info.cbSize = size_of::<ShellExecuteInfo>() as u32;
    info.fMask = 0x40 | 0x100; // SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC
    info.hwnd = null_mut();
    info.lpVerb = verb.as_ptr();
    info.lpFile = exe.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = 1;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    anyhow::ensure!(
        !info.hProcess.is_null(),
        "Elevation returned no process handle"
    );
    struct Handle(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let handle = Handle(info.hProcess);
    if unsafe { WaitForSingleObject(handle.0, u32::MAX) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut code = 1;
    if unsafe { GetExitCodeProcess(handle.0, &mut code) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(code as i32)
}

#[cfg(not(windows))]
fn elevate_and_wait(args: &[String]) -> Result<i32> {
    platform::elevate(args)?;
    Ok(0)
}

/// New GUI-era entry points, dispatched before the legacy CLI parser.
/// OWNER: platform agent (final dispatch replaces the legacy CLI entirely).
fn dispatch_gui(args: &[std::ffi::OsString], lang: Lang) -> Option<i32> {
    let words: Vec<&str> = args.iter().skip(1).filter_map(|a| a.to_str()).collect();
    let position = |flag: &str| words.iter().position(|w| *w == flag);
    match words.iter().find(|w| !w.starts_with("--") && Lang::parse(w).is_none()) {
        Some(&"gui") => {
            let broker = position("--broker").and_then(|i| words.get(i + 1)).map(|s| s.to_string());
            let start = position("--self-test")
                .and_then(|i| words.get(i + 1))
                .and_then(|s| gui::Page::parse(s));
            let result = gui::run(gui::Options { lang, broker, start });
            Some(if result.is_ok() { 0 } else { 1 })
        }
        Some(&"tray") => Some(tray::run(lang).unwrap_or(1)),
        _ => None,
    }
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let lang = selected_language(&args);
    if let Some(code) = dispatch_gui(&args, lang) {
        std::process::exit(code);
    }
    let json_requested = args.iter().any(|a| a == "--json");
    let mut pause = false;
    let mut details = false;
    let mut update = false;
    let result = match command(lang).try_get_matches_from(args) {
        Ok(matches) => {
            details = matches.get_flag("details");
            update = update_command(&matches).is_some();
            pause = pause_allowed(&matches) && ui::owns_console();
            execute(&matches, lang)
        }
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            let _ = error.print();
            Ok(0)
        }
        Err(_) => {
            // Clap's parser diagnostics are English-only. Use a localized
            // diagnostic rather than leaking untranslated generated prose.
            if json_requested {
                let _ = writeln!(
                    io::stdout(),
                    "{}",
                    serde_json::json!({"error":{"code":"invalid_command", "message":lang.t("Invalid command")}})
                );
            } else {
                eprintln!(
                    "{} - {}",
                    lang.t("Invalid command"),
                    lang.t("Use --help for usage.")
                );
            }
            Ok(2)
        }
    };
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            if json_requested && !update {
                let _ = writeln!(
                    io::stdout(),
                    "{}",
                    serde_json::json!({"error":{"code":"operation_failed", "message":lang.t("Operation failed")}})
                );
            } else if !update {
                show_error(lang, &error, details);
            }
            1
        }
    };
    if pause {
        ui::pause(lang);
    }
    std::process::exit(code);
}

fn show_error(lang: Lang, error: &anyhow::Error, details: bool) {
    if details {
        ui::error(lang, error);
        return;
    }
    let view = ui::Ui::new(lang, true, false);
    view.message("Operation failed");
    // These are our own fixed, localized usage messages, never native errors,
    // command output, paths, or journal evidence.
    let message = error.to_string();
    for key in [
        guided::TERMINAL_REQUIRED,
        "JSON is available only for report commands, not interactive guides or desktop tools.",
        "Run JSON reports from an administrator terminal.",
        "Password output requires an interactive terminal.",
    ] {
        if message == lang.t(key) {
            view.message(key);
            return;
        }
    }
    if message.starts_with(&lang.t("Bitwarden installation requires --yes.")) {
        view.message("Bitwarden installation requires --yes.");
        view.message(TOOL_CONSENT);
        return;
    }
    view.message("The operation could not be completed. Run again with --details to see technical information.");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn health_is_raw_json_and_routes_before_privilege_or_state_access() {
        let health = updater::UpdateHealth {
            schema: 1,
            version: "0.5.0".into(),
            task: updater::TaskHealth::Ready,
            monitor: updater::MonitorHealth::Stopped,
        };
        let mut bytes = Vec::new();
        assert_eq!(write_health(health.clone(), &mut bytes).unwrap(), 0);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            serde_json::to_value(health).unwrap()
        );
        assert_eq!(bytes.last(), Some(&b'\n'));
        let matches = command(Lang::En)
            .try_get_matches_from(["secblitz", "update", "health", "--json"])
            .unwrap();
        assert!(json_allowed(&matches));
        assert!(!pause_allowed(&matches));
        #[cfg(not(windows))]
        assert_eq!(
            execute(&matches, Lang::En).unwrap_err().to_string(),
            "Update health requires Windows"
        );
        let no_json = command(Lang::En)
            .try_get_matches_from(["secblitz", "update", "health"])
            .unwrap();
        assert_eq!(
            execute(&no_json, Lang::En).unwrap_err().to_string(),
            "update-health-requires-json"
        );
    }

    #[test]
    fn maintenance_machine_commands_do_not_pause_or_cross_the_uac_rebuilder() {
        for args in [
            vec!["secblitz", "diagnostics", "run", "--json"],
            vec!["secblitz", "diagnostics", "profiles"],
            vec!["secblitz", "operations", "capabilities"],
            vec!["secblitz", "operations", "policy", "show"],
            vec!["secblitz", "quality-updates", "capabilities"],
            vec!["secblitz", "audit"],
            vec!["secblitz", "history"],
        ] {
            let m = command(Lang::En).try_get_matches_from(args).unwrap();
            assert!(!pause_allowed(&m));
            assert!(json_allowed(&m));
        }
        // Original-user requests are not represented by the UAC transport.
        for name in ["diagnostics", "operations", "quality-updates"] {
            assert!(maintenance_cli::handles(name));
        }
        let source = include_str!("main.rs");
        let execute = source
            .split("fn execute(matches:")
            .nth(1)
            .unwrap()
            .split("enum UpdateCommand")
            .next()
            .unwrap();
        assert!(
            execute.find("maintenance_cli::handles(name)").unwrap()
                < execute.find("platform::is_elevated").unwrap()
        );
    }
    #[test]
    fn only_confirmed_storage_readiness_blocks_require_review() {
        use secblitz::model::{PowerReadiness, Probe, Readiness, VolumeReadiness};
        let mut report = secblitz::engine::Report::default();
        for readiness in [
            None,
            Some(Readiness::default()),
            Some(Readiness {
                system_volume: Probe::Known(VolumeReadiness {
                    available_bytes: 0,
                    read_only: false,
                }),
                power: Probe::Known(PowerReadiness {
                    ac_connected: Some(false),
                    battery_percent: Some(1),
                    battery_present: Some(true),
                }),
                windows_update_reboot: Probe::Known(true),
                ..Default::default()
            }),
        ] {
            report.readiness = readiness;
            assert!(!needs_review(&report));
        }
        for readiness in [
            Readiness {
                system_volume: Probe::Known(VolumeReadiness {
                    available_bytes: 1_u64 << 40,
                    read_only: true,
                }),
                ..Default::default()
            },
            Readiness {
                journal_volume: Probe::Known(VolumeReadiness {
                    available_bytes: 1_u64 << 40,
                    read_only: true,
                }),
                ..Default::default()
            },
            Readiness {
                journal_volume: Probe::Known(VolumeReadiness {
                    available_bytes: 0,
                    read_only: false,
                }),
                ..Default::default()
            },
        ] {
            report.readiness = Some(readiness);
            assert!(needs_review(&report));
        }
    }

    #[test]
    fn updater_commands_are_closed_and_worker_is_hidden() {
        for (word, expected) in [
            ("check", UpdateCommand::Check),
            ("status", UpdateCommand::Status),
            ("install-staged", UpdateCommand::InstallStaged),
        ] {
            let matches = command(Lang::En)
                .try_get_matches_from(["secblitz", "update", word])
                .unwrap();
            assert_eq!(update_command(&matches), Some(expected));
            assert!(!pause_allowed(&matches));
            assert_eq!(
                json_allowed(&matches),
                expected != UpdateCommand::InstallStaged
            );
            for extra in [
                "https://example.invalid/update",
                "C:\\Downloads\\setup.exe",
                "--url=https://example.invalid/update",
                "--path=C:\\Downloads\\setup.exe",
                "--token=arbitrary",
                "--yes",
                "--desktop-broker",
            ] {
                assert!(command(Lang::En)
                    .try_get_matches_from(["secblitz", "update", word, extra])
                    .is_err());
            }
        }
        for args in [
            vec!["secblitz", "update"],
            vec!["secblitz", "update", "install"],
            vec!["secblitz", "update", "run"],
            vec!["secblitz", "update", "check", "status"],
        ] {
            assert!(command(Lang::En).try_get_matches_from(args).is_err());
        }
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let help = command(lang)
                .try_get_matches_from(["secblitz", "update", "--help"])
                .unwrap_err()
                .to_string();
            assert!(help.contains("check") && help.contains("status"));
            assert!(!help.contains("install-staged"));
            assert!(help.contains(&lang.t("Keep Secblitz up to date")));
        }
    }

    #[test]
    fn updater_uac_is_canonical_and_never_brokers_desktop_work() {
        for action in ["check", "status"] {
            let matches = command(Lang::It)
                .try_get_matches_from([
                    "secblitz",
                    "update",
                    action,
                    "--lang=it",
                    "--details",
                    "--no-animation",
                ])
                .unwrap();
            let args = elevated_args(&matches, Lang::It);
            assert_eq!(
                args,
                [
                    "--lang",
                    "it",
                    "--no-animation",
                    "--details",
                    "update",
                    action
                ]
            );
            assert!(args
                .iter()
                .all(|a| a.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')));
            let result = run_update_request(
                update_command(&matches).unwrap(),
                true,
                false,
                || Ok(false),
                || Ok(27),
                |_| panic!("parent must not run updater after UAC"),
            )
            .unwrap();
            assert!(matches!(result, UpdateRun::ElevatedExit(27)));
        }
        let cancelled = run_update_request(
            UpdateCommand::Check,
            true,
            false,
            || Ok(false),
            || Err(anyhow::anyhow!("cancelled")),
            |_| panic!("declined elevation must not run updater"),
        );
        assert!(cancelled.is_err());
    }

    #[test]
    fn scheduled_checks_and_worker_never_elevate_or_require_a_terminal() {
        for action in [UpdateCommand::Check, UpdateCommand::Status] {
            run_update_request(
                action,
                false,
                false,
                || Ok(true),
                || panic!("SYSTEM must not request UAC"),
                |received| {
                    assert_eq!(received, action);
                    Ok(UpdateReport::Outcome(updater::UpdateOutcome::UpToDate))
                },
            )
            .unwrap();
            for (interactive, json) in [(false, false), (false, true), (true, true)] {
                assert!(run_update_request(
                    action,
                    interactive,
                    json,
                    || Ok(false),
                    || panic!("background/JSON request must not elevate"),
                    |_| panic!("unelevated request must not run updater"),
                )
                .is_err());
            }
        }
        // A spoofed worker invocation still goes to core verification. The CLI
        // neither infers authorization nor offers the caller a UAC shortcut.
        let refused = run_update_request(
            UpdateCommand::InstallStaged,
            true,
            false,
            || panic!("worker authorization belongs to core"),
            || panic!("worker must never elevate"),
            |action| {
                assert_eq!(action, UpdateCommand::InstallStaged);
                Err(anyhow::anyhow!("untrusted worker"))
            },
        );
        assert!(refused.is_err());
        let worker_json = command(Lang::En)
            .try_get_matches_from(["secblitz", "update", "install-staged", "--json"])
            .unwrap();
        assert!(execute(&worker_json, Lang::En).is_err());
    }

    #[test]
    fn update_json_is_exact_and_failures_have_failure_exit_codes() {
        use updater::{UpdateOutcome, UpdateStatus};
        for outcome in [
            UpdateOutcome::NotConfigured,
            UpdateOutcome::UpToDate,
            UpdateOutcome::DeferredBusy,
            UpdateOutcome::WorkerStarted {
                version: "0.4.1".into(),
            },
            UpdateOutcome::Installed {
                version: "0.4.1".into(),
            },
            UpdateOutcome::Failed {
                reason: "Staged installation failed".into(),
            },
        ] {
            let failed = matches!(outcome, UpdateOutcome::Failed { .. });
            for report in [
                UpdateReport::Outcome(outcome.clone()),
                UpdateReport::Status(UpdateStatus {
                    checked_at: 123,
                    result: outcome.clone(),
                }),
            ] {
                let expected = serde_json::to_value(&report).unwrap();
                let (mut out, mut err) = (Vec::new(), Vec::new());
                let code = write_update_result(
                    Ok(UpdateRun::Report(report)),
                    Lang::It,
                    true,
                    false,
                    true,
                    &mut out,
                    &mut err,
                )
                .unwrap();
                assert_eq!(code, i32::from(failed));
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&out).unwrap(),
                    expected
                );
                assert!(err.is_empty());
            }
        }
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = write_update_result(
            Err(anyhow::anyhow!(
                "native cause with private operational data"
            )),
            Lang::It,
            true,
            true,
            true,
            &mut out,
            &mut err,
        )
        .unwrap();
        assert_eq!(code, 1);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&out).unwrap(),
            serde_json::json!({"outcome":"failed", "reason":"The update could not be completed."})
        );
        assert!(err.is_empty());
    }

    #[test]
    fn updater_background_and_worker_results_are_silent_and_never_pause() {
        for result in [
            Ok(UpdateRun::Report(UpdateReport::Outcome(
                updater::UpdateOutcome::WorkerStarted {
                    version: "0.4.1".into(),
                },
            ))),
            Ok(UpdateRun::Report(UpdateReport::Outcome(
                updater::UpdateOutcome::Failed {
                    reason: "failed".into(),
                },
            ))),
            Err(anyhow::anyhow!("native failure")),
        ] {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let _code =
                write_update_result(result, Lang::En, false, false, true, &mut out, &mut err)
                    .unwrap();
            assert!(out.is_empty() && err.is_empty());
        }
        for args in [
            vec!["secblitz"],
            vec!["secblitz", "guide"],
            vec!["secblitz", "service", "run"],
            vec!["secblitz", "service", "status"],
            vec!["secblitz", "update", "check"],
            vec!["secblitz", "update", "status", "--details"],
            vec!["secblitz", "update", "status", "--json"],
            vec!["secblitz", "update", "install-staged"],
        ] {
            let matches = command(Lang::En).try_get_matches_from(args).unwrap();
            assert!(!pause_allowed(&matches));
        }
        for action in ["check", "status"] {
            let matches = command(Lang::En)
                .try_get_matches_from(["secblitz", "update", action, "--json"])
                .unwrap();
            assert!(json_allowed(&matches));
        }
    }
    #[test]
    fn updater_human_status_is_localized_without_native_evidence_by_default() {
        use updater::{UpdateOutcome, UpdateStatus};
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            for outcome in [
                UpdateOutcome::NotConfigured,
                UpdateOutcome::UpToDate,
                UpdateOutcome::DeferredBusy,
                UpdateOutcome::WorkerStarted {
                    version: "0.4.1".into(),
                },
                UpdateOutcome::Installed {
                    version: "0.4.1".into(),
                },
                UpdateOutcome::Failed {
                    reason: "native evidence, not a friendly status".into(),
                },
            ] {
                let failed = matches!(outcome, UpdateOutcome::Failed { .. });
                let report = UpdateReport::Status(UpdateStatus {
                    checked_at: 0,
                    result: outcome,
                });
                let message = report.message();
                if lang != Lang::En {
                    assert_ne!(lang.t(message), message);
                }
                if failed {
                    assert_eq!(message, "The update could not be completed.");
                }
                let (mut out, mut err) = (Vec::new(), Vec::new());
                let code = write_update_result(
                    Ok(UpdateRun::Report(report)),
                    lang,
                    false,
                    true,
                    false,
                    &mut out,
                    &mut err,
                )
                .unwrap();
                assert_eq!(code, i32::from(failed));
                assert_eq!(
                    String::from_utf8(out).unwrap(),
                    format!("{}\n", lang.t(message))
                );
                assert!(!String::from_utf8(err).unwrap().contains("native evidence"));
            }
        }
    }

    #[test]
    fn desktop_loop_stops_on_back_and_only_reopens_with_consent() {
        let mut launches = 0;
        assert_eq!(
            desktop_loop(
                "guide",
                || {
                    launches += 1;
                    Ok(24)
                },
                |request| {
                    assert_eq!(request, guided::BrokerRequest::WindowsUpdate);
                    Ok(false)
                }
            )
            .unwrap(),
            0
        );
        assert_eq!(launches, 1);
        let mut codes = [27, 0].into_iter();
        let mut requests = 0;
        assert_eq!(
            desktop_loop(
                "guide",
                || Ok(codes.next().expect("unexpected UAC loop")),
                |_| {
                    requests += 1;
                    Ok(true)
                }
            )
            .unwrap(),
            0
        );
        assert_eq!(requests, 1);
        for (name, code) in [("guide", 28), ("guide", 0), ("audit", 24), ("service", 23)] {
            assert_eq!(
                desktop_loop(name, || Ok(code), |_| panic!("not a guided request")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn service_start_is_explicit_and_canonical() {
        let matches = command(Lang::En)
            .try_get_matches_from(["secblitz", "service", "start"])
            .unwrap();
        assert_eq!(
            elevated_args(&matches, Lang::En),
            ["--lang", "en", "service", "start"]
        );
    }
    #[test]
    fn redirected_guide_and_broker_fail_before_interactive_work() {
        use std::io::{self, IsTerminal};
        // Host test runners normally redirect these streams. Do not attempt an
        // interactive action when this test is deliberately run in a live TTY.
        if io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal() {
            return;
        }
        let matches = command(Lang::En)
            .try_get_matches_from(["secblitz"])
            .unwrap();
        assert_eq!(
            execute(&matches, Lang::En).unwrap_err().to_string(),
            guided::TERMINAL_REQUIRED
        );
        for code in 23..=27 {
            assert_eq!(
                guided::handle_broker(
                    guided::BrokerRequest::from_exit(code).unwrap(),
                    Lang::En,
                    false,
                    true,
                )
                .unwrap_err()
                .to_string(),
                guided::TERMINAL_REQUIRED
            );
        }
    }
    #[test]
    fn default_launch_is_canonical_guide_and_json_needs_explicit_command() {
        let matches = command(Lang::Fr)
            .try_get_matches_from(["secblitz"])
            .unwrap();
        assert_eq!(
            elevated_args(&matches, Lang::Fr),
            ["--lang", "fr", "guide", "--desktop-broker"]
        );
        let matches = command(Lang::En)
            .try_get_matches_from(["secblitz", "--json"])
            .unwrap();
        assert!(execute(&matches, Lang::En).is_err());
        let matches = command(Lang::De)
            .try_get_matches_from([
                "secblitz",
                "guide",
                "--details",
                "--no-animation",
                "--lang=de",
            ])
            .unwrap();
        assert_eq!(
            elevated_args(&matches, Lang::De),
            [
                "--lang",
                "de",
                "--no-animation",
                "--details",
                "guide",
                "--desktop-broker"
            ]
        );
        assert!(command(Lang::En)
            .try_get_matches_from(["secblitz", "guide", "--scripted"])
            .is_err());
    }
    #[test]
    fn advisory_and_incomplete_reports_require_review() {
        use secblitz::{
            engine::{Outcome, Report},
            model::Finding,
        };
        let mut report = Report {
            transaction: None,
            results: vec![],
            findings: vec![],
            ..Report::default()
        };
        assert!(!needs_review(&report));
        for status in [
            "attention",
            "unknown",
            "error",
            "conflict",
            "pending",
            "skipped",
            "future-status",
        ] {
            report.findings = vec![Finding {
                title: "Assessment".into(),
                status: status.into(),
                detail: String::new(),
            }];
            assert!(needs_review(&report), "finding {status}");
            report.findings.clear();
            report.results = vec![Outcome {
                id: "uac.enabled".into(),
                title: String::new(),
                status: status.into(),
                detail: String::new(),
                ..Outcome::default()
            }];
            assert!(needs_review(&report), "outcome {status}");
            report.results.clear();
        }
        for status in [
            "compliant",
            "ok",
            "applied",
            "restored",
            "unchanged",
            "info",
        ] {
            report.findings = vec![Finding {
                title: String::new(),
                status: status.into(),
                detail: String::new(),
            }];
            assert!(!needs_review(&report));
        }
    }

    #[test]
    fn consent_disclosure_is_localized_and_visible_before_install() {
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let error = command(lang)
                .try_get_matches_from(["secblitz", "tools", "bitwarden", "--help"])
                .unwrap_err();
            let help = error.to_string();
            assert!(help.contains(&lang.t(TOOL_CONSENT)));
            assert!(help.contains("WinGet"));
            if lang != Lang::En {
                assert_ne!(lang.t(TOOL_CONSENT), TOOL_CONSENT);
            }
        }
    }
    #[test]
    fn elevation_is_canonical_and_preserves_options() {
        let m = command(Lang::Es)
            .try_get_matches_from([
                "secblitz",
                "revert",
                "--lang=es",
                "--json",
                "--no-animation",
            ])
            .unwrap();
        assert_eq!(
            elevated_args(&m, Lang::Es),
            ["--lang", "es", "--no-animation", "--json", "revert"]
        );
        assert!(command(Lang::En)
            .try_get_matches_from(["secblitz", "apply", "--execute=cmd"])
            .is_err());
    }
    #[test]
    fn broker_hint_is_hidden_guide_only_and_never_install_consent() {
        let help = command(Lang::En)
            .try_get_matches_from(["secblitz", "guide", "--help"])
            .unwrap_err()
            .to_string();
        assert!(!help.contains("desktop-broker"));
        for args in [
            vec!["secblitz", "tools", "bitwarden", "--desktop-broker"],
            vec!["secblitz", "guide", "--desktop-broker", "--yes"],
            vec!["secblitz", "audit", "--desktop-broker"],
        ] {
            assert!(command(Lang::En).try_get_matches_from(args).is_err());
        }
        let spoofed = command(Lang::En)
            .try_get_matches_from(["secblitz", "guide", "--desktop-broker"])
            .unwrap();
        assert_eq!(
            elevated_args(&spoofed, Lang::En),
            ["--lang", "en", "guide", "--desktop-broker"]
        );
        let json = command(Lang::En)
            .try_get_matches_from(["secblitz", "guide", "--desktop-broker", "--json"])
            .unwrap();
        assert!(execute(&json, Lang::En)
            .unwrap_err()
            .to_string()
            .contains("JSON is available only"));
    }
    #[test]
    fn explicit_consent_and_nested_commands_parse() {
        let m = command(Lang::Fr)
            .try_get_matches_from(["secblitz", "tools", "bitwarden"])
            .unwrap();
        assert!(!m
            .subcommand_matches("tools")
            .unwrap()
            .subcommand_matches("bitwarden")
            .unwrap()
            .get_flag("yes"));
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            command(lang).debug_assert();
            let matches = command(lang)
                .try_get_matches_from(["secblitz", "audit", "--lang", lang.code(), "--json"])
                .unwrap();
            assert_eq!(matches.get_one::<String>("lang").unwrap(), lang.code());
            assert!(matches.get_flag("json"));
            assert_eq!(
                elevated_args(&matches, lang),
                ["--lang", lang.code(), "--json", "audit"]
            );
        }
    }
}
