use crate::money::Money;
use crate::review_audit::{self, Intent};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Serialize)]
pub struct PendingLedgerTransaction {
    pub date: String,
    pub payee: String,
    pub amount: Money,
    pub currency: String,
    pub category: String,
    pub permitted_categories: Vec<String>,
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
    validate_supported_top_level(&lines)?;
    validate_transaction_candidates(&lines)?;
    let account_states = account_lifetimes(&lines)?;
    let mut pending = Vec::new();
    for (line_num, line) in lines.iter().enumerate() {
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some(header) = parse_transaction_header(line, line_num + 1)? else {
            continue;
        };
        if header.flag != '!' {
            continue;
        }
        let end = transaction_block_end(&lines, line_num);
        let block = lines[line_num..end].concat();
        let located = locate_category_posting(&lines, line_num, end)?;
        let permitted_categories =
            eligible_categories(&account_states, &header.date, &located.currency);
        pending.push(PendingLedgerTransaction {
            date: header.date,
            payee: header.payee,
            amount: located.amount,
            currency: located.currency,
            category: located.account,
            permitted_categories,
            line_num,
            transaction_id: sha256_hex(block.as_bytes()),
            ledger_revision: revision.to_string(),
        });
    }
    Ok(pending)
}

fn transaction_block_end(lines: &[&str], start: usize) -> usize {
    let mut end = start + 1;
    while end < lines.len() {
        let body = line_body(lines[end]);
        if is_genuine_top_level_directive(body) {
            break;
        }
        end += 1;
    }
    end
}

fn is_genuine_top_level_directive(line: &str) -> bool {
    if line.starts_with(char::is_whitespace)
        || line.trim().is_empty()
        || line.trim_start().starts_with(';')
    {
        return false;
    }
    let fields: Vec<_> = line.split_whitespace().collect();
    if fields
        .first()
        .is_some_and(|date| crate::ledger_parser::valid_date(date))
        && fields.get(1).is_some_and(|directive| {
            matches!(
                *directive,
                "open"
                    | "close"
                    | "balance"
                    | "commodity"
                    | "pad"
                    | "event"
                    | "price"
                    | "note"
                    | "document"
                    | "query"
                    | "custom"
                    | "txn"
                    | "*"
                    | "!"
            )
        })
    {
        return true;
    }
    matches!(
        fields.first().copied(),
        Some("option" | "operating_currency" | "include" | "plugin")
    )
}

struct TransactionHeader {
    date: String,
    flag: char,
    flag_offset: usize,
    payee: String,
}
fn parse_transaction_header(
    line: &str,
    line_no: usize,
) -> Result<Option<TransactionHeader>, String> {
    let raw = line_body(line);
    let bytes = raw.as_bytes();
    if bytes.len() < 10
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || !bytes[..10].iter().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return Ok(None);
    }
    let date = &raw[..10];
    let mut pos = 10;
    if pos < bytes.len() && (bytes[pos] == b'!' || bytes[pos] == b'*') {
        return Err(format!(
            "line {line_no}: transaction date and flag require whitespace; review manually"
        ));
    }
    if pos == bytes.len() || !bytes[pos].is_ascii_whitespace() {
        return Ok(None);
    }
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    let flag = *bytes.get(pos).ok_or_else(|| {
        format!("line {line_no}: malformed dated transaction header; review manually")
    })?;
    if flag != b'!' && flag != b'*' {
        return Ok(None);
    }
    let flag_offset = pos;
    pos += 1;
    if pos == bytes.len() || !bytes[pos].is_ascii_whitespace() {
        return Err(format!(
            "line {line_no}: transaction flag must be followed by whitespace; review manually"
        ));
    }
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    let mut quoted = Vec::new();
    while bytes.get(pos) == Some(&b'"') && quoted.len() < 2 {
        let start = pos + 1;
        pos = start;
        while pos < bytes.len() && bytes[pos] != b'"' {
            if bytes[pos] == b'\\' {
                return Err(format!(
                    "line {line_no}: escaped header text is unsupported; review manually"
                ));
            }
            pos += 1;
        }
        if pos == bytes.len() {
            return Err(format!(
                "line {line_no}: unterminated header description; review manually"
            ));
        }
        quoted.push(raw[start..pos].to_string());
        pos += 1;
        if bytes.get(pos).is_some_and(u8::is_ascii_whitespace) {
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
        } else if pos < bytes.len() && bytes[pos] != b';' {
            return Err(format!(
                "line {line_no}: unsupported transaction header tail; review manually"
            ));
        }
    }
    if quoted.is_empty() {
        return Err(format!(
            "line {line_no}: transaction description must be quoted; review manually"
        ));
    }
    if pos < bytes.len() && bytes[pos] != b';' {
        return Err(format!(
            "line {line_no}: unsupported transaction header tail; review manually"
        ));
    }
    if !crate::ledger_parser::valid_date(date) {
        if flag == b'!' {
            return Err(format!(
                "line {line_no}: invalid pending transaction date; review manually"
            ));
        }
        return Ok(None);
    }
    let payee = if quoted.len() == 2 {
        format!("{} {}", quoted[0], quoted[1])
    } else {
        quoted.remove(0)
    };
    Ok(Some(TransactionHeader {
        date: date.into(),
        flag: flag as char,
        flag_offset,
        payee,
    }))
}

fn validate_transaction_candidates(lines: &[&str]) -> Result<(), String> {
    for (index, raw) in lines.iter().enumerate() {
        if raw.starts_with(char::is_whitespace) {
            continue;
        }
        let body = line_body(raw)
            .split_once(';')
            .map_or(line_body(raw), |(before, _)| before);
        let mut fields = body.split_whitespace();
        let date = fields.next().unwrap_or_default();
        let marker = fields.next().unwrap_or_default();
        let attached_marker = date.ends_with('!') || date.ends_with('*');
        let date_shaped = date
            .as_bytes()
            .get(..4)
            .is_some_and(|year| year.iter().all(u8::is_ascii_digit))
            && date.as_bytes().get(4) == Some(&b'-');
        if date_shaped && (attached_marker || marker == "!" || marker == "*") {
            let bytes = date.as_bytes();
            let valid_shape = bytes.len() == 10
                && bytes.get(4) == Some(&b'-')
                && bytes.get(7) == Some(&b'-')
                && bytes
                    .iter()
                    .enumerate()
                    .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
            if !valid_shape || attached_marker {
                return Err(format!(
                    "line {}: malformed dated transaction header; review manually",
                    index + 1
                ));
            }
        }
    }
    Ok(())
}

