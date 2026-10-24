use crate::money::Money;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Serialize)]
pub struct PendingLedgerTransaction {
    pub date: String,
    pub payee: String,
    pub amount: Money,
    pub currency: String,
    pub category: String,
    pub line_num: usize,
    pub transaction_id: String,
    pub ledger_revision: String,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_ledger() -> Result<String, String> {
    let bytes = fs::read(crate::config::get("ledger_path"))
        .map_err(|error| format!("failed to read ledger: {error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("ledger is not valid UTF-8: {error}"))
}

#[tauri::command]
pub fn get_pending_ledger() -> Result<Vec<PendingLedgerTransaction>, String> {
    let path = crate::config::get("ledger_path");
    get_pending_ledger_at(path)
}

fn get_pending_ledger_at(path: &str) -> Result<Vec<PendingLedgerTransaction>, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read ledger: {error}"))?;
    let ledger = String::from_utf8(bytes.clone())
        .map_err(|error| format!("ledger is not valid UTF-8: {error}"))?;
    parse_pending_ledger(&ledger, &sha256_hex(&bytes)).map_err(|error| {
        let filename = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        format!("failed to parse {filename} (partial results unavailable): {error}")
    })
}

fn parse_pending_ledger(
    ledger: &str,
    revision: &str,
) -> Result<Vec<PendingLedgerTransaction>, String> {
    if !valid_hash(revision) {
        return Err("invalid ledger revision".into());
    }
    let lines: Vec<&str> = ledger.split_inclusive('\n').collect();
    let mut pending = Vec::new();
    for (line_num, line) in lines.iter().enumerate() {
        if !line.starts_with("20") || !line.contains(" ! ") {
            continue;
        }
        let (date, payee) = line
            .split_once(" ! ")
            .ok_or("malformed pending transaction header")?;
        let end = transaction_block_end(&lines, line_num);
        let block = lines[line_num..end].concat();
        let posting = lines
            .get(line_num + 1)
            .ok_or_else(|| format!("line {}: pending transaction has no posting", line_num + 1))?;
        let posting_parts: Vec<&str> = posting.split_whitespace().collect();
        if posting_parts.len() < 2 {
            continue;
        }
        if posting_parts.len() < 3
            || (posting_parts.len() > 3 && !posting_parts[3].starts_with(';'))
        {
            return Err(format!(
                "line {}: pending posting requires account, amount, and explicit unit",
                line_num + 2
            ));
        }
        let amount = Money::parse(posting_parts[1])
            .map_err(|e| format!("line {}: invalid pending amount: {e}", line_num + 2))?;
        let currency = posting_parts[2];
        if !crate::ledger_parser::valid_currency(currency) {
            return Err(format!(
                "line {}: invalid pending currency unit",
                line_num + 2
            ));
        }
        pending.push(PendingLedgerTransaction {
            date: date.trim().to_string(),
            payee: payee.trim().trim_matches('"').to_string(),
            amount,
            currency: currency.to_string(),
            category: posting_parts[0].to_string(),
            line_num,
            transaction_id: sha256_hex(block.as_bytes()),
            ledger_revision: revision.to_string(),
        });
    }
    Ok(pending)
}

fn transaction_block_end(lines: &[&str], start: usize) -> usize {
    let mut end = start + 1;
    while end < lines.len()
        && !lines[end].trim().is_empty()
        && lines[end].starts_with(char::is_whitespace)
    {
        end += 1;
    }
    end
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}

#[tauri::command]
pub fn get_ledger_categories() -> Result<Vec<String>, String> {
    let ledger = read_ledger()?;
    let mut categories = Vec::new();
    for line in ledger.lines() {
        let Some((_, account)) = line.split_once(" open ") else {
            continue;
        };
        let account = account.split_whitespace().next().unwrap_or_default();
        if account.starts_with("Expenses:") || account.starts_with("Income:") {
            categories.push(account.to_string());
        }
    }
    categories.sort();
    categories.dedup();
    Ok(categories)
}

static APPROVAL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[tauri::command]
pub fn approve_ledger_transaction(
    line_num: usize,
    transaction_id: String,
    ledger_revision: String,
    category: String,
) -> Result<(), String> {
    approve_ledger_transaction_at(
        crate::config::get("ledger_path"),
        line_num,
        &transaction_id,
        &ledger_revision,
        &category,
    )
}

fn approve_ledger_transaction_at(
    path: &str,
    line_num: usize,
    transaction_id: &str,
    ledger_revision: &str,
    category: &str,
) -> Result<(), String> {
    let _guard = APPROVAL_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "ledger approval lock is unavailable".to_string())?;
    if !valid_hash(transaction_id) || !valid_hash(ledger_revision) {
        return Err("missing or malformed transaction identity or ledger revision; refresh pending transactions".into());
    }
    if category.is_empty() || category.chars().any(char::is_whitespace) {
        return Err("category must be a single account token".into());
    }
    #[cfg(not(windows))]
    return Err("ledger approvals are supported only on Windows desktop".into());
    #[cfg(windows)]
    {
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let mut file = options.open(path).map_err(|error| format!("cannot acquire exclusive ledger handle: {error}; close other programs using the ledger and refresh"))?;
        #[cfg(windows)]
        let _range_lock = WindowsRangeLock::acquire(&file)?;
        let mut original = Vec::new();
        file.read_to_end(&mut original)
            .map_err(|error| format!("failed to read ledger: {error}"))?;
        if sha256_hex(&original) != ledger_revision {
            return Err(
                "ledger changed since these rows were listed; refresh pending transactions".into(),
            );
        }
        let text = std::str::from_utf8(&original)
            .map_err(|error| format!("ledger is not valid UTF-8: {error}"))?;
        let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
        let line = lines.get(line_num).ok_or_else(|| {
            "transaction line is no longer present; refresh pending transactions".to_string()
        })?;
        if !line.starts_with("20") || !line.contains(" ! ") {
            return Err("transaction is no longer pending; refresh pending transactions".into());
        }
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let end = transaction_block_end(&line_refs, line_num);
        if sha256_hex(lines[line_num..end].concat().as_bytes()) != transaction_id {
            return Err("transaction content changed; refresh pending transactions".into());
        }
        // S005 owns complex posting selection. Only a direct, well-formed first posting is supported here.
        let posting = lines.get(line_num + 1).ok_or_else(|| {
            "pending transaction has no direct posting; refresh after review".to_string()
        })?;
        if posting.trim().is_empty()
            || posting.trim_start().starts_with(';')
            || !posting.starts_with(char::is_whitespace)
        {
            return Err("pending transaction does not have a supported direct first posting; refresh and review manually".into());
        }
        let posting_parts: Vec<&str> = posting.split_whitespace().collect();
        if posting_parts.len() < 3
            || (posting_parts.len() > 3 && !posting_parts[3].starts_with(';'))
            || !posting_parts[0].contains(':')
        {
            return Err("pending transaction does not have a supported direct first posting; refresh and review manually".into());
        }
        let header = &mut lines[line_num];
        *header = header.replacen(" ! ", " * ", 1);
        let posting = &mut lines[line_num + 1];
        let start = posting
            .find(|character: char| !character.is_whitespace())
            .ok_or("posting line is empty")?;
        let finish = posting[start..]
            .find(char::is_whitespace)
            .map(|offset| start + offset)
            .unwrap_or(posting.len());
        posting.replace_range(start..finish, category);
        let updated = lines.concat().into_bytes();
        if let Err(write_error) = write_through_handle(&mut file, path, &updated, &original) {
            return Err(format!("ledger write failed: {write_error}"));
        }
        Ok(())
    }
}

#[cfg(windows)]
struct WindowsRangeLock {
    handle: windows_sys::Win32::Foundation::HANDLE,
    overlapped: windows_sys::Win32::System::IO::OVERLAPPED,
}

#[cfg(windows)]
impl WindowsRangeLock {
    fn acquire(file: &File) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{LockFileEx, LOCKFILE_EXCLUSIVE_LOCK};
        let mut lock = Self {
            handle: file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
            overlapped: Default::default(),
        };
        let locked = unsafe {
            LockFileEx(
                lock.handle,
                LOCKFILE_EXCLUSIVE_LOCK,
                0,
                u32::MAX,
                u32::MAX,
                &mut lock.overlapped,
            )
        };
        if locked == 0 {
            return Err(format!(
                "cannot lock ledger against existing writers: {}; refresh after other programs finish",
                std::io::Error::last_os_error()
            ));
        }
        Ok(lock)
    }
}

