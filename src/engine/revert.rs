//! Reverting journaled changes, newest batch first.

use super::catalog::{recorded_scope, restore_eligible, scope, target_for};
use super::journal::{Record, State, Transaction};
use super::{Engine, Outcome, Report};
use anyhow::{ensure, Result};

impl Engine {
    pub(super) fn revert_transaction(
        &mut self,
        tx: &mut Transaction,
        report: &mut Report,
        callback: &mut impl FnMut(&str, &str),
    ) -> Result<()> {
        if !tx.reverting {
            self.append(tx, Record::Reverting)?;
            tx.reverting = true;
        }
        for i in (0..tx.entries.len()).rev() {
            if tx.entries[i].state == State::Restored {
                continue;
            }
            let id = tx.entries[i].id.clone();
            let before = tx.entries[i].before.clone();
            let c = self.control(&id)?.clone();
            let expected = target_for(&id, &before)?;
            let observation = self.observe(&id)?;
            let before_eff = recorded_scope(&id, &before, &observation.value);
            let expected_eff = recorded_scope(&id, &expected, &observation.value);
            let seen = scope(&id, &observation.value, Some(&before_eff));
            let result = if seen == before_eff {
                if tx.entries[i].state != State::Restoring {
                    self.append(tx, Record::RestorePending { id: id.clone() })?;
                }
                self.append(tx, Record::Restored { id: id.clone() })?;
                tx.entries[i].state = State::Restored;
                Self::observed_outcome(
                    &c,
                    "unchanged",
                    "Original preference already present",
                    &observation,
                )
            } else if seen != expected_eff {
                Self::observed_outcome(
                    &c,
                    "conflict",
                    "Preference differs from both target and before image; no write performed",
                    &observation,
                )
            } else if !restore_eligible(&c, &observation) {
                Self::observed_outcome(&c, "skipped", &observation.reason, &observation)
            } else {
                if tx.entries[i].state != State::Restoring {
                    self.append(tx, Record::RestorePending { id: id.clone() })?;
                    tx.entries[i].state = State::Restoring;
                }
                let fresh = self.observe(&id)?;
                if scope(&id, &fresh.value, Some(&before_eff)) != expected_eff {
                    Self::observed_outcome(
                        &c,
                        "conflict",
                        "Preference changed immediately before restore; no write performed",
                        &fresh,
                    )
                } else if !restore_eligible(&c, &fresh) {
                    Self::observed_outcome(&c, "skipped", &fresh.reason, &fresh)
                } else {
                    if let Err(e) = self.backend.write(&id, &before_eff) {
                        callback(&id, "error");
                        return Err(e.context(format!(
                            "Restore {id} has unknown outcome; pending transaction {} retained",
                            tx.name
                        )));
                    }
                    let readback = self.observe(&id)?;
                    ensure!(
                        scope(&id, &readback.value, Some(&before_eff)) == before_eff,
                        "Restore {id} readback differs from original; pending transaction {} retained",
                        tx.name
                    );
                    self.append(tx, Record::Restored { id: id.clone() })?;
                    tx.entries[i].state = State::Restored;
                    Self::observed_outcome(
                        &c,
                        "restored",
                        if c.reboot {
                            "Original preference restored; restart required"
                        } else {
                            "Original preference restored"
                        },
                        &readback,
                    )
                }
            };
            callback(&result.id, &result.status);
            report.results.push(result);
        }
        if tx.entries.iter().all(|e| e.state == State::Restored) {
            self.append(tx, Record::Reverted)?;
            tx.reverted = true;
        }
        Ok(())
    }

    pub fn revert(&mut self, mut callback: impl FnMut(&str, &str)) -> Result<Report> {
        let _lock = self.lock()?;
        self.mutation_interlocks(&_lock)?;
        let mut transactions = self.load()?;
        self.durable(&transactions)?;
        let mut report = Report {
            transaction: None,
            results: Vec::new(),
            findings: Vec::new(),
            readiness: None,
        };
        if let Some(tx) = transactions.iter_mut().rev().find(|t| !t.reverted) {
            report.transaction = Some(tx.name.clone());
            self.revert_transaction(tx, &mut report, &mut callback)?;
        }
        report.findings = self.findings();
        if let Some(tx) = transactions.iter().rev().find(|t| !t.reverted) {
            report.findings.push(Self::journal_finding(tx));
        }
        Ok(report)
    }

    pub(super) fn undo_blockers(&mut self, tx: &Transaction) -> Result<Vec<Outcome>> {
        let mut blockers = Vec::new();
        for entry in tx
            .entries
            .iter()
            .rev()
            .filter(|e| e.state != State::Restored)
        {
            let c = self.control(&entry.id)?.clone();
            let expected = target_for(&entry.id, &entry.before)?;
            let observation = self.observe(&entry.id)?;
            let before_eff = recorded_scope(&entry.id, &entry.before, &observation.value);
            let expected_eff = recorded_scope(&entry.id, &expected, &observation.value);
            let seen = scope(&entry.id, &observation.value, Some(&before_eff));
            if seen == before_eff {
                continue;
            }
            blockers.push(if seen != expected_eff {
                Self::observed_outcome(
                    &c,
                    "conflict",
                    "Preference differs from both target and before image; no write performed",
                    &observation,
                )
            } else if !restore_eligible(&c, &observation) {
                Self::observed_outcome(&c, "skipped", &observation.reason, &observation)
            } else {
                continue;
            });
        }
        Ok(blockers)
    }

    /// Newest first. A batch with a conflict stays unreverted and older batches are still processed.
    pub fn revert_all(&mut self, mut callback: impl FnMut(&str, &str)) -> Result<Report> {
        let _lock = self.lock()?;
        self.mutation_interlocks(&_lock)?;
        let mut transactions = self.load()?;
        self.durable(&transactions)?;
        let mut report = Report {
            transaction: None,
            results: Vec::new(),
            findings: Vec::new(),
            readiness: None,
        };
        // Only the newest unreverted batch may be left half undone (`load`
        // rejects an incomplete batch that precedes another active one). So once
        // a newer batch is left over, an older one is only touched if it can be
        // finished completely; otherwise it is reported and left as it was.
        let mut left_over = false;
        for tx in transactions.iter_mut().rev().filter(|t| !t.reverted) {
            if left_over {
                let blockers = self.undo_blockers(tx)?;
                if !blockers.is_empty() {
                    for b in blockers {
                        callback(&b.id, &b.status);
                        report.results.push(b);
                    }
                    continue;
                }
            }
            self.revert_transaction(tx, &mut report, &mut callback)?;
            left_over |= !tx.reverted;
        }
        report.findings = self.findings();
        for tx in transactions.iter().rev().filter(|t| !t.reverted) {
            report.findings.push(Self::journal_finding(tx));
        }
        Ok(report)
    }
}
