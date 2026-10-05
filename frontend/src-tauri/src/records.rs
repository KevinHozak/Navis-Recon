use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const INVENTORY_PATH: &str = r"G:\My Drive\Navis-Vault\Records\inventory.json";

/// Core schema representing a single indexed household record.
/// Follows "index before storage" rule: pointers to file locations rather than duplicate storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HouseholdRecord {
    pub id: String,
    pub title: String,
    pub category: String,
    pub owner: String,
    #[serde(default, rename = "expirationDate")]
    pub expiration_date: Option<String>,
    #[serde(default, rename = "filePath")]
    pub file_path: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Inventory envelope for local-first persistence.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordInventory {
    #[serde(default = "default_schema_version")]
    pub version: u32,
    #[serde(default)]
    pub records: Vec<HouseholdRecord>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn default_schema_version() -> u32 {
    1
}

/// Validates a record to prevent malformed or invalid entries.
pub fn validate_record(record: &HouseholdRecord) -> Result<(), String> {
    if record.id.trim().is_empty() {
        return Err("Record ID cannot be empty".to_string());
    }
    if record.title.trim().is_empty() {
        return Err("Record Title cannot be empty".to_string());
    }
    if record.category.trim().is_empty() {
        return Err("Record Category cannot be empty".to_string());
    }
    if record.owner.trim().is_empty() {
        return Err("Record Owner cannot be empty".to_string());
    }

    if let Some(ref exp) = record.expiration_date {
        let exp_trimmed = exp.trim();
        if !exp_trimmed.is_empty() && !is_valid_date_format(exp_trimmed) {
            return Err(format!(
                "Invalid expiration date format for '{}'. Expected YYYY-MM-DD",
                exp_trimmed
            ));
        }
    }

    Ok(())
}

fn is_valid_date_format(date_str: &str) -> bool {
    if date_str.len() != 10 {
        return false;
    }
    let bytes = date_str.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    let y = parts[0].parse::<i32>();
    let m = parts[1].parse::<u32>();
    let d = parts[2].parse::<u32>();

    match (y, m, d) {
        (Ok(year), Ok(month), Ok(day)) => {
            if !(1..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
                return false;
            }
            let max_days = match month {
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                4 | 6 | 9 | 11 => 30,
                2 => {
                    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
                    if is_leap {
                        29
                    } else {
                        28
                    }
                }
                _ => return false,
            };
            day <= max_days
        }
        _ => false,
    }
}

fn read_utf8_sig(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read file: {error}"))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    String::from_utf8(bytes.to_vec()).map_err(|error| format!("file is not valid UTF-8: {error}"))
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
            "failed to replace file atomically: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    fs::rename(temp_path, path)
        .map_err(|error| format!("failed to replace file atomically: {error}"))
}

pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    if !parent.exists() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create parent directories: {error}"))?;
    }
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("temp.tmp");
    let temp_path = parent.join(format!("{file_name}.tmp"));

    fs::write(&temp_path, contents)
        .map_err(|error| format!("failed to write temp file: {error}"))?;
    if let Err(error) = replace_file(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    Ok(())
}

pub fn load_inventory_from_path(path: &Path) -> Result<RecordInventory, String> {
    if !path.exists() {
        return Ok(RecordInventory {
            version: 1,
            records: Vec::new(),
            extra: BTreeMap::new(),
        });
    }

    let json = read_utf8_sig(path)?;
    serde_json::from_str(&json).map_err(|error| format!("failed to parse record inventory: {error}"))
}

pub fn save_inventory_to_path(path: &Path, inventory: &RecordInventory) -> Result<(), String> {
    for record in &inventory.records {
        validate_record(record)?;
    }
    let json = serde_json::to_vec_pretty(inventory)
        .map_err(|error| format!("failed to serialize record inventory: {error}"))?;
    write_atomic(path, &json)
}

pub fn inventory_path() -> PathBuf {
    PathBuf::from(INVENTORY_PATH)
}

#[tauri::command]
pub fn get_records() -> Result<Vec<HouseholdRecord>, String> {
    let inventory = load_inventory_from_path(&inventory_path())?;
    Ok(inventory.records)
}

