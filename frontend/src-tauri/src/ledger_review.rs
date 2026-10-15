use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct PendingLedgerTransaction {
    pub date: String,
    pub payee: String,
    pub amount: f64,
    pub category: String,
    pub line_num: usize,
}

fn read_ledger() -> Result<String, String> {
    let bytes = fs::read(crate::config::get("ledger_path"))
        .map_err(|error| format!("failed to read ledger: {error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("ledger is not valid UTF-8: {error}"))
}

fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
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
                "failed to replace ledger atomically: {}",
                std::io::Error::last_os_error()
            ));
        }
        return Ok(());
    }

    #[cfg(not(windows))]
    fs::rename(temp_path, path)
        .map_err(|error| format!("failed to replace ledger atomically: {error}"))
}

fn write_ledger_atomic(contents: &str) -> Result<(), String> {
    let path = Path::new(crate::config::get("ledger_path"));
    let parent = path
        .parent()
        .ok_or_else(|| "ledger path has no parent".to_string())?;
    let temp_path = parent.join("Sovereign.beancount.tmp");

    fs::write(&temp_path, contents.as_bytes())
        .map_err(|error| format!("failed to write ledger temp file: {error}"))?;
    if let Err(error) = replace_file(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
pub fn get_pending_ledger() -> Result<Vec<PendingLedgerTransaction>, String> {
    let ledger = read_ledger()?;
    let lines: Vec<&str> = ledger.split_inclusive('\n').collect();
    let mut pending = Vec::new();

    for (line_num, line) in lines.iter().enumerate() {
        if !line.starts_with("20") || !line.contains(" ! ") {
            continue;
        }

        let Some((date, payee)) = line.split_once(" ! ") else {
            continue;
        };
        let Some(posting) = lines.get(line_num + 1) else {
            continue;
        };
        let posting_parts: Vec<&str> = posting.split_whitespace().collect();
        if posting_parts.len() < 2 {
            continue;
        }
        let Ok(amount) = posting_parts[1].parse::<f64>() else {
            continue;
        };

        pending.push(PendingLedgerTransaction {
            date: date.trim().to_string(),
            payee: payee.trim().trim_matches('"').to_string(),
            amount,
            category: posting_parts[0].to_string(),
            line_num,
        });
    }

    Ok(pending)
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

#[tauri::command]
pub fn approve_ledger_transaction(line_num: usize, category: String) -> Result<(), String> {
    let ledger = read_ledger()?;
    let mut lines: Vec<String> = ledger.split_inclusive('\n').map(str::to_string).collect();
    let transaction = lines
        .get_mut(line_num)
        .ok_or_else(|| "invalid transaction line number".to_string())?;
    if !transaction.contains(" ! ") {
        return Err("transaction is no longer pending".to_string());
    }
    *transaction = transaction.replacen(" ! ", " * ", 1);

    let posting = lines
        .get_mut(line_num + 1)
        .ok_or_else(|| "pending transaction has no posting line".to_string())?;
    let posting_start = posting
        .find(|character: char| !character.is_whitespace())
        .ok_or_else(|| "posting line is empty".to_string())?;
    let category_end = posting[posting_start..]
        .find(char::is_whitespace)
        .map(|offset| posting_start + offset)
        .unwrap_or(posting.len());
    posting.replace_range(posting_start..category_end, &category);

    write_ledger_atomic(&lines.concat())
}