#[cfg(windows)]
impl Drop for WindowsRangeLock {
    fn drop(&mut self) {
        use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
        unsafe {
            UnlockFileEx(self.handle, 0, u32::MAX, u32::MAX, &mut self.overlapped);
        }
    }
}

fn write_through_handle(
    file: &mut File,
    path: &str,
    updated: &[u8],
    original: &[u8],
) -> Result<(), String> {
    write_through_handle_injected(file, path, updated, original, false, false)
}

fn write_through_handle_injected(
    file: &mut File,
    path: &str,
    updated: &[u8],
    original: &[u8],
    fail_backup: bool,
    fail_after_write: bool,
) -> Result<(), String> {
    if fail_backup {
        return Err("cannot persist ledger recovery copy: injected backup failure".into());
    }
    let backup_path = create_recovery_copy(path, original)?;
    let attempt = file
        .seek(SeekFrom::Start(0))
        .and_then(|_| file.write_all(updated))
        .and_then(|_| {
            if fail_after_write {
                Err(std::io::Error::other("injected write failure"))
            } else {
                Ok(())
            }
        })
        .and_then(|_| file.set_len(updated.len() as u64))
        .and_then(|_| file.sync_all());
    if let Err(error) = attempt {
        let restore = file
            .seek(SeekFrom::Start(0))
            .and_then(|_| file.write_all(original))
            .and_then(|_| file.set_len(original.len() as u64))
            .and_then(|_| file.sync_all());
        return Err(match restore {
            Ok(()) => {
                let _ = fs::remove_file(&backup_path);
                format!("{error}; original ledger bytes were restored")
            }
            Err(restore_error) => {
                format!("{error}; restoring original ledger also failed: {restore_error}; recovery copy kept at {}", backup_path.display())
            }
        });
    }
    if let Err(error) = fs::remove_file(&backup_path) {
        eprintln!(
            "ledger write succeeded; recovery copy cleanup failed at {}: {error}",
            backup_path.display()
        );
    }
    Ok(())
}

