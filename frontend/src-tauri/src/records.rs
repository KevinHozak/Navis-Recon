use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::obligations::{add_days, add_months, current_date_utc, days_between, parse_ymd};

pub const VAULT_ROOT: &str = r"G:\My Drive\Navis-Vault";
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
    #[serde(default, rename = "recurrenceRule")]
    pub recurrence_rule: Option<RecurrenceRule>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Recurrence cadences for household document renewals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceInterval {
    Monthly,
    Quarterly,
    SemiAnnual,
    Annual,
    Biennial,      // Every 2 years
    FiveYear,      // Every 5 years
    TenYear,       // Every 10 years (e.g. US Adult Passport)
    CustomYears(u32),
    CustomMonths(u32),
    CustomDays(u32),
}

/// Recurrence rule configuration for proactively rolling or planning renewals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurrenceRule {
    pub interval: RecurrenceInterval,
    #[serde(default, rename = "leadTimeDays")]
    pub lead_time_days: Option<u32>,
}

impl RecurrenceRule {
    #[allow(dead_code)]
    pub fn new(interval: RecurrenceInterval) -> Self {
        Self {
            interval,
            lead_time_days: None,
        }
    }

    /// Calculates the next renewed expiration date given an existing date.
    pub fn project_next_date(&self, from_date: &str) -> Option<String> {
        let _ = parse_ymd(from_date)?;
        match self.interval {
            RecurrenceInterval::Monthly => add_months(from_date, 1),
            RecurrenceInterval::Quarterly => add_months(from_date, 3),
            RecurrenceInterval::SemiAnnual => add_months(from_date, 6),
            RecurrenceInterval::Annual => add_months(from_date, 12),
            RecurrenceInterval::Biennial => add_months(from_date, 24),
            RecurrenceInterval::FiveYear => add_months(from_date, 60),
            RecurrenceInterval::TenYear => add_months(from_date, 120),
            RecurrenceInterval::CustomYears(years) => {
                let months = (years as i32).checked_mul(12)?;
                add_months(from_date, months)
            }
            RecurrenceInterval::CustomMonths(months) => add_months(from_date, months as i32),
            RecurrenceInterval::CustomDays(days) => add_days(from_date, days as i32),
        }
    }
}

/// Time-sensitive health bucket for an indexed household record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordHealthBucket {
    Expired,   // Days remaining < 0
    Critical,  // Due in <= 30 days
    Upcoming,  // Due in <= 90 days (and > 30 days)
    Healthy,   // Due in > 90 days or no expiration
}

impl RecordHealthBucket {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            RecordHealthBucket::Expired => "expired",
            RecordHealthBucket::Critical => "critical",
            RecordHealthBucket::Upcoming => "upcoming",
            RecordHealthBucket::Healthy => "healthy",
        }
    }
}

/// Evaluated radar telemetry for a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordRadarItem {
    pub record: HouseholdRecord,
    pub bucket: RecordHealthBucket,
    #[serde(rename = "daysUntilExpiration")]
    pub days_until_expiration: Option<i32>,
    #[serde(rename = "nextRenewalDate")]
    pub next_renewal_date: Option<String>,
    #[serde(rename = "isActionRequired")]
    pub is_action_required: bool,
}

/// Summary counts across all health buckets for shell dashboards and sidebar badges.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordRadarSummary {
    pub total: usize,
    pub expired: usize,
    pub critical: usize,
    pub upcoming: usize,
    pub healthy: usize,
    #[serde(rename = "actionRequired")]
    pub action_required: usize,
}

/// Full radar inspection payload returned to the Tauri frontend.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordRadarReport {
    pub summary: RecordRadarSummary,
    pub items: Vec<RecordRadarItem>,
    #[serde(rename = "asOfDate")]
    pub as_of_date: String,
}