struct LocatedPosting {
    account: String,
    amount: Money,
    currency: String,
    line_index: usize,
    start: usize,
    finish: usize,
}
fn line_body(line: &str) -> &str {
    line.strip_suffix('\n')
        .unwrap_or(line)
        .strip_suffix('\r')
        .unwrap_or(line.strip_suffix('\n').unwrap_or(line))
}
fn locate_category_posting(
    lines: &[&str],
    start: usize,
    end: usize,
) -> Result<LocatedPosting, String> {
    let mut found = Vec::new();
    for index in start + 1..end {
        let raw = line_body(lines[index]);
        let line_no = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            return Err(format!(
                "line {line_no}: unsupported content inside pending transaction; review manually"
            ));
        }
        if is_metadata_line(trimmed) {
            continue;
        }
        if trimmed.starts_with("Expenses:") || trimmed.starts_with("Income:") {
            let tokens: Vec<_> = trimmed.split_whitespace().collect();
            if tokens.len() < 3 || (tokens.len() > 3 && !tokens[3].starts_with(';')) {
                return Err(format!("line {line_no}: category posting needs explicit Money and currency; flags, costs and prices require manual review"));
            }
            if !crate::ledger_parser::account_ok(tokens[0]) {
                return Err(format!(
                    "line {line_no}: malformed category account; review manually"
                ));
            }
            let amount = Money::parse(tokens[1])
                .map_err(|e| format!("line {line_no}: invalid explicit posting amount: {e}"))?;
            if !crate::ledger_parser::valid_currency(tokens[2]) {
                return Err(format!(
                    "line {line_no}: invalid pending currency unit; review manually"
                ));
            }
            found.push(LocatedPosting {
                account: tokens[0].to_string(),
                amount,
                currency: tokens[2].to_string(),
                line_index: index,
                start: raw.find(tokens[0]).unwrap(),
                finish: raw.find(tokens[0]).unwrap() + tokens[0].len(),
            });
        } else if !trimmed.starts_with("Assets:")
            && !trimmed.starts_with("Liabilities:")
            && !trimmed.starts_with("Equity:")
        {
            return Err(format!(
                "line {line_no}: unsupported transaction structure; review manually"
            ));
        } else {
            let tokens: Vec<_> = trimmed.split_whitespace().collect();
            if tokens.len() < 3
                || (tokens.len() > 3 && !tokens[3].starts_with(';'))
                || !crate::ledger_parser::account_ok(tokens[0])
                || Money::parse(tokens[1]).is_err()
                || !crate::ledger_parser::valid_currency(tokens[2])
            {
                return Err(format!("line {line_no}: unsupported posting (explicit Money and unit required; annotations require manual review)"));
            }
        }
    }
    if found.len() != 1 {
        return Err(format!("line {}: pending transaction has {} category postings; exactly one is required. Review manually.", start + 1, found.len()));
    }
    Ok(found.remove(0))
}

#[derive(Clone)]
struct AccountLifetime {
    account: String,
    opened: String,
    closed: Option<String>,
    unit: Option<String>,
}
fn account_lifetimes(lines: &[&str]) -> Result<Vec<AccountLifetime>, String> {
    let mut events = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        if raw.starts_with(char::is_whitespace)
            || raw.trim().is_empty()
            || raw.trim_start().starts_with(';')
        {
            continue;
        }
        let body = line_body(raw)
            .split_once(';')
            .map_or(line_body(raw), |(before, _)| before);
        let fields: Vec<_> = body.split_whitespace().collect();
        if fields.len() >= 2 && (fields[1] == "open" || fields[1] == "close") {
            if fields.len() < 3
                || fields.len() > if fields[1] == "open" { 4 } else { 3 }
                || !crate::ledger_parser::valid_date(fields[0])
                || !crate::ledger_parser::account_ok(fields[2])
                || (fields.len() == 4 && !crate::ledger_parser::valid_currency(fields[3]))
            {
                return Err(format!(
                    "line {}: unsupported category open/close directive; manual review required",
                    i + 1
                ));
            }
            if !matches!(fields[2].split(':').next(), Some("Expenses" | "Income")) {
                continue;
            }
            events.push((
                fields[0].to_string(),
                fields[1].to_string(),
                fields[2].to_string(),
                fields.get(3).map(|v| (*v).to_string()),
                i + 1,
            ));
        }
    }
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.4.cmp(&b.4)));
    let mut same_day = std::collections::HashMap::new();
    for event in &events {
        if let Some(previous_line) = same_day.insert((event.2.as_str(), event.0.as_str()), event.4)
        {
            return Err(format!("lines {previous_line} and {}: same-date category lifecycle events are ambiguous; manual review required", event.4));
        }
    }
    let mut out: Vec<AccountLifetime> = Vec::new();
    for (date, action, account, unit, line_no) in events {
        if action == "open" {
            if out
                .iter()
                .any(|life| life.account == account && life.closed.is_none())
            {
                return Err(format!(
                    "line {line_no}: category account is opened more than once without a close"
                ));
            }
            out.push(AccountLifetime {
                account,
                opened: date,
                closed: None,
                unit,
            });
        } else if let Some(life) = out
            .iter_mut()
            .rev()
            .find(|life| life.account == account && life.closed.is_none())
        {
            if date <= life.opened {
                return Err(format!(
                    "line {line_no}: category account closes before or on its open date"
                ));
            }
            life.closed = Some(date);
        } else {
            return Err(format!(
                "line {line_no}: close has no matching open category"
            ));
        }
    }
    Ok(out)
}
fn eligible_categories(lifetimes: &[AccountLifetime], date: &str, unit: &str) -> Vec<String> {
    let mut values: Vec<_> = lifetimes
        .iter()
        .filter(|life| {
            matches!(life.account.split(':').next(), Some("Expenses" | "Income"))
                && life.opened.as_str() <= date
                && life.closed.as_deref().is_none_or(|closed| date < closed)
                && life.unit.as_deref().is_none_or(|allowed| allowed == unit)
        })
        .map(|life| life.account.clone())
        .collect();
    values.sort();
    values.dedup();
    values
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}

fn is_metadata_line(line: &str) -> bool {
    let Some((key, value)) = line.split_once(':') else {
        return false;
    };
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        && value.starts_with(char::is_whitespace)
        && supported_metadata_scalar(value.trim())
}

