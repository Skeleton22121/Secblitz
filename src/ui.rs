use crate::i18n::Lang;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use secblitz::engine::Report;
use std::{
    io::{self, IsTerminal, Write},
    time::Duration,
};

#[path = "advice.rs"]
pub mod advice;

pub struct Ui {
    lang: Lang,
    animate: bool,
    json: bool,
    details: bool,
}
impl Ui {
    pub fn new(lang: Lang, no_animation: bool, json: bool) -> Self {
        Self {
            lang,
            animate: !no_animation
                && !json
                && io::stderr().is_terminal()
                && system_allows_animation(),
            json,
            details: false,
        }
    }
    pub fn with_details(mut self, details: bool) -> Self {
        self.details = details;
        self
    }
    pub fn animations_enabled(&self) -> bool {
        self.animate
    }
    pub fn brand(&self) {
        if self.json || crate::menu::screen_active() {
            return;
        }
        let is_term = io::stderr().is_terminal();
        let width = if is_term {
            usize::from(console::Term::stderr().size().1)
                .saturating_sub(1)
                .max(5)
        } else {
            80
        };
        // width = terminal_cols - 1; "width >= 61" means terminal_cols >= 62.
        if is_term && width >= 61 {
            let term_cols = width + 1;
            let logo_left = term_cols.saturating_sub(60) / 2;
            let indent = " ".repeat(logo_left);
            eprintln!();
            for &row_str in &crate::menu::LOGO_ROWS {
                let mut line = indent.clone();
                let mut run = String::new();
                let mut run_is_block = false;
                for ch in row_str.chars() {
                    let block = ch == '█';
                    if run.is_empty() {
                        run_is_block = block;
                    }
                    if block == run_is_block {
                        run.push(ch);
                    } else {
                        if run_is_block {
                            line.push_str(&format!("{}", stderr_style(run.clone()).cyan().bold()));
                        } else {
                            line.push_str(&format!("{}", stderr_style(run.clone()).cyan().dim()));
                        }
                        run.clear();
                        run_is_block = block;
                        run.push(ch);
                    }
                }
                if !run.is_empty() {
                    if run_is_block {
                        line.push_str(&format!("{}", stderr_style(run).cyan().bold()));
                    } else {
                        line.push_str(&format!("{}", stderr_style(run).cyan().dim()));
                    }
                }
                eprintln!("{line}");
            }
            let tagline = format!(
                "{}  {}",
                self.lang.t("Less worry. More protection."),
                concat!("v", env!("CARGO_PKG_VERSION"))
            );
            let tagline_width = console::measure_text_width(&tagline);
            let tagline_left = term_cols.saturating_sub(tagline_width) / 2;
            eprintln!(
                "{}{}",
                " ".repeat(tagline_left),
                stderr_style(&tagline).cyan().dim()
            );
            eprintln!();
        } else {
            let inner = width.min(52).saturating_sub(4).max(1);
            eprintln!();
            eprintln!(
                "{}",
                stderr_style(format!("╭{}╮", "─".repeat(inner + 2))).cyan()
            );
            for text in [
                format!("Secblitz · v{}", env!("CARGO_PKG_VERSION")),
                self.lang.t("Less worry. More protection."),
            ] {
                for line in wrap(&text, inner) {
                    let padding =
                        " ".repeat(inner.saturating_sub(console::measure_text_width(&line)));
                    eprintln!(
                        "{}",
                        stderr_style(format!("│ {line}{padding} │")).cyan().bold()
                    );
                }
            }
            eprintln!(
                "{}\n",
                stderr_style(format!("╰{}╯", "─".repeat(inner + 2))).cyan()
            );
        }
    }
    pub fn message(&self, key: &str) {
        if !self.json {
            if crate::menu::screen_note(&self.lang.t(key)).unwrap_or(false) {
                return;
            }
            eprintln!("  {}", self.lang.t(key));
        }
    }
    pub fn done(&self) {
        self.message("Complete");
    }
    pub fn maintenance_report(&self, report: &crate::maintenance_cli::Report) -> Result<()> {
        crate::maintenance_cli::write_report(
            report,
            self.lang,
            self.json,
            self.details,
            &mut io::stdout().lock(),
        )
    }
    pub fn service_status(&self, details: &secblitz::service::StatusDetails) -> Result<()> {
        let mut out = io::stdout().lock();
        self.write_service_status(&mut out, details)
    }
    fn write_service_status(
        &self,
        out: &mut impl Write,
        details: &secblitz::service::StatusDetails,
    ) -> Result<()> {
        use secblitz::service::MonitorState;
        let state = match details.state {
            MonitorState::NotInstalled => "not installed",
            MonitorState::Stopped => "Stopped",
            MonitorState::StartPending => "StartPending",
            MonitorState::StopPending => "StopPending",
            MonitorState::Running => "Running",
            MonitorState::ContinuePending => "ContinuePending",
            MonitorState::PausePending => "PausePending",
            MonitorState::Paused => "Paused",
        };
        writeln!(out, "  SecblitzMonitor: {}", self.lang.t(state))?;
        if details.state != MonitorState::NotInstalled {
            let code = |value: Option<u32>| {
                value
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| self.lang.t("Not applicable"))
            };
            writeln!(
                out,
                "  {}: Win32={}; {}={}; {}={}; {}={}",
                self.lang.t("Service diagnostic (native codes and values)"),
                code(details.win32_exit_code),
                self.lang.t("Service exit code"),
                code(details.service_exit_code),
                self.lang.t("Checkpoint"),
                details.checkpoint,
                self.lang.t("Wait hint (ms)"),
                details.wait_hint_ms
            )?;
        }
        Ok(())
    }
    pub fn progress(&self) -> Progress {
        self.progress_named("Working")
    }
    pub fn progress_named(&self, phase: &str) -> Progress {
        let guided = !self.json && crate::menu::screen_active();
        let bar = if self.animate && !guided {
            ProgressBar::new_spinner()
        } else {
            ProgressBar::with_draw_target(None, ProgressDrawTarget::hidden())
        };
        bar.set_style(
            ProgressStyle::with_template("  {spinner:.cyan} {msg}  {elapsed_precise}")
                .unwrap()
                .tick_strings(&["[=   ]", "[ =  ]", "[  = ]", "[   =]", "[  = ]", "[ =  ]"]),
        );
        bar.set_message(self.lang.t("Working"));
        if guided {
            crate::menu::screen_progress(
                &self.lang.t(phase),
                &self.lang.t("Keep this window open while Windows finishes."),
            );
        } else if self.animate {
            bar.enable_steady_tick(Duration::from_millis(100));
        } else {
            self.message("Working");
        }
        Progress {
            bar,
            lang: self.lang,
            lines: !self.animate && !self.json && !guided,
            guided,
            phase: self.lang.t(phase),
            items: std::cell::RefCell::new(Vec::new()),
        }
    }
    pub fn report(&self, report: &Report, _review: bool) -> Result<()> {
        if self.json {
            return json(report);
        }
        if crate::menu::screen_active() {
            return crate::menu::screen_note(&self.guided_report_text(report)).map(|_| ());
        }
        self.write_report(&mut io::stdout().lock(), report, terminal_width())?;
        if self.details {
            self.report_details(report)?;
        }
        Ok(())
    }
    /// A fresh scan gets a compact overview; the full table is a menu choice.
    pub fn summary(&self, report: &Report) -> Result<()> {
        if self.json {
            return Ok(());
        }
        if crate::menu::screen_active() {
            self.guided_header(Some(report));
            return Ok(());
        }
        let mut counts = [0usize; 3];
        for a in report.results.iter().map(advice::for_outcome).chain(
            report
                .findings
                .iter()
                .map(|f| advice::for_finding(&f.title, &f.status, &f.detail)),
        ) {
            counts[match a.group {
                advice::Group::Recommended => 0,
                advice::Group::Protected => 1,
                advice::Group::Choice => 2,
                advice::Group::Information => continue,
            }] += 1;
        }
        let mut out = io::stdout().lock();
        heading(
            &mut out,
            &self.lang.t("Your PC, checked."),
            terminal_width(),
        )?;
        self.write_readiness(&mut out, report, terminal_width())?;
        let text = ["Recommended fixes", "Protected", "Needs your choice"]
            .iter()
            .zip(counts)
            .map(|(key, n)| format!("{}: {n}", self.lang.t(key)))
            .collect::<Vec<_>>()
            .join(" · ");
        text_lines(&mut out, &text, terminal_width())?;
        if report.results.is_empty() && report.findings.is_empty() {
            text_lines(
                &mut out,
                &self
                    .lang
                    .t("No checks were returned. Run a new check to review protection."),
                terminal_width(),
            )?;
        }
        Ok(())
    }
    fn write_report(&self, out: &mut impl Write, report: &Report, width: usize) -> Result<()> {
        use advice::Group;
        heading(out, &self.lang.t("Your PC, checked."), width)?;
        self.write_readiness(out, report, width)?;
        let mut groups: [Vec<(String, String, String)>; 4] = Default::default();
        let mut number = 0;
        let entries = report.results.iter().map(advice::for_outcome).chain(
            report
                .findings
                .iter()
                .map(|f| advice::for_finding(&f.title, &f.status, &f.detail)),
        );
        for a in entries {
            let group = match a.group {
                Group::Recommended => 0,
                Group::Protected => 1,
                Group::Choice => 2,
                Group::Information => 3,
            };
            let mut title = self.lang.t(a.label);
            if a.step == advice::NextStep::Repair {
                number += 1;
                title = format!("{number}. {title}");
            }
            let next_cell = if let Some(imp) = impact_line(self.lang, &a) {
                format!("{imp}\n{}", self.lang.t(a.next))
            } else {
                self.lang.t(a.next)
            };
            // Status chip carries the glyph so write_table can colour it.
            let chip = format!("{} {}", advice_glyph(&a), self.lang.t(a.status));
            groups[group].push((title, chip, next_cell));
        }
        let names = [
            "Recommended fixes",
            "Protected",
            "Needs your choice",
            "More information",
        ];
        // Compact totals line with icon per group (first three groups only).
        let group_icons = ['!', '\u{2713}', '?']; // !, ✓, ?
        let totals = groups
            .iter()
            .zip(names)
            .zip(group_icons)
            .take(3)
            .map(|((rows, key), icon)| format!("{icon} {}: {}", self.lang.t(key), rows.len()))
            .collect::<Vec<_>>()
            .join(" \u{b7} "); // · separator
        text_lines(out, &totals, width)?;
        // Group headings include an icon and item count.
        let heading_icons = ['!', '\u{2713}', '?', '\u{2022}']; // !, ✓, ?, •
        for ((rows, name), icon) in groups.iter().zip(names).zip(heading_icons) {
            if !rows.is_empty() {
                heading(
                    out,
                    &format!("{icon} {} ({})", self.lang.t(name), rows.len()),
                    width,
                )?;
                self.write_table(out, rows, width)?;
            }
        }
        if report.results.is_empty() && report.findings.is_empty() {
            text_lines(
                out,
                &self
                    .lang
                    .t("No checks were returned. Run a new check to review protection."),
                width,
            )?;
        }
        if report.transaction.is_some() {
            // A revert report also carries an ID: do not promise another undo.
            let reverting = report.results.iter().any(|r| {
                r.status == "restored" || r.detail == "Original preference already present"
            });
            text_lines(out, &self.lang.t(if reverting { "Your saved changes were reviewed." } else { "Saved changes are available for review or undo. This does not mean every requested fix completed." }), width)?;
        }
        Ok(())
    }
    fn write_readiness(&self, out: &mut impl Write, report: &Report, width: usize) -> Result<()> {
        use secblitz::model::Probe;
        let Some(readiness) = &report.readiness else {
            return Ok(());
        };
        let mut parts = Vec::new();
        for (name, volume, journal) in [
            ("Windows drive", &readiness.system_volume, false),
            ("Saved changes drive", &readiness.journal_volume, true),
        ] {
            let value = match volume {
                Probe::Unknown => self.lang.t("Free space unknown"),
                Probe::Known(volume) => {
                    let mut text = self.lang.t("{gb} GB free").replace(
                        "{gb}",
                        &format!("{:.1}", volume.available_bytes as f64 / 1_000_000_000.0),
                    );
                    if volume.read_only {
                        text.push_str(&format!(
                            ". {}",
                            self.lang.t("Disk is read-only. Fixes will wait.")
                        ));
                    } else if journal && volume.available_bytes == 0 {
                        text.push_str(&format!(
                            ". {}",
                            self.lang.t("No space for saved changes. Fixes will wait.")
                        ));
                    }
                    text
                }
            };
            parts.push(format!("{}: {value}", self.lang.t(name)));
        }
        match &readiness.power {
            Probe::Unknown => parts.push(self.lang.t("Power information unknown")),
            Probe::Known(power) => {
                parts.push(self.lang.t(match power.ac_connected {
                    Some(true) => "Plugged in",
                    Some(false) => "Not plugged in",
                    None => "Power source unknown",
                }));
                parts.push(match power.battery_present {
                    Some(false) => self.lang.t("Battery: not applicable"),
                    Some(true) => match power.battery_percent.filter(|p| *p <= 100) {
                        Some(percent) => {
                            let mut text = self
                                .lang
                                .t("Battery: {percent}%")
                                .replace("{percent}", &percent.to_string());
                            if percent <= 20 {
                                text.push_str(&format!(
                                    ". {}",
                                    self.lang
                                        .t("Low battery. Connect power before making changes.")
                                ));
                            }
                            text
                        }
                        None => self.lang.t("Battery level unknown"),
                    },
                    None => self.lang.t("Battery information unknown"),
                });
            }
        }
        parts.push(self.lang.t(match readiness.windows_update_reboot {
            Probe::Known(true) => {
                "Windows Update needs a restart. Save your work and restart when ready."
            }
            Probe::Known(false) => "No update restart pending",
            Probe::Unknown => "Update restart status unknown",
        }));
        text_lines(
            out,
            &format!("{}: {}", self.lang.t("Device check"), parts.join(" · ")),
            width,
        )
    }
    /// Technical evidence is opt-in; JSON always remains the original report.
    pub fn report_details(&self, report: &Report) -> Result<()> {
        if self.json {
            return json(report);
        }
        if crate::menu::screen_active() {
            return crate::menu::screen_note(&self.details_text(report)?).map(|_| ());
        }
        self.write_report_details(&mut io::stdout().lock(), report)
    }
    pub fn details_text(&self, report: &Report) -> Result<String> {
        let mut bytes = Vec::new();
        self.write_report_details(&mut bytes, report)?;
        Ok(String::from_utf8(bytes)?)
    }
    pub fn guided_report_text(&self, report: &Report) -> String {
        use advice::Group;
        // Collect all advice items with their sequential recommended number.
        let entries: Vec<advice::Advice> = report
            .results
            .iter()
            .map(advice::for_outcome)
            .chain(
                report
                    .findings
                    .iter()
                    .map(|f| advice::for_finding(&f.title, &f.status, &f.detail)),
            )
            .collect();
        // Assign numbers to Recommended items only (first pass).
        let mut num = 0usize;
        let numbers: Vec<Option<usize>> = entries
            .iter()
            .map(|a| {
                if a.step == advice::NextStep::Repair {
                    num += 1;
                    Some(num)
                } else {
                    None
                }
            })
            .collect();
        // Bucket into four groups preserving order.
        let mut groups: [Vec<(advice::Advice, Option<usize>)>; 4] =
            [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        for (a, n) in entries.into_iter().zip(numbers) {
            let g = match a.group {
                Group::Recommended => 0,
                Group::Protected => 1,
                Group::Choice => 2,
                Group::Information => 3,
            };
            groups[g].push((a, n));
        }
        let names = [
            "Recommended fixes",
            "Protected",
            "Needs your choice",
            "More information",
        ];
        let mut parts: Vec<String> = Vec::new();
        for (g_items, name) in groups.iter().zip(names) {
            if g_items.is_empty() {
                continue;
            }
            parts.push(format!("▸ {} ({})", self.lang.t(name), g_items.len()));
            for (a, maybe_num) in g_items {
                let glyph = advice_glyph(a);
                let label = self.lang.t(a.label);
                let status = self.lang.t(a.status);
                let title_line = if let Some(n) = maybe_num {
                    format!("{glyph} {n}. {label} \u{2014} {status}")
                } else {
                    format!("{glyph} {label} \u{2014} {status}")
                };
                parts.push(title_line);
                if let Some(imp) = impact_line(self.lang, a) {
                    parts.push(format!("  \u{b7} {imp}"));
                }
                parts.push(format!("  {}", self.lang.t(a.next)));
                parts.push(String::new());
            }
        }
        // Trim trailing blank lines.
        while parts.last().map_or(false, |s| s.is_empty()) {
            parts.pop();
        }
        parts.join("\n")
    }
    pub fn guided_header(&self, report: Option<&Report>) {
        self.guided_header_with_failure(report, false);
    }
    pub fn guided_header_with_failure(&self, report: Option<&Report>, failed: bool) {
        crate::menu::screen_header(self.status_header(report, failed));
    }
    fn status_header(&self, report: Option<&Report>, failed: bool) -> crate::menu::Header {
        use crate::menu::{Badge, Header, Role};
        use secblitz::model::Authority;
        let mut header = Header {
            subtitle: self.lang.t("Last protection check"),
            badges: Vec::new(),
            tally: None,
        };
        if failed {
            header.badges.push(Badge {
                text: self.lang.t("Check failed"),
                role: Role::Failure,
            });
        }
        let Some(report) = report else {
            header.badges.push(Badge {
                text: self.lang.t("Unverified"),
                role: Role::Unknown,
            });
            return header;
        };
        let mut counts = [0usize; 5];
        let mut count = |group, status: &str, unavailable| {
            // Stable backend statuses and typed evidence, never rendered prose.
            if status == "error" {
                counts[4] += 1;
            } else if unavailable
                || matches!(status, "unknown" | "unsupported")
                || !matches!(
                    status,
                    "compliant"
                        | "ok"
                        | "applied"
                        | "restored"
                        | "unchanged"
                        | "attention"
                        | "skipped"
                        | "conflict"
                        | "pending"
                        | "info"
                        | "review"
                )
            {
                counts[3] += 1;
            } else {
                match group {
                    advice::Group::Protected => counts[0] += 1,
                    advice::Group::Recommended => counts[1] += 1,
                    advice::Group::Choice => counts[2] += 1,
                    advice::Group::Information => {}
                }
            }
        };
        for r in &report.results {
            count(
                advice::for_outcome(r).group,
                &r.status,
                r.authority == Some(Authority::Unknown)
                    || (r.id.starts_with("firewall.")
                        && (r.effective.is_none() || r.authority.is_none())),
            );
        }
        for f in &report.findings {
            count(
                advice::for_finding(&f.title, &f.status, &f.detail).group,
                &f.status,
                false,
            );
        }
        for (index, (key, role)) in [
            ("{count} protected", Role::Healthy),
            ("{count} fixes", Role::Review),
            ("{count} review", Role::Review),
            ("{count} unknown", Role::Unknown),
            ("{count} failed", Role::Failure),
        ]
        .into_iter()
        .enumerate()
        {
            if counts[index] > 0 {
                header.badges.push(Badge {
                    text: self
                        .lang
                        .t(key)
                        .replace("{count}", &counts[index].to_string()),
                    role,
                });
            }
        }
        if header.badges.is_empty() {
            header.badges.push(Badge {
                text: self.lang.t("Not assessed"),
                role: Role::Unknown,
            });
        }
        if report
            .readiness
            .as_ref()
            .is_some_and(secblitz::model::Readiness::blocks_repairs)
        {
            header.badges.push(Badge {
                text: self.lang.t("Storage blocked"),
                role: Role::Failure,
            });
        }
        // Tally: (protected_count, total_checks) for the progress bar.
        let total: usize = counts.iter().sum();
        if total > 0 {
            header.tally = Some((counts[0], total));
        }
        header
    }
    fn write_report_details(&self, out: &mut impl Write, report: &Report) -> Result<()> {
        writeln!(out, "\n{}", self.lang.t("Details"))?;
        if let Some(readiness) = &report.readiness {
            writeln!(
                out,
                "{}: {}",
                self.lang.t("Readiness evidence"),
                safe(&serde_json::to_string(readiness)?)
            )?;
        }
        if let Some(tx) = &report.transaction {
            writeln!(out, "{}: {}", self.lang.t("Transaction"), safe(tx))?;
        }
        for r in &report.results {
            self.row(
                out,
                &format!(
                    "{} ({})",
                    self.lang.t(advice::control_label(&r.id)),
                    safe(&r.id)
                ),
                &r.status,
                &r.detail,
            )?;
            if r.effective.is_some() || r.authority.is_some() {
                writeln!(
                    out,
                    "    {}: {}",
                    self.lang.t("Firewall evidence"),
                    safe(&serde_json::to_string(
                        &serde_json::json!({"effective": r.effective, "authority": r.authority})
                    )?)
                )?;
            }
        }
        for f in &report.findings {
            self.row(out, &self.lang.detail(&f.title), &f.status, &f.detail)?;
        }
        Ok(())
    }
    fn write_table(
        &self,
        out: &mut impl Write,
        rows: &[(String, String, String)],
        width: usize,
    ) -> Result<()> {
        let headers = [
            self.lang.t("Protection"),
            self.lang.t("Status"),
            self.lang.t("Why it matters / Next step"),
        ];
        if width <= 60 {
            // Narrow layout: stacked cards with rounded corners.
            let inner = width.saturating_sub(4).max(1);
            for (title, status, next) in rows {
                border(out, &[inner], '\u{256d}', '\u{252c}', '\u{256e}')?; // ╭ ┬ ╮
                for (key, value) in headers.iter().zip([title, status, next]) {
                    // Multi-line values (impact + next step) start on their own line.
                    let sep = if value.contains('\n') { ":\n" } else { ": " };
                    for line in wrap(&format!("{key}{sep}{value}"), inner) {
                        cells(out, &[line], &[inner])?;
                    }
                }
                border(out, &[inner], '\u{2570}', '\u{2534}', '\u{256f}')?; // ╰ ┴ ╯
            }
            return Ok(());
        }
        // Wide layout: table with rounded outer corners.
        let space = width.saturating_sub(10);
        let sizes = [
            space * 30 / 100,
            space * 24 / 100,
            space - space * 30 / 100 - space * 24 / 100,
        ];
        border(out, &sizes, '\u{256d}', '\u{252c}', '\u{256e}')?; // ╭ ┬ ╮
        wrapped_cells(out, &headers, &sizes)?;
        for (title_str, status_str, next_str) in rows {
            border(out, &sizes, '\u{251c}', '\u{253c}', '\u{2524}')?; // ├ ┼ ┤
            // Wrap plain text first; safe() inside wrap() would strip ANSI.
            let title_lines = wrap(title_str, sizes[0]);
            let status_lines = wrap(status_str, sizes[1]);
            let next_lines = wrap(next_str, sizes[2]);
            // Count lines that belong to the impact segment (before the \n separator).
            let impact_line_count = if next_str.contains('\n') {
                let seg = next_str.split('\n').next().unwrap_or("");
                wrap(seg, sizes[2]).len()
            } else {
                0
            };
            let max_len = title_lines
                .len()
                .max(status_lines.len())
                .max(next_lines.len());
            for i in 0..max_len {
                let t = title_lines.get(i).cloned().unwrap_or_default();
                let s_plain = status_lines.get(i).cloned().unwrap_or_default();
                let n_plain = next_lines.get(i).cloned().unwrap_or_default();
                // Apply glyph-based colour to the status chip.
                let s = if s_plain.is_empty() {
                    s_plain
                } else {
                    chip_style(&s_plain)
                };
                // Dim the impact line(s) in the next-step cell.
                let n = if i < impact_line_count && !n_plain.is_empty() {
                    format!("{}", style(n_plain).dim())
                } else {
                    n_plain
                };
                cells(out, &[t, s, n], &sizes)?;
            }
        }
        border(out, &sizes, '\u{2570}', '\u{2534}', '\u{256f}')?; // ╰ ┴ ╯
        Ok(())
    }
    fn row(&self, out: &mut impl Write, title: &str, status: &str, detail: &str) -> Result<()> {
        let label = style(safe(&self.lang.t(status))).bold();
        let label = match status {
            "compliant" | "ok" | "applied" | "restored" | "unchanged" => label.green(),
            "error" | "conflict" => label.red(),
            "attention" | "review" | "unknown" | "pending" | "skipped" => label.yellow(),
            _ => label.cyan(),
        };
        writeln!(out, "  [{}] {}", label, safe(title))?;
        if !detail.is_empty() {
            writeln!(
                out,
                "    {}: {}",
                self.lang.t("Details"),
                safe(&self.lang.detail(detail))
            )?;
        }
        Ok(())
    }
    pub fn history(&self, history: &[String]) -> Result<()> {
        if self.json {
            return json(history);
        }
        let mut out = io::stdout().lock();
        writeln!(out, "\n  {}", style(self.lang.t("History")).cyan().bold())?;
        if history.is_empty() {
            writeln!(out, "  {}", self.lang.t("No transactions recorded"))?;
        }
        for entry in history {
            if let Some((id, status)) = entry.rsplit_once(' ') {
                if self.details {
                    writeln!(out, "  {}  [{}]", safe(id), safe(&self.lang.t(status)))?;
                } else {
                    writeln!(out, "  {}", safe(&self.lang.t(status)))?;
                }
            } else {
                writeln!(
                    out,
                    "  {}",
                    if self.details {
                        safe(entry)
                    } else {
                        self.lang.t("Needs your choice")
                    }
                )?;
            }
        }
        Ok(())
    }
}

