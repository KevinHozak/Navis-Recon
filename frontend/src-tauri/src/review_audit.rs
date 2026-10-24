//! Private, ledger-scoped approval journal. Raw transaction snapshots never cross IPC.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema_version: u32,
    pub operation_id: String,
    pub operation_type: String,
    pub ledger_namespace: String,
    pub source_revision_before: String,
    pub source_revision_after: String,
    pub transaction_before: String,
    pub transaction_after: String,
    pub transaction_before_hash: String,
    pub transaction_after_hash: String,
    pub old_category: String,
    pub new_category: String,
    pub created_at_unix_ms: u128,
    pub committed_at_unix_ms: u128,
    pub undo_of: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub schema_version: u32,
    pub operation_id: String,
    pub operation_type: String,
    pub ledger_namespace: String,
    pub source_revision_before: String,
    pub source_revision_after: String,
    pub transaction_before: String,
    pub transaction_after: String,
    pub transaction_before_hash: String,
    pub transaction_after_hash: String,
    pub old_category: String,
    pub new_category: String,
    pub created_at_unix_ms: u128,
    pub undo_of: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct DisplayRecord {
    pub id: String,
    pub operation_type: String,
    pub before_hash: String,
    pub after_hash: String,
    pub old_category: String,
    pub new_category: String,
    pub timestamp_unix_ms: u128,
    pub undo_of: Option<String>,
    pub undone: bool,
}