fn supported_metadata_scalar(value: &str) -> bool {
    let value = value.trim();
    let (scalar, comment) = if value.starts_with('"') {
        let Some(end) = value[1..].find('"').map(|offset| offset + 1) else {
            return false;
        };
        let scalar = &value[..=end];
        if value[1..end].contains('\\')
            || value[end + 1..].contains('"')
            || value[end + 1..].contains('\\')
        {
            return false;
        }
        (scalar, value[end + 1..].trim())
    } else if let Some(index) = value.find(" ;") {
        (&value[..index], value[index + 1..].trim())
    } else {
        (value, "")
    };
    if !comment.is_empty() && !comment.starts_with(';') {
        return false;
    }
    if scalar.starts_with('"') {
        return scalar.ends_with('"');
    }
    scalar == "TRUE"
        || scalar == "FALSE"
        || crate::ledger_parser::valid_date(scalar)
        || Money::parse(scalar).is_ok()
        || (scalar.starts_with('#')
            && scalar.len() > 1
            && scalar[1..]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

fn validate_supported_top_level(lines: &[&str]) -> Result<(), String> {
    for (i, raw) in lines.iter().enumerate() {
        let body = line_body(raw);
        let trimmed = body.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || body.starts_with(char::is_whitespace) {
            continue;
        }
        let first = trimmed.split_whitespace().next().unwrap_or_default();
        if matches!(
            first,
            "include" | "plugin" | "pushtag" | "poptag" | "pushmeta" | "popmeta"
        ) {
            return Err(format!("line {}: unsupported top-level directive can affect ledger review; manual review required", i + 1));
        }
        if matches!(first, "open" | "close") {
            return Err(format!("line {}: lifecycle directive requires a valid dated account; manual review required", i + 1));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_ledger_categories() -> Result<Vec<String>, String> {
    let ledger = read_ledger()?;
    let lines: Vec<_> = ledger.split_inclusive('\n').collect();
    validate_supported_top_level(&lines)?;
    let lifetimes = account_lifetimes(&lines)?;
    let mut categories: Vec<_> = lifetimes
        .iter()
        .filter(|life| matches!(life.account.split(':').next(), Some("Expenses" | "Income")))
        .map(|life| life.account.clone())
        .collect();
    categories.sort();
    categories.dedup();
    Ok(categories)
}

static APPROVAL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[tauri::command]
pub fn get_ledger_approval_history() -> Result<Vec<review_audit::DisplayRecord>, String> {
    let path = crate::config::get("ledger_path");
    let (ns, dir) = review_audit::namespace(path)?;
    get_history_with_store(path, &dir, &ns)
}

fn get_history_with_store(
    path: &str,
    dir: &Path,
    ns: &str,
) -> Result<Vec<review_audit::DisplayRecord>, String> {
    let _guard = APPROVAL_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "ledger approval lock is unavailable".to_string())?;
    let mut file = exclusive_ledger(path)?;
    #[cfg(windows)]
    let _range = WindowsRangeLock::acquire(&file)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    review_audit::reconcile(dir, ns, &sha256_hex(&bytes))?;
    let records = review_audit::read_records(dir, ns)?;
    #[cfg(windows)]
    drop(_range);
    drop(file);
    Ok(review_audit::display(&records))
}

fn exclusive_ledger(path: &str) -> Result<File, String> {
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("ledger review is supported only on Windows desktop".into())
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let file=OpenOptions::new().read(true).write(true).share_mode(0).open(path).map_err(|e|format!("cannot acquire exclusive ledger handle: {e}; close other programs using the ledger"))?;
        Ok(file)
    }
}

#[tauri::command]
pub fn undo_ledger_approval(record_id: String) -> Result<(), String> {
    let path = crate::config::get("ledger_path");
    let (ns, dir) = review_audit::namespace(path)?;
    let _guard = APPROVAL_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "ledger approval lock is unavailable".to_string())?;
    undo_ledger_approval_with_store(path, &record_id, &dir, &ns)
}

fn undo_ledger_approval_with_store(
    path: &str,
    id: &str,
    dir: &Path,
    ns: &str,
) -> Result<(), String> {
    undo_ledger_approval_with_fault(path, id, dir, ns, review_audit::FaultPlan::NONE)
}

fn undo_ledger_approval_with_fault(
    path: &str,
    id: &str,
    dir: &Path,
    ns: &str,
    faults: review_audit::FaultPlan,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (path, id, dir, ns);
        return Err("ledger review is supported only on Windows desktop".into());
    }
    #[cfg(windows)]
    {
        let mut file = exclusive_ledger(path)?;
        let _range_lock = WindowsRangeLock::acquire(&file)?;
        let mut original = Vec::new();
        file.read_to_end(&mut original).map_err(|e| e.to_string())?;
        review_audit::reconcile(dir, ns, &sha256_hex(&original))?;
        let records = review_audit::read_records(dir, ns)?;
        let (record, _) = records
            .iter()
            .find(|(r, _)| r.operation_id == id && r.operation_type == "approve")
            .ok_or("approval history record was not found")?;
        if records
            .iter()
            .any(|(r, _)| r.undo_of.as_deref() == Some(id))
        {
            return Err("this approval was already undone".into());
        }
        if !valid_approval_transition(record) {
            return Err(
                "approval history transition is invalid; manual recovery review required".into(),
            );
        }
        let text = std::str::from_utf8(&original).map_err(|e| e.to_string())?;
        let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
        let candidates: Vec<usize> = (0..lines.len())
            .filter(|&i| {
                parse_transaction_header(&lines[i], i + 1)
                    .ok()
                    .flatten()
                    .is_some_and(|h| h.flag == '*')
            })
            .collect();
        let matches: Vec<(usize, usize)> = candidates
            .into_iter()
            .filter_map(|start| {
                let refs: Vec<_> = lines.iter().map(String::as_str).collect();
                let end = transaction_block_end(&refs, start);
                (lines[start..end].concat() == record.transaction_after).then_some((start, end))
            })
            .collect();
        if matches.len() != 1 {
            return Err("undo conflict: recorded transaction is missing, duplicated, or edited; refresh and review manually".into());
        }
        let (start, end) = matches[0];
        let before = lines[start..end].concat();
        let restored = record.transaction_before.clone();
        lines.splice(
            start..end,
            restored.split_inclusive('\n').map(str::to_string),
        );
        let updated = lines.concat().into_bytes();
        let intent = Intent {
            schema_version: 1,
            operation_id: format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            operation_type: "undo".into(),
            ledger_namespace: ns.into(),
            source_revision_before: sha256_hex(&original),
            source_revision_after: sha256_hex(&updated),
            transaction_before_hash: sha256_hex(before.as_bytes()),
            transaction_after_hash: sha256_hex(restored.as_bytes()),
            transaction_before: before,
            transaction_after: restored,
            old_category: record.new_category.clone(),
            new_category: record.old_category.clone(),
            created_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            undo_of: Some(id.into()),
        };
        audited_write(&mut file, path, &updated, &original, dir, &intent, faults)
    }
}

fn valid_approval_transition(r: &review_audit::Record) -> bool {
    if r.operation_type != "approve"
        || r.transaction_before_hash != sha256_hex(r.transaction_before.as_bytes())
        || r.transaction_after_hash != sha256_hex(r.transaction_after.as_bytes())
    {
        return false;
    }
    let a: Vec<_> = r.transaction_before.split_inclusive('\n').collect();
    let b: Vec<_> = r.transaction_after.split_inclusive('\n').collect();
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let Some(h) = parse_transaction_header(a[0], 1).ok().flatten() else {
        return false;
    };
    if a.iter().skip(1).enumerate().any(|(index, line)| {
        parse_transaction_header(line, index + 2)
            .ok()
            .flatten()
            .is_some()
    }) {
        return false;
    }
    if h.flag != '!' {
        return false;
    }
    let mut expected: Vec<String> = a.iter().map(|s| s.to_string()).collect();
    expected[0].replace_range(h.flag_offset..h.flag_offset + 1, "*");
    let refs: Vec<_> = a.iter().copied().collect();
    let end = transaction_block_end(&refs, 0);
    let Ok(loc) = locate_category_posting(&refs, 0, end) else {
        return false;
    };
    if loc.account != r.old_category {
        return false;
    }
    expected[loc.line_index].replace_range(loc.start..loc.finish, &r.new_category);
    expected.concat() == r.transaction_after
}

pub(crate) fn valid_audit_transition(i: &Intent) -> bool {
    let (before, after, old, new) = if i.operation_type == "approve" {
        (
            &i.transaction_before,
            &i.transaction_after,
            &i.old_category,
            &i.new_category,
        )
    } else if i.operation_type == "undo" {
        (
            &i.transaction_after,
            &i.transaction_before,
            &i.new_category,
            &i.old_category,
        )
    } else {
        return false;
    };
    let r = review_audit::Record {
        schema_version: i.schema_version,
        operation_id: i.operation_id.clone(),
        operation_type: "approve".into(),
        ledger_namespace: i.ledger_namespace.clone(),
        source_revision_before: i.source_revision_before.clone(),
        source_revision_after: i.source_revision_after.clone(),
        transaction_before: before.clone(),
        transaction_after: after.clone(),
        transaction_before_hash: sha256_hex(before.as_bytes()),
        transaction_after_hash: sha256_hex(after.as_bytes()),
        old_category: old.clone(),
        new_category: new.clone(),
        created_at_unix_ms: i.created_at_unix_ms,
        committed_at_unix_ms: i.created_at_unix_ms,
        undo_of: None,
    };
    valid_account_token(old) && valid_account_token(new) && valid_approval_transition(&r)
}

fn valid_account_token(s: &str) -> bool {
    crate::ledger_parser::account_ok(s)
        && !s.chars().any(|c| matches!(c, '\r' | '\n' | '\0'))
        && matches!(s.split(':').next(), Some("Expenses" | "Income"))
}

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
    #[cfg(test)]
    let (namespace, audit_dir) = review_audit::test_store(path);
    #[cfg(not(test))]
    let (namespace, audit_dir) = review_audit::namespace(path)?;
    approve_ledger_transaction_with_store(
        path,
        line_num,
        transaction_id,
        ledger_revision,
        category,
        &audit_dir,
        &namespace,
    )
}

fn approve_ledger_transaction_with_store(
    path: &str,
    line_num: usize,
    transaction_id: &str,
    ledger_revision: &str,
    category: &str,
    audit_dir: &Path,
    namespace: &str,
) -> Result<(), String> {
    approve_ledger_transaction_with_fault(
        path,
        line_num,
        transaction_id,
        ledger_revision,
        category,
        audit_dir,
        namespace,
        review_audit::FaultPlan::NONE,
    )
}

