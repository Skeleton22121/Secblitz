//! Journal records, transactions and strict decoding of the on-disk format.

use super::catalog::{target_for, validate_value};
use super::fsio::{
    io_boundary, metadata_safe, open_file, read_bytes, same_file, sync_directory,
    validate_update_file,
};
use super::{
    Engine, JournalRecoveryRequired, LEGACY_UPDATE_FILES, LOCK_NAME, MAX_EVIDENCE, MAX_LINE,
    MAX_TRANSACTIONS, MAX_WAL, SCHEMA,
};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, fs, fs::File};
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Record {
    Header {
        schema: u32,
        machine: String,
        transaction: String,
        sequence: u64,
    },
    Prepare {
        id: String,
        #[serde(deserialize_with = "deserialize_before")]
        before: Value,
    },
    Applied {
        id: String,
    },
    Sealed,
    Reverting,
    RestorePending {
        id: String,
    },
    Restored {
        id: String,
    },
    Reverted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    Pending,
    Applied,
    Restoring,
    Restored,
}

pub(super) struct Entry {
    pub(super) id: String,
    pub(super) before: Value,
    pub(super) state: State,
}

pub(super) struct Transaction {
    pub(super) name: String,
    pub(super) sequence: u64,
    pub(super) entries: Vec<Entry>,
    pub(super) sealed: bool,
    pub(super) reverting: bool,
    pub(super) reverted: bool,
    pub(super) file: Option<File>,
    // Length of the exact file we created/validated. Never publish a replacement
    // from a file that changed behind this transaction's handle.
    pub(super) length: u64,
    // Exact validated logical snapshot, and the bytes held on disk. These differ
    // only for a legacy incomplete tail, whose original stays in place until a
    // later COW commit. Never reconstruct a before-image from current probes.
    pub(super) bytes: Vec<u8>,
    pub(super) disk_bytes: Vec<u8>,
}

impl Transaction {
    pub(super) fn incomplete(&self) -> bool {
        !self.sealed || self.reverting || self.bytes != self.disk_bytes
    }
}

// Do not deserialize before images directly as Value: Value silently accepts
// duplicate object keys. Typed parsing also excludes arbitrary restore payloads
// before the id-specific domain check is reached.
pub(super) fn deserialize_before<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Value, D::Error> {
    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct Uac {
        present: bool,
        #[serde(deserialize_with = "required_value")]
        value: Option<u32>,
    }
    fn required_value<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Option<u32>, D::Error> {
        Option::<u32>::deserialize(d)
    }
    /// Duplicate keys are rejected here; id-specific domains are checked by the catalog.
    #[derive(Serialize)]
    #[serde(deny_unknown_fields)]
    struct Items {
        items: std::collections::BTreeMap<String, Option<u32>>,
    }
    impl<'de> Deserialize<'de> for Items {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Raw {
                items: UniqueMap,
            }
            struct UniqueMap(std::collections::BTreeMap<String, Option<u32>>);
            impl<'de> Deserialize<'de> for UniqueMap {
                fn deserialize<D: serde::Deserializer<'de>>(
                    d: D,
                ) -> std::result::Result<Self, D::Error> {
                    struct V;
                    impl<'de> serde::de::Visitor<'de> for V {
                        type Value = UniqueMap;
                        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                            f.write_str("a map of unique hardening items")
                        }
                        fn visit_map<A: serde::de::MapAccess<'de>>(
                            self,
                            mut a: A,
                        ) -> std::result::Result<UniqueMap, A::Error> {
                            let mut map = std::collections::BTreeMap::new();
                            while let Some((k, v)) = a.next_entry::<String, Option<u32>>()? {
                                if map.insert(k, v).is_some() {
                                    return Err(serde::de::Error::custom(
                                        "duplicate hardening item",
                                    ));
                                }
                            }
                            Ok(UniqueMap(map))
                        }
                    }
                    d.deserialize_map(V)
                }
            }
            Ok(Items {
                items: Raw::deserialize(d)?.items.0,
            })
        }
    }
    #[derive(Deserialize, Serialize)]
    #[serde(untagged)]
    enum Before {
        Boolean(bool),
        Inbound(String),
        Uac(Uac),
        Items(Items),
    }
    serde_json::to_value(Before::deserialize(d)?).map_err(serde::de::Error::custom)
}

pub(super) fn journal_name(stem: &str) -> Result<u64> {
    let (sequence, uuid) = stem.split_once('-').context("Invalid journal filename")?;
    let seq: u64 = sequence.parse()?;
    ensure!(
        seq > 0 && sequence == format!("{seq:020}"),
        "Invalid journal sequence"
    );
    ensure!(
        uuid::Uuid::parse_str(uuid)?.to_string() == uuid,
        "Invalid transaction UUID"
    );
    Ok(seq)
}

