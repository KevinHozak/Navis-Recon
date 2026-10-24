//! Redacted, immutable run summaries. This store is independent of the private
//! item journal in `managed_import` and never needs ledger configuration.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 512 * 1024;
const MAX_ITEMS: usize = 1000;
const MAX_RECEIPTS: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunReceipt {
    pub version: u8,
    pub receipt_id: String,
    pub run_id: String,
    pub operation_kind: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub duration_ms: u64,
    pub terminal_state: String,
    pub timed_out: bool,
    pub counts: Counts,
    pub items: Vec<ReceiptItem>,
    pub items_truncated: bool,
    pub diagnostic_codes: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub matched: u32,
    pub unmatched: u32,
    pub error: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub uncertain: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReceiptItem {
    pub item_id: String,
    pub operation_id: Option<String>,
    pub state: String,
    pub retry_safe: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceiptList {
    pub receipts: Vec<RunReceipt>,
    pub error: Option<String>,
}

pub trait ReceiptStore: Send + Sync {
    fn write(&self, receipt: &RunReceipt) -> Result<(), &'static str>;
}

pub struct FileStore {
    root: PathBuf,
}
impl FileStore {
    pub fn production() -> Result<Self, &'static str> {
        let base = std::env::var_os("LOCALAPPDATA").ok_or("store_unavailable")?;
        let root = PathBuf::from(base).join("Navis-Recon").join("sync-runs");
        let prospective = canonical_with_existing_ancestor(&root).ok_or("store_unavailable")?;
        if in_git_checkout(&prospective) {
            return Err("store_unsafe_location");
        }
        fs::create_dir_all(&root).map_err(|_| "store_unavailable")?;
        let canonical = fs::canonicalize(&root).map_err(|_| "store_unavailable")?;
        if in_git_checkout(&canonical) {
            return Err("store_unsafe_location");
        }
        Ok(Self { root: canonical })
    }
    pub(crate) fn at(root: PathBuf) -> Self {
        Self { root }
    }
}
impl ReceiptStore for FileStore {
    fn write(&self, receipt: &RunReceipt) -> Result<(), &'static str> {
        validate(receipt)?;
        fs::create_dir_all(&self.root).map_err(|_| "store_unavailable")?;
        let stem = &receipt.receipt_id;
        let final_path = self.root.join(format!("{stem}.json"));
        let stage = self.root.join(format!(".{stem}.stage"));
        let bytes = serde_json::to_vec(receipt).map_err(|_| "receipt_invalid")?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("receipt_too_large");
        }
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
            .map_err(|_| "publication_failed")?;
        let write_result = f.write_all(&bytes).and_then(|_| f.sync_all());
        drop(f);
        if write_result.is_err() {
            let _ = fs::remove_file(&stage);
            return Err("publication_failed");
        }
        if final_path.exists() {
            let _ = fs::remove_file(&stage);
            return Err("receipt_collision");
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
            let from: Vec<u16> = stage.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = final_path
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
                let _ = fs::remove_file(&stage);
                return Err("publication_failed");
            }
        }
        #[cfg(not(windows))]
        {
            fs::hard_link(&stage, &final_path).map_err(|_| "publication_failed")?;
            fs::remove_file(&stage).map_err(|_| "publication_failed")?;
        }
        Ok(())
    }
}

pub fn list_production() -> ReceiptList {
    let store = match FileStore::production() {
        Ok(v) => v,
        Err(e) => {
            return ReceiptList {
                receipts: vec![],
                error: Some(e.into()),
            }
        }
    };
    list_at(&store.root)
}
pub(crate) fn list_at(root: &Path) -> ReceiptList {
    let entries = match fs::read_dir(root) {
        Ok(v) => v,
        Err(_) => {
            return ReceiptList {
                receipts: vec![],
                error: Some("store_read_failed".into()),
            }
        }
    };
    let mut paths = vec![];
    for entry in entries {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => {
                return ReceiptList {
                    receipts: vec![],
                    error: Some("store_read_failed".into()),
                }
            }
        };
        if entry.path().extension().is_some_and(|e| e == "json") {
            paths.push(entry.path());
        }
    }
    paths.sort_by_key(|p| {
        std::cmp::Reverse(
            fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH),
        )
    });
    let mut receipts = vec![];
    for path in paths.into_iter().take(MAX_RECEIPTS) {
        let bytes = match fs::read(&path) {
            Ok(v) if v.len() as u64 <= MAX_BYTES => v,
            _ => {
                return ReceiptList {
                    receipts: vec![],
                    error: Some("receipt_corrupt".into()),
                }
            }
        };
        let value: RunReceipt = match serde_json::from_slice(&bytes) {
            Ok(v) if validate(&v).is_ok() => v,
            _ => {
                return ReceiptList {
                    receipts: vec![],
                    error: Some("receipt_corrupt".into()),
                }
            }
        };
        receipts.push(value);
    }
    ReceiptList {
        receipts,
        error: None,
    }
}

