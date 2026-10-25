use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct ChecklistPayload {
    pub year: Value,
    #[serde(default)]
    pub triage: Vec<MonthlyTriageWeek>,
    #[serde(default)]
    pub statements: Vec<StatementGridItem>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MonthlyTriageWeek {
    pub week: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub tasks: Vec<TriageTask>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TriageTask {
    pub id: Value,
    pub title: String,
    pub day: Value,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub completed: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatementGridItem {
    pub institution: String,
    #[serde(default)]
    pub months: Vec<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct ChecklistSnapshot {
    #[serde(flatten)]
    pub payload: ChecklistPayload,
    pub revision: String,
}
fn snapshot_at(path: &Path) -> Result<ChecklistSnapshot, String> {
    let (bytes, revision) = crate::document_store::read(path)?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let payload =
        serde_json::from_slice(bytes).map_err(|e| format!("Checklist unavailable: {e}"))?;
    Ok(ChecklistSnapshot { payload, revision })
}
#[tauri::command]
pub fn get_vault_checklist() -> Result<ChecklistSnapshot, String> {
    let path = Path::new(crate::config::get("checklist_path"));
    snapshot_at(path)
}
fn save_at(
    path: &Path,
    mut payload: ChecklistPayload,
    expected_revision: &str,
) -> Result<ChecklistSnapshot, String> {
    let _lock = crate::document_store::lock(path)?;
    payload.extra.remove("revision");
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    crate::document_store::save(path, expected_revision, &bytes)?;
    snapshot_at(path)
}
#[tauri::command]
pub fn save_vault_checklist(
    payload: ChecklistPayload,
    expected_revision: String,
) -> Result<ChecklistSnapshot, String> {
    save_at(
        Path::new(crate::config::get("checklist_path")),
        payload,
        &expected_revision,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn fixture() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "recon-checklist-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("checklist.json");
        fs::write(&path, b"\xef\xbb\xbf{\"year\":2026,\"triage\":[],\"statements\":[{\"institution\":\"Synthetic Bank\",\"months\":[true,false]}],\"unknown\":{\"preserve\":1}}").unwrap();
        path
    }
    #[test]
    fn revision_save_readback_preserves_legacy_flags_and_unknown_fields() {
        let path = fixture();
        let mut original = snapshot_at(&path).unwrap();
        assert!(original.payload.statements[0].months[0]);
        original.payload.statements[0].months[1] = true;
        let saved = save_at(&path, original.payload, &original.revision).unwrap();
        assert_ne!(saved.revision, original.revision);
        assert_eq!(
            saved.revision,
            crate::document_store::hash(&fs::read(&path).unwrap())
        );
        assert_eq!(saved.payload.extra["unknown"]["preserve"], 1);
        assert_eq!(saved.payload.statements[0].months, vec![true, true]);
        assert!(!fs::read_to_string(&path).unwrap().contains("reconciled"));
    }
    #[test]
    fn stale_save_and_busy_write_preserve_concurrent_choices() {
        let path = fixture();
        let stale = snapshot_at(&path).unwrap();
        let mut fresh = snapshot_at(&path).unwrap();
        fresh.payload.statements[0].months[1] = true;
        save_at(&path, fresh.payload, &fresh.revision).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(save_at(&path, stale.payload, &stale.revision)
            .unwrap_err()
            .contains("conflict"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let busy = crate::document_store::lock(&path).unwrap();
        let current = snapshot_at(&path).unwrap();
        assert!(save_at(&path, current.payload, &current.revision)
            .unwrap_err()
            .contains("busy"));
        drop(busy);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    #[cfg(windows)]
    #[test]
    fn write_failure_does_not_claim_saved_or_change_persisted_bytes() {
        use std::os::windows::fs::OpenOptionsExt;
        let path = fixture();
        let current = snapshot_at(&path).unwrap();
        let before = fs::read(&path).unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(save_at(&path, current.payload, &current.revision)
            .unwrap_err()
            .contains("publication failed"));
        drop(held);
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
