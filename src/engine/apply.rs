//! Applying controls: preflight, journaled writes and the final gate.

use super::catalog::{
    apply_eligible, firewall_control, firewall_protected, permission_control, scope, target_for,
};
use super::journal::{Record, Transaction};
use super::{Engine, Report, MAX_TRANSACTIONS};
use anyhow::{ensure, Context, Result};
use std::collections::HashSet;

impl Engine {
    pub fn apply(&mut self, callback: impl FnMut(&str, &str)) -> Result<Report> {
        self.apply_impl(None, callback)
    }

    pub fn apply_selected(
        &mut self,
        ids: &[String],
        callback: impl FnMut(&str, &str),
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

    pub(super) fn apply_impl(
        &mut self,
        selected: Option<&HashSet<&str>>,
        mut callback: impl FnMut(&str, &str),
    ) -> Result<Report> {
        let _lock = self.lock()?;
        self.mutation_interlocks(&_lock)?;
        let transactions = self.load()?;
        let controls: Vec<_> = self
            .controls
            .iter()
            .filter(|c| selected.is_none_or(|ids| ids.contains(c.id.as_str())))
            .cloned()
            .collect();
        let mut report = Report {
            transaction: None,
            results: Vec::new(),
            findings: Vec::new(),
            readiness: None,
        };
        if let Some(tx) = transactions
            .iter()
            .rev()
            .find(|t| !t.reverted && (selected.is_none() || t.incomplete()))
        {
            // Never refresh before images, even if an earlier apply only partly
            // completed. Recovery is explicitly revert, not an implicit write.
            report.transaction = Some(tx.name.clone());
            for c in controls {
                let result = if tx.incomplete() {
                    Self::outcome(
                        &c,
                        "pending",
                        "Revert the active transaction before applying again",
                    )
                } else {
                    match self.observe(&c.id).and_then(|o| {
                        let entry = tx.entries.iter().find(|e| e.id == c.id);
                        let expected = if entry.is_none() && firewall_control(&c.id) {
                            firewall_protected(&o)?.then(|| o.value.clone())
                        } else if entry.is_none() && permission_control(&c.id) && !o.eligible {
                            None
                        } else {
                            Some(target_for(&c.id, entry.map_or(&o.value, |e| &e.before))?)
                        };
                        Ok((expected, o))
                    }) {
                        Ok((expected, o))
                            if expected.as_ref()
                                == Some(&scope(
                                    &c.id,
                                    &o.value,
                                    tx.entries.iter().find(|e| e.id == c.id).map(|e| &e.before),
                                )) =>
                        {
                            Self::observed_outcome(
                                &c,
                                "unchanged",
                                "Target preference already present; original before image retained",
                                &o,
                            )
                        }
                        Ok((_, o)) if tx.entries.iter().any(|e| e.id == c.id) => {
                            Self::observed_outcome(
                                &c,
                                "conflict",
                                "Preference drifted; original before image retained",
                                &o,
                            )
                        }
                        Ok((_, o)) => Self::observed_outcome(
                            &c,
                            "skipped",
                            "Revert the active transaction before starting another apply",
                            &o,
                        ),
                        Err(e) => Self::outcome(&c, "error", format!("{e:#}")),
                    }
                };
                callback(&result.id, &result.status);
                report.results.push(result);
            }
            report.findings = self.findings();
            report.findings.push(Self::journal_finding(tx));
            return Ok(report);
        }
        let mut owned = Vec::new();
        for c in &controls {
            if let Some(entry) = transactions
                .iter()
                .filter(|t| !t.reverted)
                .flat_map(|t| &t.entries)
                .find(|e| e.id == c.id)
            {
                let expected = target_for(&c.id, &entry.before)?;
                let result = match self.observe(&c.id) {
                    Ok(o) if scope(&c.id, &o.value, Some(&entry.before)) == expected => {
                        Self::observed_outcome(
                            c,
                            "unchanged",
                            "Target preference already present; original before image retained",
                            &o,
                        )
                    }
                    Ok(o) => Self::observed_outcome(
                        c,
                        "conflict",
                        "Preference drifted; original before image retained",
                        &o,
                    ),
                    Err(e) => Self::outcome(c, "error", format!("{e:#}")),
                };
                owned.push(result);
            }
        }
        if owned.iter().any(|r| r.status != "unchanged") {
            for c in &controls {
                let result = if let Some(i) = owned.iter().position(|r| r.id == c.id) {
                    owned.remove(i)
                } else {
                    Self::outcome(
                        c,
                        "skipped",
                        "Selected batch blocked by an owned control conflict or probe failure",
                    )
                };
                callback(&result.id, &result.status);
                report.results.push(result);
            }
            report.findings = self.findings();
            return Ok(report);
        }
        let readiness = self.readiness(&mut callback);
        let blocked = readiness.blocks_repairs();
        report.readiness = Some(readiness);
        if blocked {
            for c in &controls {
                let result = if let Some(i) = owned.iter().position(|r| r.id == c.id) {
                    owned.remove(i)
                } else {
                    Self::outcome(c, "skipped", "Repair readiness blocks new changes")
                };
                callback(&result.id, &result.status);
                report.results.push(result);
            }
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
        let mut tx: Option<Transaction> = None;
        for c in controls {
            if let Some(i) = owned.iter().position(|r| r.id == c.id) {
                let result = owned.remove(i);
                callback(&result.id, &result.status);
                report.results.push(result);
                continue;
            }
            // Before Prepare there is no uncertain write to recover. A runtime
            // unsupported control must not strand earlier selected successes.
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
                Err(e) if selected.is_some() => {
                    let result = Self::outcome(&c, "error", format!("{e:#}"));
                    callback(&result.id, &result.status);
                    report.results.push(result);
                    continue;
                }
                Err(e) => return Err(e),
            };
            let Some(expected) = expected else {
                let result =
                    Self::observed_outcome(&c, "skipped", &observation.reason, &observation);
                callback(&result.id, &result.status);
                report.results.push(result);
                continue;
            };
            let result = if firewall_control(&c.id) && !observation.eligible {
                Self::observed_outcome(&c, "skipped", &observation.reason, &observation)
            } else if protected || observation.value == expected {
                Self::observed_outcome(
                    &c,
                    "unchanged",
                    "Target preference already present",
                    &observation,
                )
            } else if !apply_eligible(&c.id, &observation) {
                Self::observed_outcome(
                    &c,
                    "skipped",
                    if observation.eligible {
                        "Preserving absent or already-safe machine preference".into()
                    } else {
                        observation.reason.clone()
                    },
                    &observation,
                )
            } else {
                if tx.is_none() {
                    tx = Some(self.create(sequence)?);
                }
                let tx = tx.as_mut().unwrap();
                report.transaction = Some(tx.name.clone());
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
                if let Err(e) = self.backend.write(&c.id, &expected) {
                    callback(&c.id, "error");
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
                    readback.value == expected,
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
                Self::observed_outcome(
                    &c,
                    "applied",
                    if c.reboot {
                        "Preference applied; restart required"
                    } else {
                        "Preference applied"
                    },
                    &readback,
                )
            };
            callback(&result.id, &result.status);
            report.results.push(result);
        }
        if let Some(tx) = tx.as_mut() {
            self.append(tx, Record::Sealed)?;
        }
        report.findings = self.findings();
        Ok(report)
    }
}