pub(super) fn record_bytes(record: &Record) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(record)?;
    ensure!(bytes.len() <= MAX_LINE, "Journal record exceeds limit");
    bytes.push(b'\n');
    Ok(bytes)
}

impl Engine {
    pub(super) fn decode(
        &self,
        stem: &str,
        file: Option<File>,
        bytes: Vec<u8>,
    ) -> Result<Transaction> {
        ensure!(bytes.len() as u64 <= MAX_WAL, "Journal exceeds size limit");
        let seq = journal_name(stem)?;
        let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1);
        ensure!(
            end > 0,
            "Missing complete journal header; manual review required"
        );
        let mut records = bytes[..end - 1].split(|b| *b == b'\n');
        let parse = |line: &[u8]| -> Result<Record> {
            ensure!(
                !line.is_empty() && line.len() <= MAX_LINE,
                "Invalid journal record size"
            );
            let record: Record = serde_json::from_slice(line).context("Invalid journal record")?;
            // Serde's internally tagged unit variants otherwise accept
            // extra fields despite deny_unknown_fields on the enum.
            if matches!(
                record,
                Record::Sealed | Record::Reverting | Record::Reverted
            ) {
                let object: serde_json::Map<String, Value> = serde_json::from_slice(line)?;
                ensure!(object.len() == 1, "Unexpected fields in journal marker");
            }
            Ok(record)
        };
        match parse(records.next().context("Missing header")?)? {
            Record::Header {
                schema,
                machine,
                transaction,
                sequence,
            } => {
                ensure!(
                    schema == SCHEMA
                        && machine == self.machine
                        && transaction == stem
                        && sequence == seq,
                    "Journal schema, machine, or transaction identity mismatch"
                );
            }
            _ => bail!("Journal must start with a header"),
        }
        let mut tx = Transaction {
            name: stem.into(),
            sequence: seq,
            entries: Vec::new(),
            sealed: false,
            reverting: false,
            reverted: false,
            file,
            length: bytes.len() as u64,
            bytes: bytes[..end].to_vec(),
            disk_bytes: bytes.clone(),
        };
        for line in records {
            let record = parse(line)?;
            ensure!(!tx.reverted, "Records after transaction completion");
            match record {
                Record::Prepare { id, before } => {
                    self.control(&id)?;
                    validate_value(&id, &before)?;
                    ensure!(
                        before != target_for(&id, &before)?,
                        "Redundant before image"
                    );
                    ensure!(
                        !tx.sealed
                            && !tx.reverting
                            && tx
                                .entries
                                .iter()
                                .all(|e| e.state == State::Applied && e.id != id),
                        "Invalid prepare ordering or duplicate before image"
                    );
                    tx.entries.push(Entry {
                        id,
                        before,
                        state: State::Pending,
                    });
                }
                Record::Applied { id } => {
                    ensure!(!tx.sealed && !tx.reverting, "Apply after seal/revert");
                    let e = tx.entries.last_mut().context("Apply without prepare")?;
                    ensure!(
                        e.id == id && e.state == State::Pending,
                        "Invalid apply result"
                    );
                    e.state = State::Applied;
                }
                Record::Sealed => {
                    ensure!(
                        !tx.sealed
                            && !tx.reverting
                            && tx.entries.iter().all(|e| e.state == State::Applied),
                        "Invalid seal"
                    );
                    tx.sealed = true;
                }
                Record::Reverting => {
                    ensure!(!tx.reverting, "Duplicate revert start");
                    tx.reverting = true;
                }
                Record::RestorePending { id } => {
                    ensure!(tx.reverting, "Restore before revert start");
                    let e = tx
                        .entries
                        .iter_mut()
                        .find(|e| e.id == id)
                        .context("Restore without before image")?;
                    ensure!(e.state != State::Restored, "Restore after completion");
                    e.state = State::Restoring;
                }
                Record::Restored { id } => {
                    ensure!(tx.reverting, "Restore result before revert start");
                    let e = tx
                        .entries
                        .iter_mut()
                        .find(|e| e.id == id)
                        .context("Result without before image")?;
                    ensure!(e.state == State::Restoring, "Restore result without intent");
                    e.state = State::Restored;
                }
                Record::Reverted => {
                    ensure!(
                        tx.reverting && tx.entries.iter().all(|e| e.state == State::Restored),
                        "Premature revert completion"
                    );
                    tx.reverted = true;
                }
                Record::Header { .. } => bail!("Duplicate journal header"),
            }
        }
        if end != bytes.len() && !self.incomplete_tail(&tx, &bytes[end..], &[])? {
            return Err(JournalRecoveryRequired {
                    transaction: stem.into(), validated_bytes: end,
                    reason: "tail is not a provably incomplete legal append (complete JSON without a newline is ambiguous)",
                }.into());
        }
        Ok(tx)
    }

    pub(super) fn load(&self) -> Result<Vec<Transaction>> {
        let mut transactions = Vec::new();
        let mut staged = Vec::new();
        let mut evidence_count = 0;
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 journal filename"))?;
            if name == LOCK_NAME {
                continue;
            }
            if matches!(
                name.as_str(),
                "Updates" | "operations" | "Patching" | "App" | crate::platform::WEB_PROTECTION
            ) {
                // Module-owned protected namespaces, never journal payloads.
                // Production platform validation supplies ACL/owner protection.
                metadata_safe(&fs::symlink_metadata(entry.path())?, true)?;
                continue;
            }
            if LEGACY_UPDATE_FILES.contains(&name.as_str()) {
                validate_update_file(&entry.path())?;
                continue;
            }
            if let Some((stem, digest)) = name.split_once(".evidence-") {
                journal_name(stem)?;
                ensure!(
                    digest.len() == 64
                        && digest
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                    "Invalid evidence filename"
                );
                evidence_count += 1;
                ensure!(
                    evidence_count <= MAX_EVIDENCE,
                    "Too many journal evidence files"
                );
                let file = open_file(&entry.path(), false)?;
                ensure!(
                    file.metadata()?.len() <= MAX_WAL,
                    "Oversized journal evidence"
                );
                // Opaque evidence may itself be a partial failed copy. It is
                // never a source of originals or a replacement journal.
                continue;
            }
            if let Some(stem) = name.strip_suffix(".jsonl.next") {
                journal_name(stem)?;
                ensure!(staged.is_empty(), "Multiple unpublished journal snapshots");
                let mut file = open_file(&entry.path(), false)?;
                let bytes = read_bytes(&mut file)?;
                staged.push((stem.to_owned(), file, bytes));
                continue;
            }
            ensure!(
                transactions.len() < MAX_TRANSACTIONS,
                "Too many journal transactions"
            );
            let stem = name
                .strip_suffix(".jsonl")
                .context("Unexpected journal entry")?;
            journal_name(stem)?;
            let mut file = open_file(&entry.path(), false)?;
            let bytes = read_bytes(&mut file)?;
            transactions.push(self.decode(stem, Some(file), bytes)?);
        }
        transactions.sort_by_key(|t| t.sequence);
        ensure!(
            transactions
                .windows(2)
                .all(|w| w[0].sequence < w[1].sequence),
            "Duplicate transaction sequence"
        );
        // Validate the whole active stack before any caller probes or replays.
        // Only the newest active batch can be incomplete; originals must have
        // exactly one active owner, even when each WAL is valid in isolation.
        let active: Vec<_> = transactions.iter().filter(|t| !t.reverted).collect();
        let mut owners = HashSet::new();
        for (i, tx) in active.iter().enumerate() {
            ensure!(
                i + 1 == active.len() || !tx.incomplete(),
                "Incomplete transaction precedes another active transaction"
            );
            for entry in &tx.entries {
                ensure!(
                    owners.insert(&entry.id),
                    "Duplicate active control owner; journal history is invalid"
                );
            }
        }
        // Do not hide an older torn append behind subsequent history. Validate
        // the entire directory/active stack and all staging before any recovery.
        for tx in &transactions {
            ensure!(
                tx.bytes == tx.disk_bytes
                    || !transactions
                        .iter()
                        .any(|later| later.sequence > tx.sequence && !later.reverted),
                "Incomplete append precedes another active transaction"
            );
            if tx.bytes != tx.disk_bytes {
                ensure!(
                    self.incomplete_tail(tx, &tx.disk_bytes[tx.bytes.len()..], &transactions)?,
                    "Incomplete append conflicts with another active control owner"
                );
            }
        }
        for (stem, _, bytes) in &staged {
            self.validate_staged(stem, bytes, &transactions)?;
        }
        for tx in &transactions {
            if tx.bytes != tx.disk_bytes {
                self.preserve_evidence(&tx.name, &tx.disk_bytes)?;
            }
        }
        for (stem, mut file, bytes) in staged {
            let path = self.dir.join(format!("{stem}.jsonl.next"));
            self.preserve_evidence(&stem, &bytes)?;
            same_file(&file, &path)?;
            ensure!(
                read_bytes(&mut file)? == bytes,
                "Unpublished snapshot changed during recovery"
            );
            drop(file); // Windows handles deny delete until explicitly released.
            io_boundary("retire_stage")?;
            fs::remove_file(path)?;
            sync_directory(&self.dir)?;
        }
        Ok(transactions)
    }
}
