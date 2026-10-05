use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Primary path per Vault Protocol:
pub const VAULT_STATEMENTS_PATH: &str = r"G:\My Drive\Navis-Vault\Ledger\Statements";

/// Canonical archive directory used by harvester / historical archive:
pub const FINANCES_STATEMENTS_PATH: &str = r"G:\My Drive\Finances\Statements";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StatementMetadata {
    pub filename: String,
    pub path: String,
    pub relative_path: String,
    pub folder: String,
    pub institution: String,
    pub date: Option<String>,
    pub disambiguation: Option<String>,
    pub is_canonical: bool,
    pub size_bytes: u64,
    pub modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyKind {
    MissingInterval,
    ExactDuplicate,
    DisambiguationCollision,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StatementAnomaly {
    pub institution: String,
    pub kind: AnomalyKind,
    pub message: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub missing_periods: Vec<String>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StatementCatalog {
    pub base_path: String,
    pub total_count: usize,
    pub canonical_count: usize,
    pub statements: Vec<StatementMetadata>,
    pub anomalies: Vec<StatementAnomaly>,
}

/// Resolves the statement repository base directory.
/// Prefers `G:\My Drive\Navis-Vault\Ledger\Statements` if it contains files;
/// falls back to `G:\My Drive\Finances\Statements` if the primary directory is empty or missing.
pub fn resolve_statements_base_dir() -> PathBuf {
    let vault_path = PathBuf::from(VAULT_STATEMENTS_PATH);
    if dir_has_entries(&vault_path) {
        return vault_path;
    }

    let finances_path = PathBuf::from(FINANCES_STATEMENTS_PATH);
    if dir_has_entries(&finances_path) {
        return finances_path;
    }

    // Default to the primary path if neither exists or has files
    vault_path
}

fn dir_has_entries(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }

    if let Ok(mut entries) = fs::read_dir(dir) {
        entries.next().is_some()
    } else {
        false
    }
}

/// Parses the filename adhering to `[Institution] [YYYY-MM-DD][ Disambiguation]?.pdf` conventions.
/// Returns: (institution, date, disambiguation, is_canonical)
pub fn parse_statement_filename(stem: &str) -> (String, Option<String>, Option<String>, bool) {
    let mut date_match: Option<(usize, usize)> = None;
    let bytes = stem.as_bytes();

    // Look for YYYY-MM-DD pattern in the stem
    if bytes.len() >= 10 {
        for i in 0..=(bytes.len() - 10) {
            let slice = &bytes[i..i + 10];
            if slice[0..4].iter().all(u8::is_ascii_digit)
                && slice[4] == b'-'
                && slice[5..7].iter().all(u8::is_ascii_digit)
                && slice[7] == b'-'
                && slice[8..10].iter().all(u8::is_ascii_digit)
            {
                date_match = Some((i, i + 10));
                break;
            }
        }
    }

    if let Some((start, end)) = date_match {
        let date_str = stem[start..end].to_string();
        let prefix = stem[..start].trim().to_string();
        let suffix = stem[end..].trim().to_string();

        let institution = if !prefix.is_empty() {
            prefix
        } else {
            stem.to_string()
        };

        let disambiguation = if !suffix.is_empty() {
            Some(suffix)
        } else {
            None
        };

        // Strictly canonical format is "[Institution] [YYYY-MM-DD]" without extra suffix
        let is_canonical = !institution.is_empty() && disambiguation.is_none() && start > 0;

        (institution, Some(date_str), disambiguation, is_canonical)
    } else {
        // Non-standard filename: keep the full stem as the institution, no date
        (stem.trim().to_string(), None, None, false)
    }
}

/// Detects sequence gaps and collisions/duplicates across the given list of statements.
pub fn detect_statement_anomalies(statements: &[StatementMetadata]) -> Vec<StatementAnomaly> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut anomalies = Vec::new();

    // 1. Group statements by institution
    let mut by_institution: BTreeMap<&str, Vec<&StatementMetadata>> = BTreeMap::new();
    for stmt in statements {
        by_institution
            .entry(stmt.institution.as_str())
            .or_default()
            .push(stmt);
    }

    for (institution, inst_stmts) in by_institution {
        // --- A. Collision / Duplicate Detection ---
        // Group by statement date (if available)
        let mut by_date: BTreeMap<&str, Vec<&StatementMetadata>> = BTreeMap::new();
        for stmt in &inst_stmts {
            if let Some(ref d) = stmt.date {
                by_date.entry(d.as_str()).or_default().push(stmt);
            }
        }

        for (date, date_stmts) in &by_date {
            if date_stmts.len() > 1 {
                // Check if any have identical filename/disambiguation or identical size
                let mut exact_matches = false;
                let first_filename = &date_stmts[0].filename;
                let first_size = date_stmts[0].size_bytes;

                for other in &date_stmts[1..] {
                    if other.filename == *first_filename || other.size_bytes == first_size {
                        exact_matches = true;
                        break;
                    }
                }

                let (kind, message) = if exact_matches {
                    (
                        AnomalyKind::ExactDuplicate,
                        format!(
                            "Duplicate statements detected for {} on {}",
                            institution, date
                        ),
                    )
                } else {
                    (
                        AnomalyKind::DisambiguationCollision,
                        format!(
                            "Multiple statement files found for {} on {} ({})",
                            institution,
                            date,
                            date_stmts
                                .iter()
                                .map(|s| s.filename.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    )
                };

                anomalies.push(StatementAnomaly {
                    institution: institution.to_string(),
                    kind,
                    message,
                    start_date: Some(date.to_string()),
                    end_date: Some(date.to_string()),
                    missing_periods: Vec::new(),
                    files: date_stmts.iter().map(|s| s.filename.clone()).collect(),
                });
            }
        }

        // --- B. Sequence Gap (Missing Calendar Months) Detection ---
        // Extract distinct (year, month) pairs where statements are present
        let mut present_months: BTreeSet<(i32, u32)> = BTreeSet::new();
        for stmt in &inst_stmts {
            if let Some(ref d) = stmt.date {
                if d.len() >= 7 {
                    if let (Ok(y), Ok(m)) = (d[0..4].parse::<i32>(), d[5..7].parse::<u32>()) {
                        if (1..=12).contains(&m) {
                            present_months.insert((y, m));
                        }
                    }
                }
            }
        }

        // Only analyze gaps for institutions with 3 or more statement months spanning at least 3 months
        if present_months.len() >= 3 {
            let (first_year, first_month) = *present_months.iter().next().unwrap();
            let (last_year, last_month) = *present_months.iter().next_back().unwrap();

            let mut cur_year = first_year;
            let mut cur_month = first_month;
            let mut missing_run: Vec<String> = Vec::new();

            while (cur_year, cur_month) <= (last_year, last_month) {
                if !present_months.contains(&(cur_year, cur_month)) {
                    missing_run.push(format!("{:04}-{:02}", cur_year, cur_month));
                } else if !missing_run.is_empty() {
                    let start_date = format!("{}-01", missing_run[0]);
                    let end_date = format!("{}-01", missing_run.last().unwrap());
                    anomalies.push(StatementAnomaly {
                        institution: institution.to_string(),
                        kind: AnomalyKind::MissingInterval,
                        message: format!(
                            "Missing statement period(s) for {}: {}",
                            institution,
                            missing_run.join(", ")
                        ),
                        start_date: Some(start_date),
                        end_date: Some(end_date),
                        missing_periods: missing_run.clone(),
                        files: Vec::new(),
                    });
                    missing_run.clear();
                }

                // Advance one month
                cur_month += 1;
                if cur_month > 12 {
                    cur_month = 1;
                    cur_year += 1;
                }
            }

            // Flush trailing missing run if interval ended before last month (should be handled, but safe)
            if !missing_run.is_empty() {
                let start_date = format!("{}-01", missing_run[0]);
                let end_date = format!("{}-01", missing_run.last().unwrap());
                anomalies.push(StatementAnomaly {
                    institution: institution.to_string(),
                    kind: AnomalyKind::MissingInterval,
                    message: format!(
                        "Missing statement period(s) for {}: {}",
                        institution,
                        missing_run.join(", ")
                    ),
                    start_date: Some(start_date),
                    end_date: Some(end_date),
                    missing_periods: missing_run,
                    files: Vec::new(),
                });
            }
        }
    }

    // Sort anomalies: missing intervals first, then by institution and start date
    anomalies.sort_by(|a, b| {
        a.institution
            .cmp(&b.institution)
            .then_with(|| a.start_date.cmp(&b.start_date))
    });

    anomalies
}

/// Recursively scans the target directory strictly read-only for PDF statements.
pub fn scan_statement_directory(base_dir: &Path) -> StatementCatalog {
    let mut statements = Vec::new();

    if base_dir.is_dir() {
        scan_dir_recursive(base_dir, base_dir, &mut statements);
    }

    // Sort statements deterministically: by date descending, then institution ascending
    statements.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| a.institution.cmp(&b.institution))
            .then_with(|| a.filename.cmp(&b.filename))
    });

    let canonical_count = statements.iter().filter(|s| s.is_canonical).count();
    let total_count = statements.len();
    let anomalies = detect_statement_anomalies(&statements);

    StatementCatalog {
        base_path: base_dir.to_string_lossy().to_string(),
        total_count,
        canonical_count,
        statements,
        anomalies,
    }
}