fn approve_ledger_transaction_with_fault(
    path: &str,
    line_num: usize,
    transaction_id: &str,
    ledger_revision: &str,
    category: &str,
    audit_dir: &Path,
    namespace: &str,
    faults: review_audit::FaultPlan,
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
        review_audit::reconcile(audit_dir, namespace, &sha256_hex(&original))?;
        if sha256_hex(&original) != ledger_revision {
            return Err(
                "ledger changed since these rows were listed; refresh pending transactions".into(),
            );
        }
        let text = std::str::from_utf8(&original)
            .map_err(|error| format!("ledger is not valid UTF-8: {error}"))?;
        let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
        if lines.get(line_num).is_none() {
            return Err(
                "transaction line is no longer present; refresh pending transactions".to_string(),
            );
        }
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        validate_supported_top_level(&line_refs)?;
        let transaction_header = parse_transaction_header(&lines[line_num], line_num + 1)?;
        let Some(transaction_header) = transaction_header.filter(|header| header.flag == '!')
        else {
            return Err("transaction is no longer pending; refresh pending transactions".into());
        };
        let end = transaction_block_end(&line_refs, line_num);
        if sha256_hex(lines[line_num..end].concat().as_bytes()) != transaction_id {
            return Err("transaction content changed; refresh pending transactions".into());
        }
        let before_tx = lines[line_num..end].concat();
        let located = locate_category_posting(&line_refs, line_num, end)?;
        if !crate::ledger_parser::account_ok(category)
            || !matches!(category.split(':').next(), Some("Expenses" | "Income"))
        {
            return Err("selected category is not a valid Expenses or Income account; refresh and review manually".into());
        }
        let lifetimes = account_lifetimes(&line_refs)?;
        if !eligible_categories(&lifetimes, &transaction_header.date, &located.currency)
            .iter()
            .any(|allowed| allowed == category)
        {
            return Err("selected category is not open and permitted for this transaction date and currency; refresh and choose an eligible category".into());
        }
        drop(line_refs);
        let header = &mut lines[line_num];
        header.replace_range(
            transaction_header.flag_offset..transaction_header.flag_offset + 1,
            "*",
        );
        let target = &mut lines[located.line_index];
        target.replace_range(located.start..located.finish, category);
        let updated = lines.concat().into_bytes();
        let after_text = String::from_utf8(updated.clone()).map_err(|e| e.to_string())?;
        let after_lines: Vec<_> = after_text.split_inclusive('\n').collect();
        let after_end = transaction_block_end(&after_lines, line_num);
        let after_tx = after_lines[line_num..after_end].concat();
        let operation_id = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let intent = Intent {
            schema_version: 1,
            operation_id,
            operation_type: "approve".into(),
            ledger_namespace: namespace.into(),
            source_revision_before: sha256_hex(&original),
            source_revision_after: sha256_hex(&updated),
            transaction_before_hash: sha256_hex(before_tx.as_bytes()),
            transaction_after_hash: sha256_hex(after_tx.as_bytes()),
            transaction_before: before_tx,
            transaction_after: after_tx,
            old_category: located.account,
            new_category: category.into(),
            created_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            undo_of: None,
        };
        audited_write(
            &mut file, path, &updated, &original, audit_dir, &intent, faults,
        )
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

fn write_through_handle_injected(
    file: &mut File,
    path: &str,
    updated: &[u8],
    original: &[u8],
    fail_backup: bool,
    fail_after_write: bool,
) -> Result<PathBuf, String> {
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
        let restore = restore_source(file, original, false, false);
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
    // The caller owns this synced whole-ledger copy until the audit commit succeeds.
    Ok(backup_path)
}

fn restore_source(
    file: &mut File,
    original: &[u8],
    inject_failure: bool,
    partial_failure: bool,
) -> std::io::Result<()> {
    if inject_failure {
        return Err(std::io::Error::other("injected restoration failure"));
    }
    if partial_failure {
        let mut current = Vec::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_end(&mut current)?;
        let changed = original
            .iter()
            .zip(current.iter())
            .position(|(before, after)| before != after)
            .ok_or_else(|| {
                std::io::Error::other("injected partial restoration had no changed byte")
            })?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&original[..=changed])?;
        return Err(std::io::Error::other(
            "injected restoration failure after partial write",
        ));
    }
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.write_all(original))
        .and_then(|_| file.set_len(original.len() as u64))
        .and_then(|_| file.sync_all())
}