pub(crate) fn system_allows_animation() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        extern "system" {
            fn SystemParametersInfoW(
                action: u32,
                param: u32,
                value: *mut std::ffi::c_void,
                flags: u32,
            ) -> i32;
        }
        let mut enabled: i32 = 0;
        // SPI_GETCLIENTAREAANIMATION: honor Windows accessibility preferences.
        unsafe {
            SystemParametersInfoW(0x1042, 0, (&mut enabled as *mut i32).cast(), 0) != 0
                && enabled != 0
        }
    }
    #[cfg(not(windows))]
    {
        true
    }
}

pub(crate) fn system_high_contrast() -> bool {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct HighContrast {
            size: u32,
            flags: u32,
            scheme: *mut u16,
        }
        #[link(name = "user32")]
        extern "system" {
            fn SystemParametersInfoW(
                action: u32,
                param: u32,
                value: *mut std::ffi::c_void,
                flags: u32,
            ) -> i32;
        }
        let mut value = HighContrast {
            size: std::mem::size_of::<HighContrast>() as u32,
            flags: 0,
            scheme: std::ptr::null_mut(),
        };
        unsafe {
            SystemParametersInfoW(
                0x42,
                value.size,
                (&mut value as *mut HighContrast).cast(),
                0,
            ) != 0
                && value.flags & 1 != 0
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn json(value: &(impl serde::Serialize + ?Sized)) -> Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}

pub struct Progress {
    bar: ProgressBar,
    lang: Lang,
    lines: bool,
    guided: bool,
    phase: String,
    /// Accumulated finished-check lines for the live guided-mode checklist.
    items: std::cell::RefCell<Vec<String>>,
}
impl Progress {
    pub fn update(&self, id: &str, status: &str) {
        let message = safe(&format!(
            "{} · {}",
            self.lang.t(advice::control_label(id)),
            self.lang.t(match status {
                "pending" => "Checking",
                "complete" => "Complete",
                "attention" => "Needs your choice",
                "compliant" | "ok" => "Good to go",
                "applied" => "Fixed",
                "error" | "unknown" => "Couldn't check",
                _ => "Needs your choice",
            })
        ));
        if self.guided {
            // Accumulate finished checks for the live checklist.
            // "pending" is transient (not a final state); skip it.
            let prefix = match status {
                // A finished phase (device check, extra checks) is not protection.
                "complete" => "· ",
                "compliant" | "ok" | "applied" => "✓ ",
                "error" | "unknown" => "? ",
                "pending" => "",
                _ => "! ",
            };
            if !prefix.is_empty() {
                self.items.borrow_mut().push(format!("{prefix}{message}"));
            }
            let body = {
                let items = self.items.borrow();
                if items.is_empty() {
                    message.clone()
                } else {
                    items.join("\n")
                }
            };
            crate::menu::screen_progress(&self.phase, &body);
        } else if self.lines {
            eprintln!("  {message}");
        } else {
            self.bar.set_message(message);
        }
    }
}

fn terminal_width() -> usize {
    if io::stdout().is_terminal() {
        // Reserve the final physical column: legacy consoles may wrap there
        // before our newline, producing a second, apparently blank row.
        usize::from(console::Term::stdout().size().1)
            .saturating_sub(1)
            .max(5)
    } else {
        80
    }
}

fn style<D: std::fmt::Display>(value: D) -> console::StyledObject<D> {
    console::style(value).force_styling(crate::menu::colors_enabled_read_only(false))
}

fn stderr_style<D: std::fmt::Display>(value: D) -> console::StyledObject<D> {
    console::style(value)
        .for_stderr()
        .force_styling(crate::menu::colors_enabled_read_only(true))
}

pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut result = Vec::new();
    // Split on explicit newlines first; wrap each segment independently.
    for segment in text.split('\n') {
        let segment = safe(segment);
        let body = segment.trim_start();
        // Leading indentation is layout (e.g. an item's detail lines), so keep it
        // on the first line and its continuations, unless it would leave no room.
        let indent = " ".repeat(segment.len() - segment.trim_start_matches(' ').len());
        let indent = if indent.len() * 2 < width { indent } else { String::new() };
        let width = width - indent.len();
        let start = result.len();
        let mut line = String::new();
        for word in body.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if console::measure_text_width(&candidate) <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                result.push(std::mem::take(&mut line));
            }
            for c in word.chars() {
                let candidate = format!("{line}{c}");
                if console::measure_text_width(&candidate) > width && !line.is_empty() {
                    result.push(std::mem::take(&mut line));
                }
                if console::measure_text_width(&c.to_string()) <= width {
                    line.push(c);
                } else {
                    line.push('?');
                }
            }
        }
        result.push(line);
        if !indent.is_empty() {
            for wrapped in &mut result[start..] {
                if !wrapped.is_empty() {
                    wrapped.insert_str(0, &indent);
                }
            }
        }
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}
/// Returns a translated impact line "{prefix} {phrase}", or None when the
/// advice has no impact phrase. Prefix and phrase are translated separately.
pub(crate) fn impact_line(lang: Lang, a: &advice::Advice) -> Option<String> {
    let prefix = a.impact_prefix();
    if prefix.is_empty() {
        return None;
    }
    Some(format!("{} {}", lang.t(prefix), lang.t(a.impact)))
}