pub fn namespace(path: &str) -> Result<(String, PathBuf), String> {
    let canonical = fs::canonicalize(path)
        .map_err(|e| format!("cannot identify ledger for private approval history: {e}"))?;
    let identity = canonical.to_string_lossy().to_lowercase();
    let ns = hash(identity.as_bytes());
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("private approval history unavailable: LOCALAPPDATA is not set".to_string())?;
    #[cfg(not(windows))]
    return Err("approval history is supported only on Windows desktop".into());
    #[cfg(windows)]
    {
        let dir = resolve_private_store(&root, &canonical, &ns)?;
        fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create private approval history store: {e}"))?;
        Ok((ns, dir))
    }
}
#[cfg(windows)]
fn resolve_private_store(root: &Path, ledger: &Path, ns: &str) -> Result<PathBuf, String> {
    let root = fs::canonicalize(root)
        .map_err(|e| format!("cannot identify private approval store root: {e}"))?;
    let _ledger = fs::canonicalize(ledger).map_err(|e| format!("cannot identify ledger: {e}"))?;
    let dir = root.join("Navis-Recon").join("approval-history").join(ns);
    // Resolve every existing ancestor so junctions/symlinks cannot redirect storage into a checkout.
    let mut probe = dir.as_path();
    while !probe.exists() {
        probe = probe
            .parent()
            .ok_or("invalid private approval store path")?;
    }
    let actual = fs::canonicalize(probe)
        .map_err(|e| format!("cannot resolve private approval store: {e}"))?;
    if is_git_checkout(&actual) {
        return Err("private approval history location is unsafe; configure LOCALAPPDATA outside all Git checkouts".into());
    }
    Ok(dir)
}
#[cfg(windows)]
fn is_git_checkout(path: &Path) -> bool {
    path.ancestors().any(|p| p.join(".git").exists())
}
#[cfg(test)]
pub fn test_store(path: &str) -> (String, PathBuf) {
    let ns = hash(path.as_bytes());
    let dir = std::env::temp_dir()
        .join("navis-recon-review-audit-tests")
        .join(format!("{}-{ns}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    (ns, dir)
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum PersistFault {
    None,
    StagingWrite,
    StagingSync,
    Publish,
}

/// Per invocation fault points. Production callers always use `NONE`; tests can fail
/// intent publication and final audit publication independently.
#[derive(Clone, Copy)]
pub(crate) struct FaultPlan {
    pub intent: PersistFault,
    pub commit: PersistFault,
    pub fail_source_after_write: bool,
    pub fail_restore: bool,
    pub fail_restore_after_partial: bool,
    pub stop_after_prepare: bool,
    pub stop_after_source: bool,
    pub fail_intent_cleanup: bool,
}

impl FaultPlan {
    pub const NONE: Self = Self {
        intent: PersistFault::None,
        commit: PersistFault::None,
        fail_source_after_write: false,
        fail_restore: false,
        fail_restore_after_partial: false,
        stop_after_prepare: false,
        stop_after_source: false,
        fail_intent_cleanup: false,
    };
}
fn persist_new_fault(path: &Path, bytes: &[u8], fault: PersistFault) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().ok_or("invalid journal path")?;
    let mut stage = None;
    for _ in 0..100 {
        let candidate = parent.join(format!(
            ".stage-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                stage = Some((candidate, file));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot create private approval staging file: {e}")),
        }
    }
    let (stage_path, mut file) =
        stage.ok_or("cannot allocate unique private approval staging file")?;
    let result = match fault {
        PersistFault::StagingWrite => {
            let _ = file.write_all(&bytes[..bytes.len() / 2]);
            Err(std::io::Error::other("injected staging write failure"))
        }
        PersistFault::StagingSync => file
            .write_all(bytes)
            .and_then(|_| Err(std::io::Error::other("injected staging sync failure"))),
        _ => file.write_all(bytes).and_then(|_| file.sync_all()),
    };
    drop(file);
    if let Err(e) = result {
        let _ = fs::remove_file(&stage_path);
        return Err(format!("cannot sync private approval staging file: {e}"));
    }
    if fault == PersistFault::Publish {
        let _ = fs::remove_file(&stage_path);
        return Err("injected private journal publication failure".into());
    }
    #[cfg(windows)]
    let publish = {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        let from: Vec<u16> = stage_path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    };
    #[cfg(not(windows))]
    let publish = fs::hard_link(&stage_path, path).and_then(|_| fs::remove_file(&stage_path));
    if let Err(e) = publish {
        let _ = fs::remove_file(&stage_path);
        return Err(format!(
            "cannot publish private approval journal without replacing an existing record: {e}"
        ));
    }
    Ok(())
}
#[cfg(test)]
pub fn prepare(dir: &Path, intent: &Intent) -> Result<PathBuf, String> {
    prepare_fault(dir, intent, PersistFault::None)
}
pub(crate) fn prepare_fault(
    dir: &Path,
    intent: &Intent,
    fault: PersistFault,
) -> Result<PathBuf, String> {
    let path = dir.join(format!("intent-{}.json", intent.operation_id));
    let bytes = serde_json::to_vec(intent).map_err(|e| e.to_string())?;
    persist_new_fault(&path, &bytes, fault)?;
    Ok(path)
}
pub fn commit(dir: &Path, intent: &Intent) -> Result<(), String> {
    commit_fault(dir, intent, PersistFault::None)
}
pub(crate) fn commit_fault(dir: &Path, intent: &Intent, fault: PersistFault) -> Result<(), String> {
    let mut record = Record {
        schema_version: intent.schema_version,
        operation_id: intent.operation_id.clone(),
        operation_type: intent.operation_type.clone(),
        ledger_namespace: intent.ledger_namespace.clone(),
        source_revision_before: intent.source_revision_before.clone(),
        source_revision_after: intent.source_revision_after.clone(),
        transaction_before: intent.transaction_before.clone(),
        transaction_after: intent.transaction_after.clone(),
        transaction_before_hash: intent.transaction_before_hash.clone(),
        transaction_after_hash: intent.transaction_after_hash.clone(),
        old_category: intent.old_category.clone(),
        new_category: intent.new_category.clone(),
        created_at_unix_ms: intent.created_at_unix_ms,
        committed_at_unix_ms: now(),
        undo_of: intent.undo_of.clone(),
    };
    if record.committed_at_unix_ms == 0 {
        record.committed_at_unix_ms = 1;
    }
    let path = dir.join(format!("record-{}.json", record.operation_id));
    let bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
    persist_new_fault(&path, &bytes, fault)?;
    Ok(())
}
pub fn remove_intent(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|e| format!("cannot remove reconciled approval intent: {e}"))
}
pub fn read_records(dir: &Path, ns: &str) -> Result<Vec<(Record, String)>, String> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).map_err(|e| format!("cannot read private approval history: {e}"))? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with("record-") || !name.ends_with(".json") {
            continue;
        }
        let bytes = fs::read(e.path()).map_err(|e| e.to_string())?;
        let r: Record = serde_json::from_slice(&bytes).map_err(|_| {
            "private approval history is corrupt; manual recovery review required".to_string()
        })?;
        if r.schema_version != 1
            || r.ledger_namespace != ns
            || !valid_id(&r.operation_id)
            || !valid_hex(&r.source_revision_before)
            || !valid_hex(&r.source_revision_after)
            || !valid_hex(&r.transaction_before_hash)
            || !valid_hex(&r.transaction_after_hash)
            || !valid_timestamp(r.created_at_unix_ms)
            || !valid_timestamp(r.committed_at_unix_ms)
            || r.committed_at_unix_ms < r.created_at_unix_ms
            || r.source_revision_before == r.source_revision_after
            || r.transaction_before_hash != hash(r.transaction_before.as_bytes())
            || r.transaction_after_hash != hash(r.transaction_after.as_bytes())
            || r.operation_id != name[7..name.len() - 5]
            || (r.operation_type == "approve" && r.undo_of.is_some())
            || (r.operation_type == "undo" && r.undo_of.is_none())
        {
            return Err("private approval history is invalid or belongs to another ledger; manual recovery review required".into());
        }
        let intent = Intent {
            schema_version: r.schema_version,
            operation_id: r.operation_id.clone(),
            operation_type: r.operation_type.clone(),
            ledger_namespace: r.ledger_namespace.clone(),
            source_revision_before: r.source_revision_before.clone(),
            source_revision_after: r.source_revision_after.clone(),
            transaction_before: r.transaction_before.clone(),
            transaction_after: r.transaction_after.clone(),
            transaction_before_hash: r.transaction_before_hash.clone(),
            transaction_after_hash: r.transaction_after_hash.clone(),
            old_category: r.old_category.clone(),
            new_category: r.new_category.clone(),
            created_at_unix_ms: r.created_at_unix_ms,
            undo_of: r.undo_of.clone(),
        };
        validate_intent(&intent, ns)?;
        out.push((r, e.path().to_string_lossy().into_owned()));
    }
    out.sort_by_key(|x| x.0.committed_at_unix_ms);
    validate_links(&out)?;
    Ok(out)
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn valid_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn same_operation(r: &Record, i: &Intent) -> bool {
    r.schema_version == i.schema_version
        && r.operation_id == i.operation_id
        && r.operation_type == i.operation_type
        && r.ledger_namespace == i.ledger_namespace
        && r.source_revision_before == i.source_revision_before
        && r.source_revision_after == i.source_revision_after
        && r.transaction_before == i.transaction_before
        && r.transaction_after == i.transaction_after
        && r.transaction_before_hash == i.transaction_before_hash
        && r.transaction_after_hash == i.transaction_after_hash
        && r.old_category == i.old_category
        && r.new_category == i.new_category
        && r.created_at_unix_ms == i.created_at_unix_ms
        && r.undo_of == i.undo_of
}
fn validate_links(records: &[(Record, String)]) -> Result<(), String> {
    let mut ids = std::collections::HashMap::new();
    for (r, _) in records {
        if ids.insert(r.operation_id.as_str(), r).is_some() {
            return Err("duplicate approval operation ID; manual recovery review required".into());
        }
    }
    let mut undone = std::collections::HashSet::new();
    for (r, _) in records {
        match r.operation_type.as_str() {
            "approve" if r.undo_of.is_none() => {}
            "undo" => {
                let parent_id = r.undo_of.as_deref().ok_or("undo record has no parent")?;
                let parent = ids
                    .get(parent_id)
                    .ok_or("undo record references a missing approval")?;
                if parent.operation_type != "approve"
                    || r.transaction_before != parent.transaction_after
                    || r.transaction_after != parent.transaction_before
                    || r.old_category != parent.new_category
                    || r.new_category != parent.old_category
                    || !undone.insert(parent_id)
                {
                    return Err("undo history has an inconsistent or duplicate parent link; manual recovery review required".into());
                }
            }
            _ => {
                return Err(
                    "approval history contains an unknown or inconsistent operation type".into(),
                )
            }
        }
    }
    Ok(())
}
pub fn display(records: &[(Record, String)]) -> Vec<DisplayRecord> {
    records
        .iter()
        .map(|(r, _)| DisplayRecord {
            id: r.operation_id.clone(),
            operation_type: r.operation_type.clone(),
            before_hash: r.transaction_before_hash.clone(),
            after_hash: r.transaction_after_hash.clone(),
            old_category: r.old_category.clone(),
            new_category: r.new_category.clone(),
            timestamp_unix_ms: r.committed_at_unix_ms,
            undo_of: r.undo_of.clone(),
            undone: records
                .iter()
                .any(|(u, _)| u.undo_of.as_deref() == Some(&r.operation_id)),
        })
        .collect()
}
pub fn validate_intent(i: &Intent, ns: &str) -> Result<(), String> {
    if i.schema_version != 1
        || i.ledger_namespace != ns
        || i.transaction_before_hash != hash(i.transaction_before.as_bytes())
        || i.transaction_after_hash != hash(i.transaction_after.as_bytes())
        || !valid_id(&i.operation_id)
        || !matches!(i.operation_type.as_str(), "approve" | "undo")
        || !valid_hex(&i.source_revision_before)
        || !valid_hex(&i.source_revision_after)
        || !valid_hex(&i.transaction_before_hash)
        || !valid_hex(&i.transaction_after_hash)
        || !valid_timestamp(i.created_at_unix_ms)
        || i.source_revision_before == i.source_revision_after
        || !crate::ledger_review::valid_audit_transition(i)
    {
        return Err("approval recovery intent is corrupt or belongs to another ledger; manual recovery review required".into());
    }
    Ok(())
}