#[tauri::command]
pub fn save_record(record: HouseholdRecord) -> Result<HouseholdRecord, String> {
    validate_record(&record)?;
    let target = inventory_path();
    let mut inventory = load_inventory_from_path(&target)?;

    let mut found = false;
    for existing in inventory.records.iter_mut() {
        if existing.id == record.id {
            *existing = record.clone();
            found = true;
            break;
        }
    }

    if !found {
        inventory.records.push(record.clone());
    }

    save_inventory_to_path(&target, &inventory)?;
    Ok(record)
}

#[tauri::command]
pub fn delete_record(id: String) -> Result<bool, String> {
    let target = inventory_path();
    let mut inventory = load_inventory_from_path(&target)?;
    let initial_len = inventory.records.len();
    inventory.records.retain(|r| r.id != id);

    let removed = inventory.records.len() < initial_len;
    if removed {
        save_inventory_to_path(&target, &inventory)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_validation() {
        let valid = HouseholdRecord {
            id: "rec-001".to_string(),
            title: "Passports".to_string(),
            category: "Identity".to_string(),
            owner: "Kevin".to_string(),
            expiration_date: Some("2030-05-15".to_string()),
            file_path: Some("Vault/Personal/passport.pdf".to_string()),
            notes: Some("Kept in physical safe".to_string()),
            status: Some("Active".to_string()),
            extra: BTreeMap::new(),
        };
        assert!(validate_record(&valid).is_ok());

        let empty_id = HouseholdRecord {
            id: "".to_string(),
            ..valid.clone()
        };
        assert!(validate_record(&empty_id).is_err());

        let empty_title = HouseholdRecord {
            title: "  ".to_string(),
            ..valid.clone()
        };
        assert!(validate_record(&empty_title).is_err());

        let empty_owner = HouseholdRecord {
            owner: "".to_string(),
            ..valid.clone()
        };
        assert!(validate_record(&empty_owner).is_err());

        let invalid_date = HouseholdRecord {
            expiration_date: Some("not-a-date".to_string()),
            ..valid.clone()
        };
        assert!(validate_record(&invalid_date).is_err());

        let invalid_feb = HouseholdRecord {
            expiration_date: Some("2023-02-29".to_string()),
            ..valid.clone()
        };
        assert!(validate_record(&invalid_feb).is_err());

        let valid_leap = HouseholdRecord {
            expiration_date: Some("2024-02-29".to_string()),
            ..valid.clone()
        };
        assert!(validate_record(&valid_leap).is_ok());
    }

    #[test]
    fn test_serialization_camel_case() {
        let record = HouseholdRecord {
            id: "rec-123".to_string(),
            title: "Deed".to_string(),
            category: "Property".to_string(),
            owner: "Family".to_string(),
            expiration_date: Some("2035-01-01".to_string()),
            file_path: Some("Docs/deed.pdf".to_string()),
            notes: None,
            status: Some("Active".to_string()),
            extra: BTreeMap::new(),
        };

        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"expirationDate\":"));
        assert!(json.contains("\"filePath\":"));

        let deserialized: HouseholdRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "rec-123");
        assert_eq!(deserialized.expiration_date, Some("2035-01-01".to_string()));
    }

    #[test]
    fn test_inventory_atomic_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!("navis_recon_test_{}", std::process::id()));
        let inventory_file = temp_dir.join("records").join("inventory.json");

        let loaded = load_inventory_from_path(&inventory_file).unwrap();
        assert_eq!(loaded.records.len(), 0);

        let record1 = HouseholdRecord {
            id: "rec-1".to_string(),
            title: "Car Title".to_string(),
            category: "Vehicles".to_string(),
            owner: "Kevin".to_string(),
            expiration_date: None,
            file_path: Some("Safe/car_title.pdf".to_string()),
            notes: None,
            status: Some("Active".to_string()),
            extra: BTreeMap::new(),
        };

        let mut inv = RecordInventory {
            version: 1,
            records: vec![record1.clone()],
            extra: BTreeMap::new(),
        };

        save_inventory_to_path(&inventory_file, &inv).unwrap();
        assert!(inventory_file.exists());

        let reloaded = load_inventory_from_path(&inventory_file).unwrap();
        assert_eq!(reloaded.records.len(), 1);
        assert_eq!(reloaded.records[0].title, "Car Title");

        let mut record_updated = record1.clone();
        record_updated.notes = Some("Updated note".to_string());
        inv.records[0] = record_updated;
        save_inventory_to_path(&inventory_file, &inv).unwrap();

        let reloaded2 = load_inventory_from_path(&inventory_file).unwrap();
        assert_eq!(reloaded2.records[0].notes, Some("Updated note".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