/// Returns the display glyph for a piece of advice, based on status.
pub(crate) fn advice_glyph(a: &advice::Advice) -> char {
    match a.status {
        "Good to go" | "Protected by Windows" | "Fixed" => '✓',
        "Can fix" => '!',
        "Restart needed" => '↻',
        "Couldn't check" | "Managed elsewhere" => '?',
        "For your information" | "Needs your choice" => '•',
        _ => '!',
    }
}

/// Style a chip string by its leading glyph character. Returns unstyled string
/// when colors are disabled (force_styling follows the console crate flag).
fn chip_style(line: &str) -> String {
    let first = line.chars().next();
    match first {
        Some('✓') => format!("{}", style(line.to_owned()).green()),
        Some('!') | Some('↻') => format!("{}", style(line.to_owned()).yellow()),
        Some('?') | Some('•') => format!("{}", style(line.to_owned()).dim()),
        _ => line.to_owned(),
    }
}

fn text_lines(out: &mut impl Write, text: &str, width: usize) -> Result<()> {
    for line in wrap(text, width) {
        writeln!(out, "{line}")?;
    }
    Ok(())
}
fn heading(out: &mut impl Write, text: &str, width: usize) -> Result<()> {
    writeln!(out)?;
    for line in wrap(text, width) {
        writeln!(out, "{}", style(line).cyan().bold())?;
    }
    Ok(())
}
fn border(
    out: &mut impl Write,
    widths: &[usize],
    left: char,
    join: char,
    right: char,
) -> Result<()> {
    let middle = widths
        .iter()
        .map(|w| "─".repeat(w + 2))
        .collect::<Vec<_>>()
        .join(&join.to_string());
    writeln!(out, "{}", style(format!("{left}{middle}{right}")).cyan())?;
    Ok(())
}
fn cells(out: &mut impl Write, values: &[String], widths: &[usize]) -> Result<()> {
    write!(out, "│")?;
    for (value, width) in values.iter().zip(widths) {
        write!(
            out,
            " {value}{} │",
            " ".repeat(width.saturating_sub(console::measure_text_width(value)))
        )?;
    }
    writeln!(out)?;
    Ok(())
}
fn wrapped_cells(out: &mut impl Write, values: &[String], widths: &[usize]) -> Result<()> {
    let columns: Vec<_> = values
        .iter()
        .zip(widths)
        .map(|(text, w)| wrap(text, *w))
        .collect();
    for i in 0..columns.iter().map(Vec::len).max().unwrap_or(0) {
        cells(
            out,
            &columns
                .iter()
                .map(|c| c.get(i).cloned().unwrap_or_default())
                .collect::<Vec<_>>(),
            widths,
        )?;
    }
    Ok(())
}
impl Drop for Progress {
    fn drop(&mut self) {
        if self.guided {
            crate::menu::screen_progress_end();
        }
        self.bar.finish_and_clear();
    }
}
#[cfg(test)]
impl Progress {
    /// Returns the accumulated checklist items; available for tests only.
    pub(crate) fn checklist_items(&self) -> Vec<String> {
        self.items.borrow().clone()
    }
}

