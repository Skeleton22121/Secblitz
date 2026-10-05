//! Typed command/UI adapter. No paths, scripts, JSON plans or account identities
//! are accepted. Library modules own durable records and native authority checks.
use crate::{i18n::Lang, menu::ChoiceInput, ui::Ui};
use anyhow::{Context as _, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use secblitz::{diagnostics as d, operations as o, patching as p, platform};
use serde::Serialize;
use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const ADMIN_REQUIRED: &str = "Open an administrator terminal in your own desktop account and run this command there. No automatic account substitution is used.";
pub const ORIGINAL_REQUIRED: &str = "Original-user inventory requires your normal, non-administrator desktop terminal. Administrator profiles are never substituted.";
const CONSENT: &str = "Review the exact plan and digest first. Approval does not execute it. Repairs and scans may change files or quarantine threats; there is no automatic rollback or reboot.";
const QUALITY_CONSENT: &str = "Review every selected update, bundled update and EULA. Consent authorizes the Microsoft Windows Update source, reviewed licenses, downloads and installation. There is no automatic rollback or reboot.";
const WAITING: &str = "Keep this window open until Windows finishes. Ctrl+C requests cancellation of subsequent work, not termination of servicing.";
const FAILED: &str = "Maintenance did not finish. Review the plan, owner policy and readiness. Use --details for technical information.";
const OPS: [&str; 6] = [
    "dism_check_health",
    "dism_scan_health",
    "dism_restore_health",
    "sfc_verify",
    "sfc_repair",
    "defender_quick_scan",
];
const PROFILES: [&str; 4] = ["everyday", "gaming", "development", "higher-security"];
const NEEDS: [&str; 5] = ["printers", "nas", "vpn", "games", "development"];

fn text(lang: Lang, key: &str) -> &'static str {
    Box::leak(lang.t(key).into_boxed_str())
}
fn sub(lang: Lang, name: &'static str, key: &str) -> Command {
    Command::new(name).about(text(lang, key))
}
fn yes(lang: Lang) -> Arg {
    Arg::new("yes")
        .long("yes")
        .action(ArgAction::SetTrue)
        .help(text(lang, "Explicit consent to the displayed selection"))
}
fn id() -> Arg {
    Arg::new("id")
        .required(true)
        .value_name("UUID")
        .value_parser(parse_uuid)
}
fn hash() -> Arg {
    Arg::new("digest")
        .long("digest")
        .required(true)
        .value_name("SHA256")
        .value_parser(parse_digest)
}
fn lifetime(default: &'static str, max: u64) -> Arg {
    Arg::new("valid-for")
        .long("valid-for")
        .value_name("SECONDS")
        .default_value(default)
        .value_parser(clap::value_parser!(u64).range(1..=max))
}
fn source(lang: Lang) -> Arg {
    Arg::new("accept-source")
        .long("accept-source")
        .action(ArgAction::SetTrue)
        .help(text(lang, "Consent to contacting Microsoft Windows Update"))
}
fn quality_consent(command: Command, lang: Lang) -> Command {
    command
        .arg(yes(lang))
        .arg(source(lang))
        .arg(
            Arg::new("accept-eulas")
                .long("accept-eulas")
                .action(ArgAction::SetTrue)
                .help(text(lang, "Accept all EULAs in this exact reviewed plan")),
        )
        .arg(
            Arg::new("acknowledge-no-rollback")
                .long("acknowledge-no-rollback")
                .action(ArgAction::SetTrue)
                .help(text(
                    lang,
                    "Acknowledge that automatic rollback is unavailable",
                )),
        )
        .after_help(text(lang, QUALITY_CONSENT))
}

pub fn commands(lang: Lang) -> [Command; 3] {
    let diagnostics = sub(
        lang,
        "diagnostics",
        "Read-only diagnostics and compatibility advice",
    )
    .subcommand_required(true)
    .subcommand(sub(lang, "profiles", "Show diagnostic profiles"))
    .subcommand(sub(lang, "guide", "Choose diagnostics with arrow keys"))
    .subcommand(
        sub(lang, "run", "Collect a read-only diagnostic report")
            .arg(
                Arg::new("profile")
                    .long("profile")
                    .default_value("everyday")
                    .value_parser(PROFILES)
                    .help(text(lang, "Advice profile; it does not authorize changes")),
            )
            .arg(
                Arg::new("context")
                    .long("context")
                    .action(ArgAction::Append)
                    .value_delimiter(',')
                    .value_parser(NEEDS)
                    .help(text(lang, "Declared compatibility needs")),
            )
            .arg(
                Arg::new("original-user")
                    .long("original-user")
                    .action(ArgAction::SetTrue)
                    .help(text(
                        lang,
                        "Include verified original-user browser inventory",
                    )),
            ),
    );
    let policy = sub(lang, "policy", "Owner maintenance policy")
        .subcommand_required(true)
        .subcommand(sub(lang, "show", "Show owner policy"))
        .subcommand(sub(lang, "reset", "Reset to diagnostics-only defaults").arg(yes(lang)))
        .subcommand(
            sub(lang, "set", "Replace owner policy with typed settings")
                .arg(
                    Arg::new("allow")
                        .long("allow")
                        .required(true)
                        .action(ArgAction::Append)
                        .value_delimiter(',')
                        .value_parser(parse_operation)
                        .help(text(
                            lang,
                            "Complete allowed-operation list, including dependencies",
                        )),
                )
                .arg(
                    Arg::new("opt-in-for")
                        .long("opt-in-for")
                        .value_name("SECONDS")
                        .value_parser(clap::value_parser!(u64).range(1..=2592000))
                        .help(text(lang, "Repair and scan opt-in lifetime")),
                )
                .arg(
                    Arg::new("window-start")
                        .long("window-start")
                        .default_value("60")
                        .value_parser(clap::value_parser!(u16).range(0..1440))
                        .help(text(lang, "UTC window start, minutes after midnight")),
                )
                .arg(
                    Arg::new("window-end")
                        .long("window-end")
                        .default_value("300")
                        .value_parser(clap::value_parser!(u16).range(0..1440))
                        .help(text(lang, "UTC window end, minutes after midnight")),
                )
                .arg(
                    Arg::new("idle-seconds")
                        .long("idle-seconds")
                        .default_value("300")
                        .value_parser(clap::value_parser!(u32).range(60..=86400))
                        .help(text(lang, "Required idle time in seconds")),
                )
                .arg(
                    Arg::new("exception")
                        .long("exception")
                        .action(ArgAction::Append)
                        .value_name("KIND:SCOPE:SECONDS")
                        .value_parser(parse_exception)
                        .help(text(
                            lang,
                            "Scoped exception: active-use, window or metered; at most 24 hours",
                        )),
                )
                .arg(yes(lang)),
        );
    let operations = sub(
        lang,
        "operations",
        "Durable maintenance plans and verification",
    )
    .subcommand_required(true)
    .subcommand(sub(lang, "guide", "Choose maintenance with arrow keys"))
    .subcommand(sub(
        lang,
        "capabilities",
        "Show supported operations and risks",
    ))
    .subcommand(sub(lang, "list", "List saved plans"))
    .subcommand(sub(lang, "show", "Show an exact saved plan").arg(id()))
    .subcommand(
        sub(lang, "plan", "Create a plan without executing it")
            .arg(
                Arg::new("operations")
                    .required(true)
                    .num_args(1..=6)
                    .value_delimiter(',')
                    .value_parser(parse_operation)
                    .value_name("KIND"),
            )
            .arg(lifetime("3600", 86400)),
    )
    .subcommand(
        sub(lang, "approve", "Approve the exact displayed digest")
            .arg(id())
            .arg(hash())
            .arg(lifetime("900", 900))
            .arg(yes(lang))
            .after_help(text(lang, CONSENT)),
    )
    .subcommand(
        sub(lang, "run", "Run an approved single-use plan")
            .arg(id())
            .arg(hash())
            .arg(yes(lang))
            .after_help(text(lang, CONSENT)),
    )
    .subcommand(
        sub(
            lang,
            "resume",
            "Verify interrupted work without replaying it",
        )
        .arg(id()),
    )
    .subcommand(policy);
    let quality = sub(
        lang,
        "quality-updates",
        "Exact selected Windows quality updates",
    )
    .subcommand_required(true)
    .subcommand(sub(lang, "guide", "Choose quality updates with arrow keys"))
    .subcommand(sub(
        lang,
        "capabilities",
        "Show quality-update support and limits",
    ))
    .subcommand(
        sub(
            lang,
            "discover",
            "Discover eligible updates without installing",
        )
        .arg(source(lang)),
    )
    .subcommand(sub(lang, "list", "List saved plans"))
    .subcommand(sub(lang, "show", "Show an exact saved plan").arg(id()))
    .subcommand(
        sub(lang, "plan", "Create a plan without executing it")
            .arg(
                Arg::new("update")
                    .long("update")
                    .required(true)
                    .action(ArgAction::Append)
                    .value_parser(parse_update)
                    .value_name("UUID:REVISION"),
            )
            .arg(lifetime("3600", 86400))
            .arg(source(lang)),
    )
    .subcommand(quality_consent(
        sub(lang, "approve", "Approve the exact displayed digest")
            .arg(id())
            .arg(hash())
            .arg(lifetime("900", 3600)),
        lang,
    ))
    .subcommand(quality_consent(
        sub(lang, "install", "Install only the exact approved selection")
            .arg(id())
            .arg(hash()),
        lang,
    ))
    .subcommand(
        sub(
            lang,
            "verify",
            "Verify installed identities without replaying",
        )
        .arg(id()),
    );
    [diagnostics, operations, quality]
}