/// Computes the health bucket and days until expiration for a given expiration date.
pub fn classify_expiration(
    expiration_date: Option<&str>,
    as_of_date: &str,
) -> (RecordHealthBucket, Option<i32>) {
    let exp_str = match expiration_date {
        Some(s) if !s.trim().is_empty() => s.trim(),
        _ => return (RecordHealthBucket::Healthy, None),
    };

    match days_between(as_of_date, exp_str) {
        Some(days) => {
            if days < 0 {
                (RecordHealthBucket::Expired, Some(days))
            } else if days <= 30 {
                (RecordHealthBucket::Critical, Some(days))
            } else if days <= 90 {
                (RecordHealthBucket::Upcoming, Some(days))
            } else {
                (RecordHealthBucket::Healthy, Some(days))
            }
        }
        None => (RecordHealthBucket::Healthy, None),
    }
}

/// Evaluates a single record against an as_of date.
pub fn evaluate_record_radar(record: &HouseholdRecord, as_of_date: &str) -> RecordRadarItem {
    let (bucket, days) = classify_expiration(record.expiration_date.as_deref(), as_of_date);

    // If a custom lead time is specified in the recurrence rule, honor it for actionRequired.
    // Otherwise, default to triggering action on Expired and Critical (<= 30 days).
    let is_action_required = match (days, &record.recurrence_rule) {
        (Some(d), Some(rule)) if rule.lead_time_days.is_some() => {
            let lead_days = rule.lead_time_days.unwrap() as i32;
            d <= lead_days
        }
        _ => matches!(bucket, RecordHealthBucket::Expired | RecordHealthBucket::Critical),
    };

    let next_renewal_date = match (&record.expiration_date, &record.recurrence_rule) {
        (Some(exp), Some(rule)) if !exp.trim().is_empty() => rule.project_next_date(exp.trim()),
        _ => None,
    };

    RecordRadarItem {
        record: record.clone(),
        bucket,
        days_until_expiration: days,
        next_renewal_date,
        is_action_required,
    }
}

/// Generates a complete radar report for a list of records.
pub fn generate_radar_report(records: &[HouseholdRecord], as_of_date: &str) -> RecordRadarReport {
    let mut summary = RecordRadarSummary::default();
    let mut items = Vec::with_capacity(records.len());

    for record in records {
        let item = evaluate_record_radar(record, as_of_date);
        summary.total += 1;
        match item.bucket {
            RecordHealthBucket::Expired => summary.expired += 1,
            RecordHealthBucket::Critical => summary.critical += 1,
            RecordHealthBucket::Upcoming => summary.upcoming += 1,
            RecordHealthBucket::Healthy => summary.healthy += 1,
        }
        if item.is_action_required {
            summary.action_required += 1;
        }
        items.push(item);
    }

    RecordRadarReport {
        summary,
        items,
        as_of_date: as_of_date.to_string(),
    }
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

#[tauri::command]
pub fn get_records_radar(as_of_date: Option<String>) -> Result<RecordRadarReport, String> {
    let inventory = load_inventory_from_path(&inventory_path())?;
    let effective_date = match as_of_date {
        Some(ref d) => {
            let trimmed = d.trim();
            if trimmed.is_empty() {
                current_date_utc()
            } else if !is_valid_date_format(trimmed) {
                return Err(format!(
                    "Invalid as_of_date format for '{}'. Expected YYYY-MM-DD",
                    trimmed
                ));
            } else {
                trimmed.to_string()
            }
        }
        None => current_date_utc(),
    };
    Ok(generate_radar_report(&inventory.records, &effective_date))
}

/// Resolves a candidate path against the Vault root if it is relative or not found.
pub fn resolve_record_path(file_path: &str) -> PathBuf {
    let p = Path::new(file_path);
    if p.is_absolute() && p.exists() {
        return p.to_path_buf();
    }

    let vault_base = Path::new(VAULT_ROOT);
    let trimmed = file_path.trim_start_matches(['/', '\\']);

    // Check directly relative to vault root
    let direct_vault = vault_base.join(trimmed);
    if direct_vault.exists() {
        return direct_vault;
    }

    // Check relative to vault Records folder
    let records_vault = vault_base.join("Records").join(trimmed);
    if records_vault.exists() {
        return records_vault;
    }

    // If still not found, return the absolute path if it was absolute, else direct_vault
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        direct_vault
    }
}