/// OS evidence is untrusted terminal text. Remove terminal controls and bidi
/// overrides without altering the raw JSON report.
pub(crate) fn safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}
pub(crate) fn error_details(lang: Lang, error: &anyhow::Error) -> String {
    // Native exceptions are evidence, not application prose. Preserve them
    // verbatim (apart from terminal sanitization) instead of translating tokens.
    error
        .chain()
        .map(|cause| {
            let text = cause.to_string();
            if cause.is::<io::Error>() {
                safe(&text)
            } else {
                safe(&lang.detail(&text))
            }
        })
        .collect::<Vec<_>>()
        .join(": ")
}
pub fn error(lang: Lang, error: &anyhow::Error) {
    eprintln!(
        "\n  {}\n  {}: {}",
        stderr_style(lang.t("Operation failed")).red().bold(),
        lang.t("Details"),
        error_details(lang, error)
    );
}

pub fn password(lang: Lang) -> Result<()> {
    use rand::RngCore;
    anyhow::ensure!(
        io::stdout().is_terminal(),
        lang.t("Password output requires an interactive terminal.")
    );
    // 64 distinct characters: every six-bit value is equally likely. No modulo
    // bias, character-class repair, predictable seeds, logging or clipboard use.
    let mut random = [0u8; PASSWORD_LENGTH];
    rand::rngs::OsRng
        .try_fill_bytes(&mut random)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context(lang.t("Password generation failed"))?;
    let mut secret = encode_password(&random);
    // Keep the secret out of generic renderers, report values and error context.
    let result = (|| -> io::Result<()> {
        if crate::menu::screen_active() {
            return crate::menu::private_view(
                lang,
                &lang.t("New password - save it in your password manager"),
                &secret,
                &lang.t(
                    "Not saved or copied to the clipboard. Existing passwords were not inspected.",
                ),
            )
            .map_err(|error| io::Error::other(error.to_string()));
        }
        let mut out = io::stdout().lock();
        writeln!(
            out,
            "\n{}",
            lang.t("New password - save it in your password manager")
        )?;
        out.write_all(&secret)?;
        writeln!(
            out,
            "\n{}\n",
            lang.t("Not saved or copied to the clipboard. Existing passwords were not inspected.")
        )?;
        out.flush()
    })();
    for byte in random.iter_mut().chain(secret.iter_mut()) {
        unsafe {
            std::ptr::write_volatile(byte, 0);
        }
    }
    result.context(lang.t("Password output failed"))?;
    Ok(())
}

