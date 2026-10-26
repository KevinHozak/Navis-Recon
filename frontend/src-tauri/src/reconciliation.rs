//! Explicit account/period evidence. Strict zero-difference policy; never writes the ledger.
use crate::{document_store as store, ledger_parser, money::Money};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub id: String,
    pub statement_path: String,
    pub account: String,
    pub period_start: String,
    pub period_end: String,
    pub currency: String,
    pub status: String,
    pub opening_balance: Option<Money>,
    pub closing_balance: Option<Money>,
    #[serde(default)]
    pub matched_transaction_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub draft: Draft,
    pub statement_id: String,
    pub evidence_reference: String,
    pub comparison: Option<Comparison>,
    pub saved_at_ms: u128,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: String,
    pub date: String,
    pub description: String,
    pub amount: Money,
    pub currency: String,
    pub pending: bool,
    pub transfer: bool,
    pub in_period: bool,
    pub matched: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub ledger_revision: String,
    pub ledger_opening: Money,
    pub ledger_closing: Money,
    pub opening_difference: Option<Money>,
    pub closing_difference: Option<Money>,
    pub candidates: Vec<Candidate>,
    pub can_reconcile: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u8,
    records: Vec<Record>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            schema_version: 1,
            records: Vec::new(),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub revision: String,
    pub records: Vec<Record>,
    pub stale_ids: Vec<String>,
}
pub(crate) fn private_path() -> Result<PathBuf, String> {
    let base =
        std::env::var_os("LOCALAPPDATA").ok_or("Private reconciliation store unavailable")?;
    let ledger =
        fs::canonicalize(crate::config::get("ledger_path")).map_err(|_| "Ledger unavailable")?;
    let path = PathBuf::from(base)
        .join("Navis-Recon")
        .join("reconciliation")
        .join(store::hash(ledger.to_string_lossy().as_bytes()))
        .join("records.json");
    let mut probe = path.parent().ok_or("Invalid private store")?;
    while !probe.exists() {
        probe = probe.parent().ok_or("Private store unavailable")?;
    }
    let actual = fs::canonicalize(probe).map_err(|_| "Private store unavailable")?;
    if actual.ancestors().any(|p| p.join(".git").exists()) {
        return Err("Reconciliation evidence must stay outside Git".into());
    }
    Ok(path)
}
fn read_doc(path: &Path) -> Result<(Document, String), String> {
    let (bytes, rev) = store::read(path)?;
    if rev == "missing" {
        return Ok((Document::default(), rev));
    }
    let doc: Document = serde_json::from_slice(&bytes)
        .map_err(|_| "Reconciliation store corrupt; preserve it for recovery")?;
    if doc.schema_version != 1 {
        return Err("Unsupported reconciliation schema; preserve the store".into());
    }
    let mut ids = std::collections::HashSet::new();
    for record in &doc.records {
        validate(&record.draft)?;
        if record.draft.id != record_id(&record.statement_id, &record.draft)
            || !ids.insert(&record.draft.id)
        {
            return Err("Reconciliation identity is corrupt".into());
        }
    }
    Ok((doc, rev))
}
pub(crate) fn records_for_close(path: &Path) -> Result<(Vec<Record>, String), String> {
    let (doc, revision) = read_doc(path)?;
    Ok((doc.records, revision))
}
fn validate(d: &Draft) -> Result<(), String> {
    if !ledger_parser::valid_date(&d.period_start)
        || !ledger_parser::valid_date(&d.period_end)
        || d.period_start > d.period_end
    {
        return Err("Use a valid inclusive statement period".into());
    }
    if !ledger_parser::account_ok(&d.account)
        || !(d.account.starts_with("Assets:") || d.account.starts_with("Liabilities:"))
        || !ledger_parser::valid_currency(&d.currency)
    {
        return Err("Choose an explicit asset or liability account and unit".into());
    }
    if !["received", "imported", "reviewed", "reconciled"].contains(&d.status.as_str()) {
        return Err("Unknown reconciliation status".into());
    }
    if d.matched_transaction_ids.len() > 20000 {
        return Err("Too many transaction selections".into());
    }
    Ok(())
}
fn record_id(statement: &str, d: &Draft) -> String {
    store::hash(
        serde_json::to_string(&(
            statement,
            &d.account,
            &d.period_start,
            &d.period_end,
            &d.currency,
        ))
        .unwrap()
        .as_bytes(),
    )
}
pub(crate) fn locked_read(path: &Path) -> Result<(File, Vec<u8>), String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Other financial views may read the same stable bytes. Deny write and
        // delete access until evidence publication finishes, without making our
        // own pending-review/catalog reads report a false unavailable source.
        options.share_mode(1);
    }
    let mut f = options
        .open(path)
        .map_err(|_| "Evidence file unavailable or busy; refresh")?;
    if f.metadata().map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
        return Err("Evidence file exceeds 32 MiB limit".into());
    }
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok((f, bytes))
}
pub(crate) fn evidence(d: &Draft, root: &Path) -> Result<(File, String, String), String> {
    crate::managed_import::reject_reparse_ancestry(root)?;
    crate::managed_import::reject_reparse_ancestry(Path::new(&d.statement_path))?;
    let root = fs::canonicalize(root).map_err(|_| "Statement archive unavailable")?;
    let path = fs::canonicalize(&d.statement_path).map_err(|_| "Statement evidence unavailable")?;
    if !path.starts_with(&root)
        || !path.is_file()
        || !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err("Choose a PDF in the statement catalog".into());
    }
    let (file, bytes) = locked_read(&path)?;
    Ok((
        file,
        store::hash(&bytes),
        path.strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned(),
    ))
}
pub(crate) fn compare_bytes(d: &Draft, bytes: &[u8]) -> Result<Comparison, String> {
    validate(d)?;
    let text = std::str::from_utf8(bytes).map_err(|_| "Ledger is not UTF-8")?;
    let txs = ledger_parser::parse_with_currency(text, &d.currency)
        .map_err(|e| format!("Ledger validation failed: {}", e.join("; ")))?;
    let known = txs.iter().any(|t| {
        t.postings
            .iter()
            .any(|p| p.account == d.account && p.currency == d.currency)
    }) || text.lines().any(|line| {
        let f: Vec<_> = line.split_whitespace().collect();
        f.len() >= 3
            && f[1] == "open"
            && f[2] == d.account
            && (f.len() == 3 || f.get(3).is_some_and(|unit| *unit == d.currency))
    });
    if !known {
        return Err(
            "Account/unit is absent from the validated ledger; map the correct account explicitly"
                .into(),
        );
    }
    let mut opening = Money::default();
    let mut closing = Money::default();
    let mut candidates = Vec::new();
    let revision = store::hash(bytes);
    for (i, tx) in txs.iter().enumerate() {
        let postings: Vec<_> = tx
            .postings
            .iter()
            .filter(|p| p.account == d.account && p.currency == d.currency)
            .collect();
        if postings.is_empty() {
            continue;
        }
        let amount = postings.iter().fold(Money::default(), |n, p| n + p.amount);
        if tx.date < d.period_start {
            opening += amount;
        }
        if tx.date <= d.period_end {
            closing += amount;
        }
        let in_period = tx.date >= d.period_start && tx.date <= d.period_end;
        let id = store::hash(format!("{revision}:{i}:{}:{}", d.account, d.currency).as_bytes());
        candidates.push(Candidate {
            matched: d.matched_transaction_ids.contains(&id),
            id,
            date: tx.date.clone(),
            description: format!("{} {}", tx.payee, tx.narration).trim().into(),
            amount,
            currency: d.currency.clone(),
            pending: tx.flag != '*',
            transfer: tx
                .postings
                .iter()
                .filter(|p| p.account != d.account)
                .any(|p| p.account.starts_with("Assets:") || p.account.starts_with("Liabilities:")),
            in_period,
        });
    }
    let mut selected = std::collections::HashSet::new();
    if d.matched_transaction_ids.iter().any(|id| {
        !selected.insert(id)
            || !candidates
                .iter()
                .any(|c| c.id == *id && c.in_period && !c.pending)
    }) {
        return Err("Transaction selections are stale, duplicated, pending or outside the period; compare again".into());
    }
    let opening_difference = d.opening_balance.map(|v| v - opening);
    let closing_difference = d.closing_balance.map(|v| v - closing);
    let can_reconcile = opening_difference.is_some_and(Money::is_zero)
        && closing_difference.is_some_and(Money::is_zero)
        && candidates
            .iter()
            .filter(|c| c.in_period)
            .all(|c| c.matched && !c.pending);
    Ok(Comparison {
        ledger_revision: revision,
        ledger_opening: opening,
        ledger_closing: closing,
        opening_difference,
        closing_difference,
        candidates,
        can_reconcile,
    })
}
fn snapshot_at(path: &Path, ledger: &Path, root: &Path) -> Result<Snapshot, String> {
    let (doc, revision) = read_doc(path)?;
    let ledger_revision = fs::read(ledger).ok().map(|b| store::hash(&b));
    let stale_ids = doc
        .records
        .iter()
        .filter(|r| {
            r.draft.status == "reconciled"
                && (r
                    .comparison
                    .as_ref()
                    .is_none_or(|c| Some(&c.ledger_revision) != ledger_revision.as_ref())
                    || evidence(&r.draft, root).map_or(true, |(_, hash, _)| hash != r.statement_id))
        })
        .map(|r| r.draft.id.clone())
        .collect();
    Ok(Snapshot {
        revision,
        records: doc.records,
        stale_ids,
    })
}
#[tauri::command]
pub fn get_reconciliations() -> Result<Snapshot, String> {
    let path = private_path()?;
    snapshot_at(
        &path,
        Path::new(crate::config::get("ledger_path")),
        &crate::statements::resolve_statements_base_dir(),
    )
}
#[tauri::command]
pub fn compare_reconciliation(draft: Draft) -> Result<Comparison, String> {
    let (_ledger, bytes) = locked_read(Path::new(crate::config::get("ledger_path")))?;
    compare_bytes(&draft, &bytes)
}
pub(crate) fn save_at(
    path: &Path,
    ledger: &Path,
    root: &Path,
    mut draft: Draft,
    expected_revision: &str,
) -> Result<Snapshot, String> {
    validate(&draft)?;
    let _lock = store::lock(path)?;
    let (mut doc, revision) = read_doc(path)?;
    if revision != expected_revision {
        return Err("Reconciliation conflict: refresh; external edits preserved".into());
    }
    let (_statement, statement_id, evidence_reference) = evidence(&draft, root)?;
    let id = record_id(&statement_id, &draft);
    if !draft.id.is_empty() && draft.id != id {
        return Err(
            "Account, period, unit or statement changed; create a new evidence record".into(),
        );
    }
    draft.id = id.clone();
    let previous = doc
        .records
        .iter()
        .find(|r| r.draft.id == id)
        .map(|r| r.draft.status.as_str());
    let next = draft.status.as_str();
    let allowed = match previous {
        None => next == "received",
        Some(old) if old == next => true,
        Some("received") => next == "imported",
        Some("imported") => next == "reviewed",
        Some("reviewed") => next == "reconciled",
        Some("reconciled") => next == "reviewed",
        _ => false,
    };
    if !allowed {
        return Err("Follow received → imported → reviewed → reconciled; reopen reconciled evidence as reviewed".into());
    }
    let (_ledger, bytes) = locked_read(ledger)?;
    let comparison = compare_bytes(&draft, &bytes)?;
    if next == "reconciled" && !comparison.can_reconcile {
        return Err("Reconciliation blocked: resolve opening/closing differences and review all cleared period transactions; pending entries require resolution".into());
    }
    let record = Record {
        draft,
        statement_id,
        evidence_reference,
        comparison: Some(comparison),
        saved_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    };
    if let Some(existing) = doc.records.iter_mut().find(|r| r.draft.id == id) {
        *existing = record;
    } else {
        doc.records.push(record);
    }
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
    store::save(path, expected_revision, &bytes)?;
    drop(_statement);
    drop(_ledger);
    snapshot_at(path, ledger, root)
}
#[tauri::command]
pub fn save_reconciliation(draft: Draft, expected_revision: String) -> Result<Snapshot, String> {
    save_at(
        &private_path()?,
        Path::new(crate::config::get("ledger_path")),
        &crate::statements::resolve_statements_base_dir(),
        draft,
        &expected_revision,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const LEDGER: &str = "2025-12-31 * \"Opening\"\n  Assets:Bank 10 USD\n  Equity:Opening -10 USD\n2026-01-01 * \"Transfer\"\n  Assets:Bank 2.000000000000000001 USD\n  Assets:Other -2.000000000000000001 USD\n2026-01-31 * \"Refund\"\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n2026-02-01 ! \"Pending timing\"\n  Assets:Bank 3 USD\n  Equity:Opening -3 USD\n2026-01-31 * \"Other unit\"\n  Assets:Bank 500 EUR\n  Equity:Opening -500 EUR\n";
    fn draft(path: &Path) -> Draft {
        Draft {
            id: String::new(),
            statement_path: path.to_string_lossy().into_owned(),
            account: "Assets:Bank".into(),
            period_start: "2026-01-01".into(),
            period_end: "2026-01-31".into(),
            currency: "USD".into(),
            status: "received".into(),
            opening_balance: Some(Money::parse("10").unwrap()),
            closing_balance: Some(Money::parse("11.000000000000000001").unwrap()),
            matched_transaction_ids: vec![],
        }
    }
    fn fixture() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "recon-e004-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let ledger = dir.join("ledger.bean");
        fs::write(&ledger, LEDGER).unwrap();
        let pdf = dir.join("Bank.pdf");
        fs::write(&pdf, b"synthetic pdf evidence").unwrap();
        let store = dir.join("private/records.json");
        (dir, ledger, pdf, store)
    }
    #[test]
    fn exact_cutoff_currency_fractional_refunds_and_transaction_candidates() {
        let mut d = draft(Path::new("unused.pdf"));
        let c = compare_bytes(&d, LEDGER.as_bytes()).unwrap();
        assert_eq!(c.ledger_opening.to_string(), "10");
        assert_eq!(c.ledger_closing.to_string(), "11.000000000000000001");
        assert!(c.closing_difference.unwrap().is_zero());
        assert!(!c.can_reconcile);
        assert!(c.candidates.iter().any(|c| c.transfer && c.in_period));
        assert!(c.candidates.iter().any(|c| c.pending && !c.in_period));
        assert!(!c.candidates.iter().any(|c| c.currency == "EUR"));
        d.matched_transaction_ids = c
            .candidates
            .iter()
            .filter(|c| c.in_period)
            .map(|c| c.id.clone())
            .collect();
        assert!(compare_bytes(&d, LEDGER.as_bytes()).unwrap().can_reconcile);
        d.period_end = "2026-02-01".into();
        d.matched_transaction_ids.clear();
        let c = compare_bytes(&d, LEDGER.as_bytes()).unwrap();
        assert_eq!(c.ledger_closing.to_string(), "14.000000000000000001");
        assert!(!c.can_reconcile);
        d.period_end = "2026-01-31".into();
        d.opening_balance = None;
        assert!(!compare_bytes(&d, LEDGER.as_bytes()).unwrap().can_reconcile);
        assert!(compare_bytes(&d, b"include \"other.bean\"").is_err());
    }
    #[test]
    fn explicit_mapping_same_institution_overlapping_periods_and_content_bound_identity() {
        let (root, ledger, pdf, path) = fixture();
        let first = save_at(&path, &ledger, &root, draft(&pdf), "missing").unwrap();
        let mut second = draft(&pdf);
        second.account = "Assets:Other".into();
        let second = save_at(&path, &ledger, &root, second, &first.revision).unwrap();
        assert_eq!(second.records.len(), 2);
        assert_ne!(second.records[0].draft.id, second.records[1].draft.id);
        let mut overlap = draft(&pdf);
        overlap.period_start = "2026-01-15".into();
        let overlap = save_at(&path, &ledger, &root, overlap, &second.revision).unwrap();
        assert_eq!(overlap.records.len(), 3);
        let pdf2 = root.join("Bank second.pdf");
        fs::write(&pdf2, b"different synthetic statement").unwrap();
        let more = save_at(&path, &ledger, &root, draft(&pdf2), &overlap.revision).unwrap();
        assert_eq!(more.records.len(), 4);
        assert!(save_at(&path, &ledger, &root, draft(&pdf), "missing")
            .unwrap_err()
            .contains("conflict"));
        let same = save_at(
            &path,
            &ledger,
            &root,
            more.records[0].draft.clone(),
            &more.revision,
        )
        .unwrap();
        assert_eq!(same.records.len(), 4);
        let outside = root.parent().unwrap().join("not-catalog.pdf");
        let mut bad = draft(&outside);
        bad.account = "Assets:Bank".into();
        assert!(save_at(&path, &ledger, &root, bad, &same.revision).is_err());
    }
    #[test]
    fn lifecycle_zero_difference_gate_restart_and_stale_evidence() {
        let (root, ledger, pdf, path) = fixture();
        let mut d = draft(&pdf);
        d.status = "reconciled".into();
        assert!(save_at(&path, &ledger, &root, d.clone(), "missing").is_err());
        d.status = "received".into();
        let mut s = save_at(&path, &ledger, &root, d, "missing").unwrap();
        for status in ["imported", "reviewed"] {
            let mut d = s.records[0].draft.clone();
            d.status = status.into();
            s = save_at(&path, &ledger, &root, d, &s.revision).unwrap();
        }
        let mut d = s.records[0].draft.clone();
        d.status = "reconciled".into();
        assert!(save_at(&path, &ledger, &root, d.clone(), &s.revision).is_err());
        d.matched_transaction_ids = compare_bytes(&d, LEDGER.as_bytes())
            .unwrap()
            .candidates
            .iter()
            .filter(|c| c.in_period)
            .map(|c| c.id.clone())
            .collect();
        d.closing_balance = Some(Money::parse("12").unwrap());
        assert!(save_at(&path, &ledger, &root, d.clone(), &s.revision).is_err());
        d.closing_balance = Some(Money::parse("11.000000000000000001").unwrap());
        s = save_at(&path, &ledger, &root, d, &s.revision).unwrap();
        assert!(s.stale_ids.is_empty());
        assert_eq!(
            snapshot_at(&path, &ledger, &root).unwrap().records[0]
                .draft
                .status,
            "reconciled"
        );
        let ledger_before = fs::read(&ledger).unwrap();
        fs::write(&pdf, b"changed statement bytes").unwrap();
        assert_eq!(
            snapshot_at(&path, &ledger, &root).unwrap().stale_ids.len(),
            1
        );
        assert_eq!(fs::read(&ledger).unwrap(), ledger_before);
        fs::write(&pdf, b"synthetic pdf evidence").unwrap();
        fs::write(&ledger, format!("{LEDGER}; changed revision\n")).unwrap();
        assert_eq!(
            snapshot_at(&path, &ledger, &root).unwrap().stale_ids.len(),
            1
        );
    }
    #[test]
    fn negative_liabilities_pending_and_stale_matches_are_explicit() {
        let ledger = "2025-12-31 * \"Opening\"\n  Liabilities:Card -10 USD\n  Equity:Opening 10 USD\n2026-01-31 ! \"Pending\"\n  Liabilities:Card -2 USD\n  Expenses:Food 2 USD\n";
        let mut d = draft(Path::new("unused.pdf"));
        d.account = "Liabilities:Card".into();
        d.opening_balance = Some(Money::parse("-10").unwrap());
        d.closing_balance = Some(Money::parse("-12").unwrap());
        let c = compare_bytes(&d, ledger.as_bytes()).unwrap();
        assert_eq!(c.ledger_closing.to_string(), "-12");
        assert!(c.closing_difference.unwrap().is_zero());
        assert!(!c.can_reconcile);
        assert!(c.candidates.iter().any(|c| c.pending && c.in_period));
        d.matched_transaction_ids = c
            .candidates
            .iter()
            .filter(|c| c.in_period)
            .map(|c| c.id.clone())
            .collect();
        assert!(compare_bytes(&d, ledger.as_bytes()).is_err());
        let mut d = draft(Path::new("unused.pdf"));
        let c = compare_bytes(&d, LEDGER.as_bytes()).unwrap();
        d.matched_transaction_ids = c
            .candidates
            .iter()
            .filter(|c| c.in_period)
            .map(|c| c.id.clone())
            .collect();
        assert!(compare_bytes(&d, format!("{LEDGER}; new revision\n").as_bytes()).is_err());
        d.matched_transaction_ids.clear();
        d.period_start = "2026-02-30".into();
        assert!(compare_bytes(&d, LEDGER.as_bytes()).is_err());
    }
}