fn parse_uuid(s: &str) -> std::result::Result<Uuid, String> {
    let id = Uuid::parse_str(s).map_err(|_| "UUID".to_owned())?;
    if id.is_nil() || id.to_string() != s {
        return Err("UUID".into());
    }
    Ok(id)
}
fn parse_digest(s: &str) -> std::result::Result<String, String> {
    if s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(s.into())
    } else {
        Err("SHA256".into())
    }
}
fn parse_operation(s: &str) -> std::result::Result<o::OperationKind, String> {
    s.parse().map_err(|_| "OperationKind".into())
}
fn parse_update(s: &str) -> std::result::Result<p::UpdateIdentity, String> {
    let (id, revision) = s.split_once(':').ok_or("UUID:REVISION")?;
    let revision: u32 = revision.parse().map_err(|_| "REVISION")?;
    if revision == 0 || revision.to_string() != s.split_once(':').unwrap().1 {
        return Err("REVISION".into());
    }
    Ok(p::UpdateIdentity {
        update_id: parse_uuid(id)?,
        revision,
    })
}
#[derive(Clone, Debug)]
struct ExceptionArg {
    operation: o::OperationKind,
    scope: o::ExceptionScope,
    seconds: u64,
}
fn parse_exception(s: &str) -> std::result::Result<ExceptionArg, String> {
    let parts: Vec<_> = s.split(':').collect();
    if parts.len() != 3 {
        return Err("KIND:SCOPE:SECONDS".into());
    }
    let scope = match parts[1] {
        "active-use" => o::ExceptionScope::ActiveUse,
        "window" => o::ExceptionScope::MaintenanceWindow,
        "metered" => o::ExceptionScope::MeteredNetwork,
        _ => return Err("SCOPE".into()),
    };
    let seconds = parts[2].parse::<u64>().map_err(|_| "SECONDS")?;
    if !(1..=86400).contains(&seconds) {
        return Err("SECONDS".into());
    }
    Ok(ExceptionArg {
        operation: parse_operation(parts[0])?,
        scope,
        seconds,
    })
}

#[derive(Debug)]
pub struct UserError(pub &'static str);
impl std::fmt::Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for UserError {}
fn require(condition: bool, message: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(UserError(message).into())
    }
}
fn time() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
pub fn handles(name: &str) -> bool {
    matches!(name, "diagnostics" | "operations" | "quality-updates")
}
pub fn json_allowed(matches: &ArgMatches) -> bool {
    matches
        .subcommand()
        .is_some_and(|(name, m)| handles(name) && m.subcommand_name() != Some("guide"))
}
fn consent(m: &ArgMatches) -> Result<()> {
    require(
        m.get_flag("yes"),
        "Explicit --yes consent is required after reviewing the plan or policy.",
    )
}
fn source_consent(m: &ArgMatches) -> Result<()> {
    require(
        m.get_flag("accept-source"),
        "Contacting Windows Update requires --accept-source.",
    )
}
fn selected_updates(m: &ArgMatches) -> Result<Vec<p::UpdateIdentity>> {
    let selected: Vec<_> = m
        .get_many::<p::UpdateIdentity>("update")
        .unwrap()
        .cloned()
        .collect();
    require(
        (1..=32).contains(&selected.len())
            && selected
                .iter()
                .enumerate()
                .all(|(i, identity)| !selected[..i].contains(identity)),
        "Select between 1 and 32 distinct exact update identities.",
    )?;
    Ok(selected)
}
fn patch_consent(m: &ArgMatches) -> Result<p::Consent> {
    consent(m)?;
    source_consent(m)?;
    require(m.get_flag("accept-eulas") && m.get_flag("acknowledge-no-rollback"), "Quality-update approval and installation require --accept-eulas and --acknowledge-no-rollback.")?;
    Ok(p::Consent {
        owner_opt_in: true,
        accept_windows_update_source: true,
        accept_reviewed_eulas: true,
        acknowledge_no_automatic_rollback: true,
    })
}
fn profile(word: &str) -> d::Profile {
    match word {
        "gaming" => d::Profile::Gaming,
        "development" => d::Profile::Development,
        "higher-security" => d::Profile::HigherSecurity,
        _ => d::Profile::Everyday,
    }
}
fn context(needs: &[String], original_user: bool) -> d::Context {
    let has = |value| needs.iter().any(|s| s == value);
    d::Context {
        original_user: if original_user {
            d::OriginalUserScope::VerifyCurrentDesktopUser
        } else {
            d::OriginalUserScope::Omit
        },
        compatibility: d::CompatibilityNeeds {
            printers: has("printers"),
            nas: has("nas"),
            vpn: has("vpn"),
            games: has("games"),
            development: has("development"),
        },
    }
}

/// No maintenance request is handed to ShellExecute/UAC. Desktop-user identity
/// is independently validated by diagnostics/patching at their native boundary.
fn authorize_context(
    admin: bool,
    original_user: bool,
    elevated: impl FnOnce() -> Result<bool>,
) -> Result<()> {
    if admin || original_user {
        let elevated = elevated()?;
        require(!admin || elevated, ADMIN_REQUIRED)?;
        require(!original_user || !elevated, ORIGINAL_REQUIRED)?;
    }
    Ok(())
}

