//! Read-only assessment: audit, history and findings.

use super::catalog::assessment_status;
use super::journal::{State, Transaction};
use super::{Engine, Outcome, Progress, ProgressStep, Report, READ_BATCH};
use crate::model::{CheckStatus, Control, Finding, Observation, Readiness};
use anyhow::Result;

impl Engine {
    pub(super) fn outcome(c: &Control, status: CheckStatus, detail: impl Into<String>) -> Outcome {
        Outcome {
            id: c.id.clone(),
            title: c.title.clone(),
            status,
            detail: detail.into(),
            ..Outcome::default()
        }
    }

    pub(super) fn observed_outcome(
        c: &Control,
        status: CheckStatus,
        detail: impl Into<String>,
        observation: &Observation,
    ) -> Outcome {
        Outcome {
            effective: observation.effective,
            authority: observation.authority,
            items: crate::model::ItemLabel::clean(&observation.labels),
            ..Self::outcome(c, status, detail)
        }
    }

    pub(super) fn readiness(&mut self, callback: &mut impl FnMut(Progress<'_>)) -> Readiness {
        callback(Progress::new("readiness", ProgressStep::Pending));
        let readiness = self.backend.readiness();
        callback(Progress::new("readiness", ProgressStep::Complete));
        readiness
    }

    pub(super) fn findings(&mut self) -> Vec<Finding> {
        // Findings are assessment, not mutation acknowledgment. A failed final
        // transport/probe must not discard already durable operation outcomes.
        let mut found = self.backend.findings().unwrap_or_else(|e| {
            vec![Finding {
                title: "Assessment unavailable".into(),
                status: CheckStatus::Unknown,
                detail: format!("Findings could not be collected: {e:#}"),
            }]
        });
        self.keep_own_core_protection_findings(&mut found);
        found
    }

    /// Memory integrity and stack protection "not running" notes only describe a change Secblitz made: it must be in the journal, not yet undone, and the PC must have restarted since. The note says whether undoing it is the next undo. Anything unreadable drops the note.
    pub(super) fn keep_own_core_protection_findings(&self, found: &mut Vec<Finding>) {
        if !found
            .iter()
            .any(|f| crate::vbs::finding_control(&f.title).is_some())
        {
            return;
        }
        let transactions = self.load().unwrap_or_default();
        let newest = transactions.iter().rev().find(|t| !t.reverted);
        found.retain_mut(|f| {
            let Some(id) = crate::vbs::finding_control(&f.title) else {
                return true;
            };
            let Some(boot) = crate::vbs::boot_from_detail(&f.detail) else {
                return false;
            };
            let Some(tx) = transactions.iter().rev().find(|t| {
                !t.reverted
                    && t.sealed
                    && !t.reverting
                    && t.entries
                        .iter()
                        .any(|e| e.id == id && e.state == State::Applied)
            }) else {
                return false;
            };
            let written = tx
                .file
                .as_ref()
                .and_then(|file| file.metadata().ok())
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|age| i64::try_from(age.as_secs()).ok());
            if !crate::vbs::restarted_since(written, Some(boot)) {
                return false;
            }
            let own_batch = tx.entries.iter().all(|e| e.id == id);
            if own_batch && newest.is_some_and(|n| n.sequence == tx.sequence) {
                f.detail = format!("{}. {}", crate::vbs::UNDO_READY, f.detail);
            }
            true
        });
    }

    pub(super) fn journal_finding(tx: &Transaction) -> Finding {
        let pending = tx.incomplete();
        Finding {
            title: "Journal recovery".into(),
            status: if pending { CheckStatus::Pending } else { CheckStatus::Info },
            detail: if pending {
                format!("Transaction {} has incomplete apply or rollback; use revert to resolve its recorded preferences before applying again.", tx.name)
            } else {
                format!("Transaction {} remains unreverted; use revert to restore its recorded preferences.", tx.name)
            },
        }
    }

    pub fn audit(&mut self) -> Result<Report> {
        self.audit_with_progress(|_| {})
    }

    pub fn audit_with_progress(&mut self, mut callback: impl FnMut(Progress<'_>)) -> Result<Report> {
        let _lock = self.lock()?;
        let transactions = self.load()?;
        let active = transactions.iter().rev().find(|t| !t.reverted);
        let mut report = Report {
            transaction: active.map(|t| t.name.clone()),
            ..Report::default()
        };
        let controls = self.controls.clone();
        for batch in controls.chunks(READ_BATCH) {
            let ids: Vec<&str> = batch.iter().map(|c| c.id.as_str()).collect();
            for (c, observed) in batch.iter().zip(self.observe_many(&ids)) {
                let result = match observed {
                    Ok(o) => match assessment_status(&c.id, &o) {
                        Ok(status) => Self::observed_outcome(c, status, &o.reason, &o),
                        Err(e) => Self::observed_outcome(c, CheckStatus::Error, format!("{e:#}"), &o),
                    },
                    Err(e) => Self::outcome(c, CheckStatus::Error, format!("{e:#}")),
                };
                report.push(result, &mut callback);
            }
        }
        report.readiness = Some(self.readiness(&mut callback));
        callback(Progress::new("findings", ProgressStep::Pending));
        report.findings = self.findings();
        callback(Progress::new("findings", ProgressStep::Complete));
        if let Some(tx) = active {
            report.findings.push(Self::journal_finding(tx));
        }
        Ok(report)
    }

    pub fn history(&mut self) -> Result<Vec<String>> {
        let _lock = self.lock()?;
        Ok(self
            .load()?
            .into_iter()
            .rev()
            .map(|tx| {
                format!(
                    "{} {}",
                    tx.name,
                    if tx.reverted {
                        "reverted"
                    } else if tx.reverting {
                        "reverting"
                    } else if !tx.incomplete() {
                        "applied"
                    } else {
                        "pending"
                    }
                )
            })
            .collect())
    }

    pub fn undoable_changes(&mut self) -> Result<usize> {
        let _lock = self.lock()?;
        let transactions = self.load()?;
        let mut ids = std::collections::BTreeSet::new();
        for tx in transactions.iter().filter(|t| !t.reverted) {
            for e in tx.entries.iter().filter(|e| e.state != State::Restored) {
                ids.insert(e.id.as_str());
            }
        }
        Ok(ids.len())
    }
}