/// Reveals a record file or directory in the native file explorer.
/// If `file_path` points to a file, highlights the file in the containing folder.
/// If it points to a directory, opens that directory.
#[tauri::command]
pub fn reveal_record_file(file_path: String) -> Result<(), String> {
    let resolved = resolve_record_path(&file_path);
    if !resolved.exists() {
        return Err(format!("Path does not exist: {}", file_path));
    }
    let p = &resolved;

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let mut cmd = std::process::Command::new("explorer.exe");
        if p.is_file() {
            cmd.arg(format!("/select,{}", p.display()));
        } else {
            cmd.arg(p.display().to_string());
        }
        cmd.creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open file explorer: {e}"))?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("open");
        if p.is_file() {
            cmd.arg("-R").arg(&file_path);
        } else {
            cmd.arg(&file_path);
        }
        cmd.spawn()
            .map_err(|e| format!("Failed to open file explorer: {e}"))?;
        Ok(())
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let target = if p.is_file() {
            p.parent().unwrap_or(p)
        } else {
            p
        };
        std::process::Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|e| format!("Failed to open file explorer: {e}"))?;
        Ok(())
    }
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
            recurrence_rule: None,
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
            recurrence_rule: None,
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
            recurrence_rule: None,
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

    #[test]
    fn test_classify_expiration_buckets() {
        let as_of = "2026-10-05";

        // Expired (yesterday)
        let (bucket, days) = classify_expiration(Some("2026-10-04"), as_of);
        assert_eq!(bucket, RecordHealthBucket::Expired);
        assert_eq!(days, Some(-1));

        // Critical (due today = 0 days, due in 30 days)
        let (b_today, d_today) = classify_expiration(Some("2026-10-05"), as_of);
        assert_eq!(b_today, RecordHealthBucket::Critical);
        assert_eq!(d_today, Some(0));

        let (b_30, d_30) = classify_expiration(Some("2026-11-04"), as_of);
        assert_eq!(b_30, RecordHealthBucket::Critical);
        assert_eq!(d_30, Some(30));

        // Upcoming (due in 31 days to 90 days)
        let (b_31, d_31) = classify_expiration(Some("2026-11-05"), as_of);
        assert_eq!(b_31, RecordHealthBucket::Upcoming);
        assert_eq!(d_31, Some(31));

        let (b_90, d_90) = classify_expiration(Some("2027-01-03"), as_of);
        assert_eq!(b_90, RecordHealthBucket::Upcoming);
        assert_eq!(d_90, Some(90));

        // Healthy (> 90 days)
        let (b_91, d_91) = classify_expiration(Some("2027-01-04"), as_of);
        assert_eq!(b_91, RecordHealthBucket::Healthy);
        assert_eq!(d_91, Some(91));

        // Healthy (no expiration date or empty)
        let (b_none, d_none) = classify_expiration(None, as_of);
        assert_eq!(b_none, RecordHealthBucket::Healthy);
        assert_eq!(d_none, None);

        let (b_empty, d_empty) = classify_expiration(Some("   "), as_of);
        assert_eq!(b_empty, RecordHealthBucket::Healthy);
        assert_eq!(d_empty, None);
    }

    #[test]
    fn test_recurrence_projections() {
        let annual_rule = RecurrenceRule::new(RecurrenceInterval::Annual);
        assert_eq!(
            annual_rule.project_next_date("2026-10-05"),
            Some("2027-10-05".to_string())
        );

        let passport_rule = RecurrenceRule::new(RecurrenceInterval::TenYear);
        assert_eq!(
            passport_rule.project_next_date("2025-06-15"),
            Some("2035-06-15".to_string())
        );

        let biennial_rule = RecurrenceRule::new(RecurrenceInterval::Biennial);
        assert_eq!(
            biennial_rule.project_next_date("2026-03-01"),
            Some("2028-03-01".to_string())
        );

        let custom_years = RecurrenceRule::new(RecurrenceInterval::CustomYears(3));
        assert_eq!(
            custom_years.project_next_date("2024-01-10"),
            Some("2027-01-10".to_string())
        );

        // Leap year clamping (e.g. Feb 29 -> Feb 28 on non-leap year)
        assert_eq!(
            annual_rule.project_next_date("2024-02-29"),
            Some("2025-02-28".to_string())
        );
    }

    #[test]
    fn test_generate_radar_report_summary() {
        let as_of = "2026-10-05";
        let records = vec![
            HouseholdRecord {
                id: "rec-exp".to_string(),
                title: "Expired Registration".to_string(),
                category: "Vehicles".to_string(),
                owner: "Kevin".to_string(),
                expiration_date: Some("2026-09-01".to_string()),
                file_path: None,
                notes: None,
                status: None,
                recurrence_rule: Some(RecurrenceRule::new(RecurrenceInterval::Annual)),
                extra: BTreeMap::new(),
            },
            HouseholdRecord {
                id: "rec-crit".to_string(),
                title: "Driver License".to_string(),
                category: "Identity".to_string(),
                owner: "Kevin".to_string(),
                expiration_date: Some("2026-10-20".to_string()), // 15 days out
                file_path: None,
                notes: None,
                status: None,
                recurrence_rule: Some(RecurrenceRule::new(RecurrenceInterval::FiveYear)),
                extra: BTreeMap::new(),
            },
            HouseholdRecord {
                id: "rec-up".to_string(),
                title: "Home Insurance Policy".to_string(),
                category: "Insurance".to_string(),
                owner: "Family".to_string(),
                expiration_date: Some("2026-12-01".to_string()), // 57 days out
                file_path: None,
                notes: None,
                status: None,
                recurrence_rule: Some(RecurrenceRule::new(RecurrenceInterval::Annual)),
                extra: BTreeMap::new(),
            },
            HouseholdRecord {
                id: "rec-health".to_string(),
                title: "Passport".to_string(),
                category: "Identity".to_string(),
                owner: "Kevin".to_string(),
                expiration_date: Some("2034-05-01".to_string()),
                file_path: None,
                notes: None,
                status: None,
                recurrence_rule: Some(RecurrenceRule::new(RecurrenceInterval::TenYear)),
                extra: BTreeMap::new(),
            },
            HouseholdRecord {
                id: "rec-none".to_string(),
                title: "Social Security Card".to_string(),
                category: "Identity".to_string(),
                owner: "Kevin".to_string(),
                expiration_date: None,
                file_path: None,
                notes: None,
                status: None,
                recurrence_rule: None,
                extra: BTreeMap::new(),
            },
        ];

        let report = generate_radar_report(&records, as_of);
        assert_eq!(report.summary.total, 5);
        assert_eq!(report.summary.expired, 1);
        assert_eq!(report.summary.critical, 1);
        assert_eq!(report.summary.upcoming, 1);
        assert_eq!(report.summary.healthy, 2); // 1 far date + 1 without date
        assert_eq!(report.summary.action_required, 2); // expired + critical

        // Check renewal projection on item 0
        let exp_item = &report.items[0];
        assert_eq!(exp_item.bucket, RecordHealthBucket::Expired);
        assert_eq!(exp_item.next_renewal_date, Some("2027-09-01".to_string()));
        assert!(exp_item.is_action_required);

        // Check license renewal on item 1
        let crit_item = &report.items[1];
        assert_eq!(crit_item.bucket, RecordHealthBucket::Critical);
        assert_eq!(crit_item.next_renewal_date, Some("2031-10-20".to_string()));
        assert!(crit_item.is_action_required);
    }

    #[test]
    fn test_lead_time_days_action_required() {
        let as_of = "2026-10-05";

        // Record expiring in 45 days with a 60-day lead time -> bucket is Upcoming, but action is required
        let rec_lead_60 = HouseholdRecord {
            id: "rec-lead-60".to_string(),
            title: "Car Insurance Renewal".to_string(),
            category: "Insurance".to_string(),
            owner: "Kevin".to_string(),
            expiration_date: Some("2026-11-19".to_string()), // 45 days out
            file_path: None,
            notes: None,
            status: None,
            recurrence_rule: Some(RecurrenceRule {
                interval: RecurrenceInterval::Annual,
                lead_time_days: Some(60),
            }),
            extra: BTreeMap::new(),
        };

        let item = evaluate_record_radar(&rec_lead_60, as_of);
        assert_eq!(item.bucket, RecordHealthBucket::Upcoming);
        assert_eq!(item.days_until_expiration, Some(45));
        assert!(item.is_action_required);

        // Record expiring in 20 days with a 7-day lead time -> bucket is Critical, but action required is suppressed until 7 days
        let rec_lead_7 = HouseholdRecord {
            id: "rec-lead-7".to_string(),
            title: "Short Notice Item".to_string(),
            category: "Home".to_string(),
            owner: "Kevin".to_string(),
            expiration_date: Some("2026-10-25".to_string()), // 20 days out
            file_path: None,
            notes: None,
            status: None,
            recurrence_rule: Some(RecurrenceRule {
                interval: RecurrenceInterval::Monthly,
                lead_time_days: Some(7),
            }),
            extra: BTreeMap::new(),
        };

        let item_7 = evaluate_record_radar(&rec_lead_7, as_of);
        assert_eq!(item_7.bucket, RecordHealthBucket::Critical);
        assert_eq!(item_7.days_until_expiration, Some(20));
        assert!(!item_7.is_action_required); // 20 > 7
    }

    #[test]
    fn test_as_of_date_validation() {
        // Valid date returns Ok
        let res_valid = get_records_radar(Some("2026-10-05".to_string()));
        assert!(res_valid.is_ok());

        // Empty string defaults to current date (Ok)
        let res_empty = get_records_radar(Some("".to_string()));
        assert!(res_empty.is_ok());

        // None defaults to current date (Ok)
        let res_none = get_records_radar(None);
        assert!(res_none.is_ok());

        // Invalid month/date formats return Err
        let res_invalid_month = get_records_radar(Some("2026-13-01".to_string()));
        assert!(res_invalid_month.is_err());
        assert!(res_invalid_month.unwrap_err().contains("Invalid as_of_date format"));

        let res_malformed = get_records_radar(Some("not-a-date".to_string()));
        assert!(res_malformed.is_err());
        assert!(res_malformed.unwrap_err().contains("Invalid as_of_date format"));
    }

    #[test]
    fn test_reveal_record_file_nonexistent() {
        let res = reveal_record_file("C:\\nonexistent_fake_path_12345\\file.pdf".to_string());
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Path does not exist"));
    }

    #[test]
    fn test_resolve_record_path() {
        let rel_path = "Personal/test_file.pdf";
        let resolved = resolve_record_path(rel_path);
        // Should prepend VAULT_ROOT
        assert!(resolved.to_string_lossy().contains("Navis-Vault"));

        let abs_path = if cfg!(windows) {
            "C:\\SomeFolder\\file.pdf"
        } else {
            "/tmp/file.pdf"
        };
        let resolved_abs = resolve_record_path(abs_path);
        assert_eq!(resolved_abs, PathBuf::from(abs_path));
    }
}