fn scan_dir_recursive(root_dir: &Path, current_dir: &Path, out: &mut Vec<StatementMetadata>) {
    let entries = match fs::read_dir(current_dir) {
        Ok(entries) => entries,
        Err(_) => return, // Gracefully ignore unreadable directories
    };

    for entry_res in entries {
        let entry = match entry_res {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            scan_dir_recursive(root_dir, &path, out);
        } else if metadata.is_file() {
            let is_pdf = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false);

            if !is_pdf {
                continue;
            }

            let filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string();

            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("");

            let (institution, date, disambiguation, is_canonical) =
                parse_statement_filename(stem);

            let folder = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string();

            let relative_path = path
                .strip_prefix(root_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| filename.clone());

            let size_bytes = metadata.len();
            let modified_ms = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);

            out.push(StatementMetadata {
                filename,
                path: path.to_string_lossy().to_string(),
                relative_path,
                folder,
                institution,
                date,
                disambiguation,
                is_canonical,
                size_bytes,
                modified_ms,
            });
        }
    }
}

/// Tauri IPC command to fetch the statement catalog.
#[tauri::command]
pub fn get_statement_catalog(custom_path: Option<String>) -> Result<StatementCatalog, String> {
    let base_dir = if let Some(path_str) = custom_path {
        let p = PathBuf::from(path_str);
        if !p.exists() {
            return Err(format!("Directory does not exist: {}", p.display()));
        }
        p
    } else {
        resolve_statements_base_dir()
    };

    Ok(scan_statement_directory(&base_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_parse_canonical_statement() {
        let (inst, date, disambig, canonical) =
            parse_statement_filename("Alerus Financial 2026-01-15");
        assert_eq!(inst, "Alerus Financial");
        assert_eq!(date, Some("2026-01-15".to_string()));
        assert_eq!(disambig, None);
        assert!(canonical);
    }

    #[test]
    fn test_parse_disambiguated_statement() {
        let (inst, date, disambig, canonical) =
            parse_statement_filename("Amazon 2026-02-04 B");
        assert_eq!(inst, "Amazon");
        assert_eq!(date, Some("2026-02-04".to_string()));
        assert_eq!(disambig, Some("B".to_string()));
        assert!(!canonical);
    }

    #[test]
    fn test_parse_non_standard_statement() {
        let (inst, date, disambig, canonical) =
            parse_statement_filename("some_weird_filename_without_date");
        assert_eq!(inst, "some_weird_filename_without_date");
        assert_eq!(date, None);
        assert_eq!(disambig, None);
        assert!(!canonical);
    }

    #[test]
    fn test_scan_statement_directory_empty_or_mock() {
        let temp_dir = std::env::temp_dir().join("navis_recon_test_statements");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("Statements 2026")).unwrap();

        let file1 = temp_dir.join("Statements 2026").join("Chase Freedom 2026-03-01.pdf");
        let mut f1 = File::create(&file1).unwrap();
        f1.write_all(b"%PDF-1.4 test").unwrap();

        let file2 = temp_dir.join("Statements 2026").join("Amazon 2026-03-01 B.pdf");
        let mut f2 = File::create(&file2).unwrap();
        f2.write_all(b"%PDF-1.4 test b").unwrap();

        let non_pdf = temp_dir.join("Statements 2026").join("notes.txt");
        let mut fn_pdf = File::create(&non_pdf).unwrap();
        fn_pdf.write_all(b"not a pdf").unwrap();

        let catalog = scan_statement_directory(&temp_dir);
        assert_eq!(catalog.total_count, 2);
        assert_eq!(catalog.canonical_count, 1);
        assert_eq!(catalog.statements[0].institution, "Amazon"); // Both 2026-03-01, Amazon sorted first
        assert_eq!(catalog.statements[1].institution, "Chase Freedom");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_statement_anomalies_gaps_and_duplicates() {
        let stmts = vec![
            StatementMetadata {
                filename: "Chase Freedom 2026-01-15.pdf".to_string(),
                path: "".to_string(),
                relative_path: "".to_string(),
                folder: "".to_string(),
                institution: "Chase Freedom".to_string(),
                date: Some("2026-01-15".to_string()),
                disambiguation: None,
                is_canonical: true,
                size_bytes: 1000,
                modified_ms: None,
            },
            // Missing 2026-02
            StatementMetadata {
                filename: "Chase Freedom 2026-03-15.pdf".to_string(),
                path: "".to_string(),
                relative_path: "".to_string(),
                folder: "".to_string(),
                institution: "Chase Freedom".to_string(),
                date: Some("2026-03-15".to_string()),
                disambiguation: None,
                is_canonical: true,
                size_bytes: 1000,
                modified_ms: None,
            },
            StatementMetadata {
                filename: "Chase Freedom 2026-04-15.pdf".to_string(),
                path: "".to_string(),
                relative_path: "".to_string(),
                folder: "".to_string(),
                institution: "Chase Freedom".to_string(),
                date: Some("2026-04-15".to_string()),
                disambiguation: None,
                is_canonical: true,
                size_bytes: 1000,
                modified_ms: None,
            },
            // Duplicate files on same date
            StatementMetadata {
                filename: "Capital One Venture 2026-01-24.pdf".to_string(),
                path: "".to_string(),
                relative_path: "".to_string(),
                folder: "".to_string(),
                institution: "Capital One Venture".to_string(),
                date: Some("2026-01-24".to_string()),
                disambiguation: None,
                is_canonical: true,
                size_bytes: 2048,
                modified_ms: None,
            },
            StatementMetadata {
                filename: "Capital One Venture 2026-01-24 B.pdf".to_string(),
                path: "".to_string(),
                relative_path: "".to_string(),
                folder: "".to_string(),
                institution: "Capital One Venture".to_string(),
                date: Some("2026-01-24".to_string()),
                disambiguation: Some("B".to_string()),
                is_canonical: false,
                size_bytes: 3000,
                modified_ms: None,
            },
        ];

        let anomalies = detect_statement_anomalies(&stmts);
        assert_eq!(anomalies.len(), 2);

        // Anomaly 1: Capital One Venture disambiguation collision
        let collision = anomalies
            .iter()
            .find(|a| a.institution == "Capital One Venture")
            .unwrap();
        assert_eq!(collision.kind, AnomalyKind::DisambiguationCollision);
        assert_eq!(collision.start_date, Some("2026-01-24".to_string()));
        assert_eq!(collision.files.len(), 2);

        // Anomaly 2: Chase Freedom missing Feb 2026
        let gap = anomalies
            .iter()
            .find(|a| a.institution == "Chase Freedom")
            .unwrap();
        assert_eq!(gap.kind, AnomalyKind::MissingInterval);
        assert_eq!(gap.missing_periods, vec!["2026-02".to_string()]);
    }

    #[test]
    #[ignore = "requires mounted Google Drive path; run explicitly with -- --ignored"]
    fn test_live_directory_scan_benchmark() {
        let base_dir = resolve_statements_base_dir();
        if !base_dir.exists() {
            return;
        }

        let start = std::time::Instant::now();
        let catalog = scan_statement_directory(&base_dir);
        let elapsed = start.elapsed();

        println!(
            "Scanned {} statements ({} canonical, {} anomalies) from {:?} in {:?}",
            catalog.total_count,
            catalog.canonical_count,
            catalog.anomalies.len(),
            base_dir,
            elapsed
        );

        assert!(elapsed.as_secs() < 2, "Scanning should complete within seconds");
    }
}