fn policy_from_args(m: &ArgMatches, now: u64) -> Result<o::OwnerPolicy> {
    let allowed: Vec<_> = m
        .get_many::<o::OperationKind>("allow")
        .unwrap()
        .copied()
        .collect();
    require(
        allowed
            .iter()
            .enumerate()
            .all(|(i, k)| !allowed[..i].contains(k)),
        "Choose each operation only once.",
    )?;
    let opt_in_until = m
        .get_one::<u64>("opt-in-for")
        .map(|s| now.checked_add(*s).context("clock-overflow"))
        .transpose()?;
    require(
        allowed
            .iter()
            .all(|k| k.spec().risk == o::Risk::DiagnosticIo)
            || opt_in_until.is_some(),
        "Repairs and scans require an expiring --opt-in-for policy.",
    )?;
    let window = o::MaintenanceWindow {
        start_minute_utc: *m.get_one("window-start").unwrap(),
        end_minute_utc: *m.get_one("window-end").unwrap(),
    };
    require(
        window.start_minute_utc != window.end_minute_utc,
        "The UTC maintenance window must have different start and end times.",
    )?;
    let exceptions = m
        .get_many::<ExceptionArg>("exception")
        .into_iter()
        .flatten()
        .map(|e| {
            Ok(o::PolicyException {
                operation: e.operation,
                scope: e.scope,
                expires_at: now.checked_add(e.seconds).context("clock-overflow")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    require(
        exceptions.iter().enumerate().all(|(i, e)| {
            allowed.contains(&e.operation)
                && !exceptions[..i]
                    .iter()
                    .any(|p| p.operation == e.operation && p.scope == e.scope)
        }),
        "Exceptions must be unique and limited to allowed operations.",
    )?;
    Ok(o::OwnerPolicy {
        allowed,
        opt_in_until,
        window,
        idle_seconds: *m.get_one("idle-seconds").unwrap(),
        exceptions,
    })
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Report {
    Profiles(Vec<&'static str>),
    Diagnostics(d::Report),
    OperationsCapabilities(o::Capabilities),
    OperationList(Vec<o::PlanRecord>),
    OperationPlan(o::Plan),
    Operation(o::PlanRecord),
    Policy(o::OwnerPolicy),
    QualityCapabilities(p::Capabilities),
    Catalog(p::Catalog),
    QualityList(Vec<p::Record>),
    QualityPlan(p::Plan),
    Quality(p::Record),
}
impl Report {
    pub fn exit_code(&self) -> i32 {
        let review = match self {
            Self::Diagnostics(r) => matches!(
                r.status,
                d::Status::Attention | d::Status::Unknown | d::Status::Unsupported
            ),
            Self::Operation(r) => {
                r.consumed && r.steps.iter().any(|s| s.state != o::StepState::Succeeded)
            }
            Self::Quality(r) => !matches!(r.status, p::Status::Planned | p::Status::Succeeded),
            _ => false,
        };
        if review {
            2
        } else {
            0
        }
    }
}

pub fn execute(matches: &ArgMatches, lang: Lang) -> Result<i32> {
    let (group, m) = matches.subcommand().unwrap();
    let (action, a) = m.subcommand().unwrap();
    if action == "guide" {
        crate::guided::require_terminal(lang)?;
        let _screen = crate::menu::Screen::enter(lang, !matches.get_flag("no-animation"))?;
        let mut reader = crate::menu::TerminalMenu::default();
        return guide(group, lang, &mut reader, &mut io::stdout(), false).map(|_| 0);
    }
    // Reject missing consent BEFORE any native queries, protected-state writes,
    // online search, or supervisor creation.
    match (group, action) {
        ("operations", "approve" | "run") => consent(a)?,
        ("operations", "policy") if matches!(a.subcommand_name(), Some("set" | "reset")) => {
            consent(a.subcommand().unwrap().1)?
        }
        ("quality-updates", "discover") => source_consent(a)?,
        ("quality-updates", "plan") => {
            source_consent(a)?;
            selected_updates(a)?;
        }
        ("quality-updates", "approve" | "install") => {
            patch_consent(a)?;
        }
        _ => {}
    }
    let original = group == "diagnostics" && action == "run" && a.get_flag("original-user");
    let admin = group != "diagnostics" && action != "capabilities";
    authorize_context(admin, original, platform::is_elevated)?;
    let view = Ui::new(
        lang,
        matches.get_flag("no-animation"),
        matches.get_flag("json"),
    )
    .with_details(matches.get_flag("details"));
    let json = matches.get_flag("json");
    let uuid = || *a.get_one::<Uuid>("id").unwrap();
    let digest = || a.get_one::<String>("digest").unwrap().as_str();
    let seconds = || *a.get_one::<u64>("valid-for").unwrap();
    let report = match (group, action) {
        ("diagnostics", "profiles") => Report::Profiles(PROFILES.to_vec()),
        ("diagnostics", "run") => {
            let needs = a
                .get_many::<String>("context")
                .into_iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            let progress = view.progress();
            let report = d::collect(
                profile(a.get_one::<String>("profile").unwrap()),
                &context(&needs, original),
            );
            drop(progress);
            Report::Diagnostics(report)
        }
        ("operations", "capabilities") => Report::OperationsCapabilities(o::capabilities()),
        ("operations", "list") => Report::OperationList(o::list()?),
        ("operations", "show") => Report::Operation(o::get(uuid())?),
        ("operations", "plan") => Report::OperationPlan(o::plan(o::PlanRequest {
            operations: a
                .get_many::<o::OperationKind>("operations")
                .unwrap()
                .copied()
                .collect(),
            valid_for_seconds: seconds(),
        })?),
        ("operations", "approve") => Report::Operation(o::approve(uuid(), digest(), seconds())?),
        ("operations", "run") => {
            let record = o::get(uuid())?;
            check_run(&record, digest())?;
            let guard = CancelGuard::install()?;
            let task = o::start(uuid())?;
            Report::Operation(wait_operation(&task, &guard, lang, json)?)
        }
        ("operations", "resume") => {
            let guard = CancelGuard::install()?;
            let task = o::resume(uuid())?;
            Report::Operation(wait_operation(&task, &guard, lang, json)?)
        }
        ("operations", "policy") => {
            let (action, settings) = a.subcommand().unwrap();
            match action {
                "show" => {}
                "reset" => o::set_policy(o::OwnerPolicy::default())?,
                "set" => o::set_policy(policy_from_args(settings, time()?)?)?,
                _ => unreachable!(),
            }
            Report::Policy(o::policy()?)
        }
        ("quality-updates", "capabilities") => Report::QualityCapabilities(p::capabilities()),
        ("quality-updates", "list") => Report::QualityList(p::list()?),
        ("quality-updates", "show") => Report::Quality(p::get(uuid())?),
        ("quality-updates", "approve") => {
            Report::Quality(p::approve(uuid(), digest(), patch_consent(a)?, seconds())?)
        }
        ("quality-updates", "discover") => {
            let guard = CancelGuard::install()?;
            Report::Catalog(wait_patch(&p::discover()?, &guard, lang, json)?)
        }
        ("quality-updates", "plan") => {
            let guard = CancelGuard::install()?;
            let selected = selected_updates(a)?;
            Report::QualityPlan(wait_patch(
                &p::plan(p::PlanRequest {
                    selected,
                    valid_for_seconds: seconds(),
                })?,
                &guard,
                lang,
                json,
            )?)
        }
        ("quality-updates", "install") => {
            let guard = CancelGuard::install()?;
            Report::Quality(wait_patch(
                &p::start(uuid(), digest())?,
                &guard,
                lang,
                json,
            )?)
        }
        ("quality-updates", "verify") => {
            let guard = CancelGuard::install()?;
            Report::Quality(wait_patch(&p::verify(uuid())?, &guard, lang, json)?)
        }
        _ => unreachable!(),
    };
    view.maintenance_report(&report)?;
    Ok(report.exit_code())
}

fn check_run(record: &o::PlanRecord, digest: &str) -> Result<()> {
    require(
        record.plan.digest == digest
            && record.approval.as_ref().is_some_and(|a| a.digest == digest)
            && !record.consumed,
        "The digest must match an approved, unused plan. Use resume only for verification.",
    )
}

static CANCEL: AtomicBool = AtomicBool::new(false);
struct CancelGuard;
#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}
#[cfg(windows)]
unsafe extern "system" fn console_cancel(event: u32) -> i32 {
    if matches!(event, 0 | 1) {
        CANCEL.store(true, Ordering::SeqCst);
        1
    } else {
        0
    }
}
impl CancelGuard {
    fn install() -> Result<Self> {
        CANCEL.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        anyhow::ensure!(
            unsafe { SetConsoleCtrlHandler(Some(console_cancel), 1) } != 0,
            "console-handler-unavailable"
        );
        Ok(Self)
    }
    fn requested(&self) -> bool {
        CANCEL.load(Ordering::SeqCst)
    }
}
impl Drop for CancelGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            SetConsoleCtrlHandler(Some(console_cancel), 0);
        }
    }
}

/// Rendering failure never detaches a servicing supervisor. Polling continues
/// until completion, then the original output error is returned to the caller.
fn supervise<T>(
    mut poll: impl FnMut(Duration) -> Result<Option<T>>,
    mut tick: impl FnMut() -> Result<()>,
) -> Result<T> {
    let mut rendering_error = None;
    loop {
        if let Some(result) = poll(Duration::from_millis(250))? {
            if let Some(error) = rendering_error {
                return Err(error);
            }
            return Ok(result);
        }
        if let Err(error) = tick() {
            rendering_error.get_or_insert(error);
        }
    }
}
fn wait_operation(
    task: &o::Task,
    guard: &CancelGuard,
    lang: Lang,
    json: bool,
) -> Result<o::PlanRecord> {
    let mut out = io::stderr();
    if !json && crate::menu::screen_active() {
        crate::menu::screen_progress(&lang.t("Maintenance progress"), &lang.t(WAITING));
    } else if !json {
        let _ = writeln!(out, "{}", lang.t(WAITING));
    }
    let mut last = Instant::now();
    let started = Instant::now();
    let mut cancelled = false;
    let result = supervise(
        |duration| task.wait(duration),
        || {
            if guard.requested() && !cancelled {
                task.request_cancel();
                cancelled = true;
            }
            if !json && (crate::menu::screen_active() || last.elapsed() >= Duration::from_secs(5)) {
                last = Instant::now();
                let p = task.progress()?;
                let note = match p.stop_reason {
                    Some(o::StopReason::Timeout) => {
                        Some("Observation budget exceeded; no process was killed")
                    }
                    Some(o::StopReason::CancelRequested) => {
                        Some("Cancellation requested; waiting for Windows")
                    }
                    None if cancelled => Some("Cancellation requested; waiting for Windows"),
                    None => None,
                };
                let state = p
                    .state
                    .map(|s| lang.t(operation_state(s)))
                    .unwrap_or_else(|| lang.t("Checking readiness"));
                let note = note
                    .map(|key| format!(" | {}", lang.t(key)))
                    .unwrap_or_default();
                let message = if crate::menu::screen_active() {
                    format!("{state}{note}")
                } else {
                    format!(
                        "{}: {state} | {}s{note}",
                        lang.t("Maintenance progress"),
                        started.elapsed().as_secs()
                    )
                };
                if !crate::menu::screen_progress(&lang.t("Maintenance progress"), &message) {
                    writeln!(out, "{message}")?;
                }
            }
            Ok(())
        },
    );
    crate::menu::screen_progress_end();
    result
}
fn wait_patch<T>(task: &p::Task<T>, guard: &CancelGuard, lang: Lang, json: bool) -> Result<T> {
    let mut out = io::stderr();
    if !json && crate::menu::screen_active() {
        crate::menu::screen_progress(
            &lang.t("Selected Windows quality updates"),
            &lang.t(WAITING),
        );
    } else if !json {
        let _ = writeln!(out, "{}", lang.t(WAITING));
    }
    let started = Instant::now();
    let mut last = Instant::now();
    let mut cancelled = false;
    let result = supervise(
        |duration| task.wait(duration),
        || {
            if guard.requested() && !cancelled {
                task.request_cancel();
                cancelled = true;
            }
            if !json && (crate::menu::screen_active() || last.elapsed() >= Duration::from_secs(5)) {
                last = Instant::now();
                let status = lang.t(if cancelled {
                    "Cancellation requested; waiting for Windows"
                } else {
                    "Waiting for the Windows Update supervisor"
                });
                let message = if crate::menu::screen_active() {
                    status
                } else {
                    format!("{status}: {}s", started.elapsed().as_secs())
                };
                if !crate::menu::screen_progress(
                    &lang.t("Selected Windows quality updates"),
                    &message,
                ) {
                    writeln!(out, "{message}")?;
                }
            }
            Ok(())
        },
    );
    crate::menu::screen_progress_end();
    result
}

pub fn write_failure(
    lang: Lang,
    error: &anyhow::Error,
    json: bool,
    details: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let key = error.downcast_ref::<UserError>().map_or(FAILED, |e| e.0);
    if json {
        serde_json::to_writer(
            &mut *out,
            &serde_json::json!({"error": {"code":"maintenance_failed", "message":lang.t(key)}}),
        )?;
        writeln!(out)?;
    } else {
        writeln!(err, "{}", lang.t(key))?;
        if details {
            writeln!(err, "{}", crate::ui::error_details(lang, error))?;
        }
    }
    Ok(())
}

pub fn operation_label(kind: o::OperationKind) -> &'static str {
    match kind {
        o::OperationKind::DismCheckHealth => "Check cached component-store health",
        o::OperationKind::DismScanHealth => "Scan component-store health",
        o::OperationKind::DismRestoreHealth => "Repair component store using local content",
        o::OperationKind::SfcVerify => "Verify protected system files",
        o::OperationKind::SfcRepair => "Repair protected system files",
        o::OperationKind::DefenderQuickScan => "Defender quick scan with existing settings",
    }
}
fn operation_state(state: o::StepState) -> &'static str {
    match state {
        o::StepState::Pending => "Pending",
        o::StepState::Intent => "Intent recorded",
        o::StepState::Running => "Running",
        o::StepState::Monitoring => "Monitoring Windows",
        o::StepState::Verifying => "Verifying",
        o::StepState::Succeeded => "Completed with evidence",
        o::StepState::RebootRequired => "Owner-initiated reboot required",
        o::StepState::NeedsReview => "Needs review",
        o::StepState::Failed => "Failed",
        o::StepState::Cancelled => "Cancelled",
    }
}
fn patch_state(state: p::Status) -> &'static str {
    match state {
        p::Status::Planned => "Planned",
        p::Status::Consumed => "Intent recorded",
        p::Status::Downloading => "Downloading",
        p::Status::Downloaded => "Downloaded",
        p::Status::Installing => "Installing",
        p::Status::Verifying => "Verifying",
        p::Status::Succeeded => "Installed identities verified",
        p::Status::RebootRequired => "Owner-initiated reboot required",
        p::Status::NeedsReview => "Needs review",
    }
}
fn diagnostic_state(state: d::Status) -> &'static str {
    match state {
        d::Status::Healthy => "Observed healthy",
        d::Status::Attention => "Needs attention",
        d::Status::Unknown => "Unknown",
        d::Status::Unsupported => "Unsupported",
        d::Status::Informational => "For your information",
    }
}
fn safe(value: &str) -> String {
    value
        .lines()
        .map(crate::ui::safe)
        .collect::<Vec<_>>()
        .join("\n")
}
fn field(
    out: &mut impl Write,
    lang: Lang,
    label: &str,
    value: impl std::fmt::Display,
) -> Result<()> {
    writeln!(out, "{}: {value}", lang.t(label))?;
    Ok(())
}
fn yes_no(lang: Lang, value: bool) -> String {
    lang.t(if value { "Yes" } else { "No" })
}
fn profile_label(p: d::Profile) -> &'static str {
    match p {
        d::Profile::Everyday => "Everyday use",
        d::Profile::Gaming => "Gaming",
        d::Profile::Development => "Software development",
        d::Profile::HigherSecurity => "Higher security",
    }
}