const PASSWORD_LENGTH: usize = 24;
fn encode_password(random: &[u8; PASSWORD_LENGTH]) -> [u8; PASSWORD_LENGTH] {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    random.map(|byte| ALPHABET[(byte & 63) as usize])
}

pub fn owns_console() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
        }
        let mut processes = [0u32; 2];
        unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 2) == 1 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}
pub fn pause(lang: Lang) {
    // Installer workers can inherit a terminal input handle while their output
    // is captured. Never make automation wait for an invisible prompt.
    if !interactive_pause_allowed(
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
        io::stderr().is_terminal(),
    ) {
        return;
    }
    eprintln!("{}", lang.t("Press Enter to close"));
    let _ = io::stdin().read_line(&mut String::new());
}

fn interactive_pause_allowed(input: bool, output: bool, error: bool) -> bool {
    input && output && error
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_badges_follow_evidence_roles_not_translated_labels_or_missing_data() {
        use crate::menu::Role;
        use secblitz::{
            engine::Outcome,
            model::{Authority, EffectiveFirewall},
        };
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let ui = Ui::new(lang, true, false);
            let absent = ui.status_header(None, true);
            assert_eq!(
                absent.badges.iter().map(|b| b.role).collect::<Vec<_>>(),
                [Role::Failure, Role::Unknown]
            );
            let empty = ui.status_header(Some(&Report::default()), false);
            assert_eq!(empty.badges.len(), 1);
            assert_eq!(empty.badges[0].role, Role::Unknown);
            let mut report = Report {
                results: vec![
                    Outcome {
                        id: "uac.enabled".into(),
                        status: "compliant".into(),
                        ..Outcome::default()
                    },
                    Outcome {
                        id: "uac.consent".into(),
                        status: "attention".into(),
                        ..Outcome::default()
                    },
                    Outcome {
                        id: "firewall.public.enabled".into(),
                        status: "compliant".into(),
                        ..Outcome::default()
                    },
                    Outcome {
                        id: "defender.ioav".into(),
                        status: "error".into(),
                        ..Outcome::default()
                    },
                ],
                ..Report::default()
            };
            let header = ui.status_header(Some(&report), false);
            assert_eq!(
                header.badges.iter().map(|b| b.role).collect::<Vec<_>>(),
                [Role::Healthy, Role::Review, Role::Unknown, Role::Failure]
            );
            report.results[2].effective = Some(EffectiveFirewall::Enabled(false));
            report.results[2].authority = Some(Authority::Local);
            let header = ui.status_header(Some(&report), false);
            assert_eq!(
                header
                    .badges
                    .iter()
                    .filter(|b| b.role == Role::Healthy)
                    .count(),
                1
            );
            assert_eq!(
                header.badges[0].text,
                lang.t("{count} protected").replace("{count}", "1")
            );
            report.results[0].status = "future-status".into();
            assert!(!ui
                .status_header(Some(&report), false)
                .badges
                .iter()
                .any(|b| b.role == Role::Healthy));
        }
    }

    #[test]
    fn readiness_is_informational_wraps_and_does_not_inflate_protection_totals() {
        use secblitz::model::{Finding, PowerReadiness, Probe, Readiness, VolumeReadiness};
        let mut report = Report {
            readiness: Some(Readiness {
                system_volume: Probe::Known(VolumeReadiness {
                    available_bytes: 9_000_000_000,
                    read_only: false,
                }),
                journal_volume: Probe::Known(VolumeReadiness {
                    available_bytes: 0,
                    read_only: false,
                }),
                power: Probe::Known(PowerReadiness {
                    ac_connected: Some(true),
                    battery_present: Some(false),
                    battery_percent: Some(0),
                }),
                windows_update_reboot: Probe::Known(false),
            }),
            findings: vec![Finding {
                title: "Journal recovery".into(),
                status: "info".into(),
                detail: "private evidence".into(),
            }],
            ..Default::default()
        };
        let ui = Ui::new(Lang::En, true, false);
        for width in [40, 80, 100] {
            let mut out = Vec::new();
            ui.write_report(&mut out, &report, width).unwrap();
            let text = String::from_utf8(out).unwrap();
            let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(text
                .lines()
                .all(|line| console::measure_text_width(line) <= width));
            assert!(flat.contains("9.0 GB free"));
            assert!(flat.contains("No space for saved changes. Fixes will wait."));
            assert!(flat.contains("Battery: not applicable"));
            assert!(!flat.contains("Low battery"));
            assert!(flat.contains("No update restart pending"));
            assert!(flat.contains("! Recommended fixes: 0 · ✓ Protected: 0 · ? Needs your choice: 0"));
            assert!(text.contains("More information"));
            assert!(text.find("Device check").unwrap() < text.find("Recommended fixes").unwrap());
            assert!(!text.contains("private evidence"));
            assert!(!text.contains("No checks were returned"));
        }
        report.readiness = Some(Readiness::default());
        let mut out = Vec::new();
        ui.write_readiness(&mut out, &report, 1000).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.matches("Free space unknown").count(), 2);
        assert!(text.contains("Power information unknown"));
        assert!(text.contains("Update restart status unknown"));
        assert!(!text.contains("Protected"));
    }

    #[test]
    fn localized_readiness_is_separate_from_protection_in_all_six_languages() {
        use secblitz::model::{Finding, PowerReadiness, Probe, Readiness, VolumeReadiness};
        let report = Report {
            readiness: Some(Readiness {
                system_volume: Probe::Known(VolumeReadiness {
                    available_bytes: 9_000_000_000,
                    read_only: false,
                }),
                power: Probe::Known(PowerReadiness {
                    ac_connected: Some(false),
                    battery_present: Some(true),
                    battery_percent: Some(20),
                }),
                ..Default::default()
            }),
            findings: vec![Finding {
                title: "Windows updates".into(),
                status: "info".into(),
                detail: "private technical evidence".into(),
            }],
            ..Default::default()
        };
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let ui = Ui::new(lang, true, false);
            for width in [40, 80, 100] {
                let mut out = Vec::new();
                ui.write_report(&mut out, &report, width).unwrap();
                let text = String::from_utf8(out).unwrap();
                assert!(text
                    .lines()
                    .all(|line| console::measure_text_width(line) <= width));
                let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
                for expected in [
                    lang.t("Device check"),
                    lang.t("{gb} GB free").replace("{gb}", "9.0"),
                    lang.t("Battery: {percent}%").replace("{percent}", "20"),
                    lang.t("Low battery. Connect power before making changes."),
                    lang.t("More information"),
                    format!(
                        "! {}: 0 · ✓ {}: 0 · ? {}: 0",
                        lang.t("Recommended fixes"),
                        lang.t("Protected"),
                        lang.t("Needs your choice")
                    ),
                ] {
                    assert!(flat.contains(&expected), "{}: {expected}", lang.code());
                }
                assert!(!flat.contains("private technical evidence"));
                assert!(!flat.contains(&lang.t("Readiness evidence")));
            }
        }
    }

    #[test]
    fn readiness_notices_preserve_unknowns_and_never_promise_restart_or_repairs() {
        use secblitz::model::{PowerReadiness, Probe, Readiness, VolumeReadiness};
        let ui = Ui::new(Lang::En, true, false);
        for percent in [0, 20, 21, 100, 255] {
            let report = Report {
                readiness: Some(Readiness {
                    system_volume: Probe::Known(VolumeReadiness {
                        available_bytes: u64::MAX,
                        read_only: true,
                    }),
                    power: Probe::Known(PowerReadiness {
                        ac_connected: None,
                        battery_present: Some(true),
                        battery_percent: Some(percent),
                    }),
                    windows_update_reboot: Probe::Known(true),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let mut out = Vec::new();
            ui.write_readiness(&mut out, &report, 1000).unwrap();
            let text = String::from_utf8(out).unwrap();
            assert!(text.contains("Disk is read-only. Fixes will wait."));
            assert!(text.contains("18446744073.7 GB free"));
            assert!(text.contains("Power source unknown"));
            assert_eq!(text.contains("Low battery."), percent <= 20);
            assert_eq!(text.contains("Battery level unknown"), percent > 100);
            assert!(text.contains("restart when ready"));
            assert!(!text.contains("Good to go"));
        }
    }

    #[test]
    fn captured_workers_never_wait_for_an_invisible_prompt() {
        for input in [false, true] {
            for output in [false, true] {
                for error in [false, true] {
                    assert_eq!(
                        interactive_pause_allowed(input, output, error),
                        [input, output, error].iter().all(|terminal| *terminal)
                    );
                }
            }
        }
        // Actual installer topology: console input inherited, both outputs piped.
        assert!(!interactive_pause_allowed(true, false, false));
    }

    #[test]
    fn actual_report_fits_wide_tables_and_narrow_cards() {
        use secblitz::{engine::Outcome, model::Finding};
        let report = Report {
            transaction: Some("private-transaction-id".into()),
            results: vec![
                Outcome {
                    id: "firewall.public.enabled".into(),
                    title: "technical title".into(),
                    status: "attention".into(),
                    detail: "D:(A;;GA;;;WD) MachineGuid RegistryValueKind\x1b[31m".into(),
                    effective: Some(secblitz::model::EffectiveFirewall::Enabled(false)),
                    authority: Some(secblitz::model::Authority::Local),
                },
                Outcome {
                    id: "uac.enabled".into(),
                    title: String::new(),
                    status: "skipped".into(),
                    detail: "Unknown policy authority".into(),
                    ..Default::default()
                },
            ],
            findings: vec![Finding {
                title: "Windows updates".into(),
                status: "info".into(),
                detail: "raw evidence".into(),
            }],
            ..Default::default()
        };
        let ui = Ui::new(Lang::En, true, false);
        for width in [40, 60, 80, 100] {
            let mut out = Vec::new();
            ui.write_report(&mut out, &report, width).unwrap();
            let out = String::from_utf8(out).unwrap();
            assert!(
                out.lines()
                    .all(|line| console::measure_text_width(line) <= width),
                "{out}"
            );
            assert!(out.contains("1. Public"));
            assert!(!out.contains("2."));
            for raw in [
                "MachineGuid",
                "RegistryValueKind",
                "D:(A",
                "private-transaction-id",
                "raw evidence",
            ] {
                assert!(!out.contains(raw));
            }
            if width > 60 {
                assert!(out.lines().any(|l| l.matches('│').count() == 4));
            } else {
                assert!(out.contains("Protection:"));
            }
        }
        let before = serde_json::to_value(&report).unwrap();
        let mut out = Vec::new();
        ui.write_report_details(&mut out, &report).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("private-transaction-id"));
        assert!(out.contains("D:(A;;GA;;;WD) MachineGuid RegistryValueKind"));
        assert!(!out.contains('\x1b'));
        assert_eq!(before, serde_json::to_value(&report).unwrap());
    }

    #[test]
    fn unicode_cells_wrap_and_remove_terminal_injection() {
        let ui = Ui::new(Lang::En, true, false);
        for width in [40, 80, 100] {
            let mut out = Vec::new();
            ui.write_table(
                &mut out,
                &[(
                    "保护 Proteção e\u{301} 🛡️".repeat(8),
                    "\x1b]0;spoof\x07\r\u{202e}选择".into(),
                    "家族 👨‍👩‍👧‍👦 - français".repeat(8),
                )],
                width,
            )
            .unwrap();
            let out = String::from_utf8(out).unwrap();
            assert!(
                out.lines().all(|l| console::measure_text_width(l) <= width),
                "{out}"
            );
            assert!(!out.contains(['\x1b', '\x07', '\r', '\u{202e}']));
            assert!(out.contains("保护"));
            assert!(out.contains("Proteção"));
            let widths: Vec<_> = out
                .lines()
                .filter(|l| l.starts_with('│'))
                .map(console::measure_text_width)
                .collect();
            assert!(widths.iter().all(|w| *w == width));
        }
    }

    #[test]
    fn error_context_is_translated_but_native_exception_is_preserved() {
        let error = anyhow::Error::new(io::Error::other("Open journal: native evidence\x1b"))
            .context("Password output failed");
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let detail = error_details(lang, &error);
            assert!(detail.starts_with(&lang.t("Password output failed")));
            assert!(detail.contains("Open journal: native evidence "));
            assert!(!detail.contains('\x1b'));
        }
    }

    #[test]
    fn all_service_states_use_localized_presentation() {
        use secblitz::service::{MonitorState, StatusDetails};
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            for (state, key) in [
                (MonitorState::NotInstalled, "not installed"),
                (MonitorState::Stopped, "Stopped"),
                (MonitorState::Running, "Running"),
                (MonitorState::StartPending, "StartPending"),
                (MonitorState::StopPending, "StopPending"),
                (MonitorState::ContinuePending, "ContinuePending"),
                (MonitorState::PausePending, "PausePending"),
                (MonitorState::Paused, "Paused"),
            ] {
                let mut out = Vec::new();
                Ui::new(lang, true, false)
                    .write_service_status(
                        &mut out,
                        &StatusDetails {
                            state,
                            win32_exit_code: Some(1066),
                            service_exit_code: None,
                            checkpoint: 7,
                            wait_hint_ms: 12000,
                        },
                    )
                    .unwrap();
                let out = String::from_utf8(out).unwrap();
                assert!(out.contains(&lang.t(key)));
                assert!(!out.contains("Some(") && !out.contains("None"));
                if state != MonitorState::NotInstalled {
                    assert!(out.contains("Win32=1066"));
                    assert!(out.contains(&lang.t("Not applicable")));
                    assert!(out.contains(&lang.t("Wait hint (ms)")));
                }
            }
        }
    }

    #[test]
    fn password_encoding_has_at_least_24_unbiased_characters() {
        const { assert!(PASSWORD_LENGTH >= 24) };
        let mut counts = std::collections::HashMap::new();
        for byte in 0..=u8::MAX {
            let encoded = encode_password(&[byte; PASSWORD_LENGTH]);
            assert_eq!(encoded.len(), PASSWORD_LENGTH);
            assert!(encoded
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(c)));
            *counts.entry(encoded[0]).or_insert(0) += 1;
        }
        assert_eq!(counts.len(), 64);
        assert!(counts.values().all(|count| *count == 4));
    }

    #[test]
    fn scan_checklist_accumulates_finished_checks_and_skips_pending() {
        use indicatif::{ProgressBar, ProgressDrawTarget};
        // Build a Progress with guided=true without needing a screen surface.
        // screen_progress() will be a no-op (no surface), but item accumulation
        // happens before that call so we can verify it directly.
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::hidden());
        let p = Progress {
            bar,
            lang: crate::i18n::Lang::En,
            lines: false,
            guided: true,
            phase: "Checking".into(),
            items: std::cell::RefCell::new(Vec::new()),
        };
        // "pending" is transient — must NOT be added to the list.
        p.update("defender-av", "pending");
        assert!(p.checklist_items().is_empty(), "pending must not accumulate");
        // Finished statuses DO get added with the right prefix.
        p.update("defender-av", "compliant");
        p.update("bitlocker-os", "attention");
        p.update("updates", "error");
        let items = p.checklist_items();
        assert_eq!(items.len(), 3);
        assert!(items[0].starts_with("✓ "), "compliant → Healthy prefix");
        assert!(items[1].starts_with("! "), "attention → Review prefix");
        assert!(items[2].starts_with("? "), "error → Unknown prefix");
    }
    #[test]
    fn terminal_rows_sanitize_all_untrusted_fields() {
        let attack = "\x1b]0;spoof\x07\r\n\u{009b}\u{202e}\u{2066}\u{200f}\u{061c}";
        let mut output = Vec::new();
        Ui::new(Lang::En, true, false)
            .row(&mut output, attack, attack, attack)
            .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains([
            '\x1b', '\x07', '\r', '\u{009b}', '\u{202e}', '\u{2066}', '\u{200f}', '\u{061c}'
        ]));
        assert_eq!(output.lines().count(), 2);
        assert_eq!(safe("Proteção - français"), "Proteção - français");
    }

    #[test]
    fn advice_glyph_maps_statuses_to_correct_glyphs() {
        use advice::{for_control, for_finding};
        // Protected statuses → ✓
        assert_eq!(advice_glyph(&for_control("uac.enabled", "compliant", "")), '✓');
        assert_eq!(advice_glyph(&for_control("uac.enabled", "applied", "")), '✓');
        // Can fix → !
        assert_eq!(advice_glyph(&for_control("uac.enabled", "attention", "")), '!');
        // Restart needed → ↻
        assert_eq!(
            advice_glyph(&for_control(
                "uac.enabled",
                "applied",
                "Preference applied; restart required"
            )),
            '↻'
        );
        // Couldn't check → ?
        assert_eq!(advice_glyph(&for_control("uac.enabled", "unknown", "")), '?');
        // For your information → •
        assert_eq!(
            advice_glyph(&for_finding("Windows updates", "info", "")),
            '•'
        );
    }

    #[test]
    fn guided_report_text_groups_items_with_headings_and_glyphs() {
        use secblitz::{engine::Outcome, model::Finding};
        let report = Report {
            results: vec![
                Outcome {
                    id: "uac.enabled".into(),
                    status: "attention".into(),
                    ..Default::default()
                },
                Outcome {
                    id: "uac.consent".into(),
                    status: "compliant".into(),
                    ..Default::default()
                },
            ],
            findings: vec![Finding {
                title: "Windows updates".into(),
                status: "info".into(),
                detail: String::new(),
            }],
            ..Default::default()
        };
        let ui = Ui::new(Lang::En, true, false);
        let text = ui.guided_report_text(&report);
        // Group headings
        assert!(text.contains("▸ Recommended fixes (1)"), "{text}");
        assert!(text.contains("▸ Protected (1)"), "{text}");
        assert!(text.contains("▸ More information (1)"), "{text}");
        assert!(!text.contains("▸ Needs your choice"), "{text}");
        // Numbered recommendation with glyph and em-dash
        assert!(text.contains("! 1. Permission prompts"), "{text}");
        assert!(!text.contains("2."), "{text}");
        // Impact line with · prefix
        assert!(
            text.contains("  \u{b7} Risk: Apps silently making system-wide changes without asking you"),
            "{text}"
        );
        // Protected item has ✓ glyph
        assert!(text.contains("✓ Administrator approval"), "{text}");
        // Groups appear in correct order: Recommended → Protected → More information
        let rec = text.find("▸ Recommended fixes").unwrap();
        let prot = text.find("▸ Protected").unwrap();
        let info = text.find("▸ More information").unwrap();
        assert!(rec < prot && prot < info, "{text}");
        // No ANSI escape codes (colors disabled in tests)
        assert!(!text.contains('\x1b'), "{text}");
    }

    #[test]
    fn cli_report_uses_rounded_borders_and_no_escape_codes_when_colors_disabled() {
        use secblitz::engine::Outcome;
        let report = Report {
            results: vec![
                Outcome {
                    id: "uac.enabled".into(),
                    status: "attention".into(),
                    ..Default::default()
                },
                Outcome {
                    id: "uac.consent".into(),
                    status: "compliant".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let ui = Ui::new(Lang::En, true, false);
        for width in [40, 80, 100] {
            let mut out = Vec::new();
            ui.write_report(&mut out, &report, width).unwrap();
            let text = String::from_utf8(out).unwrap();
            // Rounded corners used everywhere
            assert!(text.contains('╭'), "width={width}: {text}");
            assert!(text.contains('╯'), "width={width}: {text}");
            assert!(!text.contains('┌'), "width={width}: {text}");
            assert!(!text.contains('┘'), "width={width}: {text}");
            // No ANSI escape codes when colors are disabled
            assert!(!text.contains('\x1b'), "width={width}: {text}");
            // Group headings include icons
            assert!(text.contains("! Recommended fixes"), "width={width}: {text}");
            assert!(text.contains("✓ Protected"), "width={width}: {text}");
        }
    }

    #[test]
    fn cli_report_table_and_cards_fit_all_six_languages() {
        use secblitz::engine::Outcome;
        let report = Report {
            results: vec![
                Outcome {
                    id: "uac.enabled".into(),
                    status: "attention".into(),
                    ..Default::default()
                },
                Outcome {
                    id: "uac.consent".into(),
                    status: "compliant".into(),
                    ..Default::default()
                },
                Outcome {
                    id: "uac.enabled".into(),
                    status: "unknown".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let ui = Ui::new(lang, true, false);
            for width in [40, 80, 100] {
                let mut out = Vec::new();
                ui.write_report(&mut out, &report, width).unwrap();
                let text = String::from_utf8(out).unwrap();
                assert!(
                    text.lines()
                        .all(|line| console::measure_text_width(line) <= width),
                    "lang={} width={width}: overflow",
                    lang.code()
                );
                assert!(!text.contains('\x1b'), "lang={} width={width}: ANSI", lang.code());
            }
        }
    }
}