#[cfg(windows)]
fn audited_write(
    file: &mut File,
    source_path: &str,
    updated: &[u8],
    original: &[u8],
    audit_dir: &Path,
    intent: &review_audit::Intent,
    faults: review_audit::FaultPlan,
) -> Result<(), String> {
    let intent_path = review_audit::prepare_fault(audit_dir, intent, faults.intent)
        .map_err(|e| format!("intent preparation failed: {e}"))?;
    if faults.stop_after_prepare {
        return Err(format!(
            "injected stop after prepared intent: {}",
            intent_path.display()
        ));
    }
    let backup_path = match write_through_handle_injected(
        file,
        source_path,
        updated,
        original,
        false,
        faults.fail_source_after_write,
    ) {
        Ok(path) => path,
        Err(error) => {
            if error.contains("original ledger bytes were restored") {
                if !faults.fail_intent_cleanup {
                    let _ = review_audit::remove_intent(&intent_path);
                }
            } else {
                return Err(format!(
                    "ledger write outcome uncertain; recovery required. {}",
                    error
                ));
            }
            return Err(format!("source write failed: {error}"));
        }
    };
    if faults.stop_after_source {
        return Err(format!(
            "injected stop after source write; recovery copy retained at {}",
            backup_path.display()
        ));
    }
    if let Err(commit_error) = review_audit::commit_fault(audit_dir, intent, faults.commit) {
        let restore = restore_source(
            file,
            original,
            faults.fail_restore,
            faults.fail_restore_after_partial,
        );
        return match restore {
            Ok(()) => {
                let _ = fs::remove_file(&backup_path);
                Err(format!("audit commit failed after source write; original bytes restored and synced; recovery intent retained: {commit_error}"))
            }
            Err(restore_error) => Err(format!("outcome uncertain; recovery required. Audit commit failed ({commit_error}); restoration failed ({restore_error}); complete original ledger retained at {} and intent at {}. Inspect manually; history recovery will not rewrite the ledger.", backup_path.display(), intent_path.display())),
        };
    }
    let intent_cleaned = if faults.fail_intent_cleanup {
        false
    } else {
        match review_audit::remove_intent(&intent_path) {
            Ok(()) => true,
            Err(error) => {
                eprintln!(
                    "approval committed; intent cleanup pending at {}: {error}",
                    intent_path.display()
                );
                false
            }
        }
    };
    // Keep the owned source backup whenever its intent remains, so recovery evidence
    // remains complete if the process stops before the next reconciliation.
    if intent_cleaned {
        if let Err(error) = fs::remove_file(&backup_path) {
            eprintln!(
                "approval committed; owned recovery copy cleanup pending at {}: {error}",
                backup_path.display()
            );
        }
    } else {
        eprintln!(
            "approval committed; recovery copy retained at {}",
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
        let source = "2026-01-01 ! \"Shop\"\n  Expenses:Food 0.100000000000000001 EUR\n  Assets:Bank -0.100000000000000001 EUR\n2026-01-02 ! \"Refund\"\n  Income:Refund -2.5 USD\n  Assets:Bank 2.5 USD\n";
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
        assert!(serde_json::to_string(&rows[0])
            .unwrap()
            .contains("\"permitted_categories\":["));
    }

    #[test]
    fn header_flags_are_parsed_outside_quotes_and_comments() {
        let source = "2025-01-01 open Expenses:Food USD ; ! directive comment\n2026-01-01 * \"Shop ! promo\" ; approved !\n2026-01-02\t\t!\t\"Store; Cafe\" \"Meal\" ; note !\n  Expenses:Food 1 USD\n";
        let rows = parse_pending_ledger(source, &sha256_hex(source.as_bytes())).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, "2026-01-02");
        assert_eq!(rows[0].payee, "Store; Cafe Meal");
        let malformed = "2026-02-31 ! \"bad date\"\n  Expenses:Food 1 USD\n";
        assert!(
            parse_pending_ledger(malformed, &sha256_hex(malformed.as_bytes()))
                .unwrap_err()
                .contains("invalid pending transaction date")
        );
        let unsupported = "2026-01-01 ! \"Shop\" #unsupported\n  Expenses:Food 1 USD\n";
        assert!(
            parse_pending_ledger(unsupported, &sha256_hex(unsupported.as_bytes()))
                .unwrap_err()
                .contains("unsupported transaction header tail")
        );
    }

    #[test]
    fn pending_split_posting_after_column_zero_comment_is_rejected() {
        let source = "2025-01-01 open Expenses:Food USD\n2026-01-01 ! \"Shop\"\n  Expenses:Food 1 USD\n; separator\n  Expenses:Food 2 USD\n";
        let error = parse_pending_ledger(source, &sha256_hex(source.as_bytes())).unwrap_err();
        assert!(error.contains("exactly one"), "{error}");
    }

    #[test]
    fn metadata_requires_lowercase_key_and_supported_scalar() {
        for metadata in ["Foo:Bank 1 USD", "note: unsupported value"] {
            let source = format!("2026-01-01 ! \"Shop\"\n  {metadata}\n  Expenses:Food 1 USD\n");
            let error = parse_pending_ledger(&source, &sha256_hex(source.as_bytes())).unwrap_err();
            assert!(
                error.contains("unsupported transaction structure"),
                "{error}"
            );
        }
        let source =
            "2026-01-01 ! \"Shop\"\n  note: \"memo: value; retained\"\n  Expenses:Food 1 USD\n";
        assert!(parse_pending_ledger(source, &sha256_hex(source.as_bytes())).is_ok());
    }

    #[test]
    fn metadata_scalar_subset_preserves_quoted_comments_and_rejects_unsupported_values() {
        let source = "2025-01-01 open Expenses:Food USD\n2026-01-01 ! \"Shop\"\n  note: \"source: x; y\" ; keep comment\n  Expenses:Food 1 USD\n";
        let path = fixture(source);
        let rows = get_pending_ledger_at(path.to_str().unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        #[cfg(windows)]
        {
            let row = &rows[0];
            approve_ledger_transaction_at(
                path.to_str().unwrap(),
                row.line_num,
                &row.transaction_id,
                &row.ledger_revision,
                "Expenses:Food",
            )
            .unwrap();
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                source.replace(" ! ", " * ")
            );
        }
        fs::write(&path, source).unwrap();
        for scalar in [
            "NaN",
            "inf",
            "1e3",
            "#tag extra",
            "#bad!tag",
            "\"unterminated",
            "\"bad\\\\escape\"",
        ] {
            let invalid = format!("2025-01-01 open Expenses:Food USD\n2026-01-01 ! \"Shop\"\n  note: {scalar}\n  Expenses:Food 1 USD\n");
            fs::write(&path, &invalid).unwrap();
            let error = get_pending_ledger_at(path.to_str().unwrap()).unwrap_err();
            assert!(
                error.contains("unsupported transaction structure"),
                "{scalar}: {error}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn interleaved_same_date_lifecycle_events_fail_production_listing() {
        let source = "2024-01-01 open Expenses:Food\n2026-01-01 close Expenses:Food\n2026-01-01 open Income:Salary\n2026-01-01 open Expenses:Food\n2026-02-01 ! \"Shop\"\n  Expenses:Food 1 USD\n";
        let path = fixture(source);
        let error = get_pending_ledger_at(path.to_str().unwrap()).unwrap_err();
        assert!(
            error.contains("lines 2 and 4") && error.contains("manual review"),
            "{error}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        #[cfg(windows)]
        assert!(approve_ledger_transaction_at(
            path.to_str().unwrap(),
            4,
            &sha256_hex(b"block"),
            &sha256_hex(source.as_bytes()),
            "Expenses:Food"
        )
        .is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn malformed_pending_like_headers_fail_production_listing_without_mutation() {
        for header in ["2026-1-01 ! \"Shop\"", "2026-01-01! \"Shop\""] {
            let source = format!("{header}\n  Expenses:Food 1 USD\n");
            let path = fixture(&source);
            let error = get_pending_ledger_at(path.to_str().unwrap()).unwrap_err();
            assert!(
                error.contains("line 1") && error.contains("review manually"),
                "{error}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), source);
            #[cfg(windows)]
            assert!(approve_ledger_transaction_at(
                path.to_str().unwrap(),
                0,
                &sha256_hex(b"block"),
                &sha256_hex(source.as_bytes()),
                "Expenses:Food"
            )
            .is_err());
            fs::remove_file(path).unwrap();
        }
        let directive = "2025-01-01 open Expenses:Food USD ; ! note\noption \"title\" \"!\"\n";
        assert!(
            parse_pending_ledger(directive, &sha256_hex(directive.as_bytes()))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn unsupported_top_level_includes_and_plugins_fail_closed() {
        for directive in ["include \"other.beancount\"", "plugin \"module\""] {
            let source = format!("{directive}\n2026-01-01 ! \"Shop\"\n  Expenses:Food 1 USD\n");
            let path = fixture(&source);
            let error = get_pending_ledger_at(path.to_str().unwrap()).unwrap_err();
            assert!(error.contains("unsupported top-level directive"), "{error}");
            assert_eq!(fs::read_to_string(&path).unwrap(), source);
            #[cfg(windows)]
            {
                let before = fs::read(&path).unwrap();
                assert!(approve_ledger_transaction_at(
                    path.to_str().unwrap(),
                    1,
                    &sha256_hex(b"block"),
                    &sha256_hex(&before),
                    "Expenses:Food",
                )
                .is_err());
                assert_eq!(fs::read(&path).unwrap(), before);
            }
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn lifecycle_events_are_resolved_by_date_and_catalog_keeps_all_units() {
        let reordered = "2024-01-01 close Expenses:Food\n2023-01-01 open Expenses:Food VTI2\n";
        let reordered_lives =
            account_lifetimes(&reordered.split_inclusive('\n').collect::<Vec<_>>()).unwrap();
        assert_eq!(
            eligible_categories(&reordered_lives, "2023-06-01", "VTI2"),
            ["Expenses:Food"]
        );
        assert!(eligible_categories(&reordered_lives, "2024-06-01", "VTI2").is_empty());
        assert!(eligible_categories(&reordered_lives, "2025-01-01", "VTI2").is_empty());
        let source = "2023-01-01 open Expenses:Food VTI2\n2024-01-01 close Expenses:Food\n2025-01-01 open Expenses:Food VTI2\n";
        let lives = account_lifetimes(&source.split_inclusive('\n').collect::<Vec<_>>()).unwrap();
        assert_eq!(
            eligible_categories(&lives, "2025-06-01", "VTI2"),
            ["Expenses:Food"]
        );
        assert!(eligible_categories(&lives, "2025-06-01", "USD").is_empty());
        let duplicate_reopen = "2023-01-01 open Expenses:Food\n2025-01-01 close Expenses:Food\n2024-01-01 open Expenses:Food\n";
        assert!(
            account_lifetimes(&duplicate_reopen.split_inclusive('\n').collect::<Vec<_>>()).is_err()
        );
    }

    #[test]
    fn pending_parser_reports_invalid_amount_with_source_line() {
        let source = "2026-01-01 ! \"Shop\"\n  Expenses:Food nope EUR\n";
        let err = parse_pending_ledger(source, &sha256_hex(source.as_bytes())).unwrap_err();
        assert!(
            err.contains("line 2") && err.contains("invalid explicit posting amount"),
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
    fn list_locator_finds_category_after_metadata_and_asset_posting() {
        let source = "2025-01-01 open Assets:Bank EUR\r\n2025-01-01 open Expenses:Food EUR\r\n2025-01-01 open Income:Refund EUR\r\n2026-04-03 ! \"Café; Shop\"\r\n  note: \"memo: value; retained\"\r\n  Assets:Bank -4.25 EUR\t; bank\r\n\tIncome:Refund\t4.25\tEUR ; income\r\n  ; trailing note";
        let path = fixture(source);
        let row = get_pending_ledger_at(path.to_str().unwrap())
            .unwrap()
            .remove(0);
        assert_eq!(row.category, "Income:Refund");
        assert_eq!(row.amount.to_string(), "4.25");
        assert_eq!(row.currency, "EUR");
        assert_eq!(row.permitted_categories, ["Expenses:Food", "Income:Refund"]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn pending_list_rejects_splits_and_unsupported_postings_as_actionable_errors() {
        for posting in [
            "  Expenses:Food 1 USD\n  Expenses:Food 2 USD",
            "  Expenses:Food 1 USD {2 USD}\n  Assets:Bank -1 USD",
            "  Assets:Bank 1 USD",
            "  Expenses:Food 1 USD\n  mysterious unsupported",
        ] {
            let source = format!("2026-01-01 ! \"x\"\n{posting}");
            let error = parse_pending_ledger(&source, &sha256_hex(source.as_bytes())).unwrap_err();
            assert!(
                error.contains("line")
                    && (error.contains("manual review")
                        || error.contains("manually")
                        || error.contains("unsupported")),
                "{error}"
            );
        }
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
        let original = "2025-01-01 open Expenses:Food USD\r\n2025-01-01 open Expenses:Other USD\r\n; keep\r\n2026-01-01 ! \"Shop\"  ; note\r\n  Expenses:Food 1.00 USD  ; amount\r\n  Assets:Bank -1.00 USD\r\n";
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
        assert_eq!(changed, "2025-01-01 open Expenses:Food USD\r\n2025-01-01 open Expenses:Other USD\r\n; keep\r\n2026-01-01 * \"Shop\"  ; note\r\n  Expenses:Other 1.00 USD  ; amount\r\n  Assets:Bank -1.00 USD\r\n");
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

    #[cfg(windows)]
    #[test]
    fn approval_history_reloads_and_undo_preserves_later_unrelated_bytes() {
        let source="2025-01-01 open Expenses:Food USD\n2025-01-01 open Expenses:Other USD\n2026-01-01 ! \"Shop\"\n  Assets:Bank 1 USD\n  Expenses:Food -1 USD\n";
        let path = fixture(source);
        let path_s = path.to_str().unwrap();
        let row = get_pending_ledger_at(path_s).unwrap().remove(0);
        approve_ledger_transaction_at(
            path_s,
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Other",
        )
        .unwrap();
        let (ns, dir) = review_audit::test_store(path_s);
        let history = review_audit::read_records(&dir, &ns).unwrap();
        assert_eq!(history.len(), 1);
        let id = history[0].0.operation_id.clone();
        let unrelated = "2026-02-01 * \"Unrelated\"\n  Assets:Bank 2 USD\n  Expenses:Food -2 USD\n";
        let changed = format!(
            "; external note retained\n{}{}",
            fs::read_to_string(&path).unwrap(),
            unrelated
        );
        fs::write(&path, &changed).unwrap();
        undo_ledger_approval_with_store(path_s, &id, &dir, &ns).unwrap();
        let result = fs::read_to_string(&path).unwrap();
        assert!(result.starts_with("; external note retained\n"));
        assert!(result.ends_with(unrelated));
        assert!(
            result.contains("2026-01-01 ! \"Shop\"\n  Assets:Bank 1 USD\n  Expenses:Food -1 USD\n")
        );
        let reloaded = review_audit::read_records(&dir, &ns).unwrap();
        assert_eq!(reloaded.len(), 2);
        assert!(review_audit::display(&reloaded)[0].undone);
        assert!(undo_ledger_approval_with_store(path_s, &id, &dir, &ns).is_err());
        let _ = fs::remove_file(path);
    }
    #[cfg(windows)]
    #[test]
    fn audit_publication_faults_preserve_real_ledger_and_leave_reconcilable_intents() {
        let source="2025-01-01 open Expenses:Food USD\n2025-01-01 open Expenses:Other USD\n2026-01-01 ! \"Shop\"\n  Assets:Bank 1 USD\n  Expenses:Food -1 USD\n";
        let path = fixture(source);
        let path_s = path.to_str().unwrap();
        let (ns, dir) = review_audit::test_store(path_s);
        let before = fs::read(&path).unwrap();
        for fault in [
            review_audit::PersistFault::StagingWrite,
            review_audit::PersistFault::StagingSync,
            review_audit::PersistFault::Publish,
        ] {
            let row = get_pending_ledger_at(path_s).unwrap().remove(0);
            let error = approve_ledger_transaction_with_fault(
                path_s,
                row.line_num,
                &row.transaction_id,
                &row.ledger_revision,
                "Expenses:Other",
                &dir,
                &ns,
                review_audit::FaultPlan {
                    intent: fault,
                    ..review_audit::FaultPlan::NONE
                },
            )
            .unwrap_err();
            assert!(error.contains("intent preparation failed"), "{error}");
        }
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(review_audit::read_records(&dir, &ns).unwrap().is_empty());
        review_audit::reconcile(&dir, &ns, &sha256_hex(&before)).unwrap();
        assert!(!fs::read_dir(&dir).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("intent-")));

        let row = get_pending_ledger_at(path_s).unwrap().remove(0);
        approve_ledger_transaction_with_store(
            path_s,
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Other",
            &dir,
            &ns,
        )
        .unwrap();
        let approved = fs::read(&path).unwrap();
        let id = review_audit::read_records(&dir, &ns).unwrap()[0]
            .0
            .operation_id
            .clone();
        for fault in [
            review_audit::PersistFault::StagingWrite,
            review_audit::PersistFault::StagingSync,
            review_audit::PersistFault::Publish,
        ] {
            let error = undo_ledger_approval_with_fault(
                path_s,
                &id,
                &dir,
                &ns,
                review_audit::FaultPlan {
                    intent: fault,
                    ..review_audit::FaultPlan::NONE
                },
            )
            .unwrap_err();
            assert!(error.contains("intent preparation failed"), "{error}");
        }
        assert_eq!(fs::read(&path).unwrap(), approved);
        assert_eq!(review_audit::read_records(&dir, &ns).unwrap().len(), 1);
        review_audit::reconcile(&dir, &ns, &sha256_hex(&approved)).unwrap();
        assert_eq!(review_audit::read_records(&dir, &ns).unwrap().len(), 1);
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_file(path);
    }
    #[cfg(windows)]
    #[test]
    fn undo_rejects_edited_or_duplicated_transaction_without_mutation() {
        let source = "2025-01-01 open Expenses:Food USD\n2025-01-01 open Expenses:Other USD\n2026-01-01 ! \"Shop\"\n  Assets:Bank 1 USD\n  Expenses:Food -1 USD\n";
        let path = fixture(source);
        let path_s = path.to_str().unwrap();
        let (ns, dir) = review_audit::test_store(path_s);
        let row = get_pending_ledger_at(path_s).unwrap().remove(0);
        approve_ledger_transaction_with_store(
            path_s,
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Other",
            &dir,
            &ns,
        )
        .unwrap();
        let id = get_history_with_store(path_s, &dir, &ns).unwrap()[0]
            .id
            .clone();
        let approved = fs::read_to_string(&path).unwrap();
        let edited = approved.replace("Expenses:Other -1", "Expenses:Food -1");
        fs::write(&path, &edited).unwrap();
        assert!(undo_ledger_approval_with_store(path_s, &id, &dir, &ns)
            .unwrap_err()
            .contains("undo conflict"));
        assert_eq!(fs::read_to_string(&path).unwrap(), edited);
        assert_eq!(review_audit::read_records(&dir, &ns).unwrap().len(), 1);
        fs::write(&path, &approved).unwrap();
        let duplicated = format!("{approved}{approved}");
        fs::write(&path, &duplicated).unwrap();
        assert!(undo_ledger_approval_with_store(path_s, &id, &dir, &ns)
            .unwrap_err()
            .contains("undo conflict"));
        assert_eq!(fs::read_to_string(&path).unwrap(), duplicated);
        assert_eq!(review_audit::read_records(&dir, &ns).unwrap().len(), 1);
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_file(path);
    }

    #[cfg(windows)]
    #[test]
    fn final_audit_faults_are_injected_after_source_write_for_approval_and_undo() {
        let source = "2025-01-01 open Expenses:Food USD\n2025-01-01 open Expenses:Other USD\n2026-01-01 ! \"Shop\"\n  Assets:Bank 1 USD\n  Expenses:Food -1 USD\n";
        let path = fixture(source);
        let path_s = path.to_str().unwrap();
        let (ns, dir) = review_audit::test_store(path_s);
        let before = fs::read(&path).unwrap();
        let row = get_pending_ledger_at(path_s).unwrap().remove(0);
        let source_error = approve_ledger_transaction_with_fault(
            path_s,
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Other",
            &dir,
            &ns,
            review_audit::FaultPlan {
                fail_source_after_write: true,
                ..review_audit::FaultPlan::NONE
            },
        )
        .unwrap_err();
        assert!(
            source_error.contains("source write failed") && source_error.contains("restored"),
            "{source_error}"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(review_audit::read_records(&dir, &ns).unwrap().is_empty());
        for fault in [
            review_audit::PersistFault::StagingWrite,
            review_audit::PersistFault::StagingSync,
            review_audit::PersistFault::Publish,
        ] {
            let before = fs::read(&path).unwrap();
            let row = get_pending_ledger_at(path_s).unwrap().remove(0);
            let error = approve_ledger_transaction_with_fault(
                path_s,
                row.line_num,
                &row.transaction_id,
                &row.ledger_revision,
                "Expenses:Other",
                &dir,
                &ns,
                review_audit::FaultPlan {
                    commit: fault,
                    ..review_audit::FaultPlan::NONE
                },
            )
            .unwrap_err();
            assert!(
                error.contains("audit commit failed after source write"),
                "{error}"
            );
            assert_eq!(fs::read(&path).unwrap(), before);
            assert!(get_history_with_store(path_s, &dir, &ns)
                .unwrap()
                .is_empty());
        }
        let row = get_pending_ledger_at(path_s).unwrap().remove(0);
        approve_ledger_transaction_with_store(
            path_s,
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Other",
            &dir,
            &ns,
        )
        .unwrap();
        let approved = fs::read(&path).unwrap();
        let id = get_history_with_store(path_s, &dir, &ns).unwrap()[0]
            .id
            .clone();
        for fault in [
            review_audit::PersistFault::StagingWrite,
            review_audit::PersistFault::StagingSync,
            review_audit::PersistFault::Publish,
        ] {
            let error = undo_ledger_approval_with_fault(
                path_s,
                &id,
                &dir,
                &ns,
                review_audit::FaultPlan {
                    commit: fault,
                    ..review_audit::FaultPlan::NONE
                },
            )
            .unwrap_err();
            assert!(
                error.contains("audit commit failed after source write"),
                "{error}"
            );
            assert_eq!(fs::read(&path).unwrap(), approved);
            assert_eq!(get_history_with_store(path_s, &dir, &ns).unwrap().len(), 1);
        }
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_file(path);
    }

    #[cfg(windows)]
    #[test]
    fn restart_reads_real_before_after_and_committed_cleanup_states_once() {
        let source = "2025-01-01 open Expenses:Food USD\n2025-01-01 open Expenses:Other USD\n2026-01-01 ! \"Shop\"\n  Assets:Bank 1 USD\n  Expenses:Food -1 USD\n";
        let path = fixture(source);
        let path_s = path.to_str().unwrap();
        let (ns, dir) = review_audit::test_store(path_s);
        let initial = fs::read(&path).unwrap();
        let row = get_pending_ledger_at(path_s).unwrap().remove(0);
        let stopped = approve_ledger_transaction_with_fault(
            path_s,
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Other",
            &dir,
            &ns,
            review_audit::FaultPlan {
                stop_after_prepare: true,
                ..review_audit::FaultPlan::NONE
            },
        )
        .unwrap_err();
        assert!(stopped.contains("after prepared intent"));
        assert_eq!(fs::read(&path).unwrap(), initial);
        assert!(get_history_with_store(path_s, &dir, &ns)
            .unwrap()
            .is_empty());

        let row = get_pending_ledger_at(path_s).unwrap().remove(0);
        let stopped = approve_ledger_transaction_with_fault(
            path_s,
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Other",
            &dir,
            &ns,
            review_audit::FaultPlan {
                stop_after_source: true,
                ..review_audit::FaultPlan::NONE
            },
        )
        .unwrap_err();
        assert!(stopped.contains("after source write"));
        let history = get_history_with_store(path_s, &dir, &ns).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(get_history_with_store(path_s, &dir, &ns).unwrap().len(), 1);
        let approved = fs::read(&path).unwrap();
        let id = history[0].id.clone();
        undo_ledger_approval_with_fault(
            path_s,
            &id,
            &dir,
            &ns,
            review_audit::FaultPlan {
                fail_intent_cleanup: true,
                ..review_audit::FaultPlan::NONE
            },
        )
        .unwrap();
        let committed_intent = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("intent-")
            })
            .unwrap();
        let mut intent_permissions = fs::metadata(&committed_intent).unwrap().permissions();
        intent_permissions.set_readonly(true);
        fs::set_permissions(&committed_intent, intent_permissions).unwrap();
        assert_eq!(fs::read(&path).unwrap(), initial);
        let later_edit = [
            fs::read(&path).unwrap().as_slice(),
            b"; later unrelated edit\n",
        ]
        .concat();
        fs::write(&path, &later_edit).unwrap();
        assert!(committed_intent.exists());
        assert_eq!(get_history_with_store(path_s, &dir, &ns).unwrap().len(), 2);
        assert_eq!(get_history_with_store(path_s, &dir, &ns).unwrap().len(), 2);
        assert_eq!(fs::read(&path).unwrap(), later_edit);
        assert_ne!(approved, later_edit);
        if committed_intent.exists() {
            let mut intent_permissions = fs::metadata(&committed_intent).unwrap().permissions();
            intent_permissions.set_readonly(false);
            fs::set_permissions(&committed_intent, intent_permissions).unwrap();
        }
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_file(path);
    }

    #[cfg(windows)]
    #[test]
    fn failed_commit_and_restore_retains_complete_copy_and_restart_does_not_rewrite() {
        let source = "2025-01-01 open Expenses:Food USD\n2025-01-01 open Expenses:Other USD\n2026-01-01 ! \"Shop\"\n  Assets:Bank 1 USD\n  Expenses:Food -1 USD\n";
        let path = fixture(source);
        let path_s = path.to_str().unwrap();
        let (ns, dir) = review_audit::test_store(path_s);
        let before = fs::read(&path).unwrap();
        let row = get_pending_ledger_at(path_s).unwrap().remove(0);
        let error = approve_ledger_transaction_with_fault(
            path_s,
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Other",
            &dir,
            &ns,
            review_audit::FaultPlan {
                commit: review_audit::PersistFault::Publish,
                fail_restore_after_partial: true,
                ..review_audit::FaultPlan::NONE
            },
        )
        .unwrap_err();
        assert!(
            error.contains("outcome uncertain") && error.contains("recovery required"),
            "{error}"
        );
        let copy = error
            .split("complete original ledger retained at ")
            .nth(1)
            .unwrap()
            .split(" and intent")
            .next()
            .unwrap();
        assert_eq!(fs::read(copy).unwrap(), before);
        let ambiguous = fs::read(&path).unwrap();
        let history_error = get_history_with_store(path_s, &dir, &ns).unwrap_err();
        assert!(history_error.contains("ambiguous"));
        assert_eq!(fs::read(&path).unwrap(), ambiguous);
        assert_eq!(fs::read(copy).unwrap(), before);
        let _ = fs::remove_file(copy);
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_file(path);
    }

    #[cfg(windows)]
    #[test]
    fn concurrent_undo_attempts_commit_at_most_one_effect() {
        let source="2025-01-01 open Expenses:Food USD\n2025-01-01 open Expenses:Other USD\n2026-01-01 ! \"Shop\"\n  Assets:Bank 1 USD\n  Expenses:Food -1 USD\n";
        let path = fixture(source);
        let path_s = path.to_str().unwrap().to_string();
        let row = get_pending_ledger_at(&path_s).unwrap().remove(0);
        approve_ledger_transaction_at(
            &path_s,
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Other",
        )
        .unwrap();
        let (ns, dir) = review_audit::test_store(&path_s);
        let id = review_audit::read_records(&dir, &ns).unwrap()[0]
            .0
            .operation_id
            .clone();
        let path_a = path_s.clone();
        let dir_a = dir.clone();
        let ns_a = ns.clone();
        let id_a = id.clone();
        let a = std::thread::spawn(move || {
            undo_ledger_approval_with_store(&path_a, &id_a, &dir_a, &ns_a)
        });
        let path_b = path_s.clone();
        let dir_b = dir.clone();
        let ns_b = ns.clone();
        let b = std::thread::spawn(move || {
            undo_ledger_approval_with_store(&path_b, &id, &dir_b, &ns_b)
        });
        let results = [a.join().unwrap(), b.join().unwrap()];
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(review_audit::read_records(&dir, &ns).unwrap().len(), 2);
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_file(path);
    }
    #[cfg(windows)]
    #[test]
    fn approval_changes_only_header_flag_and_second_category_token() {
        let original = "2025-01-01 open Assets:Bank EUR\r\n2025-01-01 open Expenses:Food EUR\r\n2025-01-01 open Income:Refund EUR\r\n2026-04-03\t!\t\"Café; Shop\" ; note\r\n  note: \"memo: value; retained\"\r\n  Assets:Bank -4.25 EUR\t; bank\r\n\tIncome:Refund\t4.25\tEUR ; income\r\n  ; trailing note";
        let path = fixture(original);
        let row = get_pending_ledger_at(path.to_str().unwrap())
            .unwrap()
            .remove(0);
        approve_ledger_transaction_at(
            path.to_str().unwrap(),
            row.line_num,
            &row.transaction_id,
            &row.ledger_revision,
            "Expenses:Food",
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "2025-01-01 open Assets:Bank EUR\r\n2025-01-01 open Expenses:Food EUR\r\n2025-01-01 open Income:Refund EUR\r\n2026-04-03\t*\t\"Café; Shop\" ; note\r\n  note: \"memo: value; retained\"\r\n  Assets:Bank -4.25 EUR\t; bank\r\n\tExpenses:Food\t4.25\tEUR ; income\r\n  ; trailing note");
        fs::remove_file(path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn split_approval_rejects_without_changing_any_file_bytes() {
        let source = "2025-01-01 open Expenses:Food USD\n2026-01-01 ! \"x\"\n  Expenses:Food 1 USD\n  Expenses:Food 2 USD\n";
        let path = fixture(source);
        let error = get_pending_ledger_at(path.to_str().unwrap()).unwrap_err();
        assert!(error.contains("exactly one"));
        let before = fs::read(&path).unwrap();
        assert!(approve_ledger_transaction_at(
            path.to_str().unwrap(),
            1,
            &sha256_hex(b"block"),
            &sha256_hex(&before),
            "Expenses:Food"
        )
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_file(path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn comment_separated_split_rejected_by_listing_and_locked_approval() {
        let source = "2025-01-01 open Assets:Bank USD\n2025-01-01 open Expenses:Food USD\n2026-01-01 ! \"x\"\n  Expenses:Food 1 USD\n; separator\n  Expenses:Food 2 USD\n";
        let path = fixture(source);
        let before = fs::read(&path).unwrap();
        assert!(get_pending_ledger_at(path.to_str().unwrap())
            .unwrap_err()
            .contains("exactly one"));
        assert!(approve_ledger_transaction_at(
            path.to_str().unwrap(),
            2,
            &sha256_hex(b"block"),
            &sha256_hex(&before),
            "Expenses:Food"
        )
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_file(path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn invalid_lifecycle_is_rejected_by_locked_production_approval_without_mutation() {
        for lifecycle in [
            "2023-01-01 open Expenses:Food USD\n2025-01-01 close Expenses:Food\n2024-01-01 open Expenses:Food USD\n",
            "2025-01-01 close Expenses:Food\n2024-01-01 open Expenses:Food USD\n",
            "2023-01-01 open Expenses:Food USD\n2023-01-01 close Expenses:Food\n",
        ] {
            let source = format!("{lifecycle}2026-01-01 ! \"x\"\n  Expenses:Food 1 USD\n");
            let path = fixture(&source);
            let before = fs::read(&path).unwrap();
            assert!(approve_ledger_transaction_at(path.to_str().unwrap(), lifecycle.lines().count(), &sha256_hex(b"block"), &sha256_hex(&before), "Expenses:Food").is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
            fs::remove_file(path).unwrap();
        }
    }

    #[cfg(windows)]
    #[test]
    fn approval_rejects_forged_closed_future_wrong_root_and_unit_limited_targets() {
        let source = "2025-01-01 open Expenses:Food USD\n2025-01-01 open Expenses:Closed USD\n2025-01-02 close Expenses:Closed\n2027-01-01 open Expenses:Future USD\n2025-01-01 open Income:Euro EUR\n2025-01-01 open Assets:Bank USD\n2026-01-01 ! \"shop\"\n  Expenses:Food 1 USD\n  Assets:Bank -1 USD\n";
        let path = fixture(source);
        let row = get_pending_ledger_at(path.to_str().unwrap())
            .unwrap()
            .remove(0);
        assert_eq!(row.permitted_categories, ["Expenses:Food"]);
        let before = fs::read(&path).unwrap();
        for candidate in [
            "Expenses:Forged",
            "Expenses:Closed",
            "Expenses:Future",
            "Assets:Bank",
            "Income:Euro",
        ] {
            let error = approve_ledger_transaction_at(
                path.to_str().unwrap(),
                row.line_num,
                &row.transaction_id,
                &row.ledger_revision,
                candidate,
            )
            .unwrap_err();
            assert!(
                error.contains("not open and permitted")
                    || error.contains("not a valid Expenses or Income"),
                "{error}"
            );
            assert_eq!(fs::read(&path).unwrap(), before);
        }
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
            "2025-01-01 open Expenses:A USD\n2025-01-01 open Expenses:B USD\n2025-01-01 open Expenses:ChangedA USD\n2025-01-01 open Expenses:ChangedB USD\n2026-01-01 ! \"A\"\n  Expenses:A 1 USD\n2026-01-02 ! \"B\"\n  Expenses:B 2 USD\n";
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
        let source = "2025-01-01 open Expenses:A USD\n2025-01-01 open Expenses:B USD\n2025-01-01 open Expenses:C USD\n2026-01-01 ! \"A\"\n  Expenses:A 1 USD\n";
        let path = fixture(source);
        let revision = sha256_hex(source.as_bytes());
        let row = parse_pending_ledger(source, &revision).unwrap().remove(0);
        let line_num = row.line_num;
        let mut tasks = Vec::new();
        for category in ["Expenses:B", "Expenses:C"] {
            let path = path.clone();
            let id = row.transaction_id.clone();
            let revision = revision.clone();
            tasks.push(std::thread::spawn(move || {
                approve_ledger_transaction_at(
                    path.to_str().unwrap(),
                    line_num,
                    &id,
                    &revision,
                    category,
                )
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