fn valid_timestamp(value: u128) -> bool {
    // ECMAScript Date's largest representable millisecond timestamp.
    (1..=8_640_000_000_000_000).contains(&value)
}

pub fn reconcile(dir: &Path, ns: &str, current_revision: &str) -> Result<(), String> {
    if !valid_hex(current_revision) {
        return Err("invalid source revision during approval recovery".into());
    }
    let records = read_records(dir, ns)?;
    let mut intents = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("intent-") || !name.ends_with(".json") {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(|e| e.to_string())?;
        let intent: Intent = serde_json::from_slice(&bytes).map_err(|_| {
            "approval recovery intent is corrupt; manual review required".to_string()
        })?;
        validate_intent(&intent, ns)?;
        if intent.operation_id != name[7..name.len() - 5] {
            return Err("approval recovery intent ID is invalid; manual review required".into());
        }
        intents.push((intent, entry.path()));
    }
    let mut intent_ids = std::collections::HashSet::new();
    for (intent, _) in &intents {
        if !intent_ids.insert(intent.operation_id.as_str()) {
            return Err("duplicate recovery intent ID; manual review required".into());
        }
    }
    let mut pending_undo = std::collections::HashSet::new();
    for (intent, _) in &intents {
        if let Some((committed, _)) = records
            .iter()
            .find(|(r, _)| r.operation_id == intent.operation_id)
        {
            if !same_operation(committed, intent) {
                return Err("committed approval and recovery intent disagree; manual recovery review required".into());
            }
            continue;
        }
        if intent.operation_type == "undo" {
            let parent_id = intent
                .undo_of
                .as_deref()
                .ok_or("undo recovery has no parent")?;
            let parent = records
                .iter()
                .find(|(r, _)| r.operation_id == parent_id)
                .ok_or(
                    "undo recovery references a missing approval; manual recovery review required",
                )?;
            if parent.0.operation_type != "approve"
                || parent.0.transaction_after != intent.transaction_before
                || parent.0.transaction_before != intent.transaction_after
                || parent.0.new_category != intent.old_category
                || parent.0.old_category != intent.new_category
                || records
                    .iter()
                    .any(|(r, _)| r.undo_of.as_deref() == Some(parent_id))
                || !pending_undo.insert(parent_id)
            {
                return Err("undo recovery is inconsistent or already committed; manual recovery review required".into());
            }
        } else if intent.undo_of.is_some() {
            return Err("approval recovery has an invalid parent link".into());
        }
    }
    for (intent, path) in intents {
        if let Some((committed, _)) = records
            .iter()
            .find(|(r, _)| r.operation_id == intent.operation_id)
        {
            if !same_operation(committed, &intent) {
                return Err("committed approval and recovery intent disagree; manual recovery review required".into());
            }
            if let Err(error) = remove_intent(&path) {
                eprintln!(
                    "committed approval intent cleanup pending at {}: {error}",
                    path.display()
                );
            }
            continue;
        }
        if current_revision == intent.source_revision_before {
            remove_intent(&path)?;
            continue;
        }
        if current_revision == intent.source_revision_after {
            // Undo promotion is validated against a freshly reloaded parent projection.
            let fresh = read_records(dir, ns)?;
            if intent.operation_type == "undo" {
                let parent = intent
                    .undo_of
                    .as_deref()
                    .and_then(|id| fresh.iter().find(|(r, _)| r.operation_id == id));
                let Some((parent, _)) = parent else {
                    return Err("undo recovery references a missing approval; manual recovery review required".into());
                };
                if parent.operation_type != "approve"
                    || parent.transaction_after != intent.transaction_before
                    || parent.transaction_before != intent.transaction_after
                    || parent.new_category != intent.old_category
                    || parent.old_category != intent.new_category
                    || fresh
                        .iter()
                        .any(|(r, _)| r.undo_of.as_deref() == Some(parent.operation_id.as_str()))
                {
                    return Err("undo recovery is inconsistent or already committed; manual recovery review required".into());
                }
            } else if intent.undo_of.is_some() {
                return Err("approval recovery has an invalid parent link".into());
            }
            commit(dir, &intent)?;
            if let Err(error) = remove_intent(&path) {
                eprintln!(
                    "recovered approval intent cleanup pending at {}: {error}",
                    path.display()
                );
            }
            continue;
        }
        return Err("approval recovery is ambiguous: ledger matches neither recorded revision. No automatic ledger rewrite was made; inspect the private recovery intent and ledger manually before retrying.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (PathBuf, String, Intent) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let id = format!("audit-test-{}-{unique}", now());
        let dir = std::env::temp_dir().join("navis-recon-audit-unit").join(id);
        fs::create_dir_all(&dir).unwrap();
        let ns = "synthetic-ledger".to_string();
        let before = "2026-01-01 ! \"Shop\"\n  Expenses:Food 1 USD\n".to_string();
        let after = before.replace(" ! ", " * ");
        let i = Intent {
            schema_version: 1,
            operation_id: format!("op-{}-{unique}", now()),
            operation_type: "approve".into(),
            ledger_namespace: ns.clone(),
            source_revision_before: hash(b"before source"),
            source_revision_after: hash(b"after source"),
            transaction_before_hash: hash(before.as_bytes()),
            transaction_after_hash: hash(after.as_bytes()),
            transaction_before: before,
            transaction_after: after,
            old_category: "Expenses:Food".into(),
            new_category: "Expenses:Food".into(),
            created_at_unix_ms: now(),
            undo_of: None,
        };
        (dir, ns, i)
    }
    #[test]
    fn reload_and_after_revision_reconciliation_promotes_once() {
        let (dir, ns, i) = setup();
        let ip = prepare(&dir, &i).unwrap();
        reconcile(&dir, &ns, &i.source_revision_after).unwrap();
        assert!(!ip.exists());
        let records = read_records(&dir, &ns).unwrap();
        assert_eq!(records.len(), 1);
        reconcile(&dir, &ns, &i.source_revision_after).unwrap();
        assert_eq!(read_records(&dir, &ns).unwrap().len(), 1);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn before_revision_aborts_and_ambiguous_revision_keeps_intent() {
        let (dir, ns, i) = setup();
        let ip = prepare(&dir, &i).unwrap();
        assert!(reconcile(&dir, &ns, &hash(b"unrelated")).is_err());
        assert!(ip.exists());
        reconcile(&dir, &ns, &i.source_revision_before).unwrap();
        assert!(!ip.exists());
        assert!(read_records(&dir, &ns).unwrap().is_empty());
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn foreign_or_corrupt_intent_blocks_reconciliation() {
        let (dir, ns, i) = setup();
        let ip = prepare(&dir, &i).unwrap();
        let mut altered = i.clone();
        altered.ledger_namespace = "foreign".into();
        fs::write(ip, serde_json::to_vec(&altered).unwrap()).unwrap();
        assert!(reconcile(&dir, &ns, &i.source_revision_after).is_err());
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn staging_write_sync_and_publication_failures_never_expose_final_json() {
        let (dir, _, i) = setup();
        for fault in [
            PersistFault::StagingWrite,
            PersistFault::StagingSync,
            PersistFault::Publish,
        ] {
            assert!(prepare_fault(&dir, &i, fault).is_err());
            assert!(!dir.join(format!("intent-{}.json", i.operation_id)).exists());
            assert!(!fs::read_dir(&dir).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".stage-")));
        }
        prepare(&dir, &i).unwrap();
        for fault in [
            PersistFault::StagingWrite,
            PersistFault::StagingSync,
            PersistFault::Publish,
        ] {
            assert!(commit_fault(&dir, &i, fault).is_err());
            assert!(!dir.join(format!("record-{}.json", i.operation_id)).exists());
            assert!(!fs::read_dir(&dir).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".stage-")));
        }
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn publication_does_not_clobber_existing_record() {
        let (dir, _, i) = setup();
        commit(&dir, &i).unwrap();
        let path = dir.join(format!("record-{}.json", i.operation_id));
        let original = fs::read(&path).unwrap();
        assert!(commit(&dir, &i).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn forged_amount_transition_blocks_reconciliation_even_with_recomputed_hashes() {
        let (dir, ns, i) = setup();
        let ip = prepare(&dir, &i).unwrap();
        let mut forged = i.clone();
        forged.transaction_after = forged.transaction_after.replace("1 USD", "900 USD");
        forged.transaction_after_hash = hash(forged.transaction_after.as_bytes());
        fs::write(&ip, serde_json::to_vec(&forged).unwrap()).unwrap();
        assert!(reconcile(&dir, &ns, &i.source_revision_after).is_err());
        assert!(ip.exists());
        assert!(read_dir_records_empty(&dir));
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn unknown_operation_type_and_invalid_category_token_fail_closed() {
        let (dir, ns, i) = setup();
        let ip = prepare(&dir, &i).unwrap();
        let mut altered = i.clone();
        altered.operation_type = "approve_v2".into();
        fs::write(&ip, serde_json::to_vec(&altered).unwrap()).unwrap();
        assert!(reconcile(&dir, &ns, &i.source_revision_after).is_err());
        altered.operation_type = "approve".into();
        altered.new_category = "Expenses:Other\n  Assets:Bank 900 USD".into();
        fs::write(&ip, serde_json::to_vec(&altered).unwrap()).unwrap();
        assert!(reconcile(&dir, &ns, &i.source_revision_after).is_err());
        assert!(read_dir_records_empty(&dir));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn timestamps_revisions_and_single_transaction_snapshots_are_strict() {
        let (dir, ns, intent) = setup();
        let mut invalid = intent.clone();
        invalid.created_at_unix_ms = 8_640_000_000_000_001;
        assert!(validate_intent(&invalid, &ns).is_err());
        invalid = intent.clone();
        invalid.source_revision_after = invalid.source_revision_before.clone();
        assert!(validate_intent(&invalid, &ns).is_err());
        invalid = intent.clone();
        invalid
            .transaction_before
            .push_str("2026-01-02 ! \"second\"\n  Expenses:Food 1 USD\n");
        invalid
            .transaction_after
            .push_str("2026-01-02 * \"second\"\n  Expenses:Food 1 USD\n");
        invalid.transaction_before_hash = hash(invalid.transaction_before.as_bytes());
        invalid.transaction_after_hash = hash(invalid.transaction_after.as_bytes());
        assert!(validate_intent(&invalid, &ns).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_record_fields_fail_closed() {
        let (dir, ns, intent) = setup();
        commit(&dir, &intent).unwrap();
        let path = dir.join(format!("record-{}.json", intent.operation_id));
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["unrecognized"] = serde_json::json!(true);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(read_records(&dir, &ns).is_err());
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn committed_intent_cleanup_survives_later_unrelated_revision() {
        let (dir, ns, i) = setup();
        let ip = prepare(&dir, &i).unwrap();
        commit(&dir, &i).unwrap();
        let later_revision = hash(b"later ledger with unrelated edits");
        reconcile(&dir, &ns, &later_revision).unwrap();
        assert!(!ip.exists());
        assert_eq!(read_records(&dir, &ns).unwrap().len(), 1);
        let _ = fs::remove_dir_all(dir);
    }
    fn read_dir_records_empty(dir: &Path) -> bool {
        !fs::read_dir(dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("record-")
        })
    }
    #[cfg(windows)]
    #[test]
    fn injected_store_resolver_accepts_temp_and_rejects_checkout_destination() {
        let base =
            std::env::temp_dir().join(format!("recon-resolver-{}-{}", std::process::id(), now()));
        let safe = base.join("safe");
        let checkout = base.join("checkout");
        fs::create_dir_all(&safe).unwrap();
        fs::create_dir_all(checkout.join(".git")).unwrap();
        let ledger = safe.join("ledger.beancount");
        fs::write(&ledger, "").unwrap();
        assert!(resolve_private_store(&safe, &ledger, "abc").is_ok());
        assert!(resolve_private_store(&checkout, &ledger, "abc").is_err());
        let _ = fs::remove_dir_all(base);
    }
}