pub fn validate(r: &RunReceipt) -> Result<(), &'static str> {
    if r.version != 1 || !opaque(&r.receipt_id) || !opaque(&r.run_id) || r.receipt_id == r.run_id {
        return Err("receipt_invalid");
    }
    if !["analysis", "commit", "retry"].contains(&r.operation_kind.as_str())
        || !["review", "complete", "failed", "cancelled"].contains(&r.terminal_state.as_str())
    {
        return Err("receipt_invalid");
    }
    if r.ended_at_ms < r.started_at_ms
        || r.duration_ms > 7 * 24 * 60 * 60 * 1000
        || r.items.len() > MAX_ITEMS
        || r.diagnostic_codes.len() > 64
    {
        return Err("receipt_invalid");
    }
    if r.items.iter().any(|i| {
        !opaque(&i.item_id)
            || i.operation_id.as_ref().is_some_and(|v| !opaque(v))
            || ![
                "matched",
                "unmatched",
                "error",
                "succeeded",
                "failed",
                "uncertain",
            ]
            .contains(&i.state.as_str())
            || (i.retry_safe && i.state != "failed")
    }) {
        return Err("receipt_invalid");
    }
    if r.diagnostic_codes.iter().any(|v| {
        ![
            "timeout",
            "cancelled",
            "spawn_failed",
            "output_limit",
            "protocol_error",
            "runner_failed",
            "store_failed",
            "process_termination_uncertain",
            "item_failed",
            "item_uncertain",
        ]
        .contains(&v.as_str())
    }) {
        return Err("receipt_invalid");
    }
    let comparison = |state: &str, count: u32| {
        let listed = r.items.iter().filter(|i| i.state == state).count();
        if r.items_truncated {
            listed <= count as usize
        } else {
            listed == count as usize
        }
    };
    if !comparison("matched", r.counts.matched)
        || !comparison("unmatched", r.counts.unmatched)
        || !comparison("error", r.counts.error)
        || !comparison("succeeded", r.counts.succeeded)
        || !comparison("failed", r.counts.failed)
        || !comparison("uncertain", r.counts.uncertain)
    {
        return Err("receipt_invalid");
    }
    Ok(())
}
fn opaque(s: &str) -> bool {
    (16..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn opaque_id(raw: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn canonical_with_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut p = path.to_path_buf();
    let mut suffix = vec![];
    while !p.exists() {
        suffix.push(p.file_name()?.to_os_string());
        p.pop();
    }
    let mut p = fs::canonicalize(p).ok()?;
    for part in suffix.into_iter().rev() {
        p.push(part);
    }
    Some(p)
}
fn in_git_checkout(path: &Path) -> bool {
    let mut p = path.to_path_buf();
    loop {
        if p.join(".git").exists() {
            return true;
        }
        if !p.pop() {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> RunReceipt {
        RunReceipt {
            version: 1,
            receipt_id: opaque_id("receipt-a"),
            run_id: opaque_id("run-a"),
            operation_kind: "analysis".into(),
            started_at_ms: 100,
            ended_at_ms: 120,
            duration_ms: 20,
            terminal_state: "review".into(),
            timed_out: false,
            counts: Counts {
                matched: 1,
                unmatched: 0,
                error: 0,
                succeeded: 0,
                failed: 0,
                uncertain: 0,
            },
            items: vec![ReceiptItem {
                item_id: opaque_id("item-private statement.pdf"),
                operation_id: None,
                state: "matched".into(),
                retry_safe: false,
            }],
            items_truncated: false,
            diagnostic_codes: vec![],
        }
    }
    #[test]
    fn redacted_json_is_strict_and_opaque() {
        let r = sample();
        let s = serde_json::to_string(&r).unwrap();
        for secret in ["private statement.pdf", "C:/vault", "12.34", "child log"] {
            assert!(!s.contains(secret));
        }
        assert!(serde_json::from_str::<RunReceipt>(&format!(
            "{}",
            s.trim_end_matches('}').to_owned() + ",\"filename\":\"secret\"}"
        ))
        .is_err());
        assert!(validate(&r).is_ok());
    }
    #[test]
    fn immutable_publication_collision_and_restart_list() {
        let d = std::env::temp_dir().join(format!(
            "navis-receipts-{}",
            opaque_id(&format!("{:?}", std::thread::current().id()))
        ));
        let _ = fs::remove_dir_all(&d);
        let store = FileStore::at(d.clone());
        let r = sample();
        store.write(&r).unwrap();
        assert_eq!(store.write(&r), Err("receipt_collision"));
        assert_eq!(list_at(&d).receipts, vec![r]);
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn corrupt_receipt_is_explicit() {
        let d = std::env::temp_dir().join(format!("navis-receipts-corrupt-{}", now_ms()));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("bad.json"), b"{}").unwrap();
        assert_eq!(list_at(&d).error.as_deref(), Some("receipt_corrupt"));
        fs::remove_dir_all(d).unwrap();
    }
}