fn create_recovery_copy(path: &str, original: &[u8]) -> Result<std::path::PathBuf, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT_BACKUP: AtomicUsize = AtomicUsize::new(0);
    let source = Path::new(path);
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ledger");
    for _ in 0..100 {
        let suffix = NEXT_BACKUP.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{name}.recon-recovery-{}-{suffix}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut backup) => {
                if let Err(error) = backup.write_all(original).and_then(|_| backup.sync_all()) {
                    let _ = fs::remove_file(&candidate);
                    return Err(format!("cannot persist ledger recovery copy: {error}"));
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "cannot create ledger recovery copy beside source: {error}"
                ))
            }
        }
    }
    Err("cannot allocate a unique ledger recovery copy beside source".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn fixture(contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "navis-recon-review-{}-{}.beancount",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, contents).unwrap();
        path
    }
    #[test]
    fn pending_parser_preserves_exact_amount_unit_and_line_number() {
        let source = "2026-01-01 ! \"Shop\"\n  Expenses:Food 0.100000000000000001 EUR\n2026-01-02 ! \"Refund\"\n  Assets:Bank -2.5 USD\n";
        let rows = parse_pending_ledger(source, &sha256_hex(source.as_bytes())).unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.amount.to_string())
                .collect::<Vec<_>>(),
            ["0.100000000000000001", "-2.5"]
        );
        assert_eq!(rows[0].currency, "EUR");
        assert_eq!(rows[0].line_num, 0);
        assert!(serde_json::to_string(&rows[0])
            .unwrap()
            .contains("\"amount\":\"0.100000000000000001\""));
    }

    #[test]
    fn pending_parser_reports_invalid_amount_with_source_line() {
        let source = "2026-01-01 ! \"Shop\"\n  Expenses:Food nope EUR\n";
        let err = parse_pending_ledger(source, &sha256_hex(source.as_bytes())).unwrap_err();
        assert!(
            err.contains("line 2") && err.contains("invalid pending amount"),
            "{err}"
        );
    }

    #[test]
    fn pending_parser_uses_shared_currency_grammar() {
        let source = "2026-01-01 ! \"Shop\"\n  Expenses:Food 1 USD\n2026-01-02 ! \"Shop\"\n  Expenses:Food 1 EUR\n2026-01-03 ! \"Shop\"\n  Expenses:Food 1 VTI2\n2026-01-04 ! \"Shop\"\n  Expenses:Food 1 BOND-1\n";
        let rows = parse_pending_ledger(source, &sha256_hex(source.as_bytes())).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.currency.as_str())
                .collect::<Vec<_>>(),
            ["USD", "EUR", "VTI2", "BOND-1"]
        );
        let bad = "2026-01-01 ! \"Shop\"\n  Expenses:Food 1 BAD_unit\n";
        let error = parse_pending_ledger(bad, &sha256_hex(bad.as_bytes())).unwrap_err();
        assert!(
            error.contains("line 2") && error.contains("invalid pending currency"),
            "{error}"
        );
    }

    #[test]
    fn identity_and_revision_are_raw_content_hashes_and_duplicates_have_line_keys() {
        let source = "2026-01-01 ! \"Shop\"\r\n  Expenses:Food 0.100000000000000001 EUR\r\n  Assets:Bank -0.100000000000000001 EUR\r\n2026-01-01 ! \"Shop\"\r\n  Expenses:Food 0.100000000000000001 EUR\r\n  Assets:Bank -0.100000000000000001 EUR\r\n";
        let revision = sha256_hex(source.as_bytes());
        let rows = parse_pending_ledger(source, &revision).unwrap();
        assert_eq!(rows[0].transaction_id, rows[1].transaction_id);
        assert_ne!(rows[0].line_num, rows[1].line_num);
        assert_eq!(rows[0].ledger_revision, revision);
        assert_eq!(rows[0].amount.to_string(), "0.100000000000000001");
    }
    #[test]
    fn production_list_helper_tracks_revision_and_stable_block_across_unrelated_insertion() {
        let source = "2026-01-01 ! \"Shop\"\n  Expenses:Food 1 USD\n";
        let path = fixture(source);
        let before = get_pending_ledger_at(path.to_str().unwrap())
            .unwrap()
            .remove(0);
        let inserted = "; comment\noption \"title\" \"Household\"\n".to_owned() + source;
        fs::write(&path, inserted).unwrap();
        let after = get_pending_ledger_at(path.to_str().unwrap())
            .unwrap()
            .remove(0);
        assert_eq!(before.transaction_id, after.transaction_id);
        assert_ne!(before.ledger_revision, after.ledger_revision);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn independent_posting_edits_and_reordering_conflict_without_mutation() {
        let source = "2026-01-01 ! \"A\"\n  Expenses:A 1 USD\n  Assets:Cash -1 USD\n2026-01-02 ! \"B\"\n  Expenses:B 2 USD\n";
        let path = fixture(source);
        let rows = parse_pending_ledger(source, &sha256_hex(source.as_bytes())).unwrap();
        let mut edited = source.replace("Expenses:A 1", "Expenses:X 1");
        fs::write(&path, &edited).unwrap();
        assert!(approve_ledger_transaction_at(
            path.to_str().unwrap(),
            rows[0].line_num,
            &rows[0].transaction_id,
            &rows[0].ledger_revision,
            "Expenses:Y"
        )
        .is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), edited);
        edited = format!(
            "2026-01-02 ! \"B\"\n  Expenses:B 2 USD\n{}",
            "2026-01-01 ! \"A\"\n  Expenses:A 1 USD\n  Assets:Cash -1 USD\n"
        );
        fs::write(&path, &edited).unwrap();
        assert!(approve_ledger_transaction_at(
            path.to_str().unwrap(),
            rows[0].line_num,
            &rows[0].transaction_id,
            &rows[0].ledger_revision,
            "Expenses:Y"
        )
        .is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), edited);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn injected_write_failure_restores_original_bytes_and_removes_recovery_copy() {
        let original = b"synthetic original bytes\n";
        let path = fixture(std::str::from_utf8(original).unwrap());
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let error = write_through_handle_injected(
            &mut file,
            path.to_str().unwrap(),
            b"partial updated content",
            original,
            false,
            true,
        )
        .unwrap_err();
        assert!(error.contains("original ledger bytes were restored"));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(!fs::read_dir(path.parent().unwrap())
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&format!(
                    ".{}.recon-recovery-",
                    path.file_name().unwrap().to_string_lossy()
                ))));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn injected_backup_failure_rejects_before_changing_source_bytes() {
        let original = b"synthetic original bytes\n";
        let path = fixture(std::str::from_utf8(original).unwrap());
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let error = write_through_handle_injected(
            &mut file,
            path.to_str().unwrap(),
            b"updated content",
            original,
            true,
            false,
        )
        .unwrap_err();
        assert!(error.contains("injected backup failure"));
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn exact_target_change_preserves_other_bytes_and_repeated_approval_conflicts() {
        let original = "; keep\r\n2026-01-01 ! \"Shop\"  ; note\r\n  Expenses:Food 1.00 USD  ; amount\r\n  Assets:Bank -1.00 USD\r\n";
        let path = fixture(original);
        let rev = sha256_hex(original.as_bytes());
        let row = parse_pending_ledger(original, &rev).unwrap().remove(0);
        approve_ledger_transaction_at(
            path.to_str().unwrap(),
            row.line_num,
            &row.transaction_id,
            &rev,
            "Expenses:Other",
        )
        .unwrap();
        let changed = fs::read_to_string(&path).unwrap();
        assert_eq!(changed, "; keep\r\n2026-01-01 * \"Shop\"  ; note\r\n  Expenses:Other 1.00 USD  ; amount\r\n  Assets:Bank -1.00 USD\r\n");
        assert!(approve_ledger_transaction_at(
            path.to_str().unwrap(),
            row.line_num,
            &row.transaction_id,
            &rev,
            "Expenses:Food"
        )
        .unwrap_err()
        .contains("changed since"));
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn insertion_edit_and_bad_tokens_fail_without_mutating_file() {
        let listed = "2026-01-01 ! \"A\"\n  Expenses:A 1 USD\n";
        let path = fixture(listed);
        let rev = sha256_hex(listed.as_bytes());
        let row = parse_pending_ledger(listed, &rev).unwrap().remove(0);
        let changed = format!("2025-12-31 * \"Earlier\"\n  Assets:Cash 0 USD\n{listed}");
        fs::write(&path, &changed).unwrap();
        assert!(approve_ledger_transaction_at(
            path.to_str().unwrap(),
            row.line_num,
            &row.transaction_id,
            &rev,
            "Expenses:B"
        )
        .is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), changed);
        fs::write(&path, listed).unwrap();
        assert!(approve_ledger_transaction_at(
            path.to_str().unwrap(),
            row.line_num,
            "bad",
            &rev,
            "Expenses:B"
        )
        .is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), listed);
        assert!(approve_ledger_transaction_at(
            path.to_str().unwrap(),
            row.line_num,
            &"c".repeat(64),
            &rev,
            "Expenses:B"
        )
        .unwrap_err()
        .contains("transaction content changed"));
        assert_eq!(fs::read_to_string(&path).unwrap(), listed);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn concurrent_distinct_approvals_allow_only_the_first_revision_writer() {
        let source =
            "2026-01-01 ! \"A\"\n  Expenses:A 1 USD\n2026-01-02 ! \"B\"\n  Expenses:B 2 USD\n";
        let path = fixture(source);
        let revision = sha256_hex(source.as_bytes());
        let rows = parse_pending_ledger(source, &revision).unwrap();
        let path_a = path.clone();
        let row_a = (rows[0].transaction_id.clone(), rows[0].line_num);
        let revision_a = revision.clone();
        let first = std::thread::spawn(move || {
            approve_ledger_transaction_at(
                path_a.to_str().unwrap(),
                row_a.1,
                &row_a.0,
                &revision_a,
                "Expenses:ChangedA",
            )
        });
        let path_b = path.clone();
        let row_b = (rows[1].transaction_id.clone(), rows[1].line_num);
        let revision_b = revision.clone();
        let second = std::thread::spawn(move || {
            approve_ledger_transaction_at(
                path_b.to_str().unwrap(),
                row_b.1,
                &row_b.0,
                &revision_b,
                "Expenses:ChangedB",
            )
        });
        let outcomes = [first.join().unwrap(), second.join().unwrap()];
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(outcomes.iter().filter(|result| result.is_err()).count(), 1);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn concurrent_same_row_approvals_allow_only_one_revision_writer() {
        let source = "2026-01-01 ! \"A\"\n  Expenses:A 1 USD\n";
        let path = fixture(source);
        let revision = sha256_hex(source.as_bytes());
        let row = parse_pending_ledger(source, &revision).unwrap().remove(0);
        let mut tasks = Vec::new();
        for category in ["Expenses:B", "Expenses:C"] {
            let path = path.clone();
            let id = row.transaction_id.clone();
            let revision = revision.clone();
            tasks.push(std::thread::spawn(move || {
                approve_ledger_transaction_at(path.to_str().unwrap(), 0, &id, &revision, category)
            }));
        }
        let outcomes: Vec<_> = tasks.into_iter().map(|task| task.join().unwrap()).collect();
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(outcomes.iter().filter(|result| result.is_err()).count(), 1);
        fs::remove_file(path).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn exclusive_source_handle_denies_second_writer() {
        use std::os::windows::fs::OpenOptionsExt;
        let path = fixture("synthetic\n");
        let mut opts = OpenOptions::new();
        opts.read(true).write(true).share_mode(0);
        let held = opts.open(&path).unwrap();
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        drop(held);
        fs::remove_file(path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn range_lock_blocks_write_through_an_already_open_compatible_handle() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        let path = fixture("synthetic\n");
        let mut existing_writer = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(&path)
            .unwrap();
        let locked_handle = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(&path)
            .unwrap();
        let _range_lock = WindowsRangeLock::acquire(&locked_handle).unwrap();
        existing_writer.seek(SeekFrom::Start(0)).unwrap();
        assert!(existing_writer.write_all(b"changed").is_err());
        drop(_range_lock);
        drop(locked_handle);
        drop(existing_writer);
        fs::remove_file(path).unwrap();
    }
}