pub fn write_report(
    report: &Report,
    lang: Lang,
    json: bool,
    details: bool,
    out: &mut impl Write,
) -> Result<()> {
    if json {
        serde_json::to_writer(&mut *out, report)?;
        writeln!(out)?;
        return Ok(());
    }
    match report {
        Report::Profiles(_) => {
            for name in PROFILES {
                writeln!(out, "{name}: {}", lang.t(profile_label(profile(name))))?;
            }
            writeln!(out, "{}", lang.t("Profiles change advice only. Compatibility needs never authorize disabling protection."))?;
        }
        Report::Diagnostics(r) => {
            field(
                out,
                lang,
                "Diagnostic profile",
                lang.t(profile_label(r.profile)),
            )?;
            field(out, lang, "Status", lang.t(diagnostic_state(r.status)))?;
            let needs = [
                (r.compatibility.printers, "Printers"),
                (r.compatibility.nas, "NAS and shared storage"),
                (r.compatibility.vpn, "VPN"),
                (r.compatibility.games, "Gaming"),
                (r.compatibility.development, "Software development"),
            ]
            .into_iter()
            .filter(|(enabled, _)| *enabled)
            .map(|(_, key)| lang.t(key))
            .collect::<Vec<_>>();
            field(
                out,
                lang,
                "Declared compatibility needs",
                if needs.is_empty() {
                    lang.t("none")
                } else {
                    needs.join(", ")
                },
            )?;
            field(
                out,
                lang,
                "Device management",
                lang.t(match r.management {
                    d::ManagementStatus::Managed => "Managed device",
                    d::ManagementStatus::PolicyPresent => "Policy indicators present",
                    d::ManagementStatus::NoIndicatorsObserved => {
                        "No management indicators observed"
                    }
                    d::ManagementStatus::Unknown => "Management authority unknown",
                }),
            )?;
            writeln!(out, "{}", lang.t(match r.profile {
                d::Profile::Everyday => "Prioritize supported software, antivirus, recovery access and tested backups.",
                d::Profile::Gaming => "Keep protections enabled. Test game, anti-cheat and driver compatibility before considering narrow exceptions.",
                d::Profile::Development => "Use least-privilege accounts and isolated build workspaces. Test compiler and container compatibility before changing protections.",
                d::Profile::HigherSecurity => "Review Secure Boot, encryption recovery, VBS and remote exposure. Stage compatibility and recovery tests before changes.",
            }))?;
            field(
                out,
                lang,
                "Probes with evidence",
                format!(
                    "{}/{}",
                    r.coverage.probes_with_evidence, r.coverage.total_probes
                ),
            )?;
            field(
                out,
                lang,
                "Unknown assessments",
                r.coverage.assessments_unknown,
            )?;
            for probe in &r.probes {
                writeln!(
                    out,
                    "  {}: {}",
                    lang.t(probe_label(probe.id)),
                    lang.t(diagnostic_state(probe.status))
                )?;
            }
            field(
                out,
                lang,
                "Advisory recommendations",
                r.recommendations.len(),
            )?;
            writeln!(out, "{}", lang.t("Cached evidence is not a fresh online update scan. Unknown is not healthy. No repairs or restore tests were performed."))?;
            writeln!(out, "{}", lang.t("Use --details for rule references, compatibility advice and collection limits."))?;
            if details {
                // Native evidence and versioned rule prose stay intact, labelled
                // as technical data rather than pretending they were translated.
                writeln!(
                    out,
                    "{}",
                    lang.t("Technical evidence and rule text (source language)")
                )?;
                writeln!(out, "{}", safe(&serde_json::to_string_pretty(r)?))?;
            }
        }
        Report::OperationsCapabilities(c) => {
            field(
                out,
                lang,
                "Windows execution available",
                yes_no(lang, c.windows_execution),
            )?;
            for spec in &c.operations {
                write_spec(out, lang, spec)?;
            }
            writeln!(out, "{}", lang.t("Application upgrades are unavailable. Use quality-updates for separately approved Windows quality updates."))?;
        }
        Report::Policy(policy) => write_policy(out, lang, policy)?,
        Report::OperationPlan(plan) => write_operation_plan(out, lang, plan)?,
        Report::Operation(record) => {
            write_operation_plan(out, lang, &record.plan)?;
            field(
                out,
                lang,
                "Approval expires (UTC Unix seconds)",
                record
                    .approval
                    .as_ref()
                    .map_or_else(|| lang.t("Not approved"), |a| a.expires_at.to_string()),
            )?;
            field(
                out,
                lang,
                "Single-use plan consumed",
                yes_no(lang, record.consumed),
            )?;
            for (i, step) in record.steps.iter().enumerate() {
                writeln!(
                    out,
                    "  {}. {}: {}",
                    i + 1,
                    lang.t(operation_label(record.plan.steps[i].operation.kind)),
                    lang.t(operation_state(step.state))
                )?;
                if let Some(evidence) = step.evidence {
                    field(out, lang, "Evidence", lang.t(evidence_label(evidence)))?;
                }
                if let Some(code) = step.exit_code {
                    field(out, lang, "Native exit code", code)?;
                }
                if let Some(reason) = step.stop_reason {
                    field(
                        out,
                        lang,
                        "Observation",
                        lang.t(match reason {
                            o::StopReason::CancelRequested => {
                                "Cancellation requested; waiting for Windows"
                            }
                            o::StopReason::Timeout => {
                                "Observation budget exceeded; no process was killed"
                            }
                        }),
                    )?;
                }
            }
        }
        Report::OperationList(records) => {
            if records.is_empty() {
                writeln!(out, "{}", lang.t("No saved plans."))?;
            }
            for r in records {
                writeln!(
                    out,
                    "{} | {} | {}",
                    r.plan.id,
                    r.plan.digest,
                    r.steps
                        .iter()
                        .map(|s| lang.t(operation_state(s.state)))
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }
        }
        Report::QualityCapabilities(c) => {
            field(
                out,
                lang,
                "Quality-update installation available",
                yes_no(lang, c.windows_quality_updates),
            )?;
            writeln!(out, "{}", lang.t("Only selected Windows security and critical quality updates are supported. Drivers, feature upgrades, previews and application upgrades are excluded."))?;
            writeln!(out, "{}", lang.t(ADMIN_REQUIRED))?;
        }
        Report::Catalog(c) => {
            field(out, lang, "Windows Update source", &c.source)?;
            field(out, lang, "Search time (UTC Unix seconds)", c.searched_at)?;
            if c.updates.is_empty() {
                writeln!(out, "{}", lang.t("No eligible quality updates were returned. This is not proof that every update is installed."))?;
            }
            for u in &c.updates {
                writeln!(
                    out,
                    "{}:{} | {} | {} bytes",
                    u.identity.update_id,
                    u.identity.revision,
                    safe(&u.title),
                    u.max_download_bytes
                )?;
            }
        }
        Report::QualityPlan(plan) => write_quality_plan(out, lang, plan)?,
        Report::Quality(record) => {
            write_quality_plan(out, lang, &record.plan)?;
            field(out, lang, "Status", lang.t(patch_state(record.status)))?;
            field(
                out,
                lang,
                "Approval expires (UTC Unix seconds)",
                record
                    .approval
                    .as_ref()
                    .map_or_else(|| lang.t("Not approved"), |a| a.expires_at.to_string()),
            )?;
            field(
                out,
                lang,
                "Earlier uncertainty retained",
                yes_no(lang, record.uncertain),
            )?;
            if let Some(v) = &record.verification {
                field(
                    out,
                    lang,
                    "Installed identities verified",
                    v.installed.len(),
                )?;
                field(out, lang, "Reboot pending", yes_no(lang, v.reboot_pending))?;
            }
        }
        Report::QualityList(records) => {
            if records.is_empty() {
                writeln!(out, "{}", lang.t("No saved plans."))?;
            }
            for r in records {
                writeln!(
                    out,
                    "{} | {} | {}",
                    r.plan.id,
                    r.plan.digest,
                    lang.t(patch_state(r.status))
                )?;
            }
        }
    }
    Ok(())
}

fn evidence_label(e: o::Evidence) -> &'static str {
    match e {
        o::Evidence::DiagnosticCompleted => "Command completed; clean integrity is not confirmed",
        o::Evidence::ComponentStoreHealthy => "Component-store health verified",
        o::Evidence::ComponentStoreRepairable => "Component-store corruption is repairable",
        o::Evidence::ComponentStoreNonRepairable => "Component-store corruption is not repairable",
        o::Evidence::DefenderScanCompleted => {
            "Quick-scan completion verified; threat absence is not confirmed"
        }
        o::Evidence::Inconclusive => "Inconclusive evidence",
    }
}
fn write_spec(out: &mut impl Write, lang: Lang, spec: &o::OperationSpec) -> Result<()> {
    writeln!(
        out,
        "{}: {}",
        spec.kind.as_str(),
        lang.t(operation_label(spec.kind))
    )?;
    field(
        out,
        lang,
        "Risk",
        lang.t(match spec.risk {
            o::Risk::DiagnosticIo => "Diagnostic disk and CPU activity",
            o::Risk::SystemRepair => "System files may be replaced",
            o::Risk::AntivirusRemediation => "Threats may be quarantined or remediated",
        }),
    )?;
    field(
        out,
        lang,
        "Rollback",
        lang.t(match spec.reversibility {
            o::Reversibility::NoConfigurationChange => "No protection-setting change",
            o::Reversibility::NoAutomaticRollback => "No automatic rollback",
        }),
    )?;
    field(
        out,
        lang,
        "Observation budget (seconds)",
        spec.timeout_seconds,
    )?;
    field(
        out,
        lang,
        "Network access",
        yes_no(lang, spec.network_access),
    )?;
    field(
        out,
        lang,
        "May require an owner-initiated reboot",
        yes_no(lang, spec.may_require_reboot),
    )?;
    if spec.kind == o::OperationKind::DismRestoreHealth {
        writeln!(
            out,
            "{}",
            lang.t("DISM uses local repair content only and then runs an independent health scan.")
        )?;
    }
    if spec.kind == o::OperationKind::SfcRepair {
        writeln!(out, "{}", lang.t("SFC repair remains Needs review until its integrity results are reviewed; exit zero is not a clean-health claim."))?;
    }
    Ok(())
}
fn write_policy(out: &mut impl Write, lang: Lang, policy: &o::OwnerPolicy) -> Result<()> {
    field(
        out,
        lang,
        "Allowed operations",
        policy
            .allowed
            .iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    )?;
    field(
        out,
        lang,
        "Owner opt-in expires (UTC Unix seconds)",
        policy
            .opt_in_until
            .map_or_else(|| lang.t("Diagnostics only"), |v| v.to_string()),
    )?;
    field(
        out,
        lang,
        "Maintenance window (UTC)",
        format!(
            "{:02}:{:02} - {:02}:{:02}",
            policy.window.start_minute_utc / 60,
            policy.window.start_minute_utc % 60,
            policy.window.end_minute_utc / 60,
            policy.window.end_minute_utc % 60
        ),
    )?;
    field(
        out,
        lang,
        "Required idle time in seconds",
        policy.idle_seconds,
    )?;
    for e in &policy.exceptions {
        writeln!(
            out,
            "{}: {} | {} | {}",
            lang.t("Scoped exception"),
            e.operation.as_str(),
            lang.t(match e.scope {
                o::ExceptionScope::ActiveUse => "Active use",
                o::ExceptionScope::MaintenanceWindow => "Maintenance window",
                o::ExceptionScope::MeteredNetwork => "Metered network",
            }),
            e.expires_at
        )?;
    }
    writeln!(out, "{}", lang.t("Policy permission does not approve a plan. Elevation, ownership, power, storage and servicing checks cannot be bypassed."))?;
    Ok(())
}
fn write_operation_plan(out: &mut impl Write, lang: Lang, plan: &o::Plan) -> Result<()> {
    field(out, lang, "Plan ID", plan.id)?;
    field(out, lang, "Exact plan digest", &plan.digest)?;
    field(
        out,
        lang,
        "Plan expires (UTC Unix seconds)",
        plan.expires_at,
    )?;
    for (i, step) in plan.steps.iter().enumerate() {
        writeln!(
            out,
            "{}. {}",
            i + 1,
            lang.t(operation_label(step.operation.kind))
        )?;
        write_spec(out, lang, &step.operation)?;
        field(
            out,
            lang,
            "Dependencies",
            step.depends_on
                .iter()
                .map(|n| (n + 1).to_string())
                .collect::<Vec<_>>()
                .join(", "),
        )?;
    }
    write_policy(out, lang, &plan.policy)?;
    writeln!(out, "{}", lang.t(CONSENT))?;
    Ok(())
}
fn write_quality_plan(out: &mut impl Write, lang: Lang, plan: &p::Plan) -> Result<()> {
    field(out, lang, "Plan ID", plan.id)?;
    field(out, lang, "Exact plan digest", &plan.digest)?;
    field(
        out,
        lang,
        "Plan expires (UTC Unix seconds)",
        plan.expires_at,
    )?;
    field(out, lang, "Windows Update source", &plan.source)?;
    for update in &plan.updates {
        write_update(out, lang, update, 0)?;
    }
    writeln!(out, "{}", lang.t(QUALITY_CONSENT))?;
    Ok(())
}
fn write_update(out: &mut impl Write, lang: Lang, u: &p::Update, depth: usize) -> Result<()> {
    field(
        out,
        lang,
        if depth == 0 {
            "Selected update"
        } else {
            "Bundled update"
        },
        format!("{}:{}", u.identity.update_id, u.identity.revision),
    )?;
    field(out, lang, "Title", safe(&u.title))?;
    field(out, lang, "Description", safe(&u.description))?;
    field(out, lang, "KB articles", u.kb_articles.join(", "))?;
    field(out, lang, "Maximum download (bytes)", u.max_download_bytes)?;
    field(out, lang, "Severity", safe(&u.severity))?;
    field(
        out,
        lang,
        "Native update metadata",
        format!(
            "{} | {} | {} | {}",
            safe(&u.handler),
            safe(&u.last_changed),
            u.reboot_behavior,
            u.categories
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )?;
    field(out, lang, "EULA (full source text)", safe(&u.eula))?;
    for child in &u.bundled {
        write_update(out, lang, child, depth + 1)?;
    }
    Ok(())
}
fn probe_label(id: d::ProbeId) -> &'static str {
    use d::ProbeId::*;
    match id {
        UpdateCache => "Cached Windows updates",
        UpdateHistory => "Update history",
        DefenderHealth => "Defender health",
        DefenderPolicy => "Defender policy",
        SecurityProviders => "Security providers",
        Management => "Device management",
        SecureBoot => "Secure Boot",
        Tpm => "Trusted Platform Module",
        BitLocker => "Device encryption",
        Vbs => "Virtualization-based security",
        WinRe => "Windows recovery environment",
        Accounts => "Local accounts",
        RemoteAccess => "Remote access",
        Software => "Installed software",
        BrowserExtensions => "Browser extensions",
        Storage => "Physical disk health",
        Ntfs => "Filesystem health",
        Backup => "Backup evidence",
        Adapters => "Network adapters",
        Dns => "DNS configuration",
        Proxy => "Machine proxy",
        Vpn => "Machine VPN",
        Permissions => "Service permissions",
    }
}

