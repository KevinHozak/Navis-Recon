use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
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

fn read_utf8_sig(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read checklist: {error}"))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    String::from_utf8(bytes.to_vec())
        .map_err(|error| format!("checklist is not valid UTF-8: {error}"))
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = temp_path
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let destination = path
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();

    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if replaced == 0 {
        return Err(format!(
            "failed to replace checklist atomically: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    fs::rename(temp_path, path)
        .map_err(|error| format!("failed to replace checklist atomically: {error}"))
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "checklist path has no parent".to_string())?;
    let temp_path = parent.join("checklist.json.tmp");

    fs::write(&temp_path, contents)
        .map_err(|error| format!("failed to write checklist temp file: {error}"))?;
    if let Err(error) = replace_file(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    Ok(())
}

#[tauri::command]
pub fn get_vault_checklist() -> Result<ChecklistPayload, String> {
    let json = read_utf8_sig(Path::new(crate::config::get("checklist_path")))?;
    serde_json::from_str(&json).map_err(|error| format!("failed to parse checklist: {error}"))
}

#[tauri::command]
pub fn save_vault_checklist(payload: ChecklistPayload) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(&payload)
        .map_err(|error| format!("failed to serialize checklist: {error}"))?;
    write_atomic(Path::new(crate::config::get("checklist_path")), &json)
}
