use crate::money::Money;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct PendingLedgerTransaction {
    pub date: String,
    pub payee: String,
    pub amount: Money,
    pub currency: String,
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
    parse_pending_ledger(&ledger).map_err(|error| {
        let path = crate::config::get("ledger_path");
        let filename = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        format!("failed to parse {filename} (partial results unavailable): {error}")
    })
}

fn parse_pending_ledger(ledger: &str) -> Result<Vec<PendingLedgerTransaction>, String> {
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
        if posting_parts.len() != 3 {
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
        });
    }

    Ok(pending)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_parser_preserves_exact_amount_unit_and_line_number() {
        let rows = parse_pending_ledger("2026-01-01 ! \"Shop\"\n  Expenses:Food 0.100000000000000001 EUR\n2026-01-02 ! \"Refund\"\n  Assets:Bank -2.5 USD\n").unwrap();
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
        let err =
            parse_pending_ledger("2026-01-01 ! \"Shop\"\n  Expenses:Food nope EUR\n").unwrap_err();
        assert!(
            err.contains("line 2") && err.contains("invalid pending amount"),
            "{err}"
        );
    }

    #[test]
    fn pending_parser_uses_shared_currency_grammar() {
        let rows = parse_pending_ledger(
            "2026-01-01 ! \"Shop\"\n  Expenses:Food 1 USD\n2026-01-02 ! \"Shop\"\n  Expenses:Food 1 EUR\n2026-01-03 ! \"Shop\"\n  Expenses:Food 1 VTI2\n2026-01-04 ! \"Shop\"\n  Expenses:Food 1 BOND-1\n",
        )
        .unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.currency.as_str())
                .collect::<Vec<_>>(),
            ["USD", "EUR", "VTI2", "BOND-1"]
        );

        let error = parse_pending_ledger("2026-01-01 ! \"Shop\"\n  Expenses:Food 1 BAD_unit\n")
            .unwrap_err();
        assert!(
            error.contains("line 2") && error.contains("invalid pending currency"),
            "{error}"
        );
    }
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