// Guided entry points use the same types and explicit library calls as the CLI.
// Definitions follow the reporting code to keep CLI parsing independent of TTYs.
pub fn guide(
    group: &str,
    lang: Lang,
    reader: &mut impl ChoiceInput,
    out: &mut impl Write,
    broker: bool,
) -> Result<()> {
    let _section = crate::menu::section(match group {
        "diagnostics" => crate::menu::Section::Diagnostics,
        "operations" => crate::menu::Section::Maintenance,
        "quality-updates" => crate::menu::Section::QualityUpdates,
        _ => unreachable!(),
    });
    crate::menu::screen_clear();
    let mut g = Guide { lang, reader, out };
    if group != "diagnostics" {
        require(!broker, ADMIN_REQUIRED)?;
        authorize_context(true, false, platform::is_elevated)?;
    }
    match group {
        "diagnostics" => g.diagnostics(),
        "operations" => g.operations(),
        "quality-updates" => g.quality(),
        _ => unreachable!(),
    }
}
struct Guide<'a, R, W> {
    lang: Lang,
    reader: &'a mut R,
    out: &'a mut W,
}
impl<R: ChoiceInput, W: Write> Guide<'_, R, W> {
    fn say(&mut self, key: &str) -> Result<()> {
        if !crate::menu::screen_note(&self.lang.t(key))? {
            writeln!(self.out, "{}", self.lang.t(key))?;
        }
        Ok(())
    }
    fn choose(&mut self, key: &str, labels: &[String], consent: bool) -> Result<Option<usize>> {
        if crate::menu::screen_active() {
            crate::menu::screen_title(&self.lang.t(key));
        } else {
            self.say(key)?;
        }
        self.out.flush()?;
        let choice = self.reader.select(
            self.lang,
            labels,
            if consent { labels.len() - 1 } else { 0 },
            consent,
        )?;
        require(
            choice.is_none_or(|n| n < labels.len()),
            "Invalid menu selection",
        )?;
        Ok(choice)
    }
    fn choices(&mut self, key: &str, labels: &[&str]) -> Result<Option<usize>> {
        let labels = labels.iter().map(|s| self.lang.t(s)).collect::<Vec<_>>();
        self.choose(key, &labels, false)
    }
    fn confirm(&mut self, key: &str) -> Result<bool> {
        let labels = [self.lang.t("Yes, continue"), self.lang.t("No, go back")];
        Ok(self.choose(key, &labels, true)? == Some(0))
    }
    fn multiple(&mut self, key: &str, labels: &[String]) -> Result<Option<Vec<usize>>> {
        if crate::menu::screen_active() {
            crate::menu::screen_title(&self.lang.t(key));
        } else {
            self.say(key)?;
        }
        self.out.flush()?;
        let selected = self.reader.multi_select(self.lang, labels)?;
        if let Some(indices) = &selected {
            require(
                indices
                    .iter()
                    .enumerate()
                    .all(|(i, n)| *n < labels.len() && !indices[..i].contains(n)),
                "Invalid menu selection",
            )?;
        }
        Ok(selected)
    }
    fn report(&mut self, report: Report) -> Result<()> {
        if !crate::menu::screen_active() {
            return write_report(&report, self.lang, false, false, self.out);
        }
        use crate::menu::Role;
        let role = match &report {
            Report::Diagnostics(r) => match r.status {
                d::Status::Unknown | d::Status::Unsupported => Role::Unknown,
                d::Status::Attention => Role::Review,
                _ => Role::Text,
            },
            Report::Operation(r) if r.steps.iter().any(|s| s.state == o::StepState::Failed) => {
                Role::Failure
            }
            Report::Operation(r)
                if r.steps.iter().any(|s| {
                    matches!(
                        s.state,
                        o::StepState::NeedsReview | o::StepState::RebootRequired
                    )
                }) =>
            {
                Role::Review
            }
            Report::Quality(r)
                if matches!(r.status, p::Status::NeedsReview | p::Status::RebootRequired) =>
            {
                Role::Review
            }
            _ => Role::Text,
        };
        crate::menu::screen_role(role);
        // Each result header is derived from its typed report. A finished
        // diagnostic must not leave a synthetic "checking" badge visible.
        let status = match &report {
            Report::Diagnostics(r) => Some((
                profile_label(r.profile),
                diagnostic_state(r.status),
                match r.status {
                    d::Status::Healthy => Role::Healthy,
                    d::Status::Attention => Role::Review,
                    d::Status::Unknown | d::Status::Unsupported => Role::Unknown,
                    d::Status::Informational => Role::Text,
                },
            )),
            Report::Operation(r) => {
                let (label, tone) = if !r.consumed {
                    ("Planned", Role::Text)
                } else if r.steps.iter().any(|s| s.state == o::StepState::Failed) {
                    ("Failed", Role::Failure)
                } else if r.steps.iter().any(|s| s.state == o::StepState::NeedsReview) {
                    ("Needs review", Role::Review)
                } else if r
                    .steps
                    .iter()
                    .any(|s| s.state == o::StepState::RebootRequired)
                {
                    ("Owner-initiated reboot required", Role::Review)
                } else if r.steps.iter().all(|s| s.state == o::StepState::Succeeded) {
                    ("Completed with evidence", Role::Text)
                } else {
                    ("Unverified", Role::Unknown)
                };
                Some(("Maintenance plans", label, tone))
            }
            Report::Quality(r) => Some((
                "Selected Windows quality updates",
                if matches!(
                    r.status,
                    p::Status::Planned
                        | p::Status::Succeeded
                        | p::Status::NeedsReview
                        | p::Status::RebootRequired
                ) {
                    patch_state(r.status)
                } else {
                    "Unverified"
                },
                match r.status {
                    p::Status::Planned | p::Status::Succeeded => Role::Text,
                    p::Status::NeedsReview | p::Status::RebootRequired => Role::Review,
                    _ => Role::Unknown,
                },
            )),
            _ => None,
        };
        if let Some((subtitle, label, tone)) = status {
            crate::menu::screen_header(crate::menu::Header {
                subtitle: self.lang.t(subtitle),
                badges: vec![crate::menu::Badge {
                    text: self.lang.t(label),
                    role: tone,
                }],
                tally: None,
            });
        }
        let mut bytes = Vec::new();
        write_report(&report, self.lang, false, false, &mut bytes)?;
        let text = String::from_utf8(bytes)?;
        match report {
            Report::Operation(_)
            | Report::OperationPlan(_)
            | Report::Quality(_)
            | Report::QualityPlan(_)
            | Report::Policy(_) => {
                crate::menu::screen_content(&text)?;
                Ok(())
            }
            Report::Catalog(catalog) if !catalog.updates.is_empty() => {
                crate::menu::screen_content(&format!(
                    "{}: {}\n{}: {}",
                    self.lang.t("Windows Update source"),
                    catalog.source,
                    self.lang.t("Eligible updates"),
                    catalog.updates.len()
                ))?;
                Ok(())
            }
            _ => self
                .reader
                .view(self.lang, &self.lang.t("Review results"), &text),
        }
    }
    fn diagnostics(&mut self) -> Result<()> {
        let Some(index) = self.choices(
            "Choose a diagnostic profile",
            &[
                "Everyday use",
                "Gaming",
                "Software development",
                "Higher security",
                "Back",
            ],
        )?
        else {
            return Ok(());
        };
        if index == 4 {
            return Ok(());
        }
        let labels = [
            "Printers",
            "NAS and shared storage",
            "VPN",
            "Gaming",
            "Software development",
        ]
        .iter()
        .map(|s| self.lang.t(s))
        .collect::<Vec<_>>();
        let Some(selected) = self.multiple(
            "Choose compatibility needs; this never disables protection",
            &labels,
        )?
        else {
            return Ok(());
        };
        let include_user = match self.choices(
            "Browser inventory scope",
            &[
                "Machine diagnostics only",
                "Include original-user browser inventory",
                "Back",
            ],
        )? {
            Some(0) => false,
            Some(1) => true,
            _ => return Ok(()),
        };
        authorize_context(false, include_user, platform::is_elevated)?;
        self.say("Read-only collection; no fixes will be applied.")?;
        let needs = selected
            .into_iter()
            .map(|i| NEEDS[i].to_owned())
            .collect::<Vec<_>>();
        let progress =
            Ui::new(self.lang, true, false).progress_named("Read-only diagnostics and profiles");
        let report = d::collect(profile(PROFILES[index]), &context(&needs, include_user));
        drop(progress);
        self.report(Report::Diagnostics(report))
    }
    fn operations(&mut self) -> Result<()> {
        loop {
            match self.choices(
                "Maintenance plans",
                &[
                    "Supported operations and risks",
                    "Create a maintenance plan",
                    "Saved maintenance plans",
                    "Owner maintenance policy",
                    "Back",
                ],
            )? {
                Some(0) => self.report(Report::OperationsCapabilities(o::capabilities()))?,
                Some(1) => {
                    let kinds = OPS
                        .iter()
                        .map(|s| s.parse::<o::OperationKind>().unwrap())
                        .collect::<Vec<_>>();
                    let labels = kinds
                        .iter()
                        .map(|k| self.lang.t(operation_label(*k)))
                        .collect::<Vec<_>>();
                    let Some(selected) = self.multiple(
                        "Select exact operations; dependencies will be included",
                        &labels,
                    )?
                    else {
                        continue;
                    };
                    if selected.is_empty() {
                        self.say("Nothing selected. No changes made.")?;
                        continue;
                    }
                    let plan = o::plan(o::PlanRequest {
                        operations: selected.into_iter().map(|i| kinds[i]).collect(),
                        valid_for_seconds: 3600,
                    })?;
                    self.operation_record(o::get(plan.id)?)?;
                }
                Some(2) => {
                    let records = o::list()?;
                    if records.is_empty() {
                        self.say("No saved plans.")?;
                        continue;
                    }
                    let mut labels = records
                        .iter()
                        .map(|r| {
                            format!(
                                "{} | {}",
                                r.plan.id,
                                r.plan
                                    .steps
                                    .iter()
                                    .map(|s| self.lang.t(operation_label(s.operation.kind)))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        })
                        .collect::<Vec<_>>();
                    labels.push(self.lang.t("Back"));
                    if let Some(i) = self.choose("Choose a saved plan", &labels, false)? {
                        if i < records.len() {
                            self.operation_record(records[i].clone())?;
                        }
                    }
                }
                Some(3) => self.policy()?,
                _ => return Ok(()),
            }
        }
    }
    fn operation_record(&mut self, mut record: o::PlanRecord) -> Result<()> {
        loop {
            self.report(Report::Operation(record.clone()))?;
            match self.choices(
                "Choose a plan action",
                &[
                    "Approve displayed digest only",
                    "Run approved displayed plan",
                    "Verify attempted work only",
                    "Back",
                ],
            )? {
                Some(0) => {
                    self.report(Report::Operation(record.clone()))?;
                    if self
                        .confirm("Approve only this displayed plan and digest for 15 minutes?")?
                    {
                        record = o::approve(record.plan.id, &record.plan.digest, 900)?;
                    }
                }
                Some(1) => {
                    self.report(Report::Operation(record.clone()))?;
                    if self.confirm(
                        "Execute this exact approved plan now, with the displayed risks?",
                    )? {
                        check_run(&record, &record.plan.digest)?;
                        let guard = CancelGuard::install()?;
                        record =
                            wait_operation(&o::start(record.plan.id)?, &guard, self.lang, false)?;
                    }
                }
                Some(2) => {
                    let guard = CancelGuard::install()?;
                    record = wait_operation(&o::resume(record.plan.id)?, &guard, self.lang, false)?;
                }
                _ => return Ok(()),
            }
        }
    }
    fn policy(&mut self) -> Result<()> {
        loop {
            self.report(Report::Policy(o::policy()?))?;
            match self.choices(
                "Owner maintenance policy",
                &[
                    "Configure allowed operations",
                    "Add short scoped exceptions",
                    "Reset to diagnostics-only defaults",
                    "Back",
                ],
            )? {
                Some(0) => {
                    let kinds = OPS
                        .iter()
                        .map(|s| s.parse::<o::OperationKind>().unwrap())
                        .collect::<Vec<_>>();
                    let labels = kinds
                        .iter()
                        .map(|k| self.lang.t(operation_label(*k)))
                        .collect::<Vec<_>>();
                    let Some(selected) = self.multiple(
                        "Choose the complete allowed list, including diagnostics needed by repairs",
                        &labels,
                    )?
                    else {
                        continue;
                    };
                    let Some(lifetime) = self.choices(
                        "Owner opt-in lifetime",
                        &["One hour", "One day", "Thirty days", "Back"],
                    )?
                    else {
                        continue;
                    };
                    if lifetime == 3 {
                        continue;
                    }
                    let Some(window) = self.choices(
                        "Maintenance window (UTC)",
                        &["01:00 to 05:00 UTC", "22:00 to 06:00 UTC", "Back"],
                    )?
                    else {
                        continue;
                    };
                    if window == 2 {
                        continue;
                    }
                    let Some(idle) = self.choices(
                        "Required idle time",
                        &["Five minutes", "Fifteen minutes", "One hour", "Back"],
                    )?
                    else {
                        continue;
                    };
                    if idle == 3 {
                        continue;
                    }
                    let policy = o::OwnerPolicy {
                        allowed: selected.into_iter().map(|i| kinds[i]).collect(),
                        opt_in_until: Some(time()? + [3600, 86400, 2592000][lifetime]),
                        window: o::MaintenanceWindow {
                            start_minute_utc: [60, 1320][window],
                            end_minute_utc: [300, 360][window],
                        },
                        idle_seconds: [300, 900, 3600][idle],
                        exceptions: Vec::new(),
                    };
                    self.report(Report::Policy(policy.clone()))?;
                    if self.confirm("Replace owner policy with these displayed settings? Existing exceptions will be removed.")? { o::set_policy(policy)?; }
                }
                Some(1) => {
                    let mut policy = o::policy()?;
                    let labels = policy
                        .allowed
                        .iter()
                        .map(|k| self.lang.t(operation_label(*k)))
                        .collect::<Vec<_>>();
                    if labels.is_empty() {
                        self.say("Nothing selected. No changes made.")?;
                        continue;
                    }
                    let Some(selected) =
                        self.multiple("Select operations for short exceptions", &labels)?
                    else {
                        continue;
                    };
                    let labels = ["Active use", "Maintenance window", "Metered network"]
                        .iter()
                        .map(|s| self.lang.t(s))
                        .collect::<Vec<_>>();
                    let Some(scopes) =
                        self.multiple("Select only the gates to except for 15 minutes", &labels)?
                    else {
                        continue;
                    };
                    if selected.is_empty() || scopes.is_empty() {
                        continue;
                    }
                    let now = time()?;
                    policy.exceptions.retain(|e| e.expires_at > now);
                    for index in selected {
                        for scope in &scopes {
                            let operation = policy.allowed[index];
                            let scope = [
                                o::ExceptionScope::ActiveUse,
                                o::ExceptionScope::MaintenanceWindow,
                                o::ExceptionScope::MeteredNetwork,
                            ][*scope];
                            policy
                                .exceptions
                                .retain(|e| e.operation != operation || e.scope != scope);
                            policy.exceptions.push(o::PolicyException {
                                operation,
                                scope,
                                expires_at: now + 900,
                            });
                        }
                    }
                    self.report(Report::Policy(policy.clone()))?;
                    if self.confirm(
                        "Save these exact scoped exceptions? Hard readiness checks still apply.",
                    )? {
                        o::set_policy(policy)?;
                    }
                }
                Some(2) => {
                    self.report(Report::Policy(o::OwnerPolicy::default()))?;
                    if self.confirm("Reset owner policy to diagnostics-only defaults and remove all exceptions?")? { o::set_policy(o::OwnerPolicy::default())?; }
                }
                _ => return Ok(()),
            }
        }
    }
    fn quality(&mut self) -> Result<()> {
        loop {
            match self.choices(
                "Selected Windows quality updates",
                &[
                    "Supported quality updates and limits",
                    "Discover and select exact updates",
                    "Saved quality-update plans",
                    "Back",
                ],
            )? {
                Some(0) => self.report(Report::QualityCapabilities(p::capabilities()))?,
                Some(1) => {
                    if !self.confirm("Contact Microsoft Windows Update for eligible update metadata? Nothing will be installed.")? { continue; }
                    let catalog = {
                        let guard = CancelGuard::install()?;
                        wait_patch(&p::discover()?, &guard, self.lang, false)?
                    };
                    self.report(Report::Catalog(catalog.clone()))?;
                    if catalog.updates.is_empty() {
                        continue;
                    }
                    let labels = catalog
                        .updates
                        .iter()
                        .map(|u| {
                            format!(
                                "{}:{} | {}",
                                u.identity.update_id,
                                u.identity.revision,
                                safe(&u.title)
                            )
                        })
                        .collect::<Vec<_>>();
                    let Some(selected) =
                        self.multiple("Select exact updates; no updates are preselected", &labels)?
                    else {
                        continue;
                    };
                    if selected.is_empty() {
                        self.say("Nothing selected. No changes made.")?;
                        continue;
                    }
                    for &index in &selected {
                        let update = &catalog.updates[index];
                        let line = format!(
                            "{}:{} | {}",
                            update.identity.update_id,
                            update.identity.revision,
                            safe(&update.title)
                        );
                        self.say(&line)?;
                    }
                    if !self.confirm("Refresh applicability from Microsoft Windows Update and create a plan for only this selection?")? { continue; }
                    let plan = {
                        let guard = CancelGuard::install()?;
                        wait_patch(
                            &p::plan(p::PlanRequest {
                                selected: selected
                                    .into_iter()
                                    .map(|i| catalog.updates[i].identity.clone())
                                    .collect(),
                                valid_for_seconds: 3600,
                            })?,
                            &guard,
                            self.lang,
                            false,
                        )?
                    };
                    self.quality_record(p::get(plan.id)?)?;
                }
                Some(2) => {
                    let records = p::list()?;
                    if records.is_empty() {
                        self.say("No saved plans.")?;
                        continue;
                    }
                    let mut labels = records
                        .iter()
                        .map(|r| format!("{} | {}", r.plan.id, self.lang.t(patch_state(r.status))))
                        .collect::<Vec<_>>();
                    labels.push(self.lang.t("Back"));
                    if let Some(i) = self.choose("Choose a saved plan", &labels, false)? {
                        if i < records.len() {
                            self.quality_record(records[i].clone())?;
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }
    fn quality_record(&mut self, mut record: p::Record) -> Result<()> {
        loop {
            self.report(Report::Quality(record.clone()))?;
            match self.choices(
                "Choose a quality-update plan action",
                &[
                    "Approve displayed digest only",
                    "Install the approved displayed selection",
                    "Verify installed identities only",
                    "Back",
                ],
            )? {
                Some(0) => {
                    self.report(Report::Quality(record.clone()))?;
                    self.say(QUALITY_CONSENT)?;
                    if self.confirm("Accept this exact digest, Microsoft source, all displayed EULAs and no automatic rollback?")? {
                        record = p::approve(record.plan.id, &record.plan.digest, p::Consent { owner_opt_in: true, accept_windows_update_source: true, accept_reviewed_eulas: true, acknowledge_no_automatic_rollback: true }, 900)?;
                    }
                }
                Some(1) => {
                    self.report(Report::Quality(record.clone()))?;
                    self.say(QUALITY_CONSENT)?;
                    if self.confirm("Download and install this exact approved digest now, accepting its source, EULAs and rollback limits?")? {
                        let guard = CancelGuard::install()?;
                        record = wait_patch(&p::start(record.plan.id, &record.plan.digest)?, &guard, self.lang, false)?;
                    }
                }
                Some(2) => {
                    let guard = CancelGuard::install()?;
                    record = wait_patch(&p::verify(record.plan.id)?, &guard, self.lang, false)?;
                }
                _ => return Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, collections::VecDeque};
    const ID: &str = "c4159750-106c-4e45-8d6e-5cf227c8d3dd";
    const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    fn parsed(args: &[&str]) -> ArgMatches {
        crate::command(Lang::En).try_get_matches_from(args).unwrap()
    }
    fn operation_record() -> o::PlanRecord {
        o::PlanRecord {
            plan: o::Plan {
                schema: 1,
                id: ID.parse().unwrap(),
                machine: HASH.into(),
                created_at: 1,
                expires_at: 900,
                policy: o::OwnerPolicy::default(),
                steps: vec![o::PlanStep {
                    operation: o::OperationKind::SfcVerify.spec(),
                    depends_on: vec![],
                }],
                digest: HASH.into(),
            },
            approval: None,
            consumed: false,
            steps: vec![o::StepRecord::default()],
        }
    }
    fn update(title: &str) -> p::Update {
        p::Update {
            identity: p::UpdateIdentity {
                update_id: ID.parse().unwrap(),
                revision: 17,
            },
            title: title.into(),
            description: "native description".into(),
            kb_articles: vec!["1234567".into()],
            categories: vec![],
            max_download_bytes: 1024,
            last_changed: "639000000000000000".into(),
            severity: "Critical".into(),
            handler: "CBS".into(),
            reboot_behavior: 2,
            eula: "First license line\nFinal license line".into(),
            bundled: vec![],
        }
    }
    fn quality_record() -> p::Record {
        let mut root = update("Root update");
        root.bundled.push(update("Bundled update"));
        p::Record {
            plan: p::Plan {
                schema: 1,
                id: ID.parse().unwrap(),
                binding: p::Binding {
                    machine: HASH.into(),
                    original_user: HASH.into(),
                },
                created_at: 1,
                expires_at: 900,
                source: "9482f4b4-e343-43b6-b170-9a65bc822c77".into(),
                updates: vec![root],
                digest: HASH.into(),
            },
            approval: None,
            status: p::Status::Planned,
            process: None,
            uncertain: false,
            verification: None,
        }
    }

    #[test]
    fn parse_uuid_digest_operation_and_update_identity_without_paths_or_scripts() {
        let m = parsed(&[
            "secblitz",
            "operations",
            "run",
            ID,
            "--digest",
            HASH,
            "--yes",
        ]);
        let args = m.subcommand().unwrap().1.subcommand().unwrap().1;
        assert_eq!(
            *args.get_one::<Uuid>("id").unwrap(),
            ID.parse::<Uuid>().unwrap()
        );
        assert_eq!(args.get_one::<String>("digest").unwrap(), HASH);
        for value in [
            "../state",
            "C:\\state.json",
            "{\"command\":\"cmd\"}",
            "dism_scan_health;cmd",
            "DismScanHealth",
            "upgrade_all",
        ] {
            assert!(crate::command(Lang::En)
                .try_get_matches_from(["secblitz", "operations", "plan", value])
                .is_err());
            assert!(parse_uuid(value).is_err());
            assert!(parse_update(value).is_err());
        }
        for hash in [
            "a",
            "A".repeat(64).as_str(),
            &format!("{HASH};cmd"),
            &format!("{HASH} "),
        ] {
            assert!(parse_digest(hash).is_err());
        }
        assert_eq!(parse_update(&format!("{ID}:17")).unwrap().revision, 17);
        for suffix in ["0", "-1", "01", "1;cmd", "4294967296", "1:2"] {
            assert!(parse_update(&format!("{ID}:{suffix}")).is_err());
        }
        for args in [
            vec!["secblitz", "operations", "plan", "--json-plan", "{}"],
            vec![
                "secblitz",
                "quality-updates",
                "plan",
                "--path",
                "C:\\update.exe",
            ],
            vec!["secblitz", "diagnostics", "run", "--user", "Administrator"],
        ] {
            assert!(crate::command(Lang::En).try_get_matches_from(args).is_err());
        }
    }

    #[test]
    fn mutation_consent_fails_before_any_native_authority_or_store_work() {
        for args in [
            vec!["secblitz", "operations", "approve", ID, "--digest", HASH],
            vec!["secblitz", "operations", "run", ID, "--digest", HASH],
            vec!["secblitz", "operations", "policy", "reset"],
            vec![
                "secblitz",
                "operations",
                "policy",
                "set",
                "--allow",
                "sfc_verify",
            ],
            vec!["secblitz", "quality-updates", "discover"],
            vec![
                "secblitz",
                "quality-updates",
                "approve",
                ID,
                "--digest",
                HASH,
                "--yes",
                "--accept-source",
            ],
            vec![
                "secblitz",
                "quality-updates",
                "install",
                ID,
                "--digest",
                HASH,
                "--yes",
                "--accept-eulas",
                "--acknowledge-no-rollback",
            ],
        ] {
            let error = execute(&parsed(&args), Lang::En).unwrap_err();
            assert!(
                error.downcast_ref::<UserError>().is_some(),
                "{args:?}: {error}"
            );
        }
        for group in ["operations", "quality-updates"] {
            assert!(crate::command(Lang::En)
                .try_get_matches_from(["secblitz", group, "approve", ID, "--yes"])
                .is_err());
        }
    }

    #[test]
    fn all_quality_consent_bits_are_required_without_inference() {
        let flags = [
            "--yes",
            "--accept-source",
            "--accept-eulas",
            "--acknowledge-no-rollback",
        ];
        for omitted in 0..=4 {
            let mut args = vec![
                "secblitz",
                "quality-updates",
                "approve",
                ID,
                "--digest",
                HASH,
            ];
            args.extend(
                flags
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| (i != omitted).then_some(*v)),
            );
            let m = parsed(&args);
            let a = m.subcommand().unwrap().1.subcommand().unwrap().1;
            assert_eq!(patch_consent(a).is_ok(), omitted == 4);
        }
    }

    #[test]
    fn original_user_boundary_is_never_satisfied_by_admin_substitution() {
        authorize_context(false, false, || {
            panic!("machine diagnostics must not demand elevation")
        })
        .unwrap();
        authorize_context(false, true, || Ok(false)).unwrap();
        assert_eq!(
            authorize_context(false, true, || Ok(true))
                .unwrap_err()
                .to_string(),
            ORIGINAL_REQUIRED
        );
        assert_eq!(
            authorize_context(true, false, || Ok(false))
                .unwrap_err()
                .to_string(),
            ADMIN_REQUIRED
        );
        authorize_context(true, false, || Ok(true)).unwrap();
        assert!(authorize_context(false, true, || Err(anyhow::anyhow!("unknown-token"))).is_err());
    }

    #[test]
    fn typed_policy_expirations_and_exception_scopes_are_explicit() {
        let m = parsed(&[
            "secblitz",
            "operations",
            "policy",
            "set",
            "--allow",
            "dism_scan_health,dism_restore_health",
            "--opt-in-for",
            "3600",
            "--exception",
            "dism_restore_health:window:900",
            "--yes",
        ]);
        let a = m
            .subcommand()
            .unwrap()
            .1
            .subcommand()
            .unwrap()
            .1
            .subcommand()
            .unwrap()
            .1;
        let p = policy_from_args(a, 1000).unwrap();
        assert_eq!(p.opt_in_until, Some(4600));
        assert_eq!(p.exceptions[0].expires_at, 1900);
        assert_eq!(p.exceptions[0].scope, o::ExceptionScope::MaintenanceWindow);
        assert_eq!(
            p.allowed,
            [
                o::OperationKind::DismScanHealth,
                o::OperationKind::DismRestoreHealth
            ]
        );
        for exception in [
            "sfc_repair:elevation:900",
            "sfc_repair:managed:900",
            "sfc_repair:window:0",
            "sfc_repair:window:86401",
            "sfc_repair:window:900:cmd",
        ] {
            assert!(parse_exception(exception).is_err());
        }
        let m = parsed(&[
            "secblitz",
            "operations",
            "policy",
            "set",
            "--allow",
            "sfc_repair",
            "--yes",
        ]);
        assert!(policy_from_args(
            m.subcommand()
                .unwrap()
                .1
                .subcommand()
                .unwrap()
                .1
                .subcommand()
                .unwrap()
                .1,
            1000
        )
        .is_err());
    }

    #[test]
    fn run_requires_exact_approved_digest_and_unused_record() {
        let mut r = operation_record();
        assert!(check_run(&r, HASH).is_err());
        r.approval = Some(o::Approval {
            digest: HASH.into(),
            approved_at: 1,
            expires_at: 900,
        });
        assert!(check_run(&r, HASH).is_ok());
        assert!(check_run(&r, &"b".repeat(64)).is_err());
        r.consumed = true;
        assert!(check_run(&r, HASH).is_err());
    }

    #[test]
    fn json_is_one_raw_report_and_errors_do_not_leak_native_evidence() {
        let report = Report::Diagnostics(d::collect(d::Profile::Gaming, &d::Context::default()));
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let mut out = Vec::new();
            write_report(&report, lang, true, true, &mut out).unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&out).unwrap(),
                serde_json::to_value(&report).unwrap()
            );
            let (mut out, mut err) = (Vec::new(), Vec::new());
            write_failure(
                lang,
                &anyhow::anyhow!("private-native-path"),
                true,
                true,
                &mut out,
                &mut err,
            )
            .unwrap();
            assert!(err.is_empty());
            assert!(!String::from_utf8(out.clone())
                .unwrap()
                .contains("private-native-path"));
            assert!(serde_json::from_slice::<serde_json::Value>(&out)
                .unwrap()
                .get("error")
                .is_some());
        }
    }

    #[test]
    fn review_and_diagnostic_completion_are_not_rendered_as_clean_repairs() {
        let mut record = operation_record();
        record.consumed = true;
        record.steps[0].state = o::StepState::NeedsReview;
        record.steps[0].evidence = Some(o::Evidence::DiagnosticCompleted);
        let report = Report::Operation(record);
        assert_eq!(report.exit_code(), 2);
        let mut out = Vec::new();
        write_report(&report, Lang::En, false, false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Needs review") && text.contains("clean integrity is not confirmed"));
        let mut r = quality_record();
        r.status = p::Status::RebootRequired;
        assert_eq!(Report::Quality(r).exit_code(), 2);
    }

    #[test]
    fn quality_plan_review_includes_all_bundle_licenses_and_exact_identities() {
        let mut r = quality_record();
        r.plan.updates[0].title = "\x1b[31mNative title".into();
        let mut out = Vec::new();
        write_report(&Report::Quality(r), Lang::Fr, false, false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.matches("Final license line").count(), 2);
        assert!(text.contains(&format!("{ID}:17")) && text.contains(HASH));
        assert!(text.contains("9482f4b4-e343-43b6-b170-9a65bc822c77"));
        assert!(!text.contains('\x1b'));
    }

    #[test]
    fn renderer_failure_never_exits_before_the_supervisor_finishes() {
        let polls = Cell::new(0);
        let result = supervise(
            |duration| {
                assert!(duration <= Duration::from_secs(1));
                polls.set(polls.get() + 1);
                Ok((polls.get() == 4).then_some(42))
            },
            || Err(anyhow::anyhow!("broken-output")),
        );
        assert_eq!(polls.get(), 4);
        assert_eq!(result.unwrap_err().to_string(), "broken-output");
    }

    struct Script(VecDeque<Option<usize>>);
    impl ChoiceInput for Script {
        fn select(
            &mut self,
            _: Lang,
            labels: &[String],
            default: usize,
            consent: bool,
        ) -> Result<Option<usize>> {
            if consent {
                assert_eq!(default, labels.len() - 1);
            }
            Ok(self.0.pop_front().unwrap_or(None))
        }
        fn multi_select(&mut self, _: Lang, _: &[String]) -> Result<Option<Vec<usize>>> {
            panic!("unexpected selection")
        }
    }
    #[test]
    fn guided_declines_never_approve_install_or_start_operations() {
        for action in [0, 1] {
            let mut reader = Script([Some(action), Some(1), Some(3)].into());
            let mut out = Vec::new();
            Guide {
                lang: Lang::En,
                reader: &mut reader,
                out: &mut out,
            }
            .operation_record(operation_record())
            .unwrap();
            assert!(reader.0.is_empty());
            let mut reader = Script([Some(action), Some(1), Some(3)].into());
            Guide {
                lang: Lang::En,
                reader: &mut reader,
                out: &mut out,
            }
            .quality_record(quality_record())
            .unwrap();
            assert!(reader.0.is_empty());
        }
        let mut reader = Script([Some(1), Some(1), Some(3)].into());
        Guide {
            lang: Lang::En,
            reader: &mut reader,
            out: &mut Vec::new(),
        }
        .quality()
        .unwrap();
        assert!(reader.0.is_empty());
    }
    #[test]
    fn brokered_privileged_guides_decline_before_native_account_access() {
        for group in ["operations", "quality-updates"] {
            let mut reader = Script(VecDeque::new());
            assert_eq!(
                guide(group, Lang::En, &mut reader, &mut Vec::new(), true)
                    .unwrap_err()
                    .to_string(),
                ADMIN_REQUIRED
            );
        }
    }
}
