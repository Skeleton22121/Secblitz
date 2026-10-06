//! Recovery of torn appends, unpublished snapshots and preserved evidence.

use super::catalog::{permission_control, target};
use super::fsio::{io_boundary, open_file, read_bytes, same_file, sync_directory};
use super::journal::{journal_name, record_bytes, Record, State, Transaction};
use super::{Engine, MAX_EVIDENCE, MAX_LINE, SCHEMA};
use anyhow::{bail, ensure, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};

impl Engine {
    // Only strict canonical prefixes of legal next records qualify. In
    // particular serde's EOF classification alone is insufficient: unknown
    // controls, invalid domains and invalid transitions must not disappear.
    pub(super) fn owned_elsewhere(
        tx: &Transaction,
        id: &str,
        transactions: &[Transaction],
    ) -> bool {
        transactions.iter().any(|other| {
            other.name != tx.name && !other.reverted && other.entries.iter().any(|e| e.id == id)
        })
    }

    pub(super) fn incomplete_tail(
        &self,
        tx: &Transaction,
        tail: &[u8],
        transactions: &[Transaction],
    ) -> Result<bool> {
        if tail.is_empty() || tail.len() > MAX_LINE || tx.reverted {
            return Ok(false);
        }
        if !serde_json::from_slice::<Value>(tail).is_err_and(|e| e.is_eof()) {
            return Ok(false);
        }
        let mut candidates = vec![Record::Sealed, Record::Reverting, Record::Reverted];
        for entry in &tx.entries {
            candidates.push(Record::Applied {
                id: entry.id.clone(),
            });
            candidates.push(Record::RestorePending {
                id: entry.id.clone(),
            });
            candidates.push(Record::Restored {
                id: entry.id.clone(),
            });
        }
        for control in &self.controls {
            if Self::owned_elsewhere(tx, &control.id, transactions) {
                continue;
            }
            // Finite raw preference domains only. An incomplete legacy ACL
            // original cannot be inferred; it requires review. COW staging has
            // an independent committed snapshot and handles ACLs below.
            let values = if permission_control(&control.id)
                || crate::hardening::is_hardening_check_id(&control.id)
            {
                Vec::new()
            } else if target(&control.id)?.is_boolean() {
                vec![json!(true), json!(false)]
            } else if target(&control.id)?.is_string() {
                vec![json!("Allow"), json!("Block"), json!("NotConfigured")]
            } else {
                let mut values = vec![json!({"present":false,"value":null})];
                values.extend(
                    (0..=if control.id == "uac.consent" { 5 } else { 1 })
                        .map(|n| json!({"present":true,"value":n})),
                );
                values
            };
            for before in values {
                candidates.push(Record::Prepare {
                    id: control.id.clone(),
                    before,
                });
            }
        }
        for record in candidates {
            let bytes = record_bytes(&record)?;
            if bytes.starts_with(tail) && tail.len() < bytes.len() - 1 {
                let mut complete = tx.bytes.clone();
                complete.extend(bytes);
                if self.decode(&tx.name, None, complete).is_ok() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub(super) fn validate_staged(
        &self,
        stem: &str,
        bytes: &[u8],
        transactions: &[Transaction],
    ) -> Result<()> {
        let seq = journal_name(stem)?;
        let Some(tx) = transactions.iter().find(|t| t.name == stem) else {
            ensure!(
                transactions.last().is_none_or(|tx| seq > tx.sequence),
                "Staged header is not newest"
            );
            ensure!(
                transactions
                    .iter()
                    .all(|tx| tx.reverted || !tx.incomplete()),
                "Staged header follows incomplete transaction"
            );
            let expected = record_bytes(&Record::Header {
                schema: SCHEMA,
                machine: self.machine.clone(),
                transaction: stem.into(),
                sequence: seq,
            })?;
            ensure!(
                expected.starts_with(bytes),
                "Invalid unpublished journal header or machine mismatch"
            );
            return Ok(());
        };
        ensure!(
            !tx.reverted
                && !transactions
                    .iter()
                    .any(|later| later.sequence > tx.sequence && !later.reverted),
            "Unpublished append is not newest active transaction"
        );
        let prefix_len = bytes.len().min(tx.bytes.len());
        ensure!(
            bytes[..prefix_len] == tx.bytes[..prefix_len],
            "Corrupt committed prefix in unpublished snapshot"
        );
        if bytes.len() <= tx.bytes.len() {
            return Ok(());
        }
        let tail = &bytes[tx.bytes.len()..];
        ensure!(tail.len() <= MAX_LINE + 1, "Invalid journal record size");
        if tail.ends_with(b"\n") || serde_json::from_slice::<Value>(tail).is_ok() {
            ensure!(
                !tail[..tail.len() - usize::from(tail.ends_with(b"\n"))].contains(&b'\n'),
                "Multiple unpublished records"
            );
            let mut complete = bytes.to_vec();
            if !complete.ends_with(b"\n") {
                complete.push(b'\n');
            }
            let next = self.decode(stem, None, complete)?;
            ensure!(
                next.entries.iter().all(|entry| !Self::owned_elsewhere(
                    tx,
                    &entry.id,
                    transactions
                )),
                "Unpublished record duplicates an active control owner"
            );
            return Ok(());
        }
        if self.incomplete_tail(tx, tail, transactions)? {
            return Ok(());
        }
        // An unpublished ACL Prepare has never authorized a backend write. Its
        // already-copied committed prefix must match exactly, and its partial
        // string must have the canonical fixed id/envelope and hex syntax. A
        // completed descriptor is always checked by decode, never discarded as
        // an incomplete string. Legacy ACL tails do not get this exception.
        if !tx.sealed && !tx.reverting && tx.entries.iter().all(|e| e.state == State::Applied) {
            for c in self.controls.iter().filter(|c| {
                permission_control(&c.id)
                    && !tx.entries.iter().any(|e| e.id == c.id)
                    && !Self::owned_elsewhere(tx, &c.id, transactions)
            }) {
                let prefix = format!(
                    "{{\"kind\":\"prepare\",\"id\":\"{}\",\"before\":\"dacl-v1:",
                    c.id
                );
                if prefix.as_bytes().starts_with(tail) {
                    return Ok(());
                }
                if let Some(hex) = tail.strip_prefix(prefix.as_bytes()) {
                    if hex.len() <= 32 * 1024
                        && hex
                            .iter()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
                    {
                        return Ok(());
                    }
                    // Only the closing object brace/newline may still be
                    // missing after a complete descriptor string.
                    if tail.ends_with(b"\"") {
                        let mut complete = bytes.to_vec();
                        complete.extend(b"}\n");
                        self.decode(stem, None, complete)?;
                        return Ok(());
                    }
                }
            }
        }
        bail!("Invalid incomplete unpublished record; manual review required")
    }

    pub(super) fn preserve_evidence(&self, stem: &str, bytes: &[u8]) -> Result<()> {
        let digest = hex::encode(Sha256::digest(bytes));
        let path = self.dir.join(format!("{stem}.evidence-{digest}"));
        let mut file = match fs::symlink_metadata(&path) {
            Ok(_) => open_file(&path, false)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut count = 0;
                for entry in fs::read_dir(&self.dir)? {
                    if entry?.file_name().to_string_lossy().contains(".evidence-") {
                        count += 1;
                    }
                }
                ensure!(count < MAX_EVIDENCE, "Too many journal evidence files");
                open_file(&path, true)?
            }
            Err(e) => return Err(e.into()),
        };
        // Resume a failed evidence copy only if every existing byte matches.
        // Never overwrite an evidence file, even on an explicit retry.
        let existing = read_bytes(&mut file)?;
        ensure!(
            bytes.starts_with(&existing),
            "Journal recovery evidence mismatch"
        );
        io_boundary("evidence_write")?;
        file.write_all(&bytes[existing.len()..])?;
        file.flush()?;
        io_boundary("evidence_sync")?;
        file.sync_all()?;
        same_file(&file, &path)?;
        io_boundary("evidence_directory")?;
        sync_directory(&self.dir)
    }
}
