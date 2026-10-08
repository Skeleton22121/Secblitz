//! Applying controls: preflight, journaled writes and the final gate.

use super::catalog::{
    apply_eligible, firewall_control, firewall_protected, permission_control, scope, target_for,
};
use super::journal::{Record, State, Transaction};
use super::{Engine, Outcome, Progress, ProgressStep, Report, MAX_TRANSACTIONS};
use crate::model::{CheckStatus, Control, Observation};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::collections::HashSet;

/// Controls already in the journal: settled results, and chosen ones to write again.
struct Owned {
    done: Vec<Outcome>,
    again: Vec<(String, Value)>,
}

/// A control that records each batch of picked items separately, with picked items that no earlier batch holds.
fn adds_batch(id: &str, observed: &Value, recorded: &Value) -> bool {
    crate::hardening::spec(id)
        .is_some_and(|s| s.adds_batches() && s.has_unrecorded_unsafe(observed, recorded))
}

enum Preflight {
    Resolved(Outcome),
    Write {
        observation: Observation,
        expected: Value,
    },
}

impl Engine {
    pub fn apply(&mut self, callback: impl FnMut(Progress<'_>)) -> Result<Report> {
        self.apply_impl(None, callback)
    }

    pub fn apply_selected(
        &mut self,
        ids: &[String],
        callback: impl FnMut(Progress<'_>),
    ) -> Result<Report> {
        ensure!(!ids.is_empty(), "Select at least one control");
        let mut selected = HashSet::new();
        for id in ids {
            ensure!(
                selected.insert(id.as_str()),
                "Duplicate selected control: {id}"
            );
            self.control(id)
                .with_context(|| format!("Unknown selected control: {id}"))?;
        }
        self.apply_impl(Some(&selected), callback)
    }

    /// Read-only: answers whether a fix (or an undo) could start right now, with the same gates the real run uses and no write to the journal or the system.
    pub fn can_change(&mut self, undo: bool) -> Result<()> {
        let lock = self.lock()?;
        self.mutation_interlocks(&lock)?;
        let transactions = self.load()?;
        self.durable(&transactions)?;
        if undo {
            return Ok(());
        }
        ensure!(
            !transactions.iter().any(|t| !t.reverted && t.incomplete()),
            "Revert the active transaction before applying again"
        );
        let readiness = self.readiness(&mut |_: Progress<'_>| {});
        ensure!(
            !readiness.blocks_repairs(),
            "Repair readiness blocks new changes"
        );
        Ok(())
    }

    fn apply_impl(
        &mut self,
        selected: Option<&HashSet<&str>>,
        mut callback: impl FnMut(Progress<'_>),
    ) -> Result<Report> {
        let _lock = self.lock()?;
        self.mutation_interlocks(&_lock)?;
        let mut transactions = self.load()?;
        self.close_finished(&mut transactions)?;
        let controls: Vec<_> = self
            .controls
            .iter()
            .filter(|c| selected.is_none_or(|ids| ids.contains(c.id.as_str())))
            .cloned()
            .collect();
        let mut report = Report::default();
        if let Some(tx) = transactions
            .iter()
            .rev()
            .find(|t| !t.reverted && (selected.is_none() || t.incomplete()))
        {
            self.report_blocked_by_active(tx, &controls, &mut report, &mut callback);
            return Ok(report);
        }
        self.settle_restoring(&mut transactions, selected)?;
        self.close_finished(&mut transactions)?;
        let mut owned = self.observe_owned(&transactions, &controls, selected.is_some())?;
        if owned
            .done
            .iter()
            .any(|r| r.status != CheckStatus::Unchanged)
        {
            report.skip_all(
                &controls,
                &mut owned.done,
                "Selected batch blocked by an owned control conflict or probe failure",
                &mut callback,
            );
            report.findings = self.findings();
            return Ok(report);
        }
        let readiness = self.readiness(&mut callback);
        let blocked = readiness.blocks_repairs();
        report.readiness = Some(readiness);
        if blocked {
            report.skip_all(
                &controls,
                &mut owned.done,
                "Repair readiness blocks new changes",
                &mut callback,
            );
            report.findings = self.findings();
            return Ok(report);
        }
        self.durable(&transactions)?;
        ensure!(
            transactions.len() < MAX_TRANSACTIONS,
            "Too many journal transactions"
        );
        let sequence = transactions
            .last()
            .map_or(Some(1), |t| t.sequence.checked_add(1))
            .context("Transaction sequence exhausted")?;
        self.apply_controls(
            controls,
            owned,
            selected.is_some(),
            sequence,
            &mut report,
            &mut callback,
        )?;
        report.findings = self.findings();
        Ok(report)
    }

    /// Never refresh before images, even if an earlier apply only partly
    /// completed. Recovery is explicitly revert, not an implicit write.
    fn report_blocked_by_active(
        &mut self,
        tx: &Transaction,
        controls: &[Control],
        report: &mut Report,
        callback: &mut impl FnMut(Progress<'_>),
    ) {
        report.transaction = Some(tx.name.clone());
        for c in controls {
            let result = if tx.incomplete() {
                Self::outcome(
                    c,
                    CheckStatus::Pending,
                    "Revert the active transaction before applying again",
                )
            } else {
                self.assess_under_active(c, tx)
            };
            report.push(result, callback);
        }
        report.findings = self.findings();
        report.findings.push(Self::journal_finding(tx));
    }

    fn assess_under_active(&mut self, c: &Control, tx: &Transaction) -> Outcome {
        let entry = tx
            .entries
            .iter()
            .find(|e| e.id == c.id && e.state != State::Restored);
        let observed = self.observe(&c.id).and_then(|o| {
            let expected = if entry.is_none() && firewall_control(&c.id) {
                firewall_protected(&o)?.then(|| o.value.clone())
            } else if entry.is_none() && permission_control(&c.id) && !o.eligible {
                None
            } else {
                Some(target_for(&c.id, entry.map_or(&o.value, |e| &e.before))?)
            };
            Ok((expected, o))
        });
        match observed {
            Ok((expected, o))
                if expected.as_ref() == Some(&scope(&c.id, &o.value, entry.map(|e| &e.before))) =>
            {
                Self::observed_outcome(
                    c,
                    CheckStatus::Unchanged,
                    "Target preference already present; original before image retained",
                    &o,
                )
            }
            Ok((_, o)) if entry.is_some() => Self::observed_outcome(
                c,
                CheckStatus::Conflict,
                "Preference drifted; original before image retained",
                &o,
            ),
            Ok((_, o)) => Self::observed_outcome(
                c,
                CheckStatus::Skipped,
                "Revert the active transaction before starting another apply",
                &o,
            ),
            Err(e) => Self::outcome(c, CheckStatus::Error, format!("{e:#}")),
        }
    }

    /// Controls whose original is already journaled keep that original. A chosen
    /// one that was switched back to an unsafe value gets its target written
    /// again; every other drift is only reported.
    fn observe_owned(
        &mut self,
        transactions: &[Transaction],
        controls: &[Control],
        selected: bool,
    ) -> Result<Owned> {
        let mut done = Vec::new();
        let mut again = Vec::new();
        for c in controls {
            let Some(entry) = transactions
                .iter()
                .filter(|t| !t.reverted)
                .flat_map(|t| &t.entries)
                .find(|e| e.id == c.id && e.state != State::Restored)
            else {
                continue;
            };
            let expected = target_for(&c.id, &entry.before)?;
            let result = match self.observe(&c.id) {
                Ok(o) if adds_batch(&c.id, &o.value, &entry.before) => continue,
                Ok(o) if scope(&c.id, &o.value, Some(&entry.before)) == expected => {
                    Self::observed_outcome(
                        c,
                        CheckStatus::Unchanged,
                        "Target preference already present; original before image retained",
                        &o,
                    )
                }
                Ok(o)
                    if selected
                        && !firewall_control(&c.id)
                        && !permission_control(&c.id)
                        && apply_eligible(&c.id, &o) =>
                {
                    again.push((c.id.clone(), expected));
                    continue;
                }
                Ok(o) => Self::observed_outcome(
                    c,
                    CheckStatus::Conflict,
                    "Preference drifted; original before image retained",
                    &o,
                ),
                Err(e) => Self::outcome(c, CheckStatus::Error, format!("{e:#}")),
            };
            done.push(result);
        }
        Ok(Owned { done, again })
    }

    fn apply_controls(
        &mut self,
        controls: Vec<Control>,
        mut owned: Owned,
        selected: bool,
        sequence: u64,
        report: &mut Report,
        callback: &mut impl FnMut(Progress<'_>),
    ) -> Result<()> {
        let mut tx: Option<Transaction> = None;
        for c in controls {
            if let Some(i) = owned.done.iter().position(|r| r.id == c.id) {
                report.push(owned.done.remove(i), callback);
                continue;
            }
            if let Some((_, expected)) = owned.again.iter().find(|(id, _)| *id == c.id) {
                let result = self.write_again(&c, expected);
                report.push(result, callback);
                continue;
            }
            let result = match self.preflight(&c, selected)? {
                Preflight::Resolved(result) => result,
                Preflight::Write {
                    observation,
                    expected,
                } => {
                    if tx.is_none() {
                        tx = Some(self.create(sequence)?);
                    }
                    let tx = tx.as_mut().unwrap();
                    report.transaction = Some(tx.name.clone());
                    self.write_managed(&c, tx, &observation, &expected, callback)?
                }
            };
            report.push(result, callback);
        }
        if let Some(tx) = tx.as_mut() {
            self.append(tx, Record::Sealed)?;
        }
        Ok(())
    }

    /// Before Prepare there is no uncertain write to recover. A runtime
    /// unsupported control must not strand earlier selected successes.
    fn preflight(&mut self, c: &Control, selected: bool) -> Result<Preflight> {
        if crate::hardening::spec(&c.id).is_some_and(|s| s.needs_choice())
            && self.chosen(&c.id).is_none_or(<[String]>::is_empty)
        {
            return Ok(Preflight::Resolved(Self::outcome(
                c,
                CheckStatus::Skipped,
                "Nothing was picked",
            )));
        }
        let observed = self.observe(&c.id).and_then(|o| {
            let protected = firewall_control(&c.id) && firewall_protected(&o)?;
            let expected = if permission_control(&c.id) && !o.eligible {
                None
            } else {
                Some(target_for(&c.id, &o.value)?)
            };
            Ok((o, expected, protected))
        });
        let (observation, expected, protected) = match observed {
            Ok(value) => value,
            Err(e) if selected => {
                return Ok(Preflight::Resolved(Self::outcome(
                    c,
                    CheckStatus::Error,
                    format!("{e:#}"),
                )))
            }
            Err(e) => return Err(e),
        };
        let Some(expected) = expected else {
            return Ok(Preflight::Resolved(Self::observed_outcome(
                c,
                CheckStatus::Skipped,
                &observation.reason,
                &observation,
            )));
        };
        let resolved = if firewall_control(&c.id) && !observation.eligible {
            Self::observed_outcome(c, CheckStatus::Skipped, &observation.reason, &observation)
        } else if protected || observation.value == expected {
            Self::observed_outcome(
                c,
                CheckStatus::Unchanged,
                "Target preference already present",
                &observation,
            )
        } else if !apply_eligible(&c.id, &observation) {
            Self::observed_outcome(
                c,
                CheckStatus::Skipped,
                if observation.eligible {
                    "Preserving absent or already-safe machine preference".into()
                } else {
                    observation.reason.clone()
                },
                &observation,
            )
        } else {
            return Ok(Preflight::Write {
                observation,
                expected,
            });
        };
        Ok(Preflight::Resolved(resolved))
    }

    /// The journal already holds this control's original, so no new record is
    /// needed: whatever happens here, undo still returns to that original.
    fn write_again(&mut self, c: &Control, expected: &Value) -> Outcome {
        if let Err(e) = self.backend.write(&c.id, expected) {
            return Self::outcome(c, CheckStatus::Error, format!("{e:#}"));
        }
        match self.observe(&c.id) {
            Ok(readback) if readback.value == *expected => Self::observed_outcome(
                c,
                CheckStatus::Applied,
                if c.reboot {
                    "Preference applied; restart required"
                } else {
                    "Preference applied"
                },
                &readback,
            ),
            Ok(readback) => Self::observed_outcome(
                c,
                CheckStatus::Conflict,
                "Preference drifted; original before image retained",
                &readback,
            ),
            Err(e) => Self::outcome(c, CheckStatus::Error, format!("{e:#}")),
        }
    }

    fn write_managed(
        &mut self,
        c: &Control,
        tx: &mut Transaction,
        observation: &Observation,
        expected: &Value,
        callback: &mut impl FnMut(Progress<'_>),
    ) -> Result<Outcome> {
        // Intent is durable before the final eligibility/read gate. A
        // failure or race at that gate is still safely recoverable.
        self.append(
            tx,
            Record::Prepare {
                id: c.id.clone(),
                before: observation.value.clone(),
            },
        )?;
        let fresh = self.observe(&c.id)?;
        ensure!(
            apply_eligible(&c.id, &fresh) && fresh.value == observation.value,
            "{} changed or became ineligible after prepare; revert transaction {}",
            c.id,
            tx.name
        );
        if let Err(e) = self.backend.write(&c.id, expected) {
            callback(Progress::new(
                &c.id,
                ProgressStep::Result(CheckStatus::Error),
            ));
            return Err(e.context(format!(
                "Apply {} has unknown outcome; pending transaction {} retained",
                c.id, tx.name
            )));
        }
        // Verify against the durable original's target, not a target
        // recomputed from readback (which would accept safe ACL drift).
        // Failure leaves Prepare pending even if the backend returned Ok.
        let readback = self.observe(&c.id)?;
        ensure!(
            readback.value == *expected,
            "Apply {} readback differs from recorded target; pending transaction {} retained",
            c.id,
            tx.name
        );
        if firewall_control(&c.id) {
            ensure!(
                firewall_protected(&readback)?,
                "Apply {} effective protection is unverified; pending transaction {} retained",
                c.id,
                tx.name
            );
        }
        self.append(tx, Record::Applied { id: c.id.clone() })?;
        Ok(Self::observed_outcome(
            c,
            CheckStatus::Applied,
            if c.reboot {
                "Preference applied; restart required"
            } else {
                "Preference applied"
            },
            &readback,
        ))
    }
}
