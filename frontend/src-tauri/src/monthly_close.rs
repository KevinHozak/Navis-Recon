//! Local evidence summaries. Closing never modifies a ledger or a statement.
use crate::{
    document_store as store, household_analysis, ledger_parser, managed_import, obligations,
    reconciliation,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeException {
    pub account: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub month: String,
    pub currency: String,
    pub accounts: Vec<String>,
    pub exceptions: Vec<ScopeException>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blocker {
    pub stage: String,
    pub owner: String,
    pub message: String,
    pub view: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub record_id: String,
    pub account: String,
    pub reference: String,
    pub sha256: String,
    pub start: String,
    pub end: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub scope: Request,
    pub action: String,
    pub saved_at_ms: u128,
    pub evidence_revision: String,
    pub ledger_revision: String,
    pub reconciliation_revision: String,
    pub sync_revision: String,
    pub statements: Vec<Evidence>,
    pub metrics: serde_json::Value,
    pub unresolved_exceptions: Vec<Blocker>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u8,
    history: Vec<Summary>,
}
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub revision: String,
    pub evidence_revision: String,
    pub state: String,
    pub accounts: Vec<String>,
    pub blockers: Vec<Blocker>,
    pub metrics: serde_json::Value,
    pub ledger_revision: String,
    pub statements: Vec<Evidence>,
    pub last_summary: Option<Summary>,
    pub history_count: usize,
}
struct Paths {
    ledger: PathBuf,
    records: PathBuf,
    root: PathBuf,
    close: PathBuf,
}
fn paths() -> Result<Paths, String> {
    let records = reconciliation::private_path()?;
    let close = records.with_file_name("monthly-close.json");
    managed_import::reject_reparse_ancestry(&close)?;
    Ok(Paths {
        ledger: PathBuf::from(crate::config::get("ledger_path")),
        records,
        root: crate::statements::resolve_statements_base_dir(),
        close,
    })
}
fn document(path: &Path) -> Result<(Document, String), String> {
    let (bytes, revision) = store::read(path)?;
    if revision == "missing" {
        return Ok((
            Document {
                schema_version: 1,
                history: vec![],
            },
            revision,
        ));
    }
    let doc: Document = serde_json::from_slice(&bytes)
        .map_err(|_| "Close summary store corrupt; preserve it for recovery")?;
    if doc.schema_version != 1
        || doc
            .history
            .iter()
            .any(|s| !["closed", "reopened"].contains(&s.action.as_str()))
    {
        return Err("Unsupported close evidence; preserve the store".into());
    }
    Ok((doc, revision))
}
fn period(r: &Request) -> Result<(String, String), String> {
    let start = format!("{}-01", r.month);
    if r.month.len() != 7
        || !ledger_parser::valid_date(&start)
        || !ledger_parser::valid_currency(&r.currency)
    {
        return Err("Choose a valid month and currency/unit".into());
    }
    let next = obligations::add_months(&start, 1).ok_or("Invalid month")?;
    let end = obligations::add_days(&next, -1).ok_or("Invalid month")?;
    if r.accounts.len() > 1000 || r.exceptions.len() > 1000 {
        return Err("Close scope exceeds 1000 accounts".into());
    }
    let mut scope = BTreeSet::new();
    for account in r
        .accounts
        .iter()
        .chain(r.exceptions.iter().map(|e| &e.account))
    {
        if !ledger_parser::account_ok(account)
            || !(account.starts_with("Assets:") || account.starts_with("Liabilities:"))
            || !scope.insert(account)
        {
            return Err(
                "Choose unique asset/liability accounts with no overlapping exceptions".into(),
            );
        }
    }
    if r.exceptions
        .iter()
        .any(|e| e.reason.trim().len() < 8 || e.reason.len() > 2000)
    {
        return Err("Explain each scoped account exception (8–2000 characters)".into());
    }
    Ok((start, end))
}
fn blocker(stage: &str, message: String, view: &str) -> Blocker {
    Blocker {
        stage: stage.into(),
        owner: "Household reviewer".into(),
        message,
        view: view.into(),
    }
}
struct Capture {
    snapshot: Snapshot,
    reconciliation_revision: String,
    sync_revision: String,
    _files: Vec<File>,
    _records_lock: File,
}
fn capture(
    paths: &Paths,
    r: &Request,
    today: &str,
    sync: &[managed_import::ItemOutcome],
    busy: bool,
) -> Result<Capture, String> {
    let (start, end) = period(r)?;
    managed_import::reject_reparse_ancestry(&paths.ledger)?;
    let records_lock = store::lock(&paths.records)?;
    let (records, reconciliation_revision) = reconciliation::records_for_close(&paths.records)?;
    let (ledger_file, bytes) = reconciliation::locked_read(&paths.ledger)?;
    let ledger_revision = store::hash(&bytes);
    let txs = ledger_parser::parse_with_currency(
        std::str::from_utf8(&bytes).map_err(|_| "Ledger is not UTF-8")?,
        &r.currency,
    )
    .map_err(|e| format!("Ledger not validated: {}", e.join("; ")))?;
    let analysis = household_analysis::analyze(
        &txs,
        ledger_revision.clone(),
        household_analysis::RangeRequest {
            start: start.clone(),
            end: end.clone(),
            currency: r.currency.clone(),
            comparison_start: None,
            comparison_end: None,
        },
    )?;
    let metrics = serde_json::to_value(&analysis.months).map_err(|e| e.to_string())?;
    let mut blockers = vec![];
    if end.as_str() >= today {
        blockers.push(blocker(
            "summarize",
            "The selected month has not ended; refresh after month end".into(),
            "overview",
        ));
    }
    if r.accounts.is_empty() {
        blockers.push(blocker(
            "collect",
            "Select at least one account requiring statement evidence".into(),
            "statements",
        ));
    }
    let mut accounts: BTreeSet<String> = txs
        .iter()
        .filter(|t| t.date <= end)
        .flat_map(|t| t.postings.iter())
        .filter(|p| {
            p.currency == r.currency
                && (p.account.starts_with("Assets:") || p.account.starts_with("Liabilities:"))
        })
        .map(|p| p.account.clone())
        .collect();
    let relevant: Vec<_> = records
        .iter()
        .filter(|record| {
            record.draft.currency == r.currency
                && record.draft.period_start <= end
                && record.draft.period_end >= start
        })
        .collect();
    accounts.extend(relevant.iter().map(|record| record.draft.account.clone()));
    for account in &accounts {
        if !r.accounts.contains(account) && !r.exceptions.iter().any(|e| &e.account == account) {
            blockers.push(blocker(
                "collect",
                format!(
                    "{account}: require statement coverage or record an explicit scope exception"
                ),
                "statements",
            ));
        }
    }
    for account in r
        .accounts
        .iter()
        .chain(r.exceptions.iter().map(|e| &e.account))
    {
        if !accounts.contains(account) {
            blockers.push(blocker(
                "collect",
                format!("{account}: absent from period ledger and mapped statement evidence"),
                "statements",
            ));
        }
    }
    if busy {
        blockers.push(blocker(
            "import",
            "Finish or abandon the active sync review/import before closing".into(),
            "sync",
        ));
    }
    for outcome in sync.iter().filter(|o| o.state != "succeeded") {
        blockers.push(blocker(
            "import",
            format!(
                "{}: {}",
                if outcome.filename.is_empty() {
                    "Sync outcome store"
                } else {
                    &outcome.filename
                },
                outcome.message
            ),
            "sync",
        ));
    }
    let pending = txs
        .iter()
        .filter(|t| {
            t.date <= end && t.flag != '*' && t.postings.iter().any(|p| p.currency == r.currency)
        })
        .count();
    if pending > 0 {
        blockers.push(blocker(
            "review",
            format!("{pending} pending entries through {end}; resolve them before closing"),
            "reviews",
        ));
    }
    let mut files = vec![ledger_file];
    let mut opened: BTreeMap<String, Result<(String, String), String>> = BTreeMap::new();
    let mut statements = vec![];
    for exception in &r.exceptions {
        if relevant
            .iter()
            .any(|record| record.draft.account == exception.account)
        {
            blockers.push(blocker("reconcile", format!("{}: mapped statement evidence exists; select and reconcile this account instead of excluding it", exception.account), "statements"));
        }
    }
    for account in &r.accounts {
        let mut intervals = vec![];
        for record in relevant
            .iter()
            .filter(|record| &record.draft.account == account)
        {
            let d = &record.draft;
            let stage = match d.status.as_str() {
                "received" => "import",
                "imported" => "review",
                _ => "reconcile",
            };
            let evidence = opened
                .entry(d.statement_path.clone())
                .or_insert_with(|| {
                    reconciliation::evidence(d, &paths.root).map(|(file, hash, reference)| {
                        files.push(file);
                        (hash, reference)
                    })
                })
                .clone();
            match evidence {
                Ok((hash, reference)) => {
                    statements.push(Evidence {
                        record_id: d.id.clone(),
                        account: account.clone(),
                        reference,
                        sha256: hash.clone(),
                        start: d.period_start.clone(),
                        end: d.period_end.clone(),
                    });
                    if hash != record.statement_id
                        || d.status != "reconciled"
                        || record
                            .comparison
                            .as_ref()
                            .is_none_or(|c| c.ledger_revision != ledger_revision)
                        || !reconciliation::compare_bytes(d, &bytes)?.can_reconcile
                    {
                        blockers.push(blocker(stage, format!("{account}: statement evidence is incomplete, changed or unreconciled ({} to {})", d.period_start, d.period_end), "statements"));
                    } else {
                        intervals.push((d.period_start.clone(), d.period_end.clone()));
                    }
                }
                Err(error) => blockers.push(blocker(
                    "collect",
                    format!("{account}: {error}"),
                    "statements",
                )),
            }
        }
        intervals.sort();
        let mut next = start.clone();
        for (a, b) in intervals {
            if a > next {
                break;
            }
            if b >= next {
                next = obligations::add_days(&b, 1).ok_or("Invalid statement period")?;
            }
        }
        if next <= end {
            blockers.push(blocker("reconcile", format!("{account}: reconciled statement coverage does not span {start} through {end}; next uncovered date {next}"), "statements"));
        }
    }
    if analysis
        .months
        .iter()
        .any(|m| m.closing_net_worth.is_none() || m.unexplained.is_none_or(|v| !v.is_zero()))
    {
        blockers.push(blocker(
            "summarize",
            "Period metrics are unavailable or contain unexplained movements".into(),
            "wealth",
        ));
    }
    let sync_revision = store::hash(&serde_json::to_vec(&(sync, busy)).map_err(|e| e.to_string())?);
    let evidence_revision = store::hash(
        &serde_json::to_vec(&(
            r,
            &ledger_revision,
            &reconciliation_revision,
            &sync_revision,
            &statements,
            &metrics,
            &blockers,
        ))
        .map_err(|e| e.to_string())?,
    );
    let (doc, revision) = document(&paths.close)?;
    let last_summary = doc
        .history
        .iter()
        .rev()
        .find(|s| s.scope.month == r.month && s.scope.currency == r.currency)
        .cloned();
    let state = match &last_summary {
        Some(s) if s.action == "reopened" => "reopened",
        Some(s) if s.evidence_revision == evidence_revision && blockers.is_empty() => "closed",
        Some(_) => "stale",
        None => "open",
    }
    .into();
    Ok(Capture {
        snapshot: Snapshot {
            revision,
            evidence_revision,
            state,
            accounts: accounts.into_iter().collect(),
            blockers,
            metrics,
            ledger_revision,
            statements,
            last_summary,
            history_count: doc.history.len(),
        },
        reconciliation_revision,
        sync_revision,
        _files: files,
        _records_lock: records_lock,
    })
}
fn save_at(
    paths: &Paths,
    request: Request,
    expected_revision: &str,
    expected_evidence: &str,
    action: &str,
    today: &str,
    sync: &[managed_import::ItemOutcome],
    busy: bool,
) -> Result<Snapshot, String> {
    if !["closed", "reopened"].contains(&action) {
        return Err("Unknown monthly-close action".into());
    }
    let _lock = store::lock(&paths.close)?;
    let mut capture = capture(paths, &request, today, sync, busy)?;
    let snapshot = &mut capture.snapshot;
    if snapshot.revision != expected_revision || snapshot.evidence_revision != expected_evidence {
        return Err(
            "Close evidence changed; refresh before saving. Existing summaries preserved".into(),
        );
    }
    if action == "closed" && !snapshot.blockers.is_empty() {
        return Err("Close blocked: resolve every listed blocker".into());
    }
    if action == "reopened" && snapshot.last_summary.is_none() {
        return Err("No saved close to reopen".into());
    }
    let (mut doc, revision) = document(&paths.close)?;
    doc.history.push(Summary {
        scope: request,
        action: action.into(),
        saved_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        evidence_revision: snapshot.evidence_revision.clone(),
        ledger_revision: snapshot.ledger_revision.clone(),
        reconciliation_revision: capture.reconciliation_revision.clone(),
        sync_revision: capture.sync_revision,
        statements: snapshot.statements.clone(),
        metrics: snapshot.metrics.clone(),
        unresolved_exceptions: snapshot.blockers.clone(),
    });
    if store::read(&paths.records)?.1 != capture.reconciliation_revision {
        return Err("Reconciliation evidence changed before publication; refresh".into());
    }
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
    let new_revision = store::save(&paths.close, &revision, &bytes)?;
    let (readback, actual) = document(&paths.close)?;
    if actual != new_revision || readback.history.len() != doc.history.len() {
        return Err("Close readback failed; refresh before retrying".into());
    }
    snapshot.revision = actual;
    snapshot.state = action.into();
    snapshot.last_summary = readback.history.last().cloned();
    snapshot.history_count = readback.history.len();
    Ok(capture.snapshot)
}
#[tauri::command]
pub fn get_monthly_close(request: Request) -> Result<Snapshot, String> {
    let sync = crate::sync::monthly_close_outcomes();
    Ok(capture(
        &paths()?,
        &request,
        &obligations::current_date_utc(),
        &sync,
        crate::sync::monthly_close_busy(),
    )?
    .snapshot)
}
#[tauri::command]
pub fn save_monthly_close(
    request: Request,
    expected_revision: String,
    expected_evidence: String,
    action: String,
) -> Result<Snapshot, String> {
    let sync = crate::sync::monthly_close_outcomes();
    save_at(
        &paths()?,
        request,
        &expected_revision,
        &expected_evidence,
        &action,
        &obligations::current_date_utc(),
        &sync,
        crate::sync::monthly_close_busy(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use std::fs;
    const LEDGER: &str = "2025-12-31 * \"Opening\"\n  Assets:Bank:A 100 USD\n  Equity:Opening -100 USD\n2026-01-10 * \"Groceries\"\n  Assets:Bank:A -10 USD\n  Expenses:Food 10 USD\n";
    fn fixture() -> (PathBuf, Paths, Request) {
        let root = std::env::temp_dir().join(format!(
            "recon-close-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let paths = Paths {
            ledger: root.join("ledger.bean"),
            records: root.join("private/records.json"),
            root: root.clone(),
            close: root.join("private/monthly-close.json"),
        };
        fs::write(&paths.ledger, LEDGER).unwrap();
        fs::write(root.join("Bank.pdf"), b"synthetic statement evidence").unwrap();
        let request = Request {
            month: "2026-01".into(),
            currency: "USD".into(),
            accounts: vec!["Assets:Bank:A".into()],
            exceptions: vec![],
        };
        (root, paths, request)
    }
    fn reconcile(paths: &Paths, start: &str, end: &str) {
        reconcile_account(paths, "Assets:Bank:A", "100", "90", start, end);
    }
    fn reconcile_account(
        paths: &Paths,
        account: &str,
        opening: &str,
        closing: &str,
        start: &str,
        end: &str,
    ) {
        let mut draft = reconciliation::Draft {
            id: String::new(),
            statement_path: paths.root.join("Bank.pdf").to_string_lossy().into_owned(),
            account: account.into(),
            period_start: start.into(),
            period_end: end.into(),
            currency: "USD".into(),
            status: "received".into(),
            opening_balance: Some(Money::parse(opening).unwrap()),
            closing_balance: Some(Money::parse(closing).unwrap()),
            matched_transaction_ids: vec![],
        };
        let bytes = fs::read(&paths.ledger).unwrap();
        let comparison = reconciliation::compare_bytes(&draft, &bytes).unwrap();
        draft.matched_transaction_ids = comparison
            .candidates
            .iter()
            .filter(|c| c.in_period)
            .map(|c| c.id.clone())
            .collect();
        let mut revision = store::read(&paths.records).unwrap().1;
        for status in ["received", "imported", "reviewed", "reconciled"] {
            draft.status = status.into();
            let snapshot = reconciliation::save_at(
                &paths.records,
                &paths.ledger,
                &paths.root,
                draft.clone(),
                &revision,
            )
            .unwrap();
            revision = snapshot.revision;
            draft.id = snapshot.records.last().unwrap().draft.id.clone();
        }
    }
    fn inspect(paths: &Paths, request: &Request) -> Snapshot {
        capture(paths, request, "2026-02-01", &[], false)
            .unwrap()
            .snapshot
    }
    #[test]
    fn shared_pdf_accounts_are_verified_once_and_sources_remain_locked_through_capture() {
        let (root, paths, mut request) = fixture();
        fs::write(&paths.ledger, format!("{LEDGER}2025-12-31 * \"Second opening\"\n  Assets:Bank:B 20 USD\n  Equity:Opening -20 USD\n")).unwrap();
        reconcile(&paths, "2026-01-01", "2026-01-31");
        reconcile_account(
            &paths,
            "Assets:Bank:B",
            "20",
            "20",
            "2026-01-01",
            "2026-01-31",
        );
        request.accounts.push("Assets:Bank:B".into());
        let held = capture(&paths, &request, "2026-02-01", &[], false).unwrap();
        assert!(held.snapshot.blockers.is_empty());
        assert_eq!(held.snapshot.statements.len(), 2);
        assert!(String::from_utf8(fs::read(&paths.ledger).unwrap())
            .unwrap()
            .contains("Second opening"));
        assert_eq!(
            fs::read(paths.root.join("Bank.pdf")).unwrap(),
            b"synthetic statement evidence"
        );
        assert!(fs::write(&paths.ledger, b"blocked synthetic change").is_err());
        assert!(fs::write(paths.root.join("Bank.pdf"), b"blocked synthetic change").is_err());
        assert!(store::lock(&paths.records).is_err());
        drop(held);
        let snapshot = inspect(&paths, &request);
        assert!(save_at(
            &paths,
            request.clone(),
            "wrong revision",
            &snapshot.evidence_revision,
            "closed",
            "2026-02-01",
            &[],
            false
        )
        .unwrap_err()
        .contains("changed"));
        assert!(!paths.close.exists());
        request.accounts.pop();
        request.exceptions.push(ScopeException {
            account: "Assets:Bank:B".into(),
            reason: "An explicit reason cannot waive existing mapped evidence".into(),
        });
        assert!(inspect(&paths, &request)
            .blockers
            .iter()
            .any(|b| b.message.contains("mapped statement evidence exists")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn complete_close_persists_exact_metrics_and_statement_provenance_then_reopens_without_ledger_writes(
    ) {
        let (root, paths, request) = fixture();
        reconcile(&paths, "2026-01-01", "2026-01-31");
        let before = fs::read(&paths.ledger).unwrap();
        let snapshot = inspect(&paths, &request);
        assert!(snapshot.blockers.is_empty());
        assert_eq!(snapshot.metrics[0]["spending"], "10");
        let saved = save_at(
            &paths,
            request.clone(),
            &snapshot.revision,
            &snapshot.evidence_revision,
            "closed",
            "2026-02-01",
            &[],
            false,
        )
        .unwrap();
        assert_eq!(saved.state, "closed");
        let readback = inspect(&paths, &request);
        assert_eq!(readback.state, "closed");
        let summary = readback.last_summary.unwrap();
        assert_eq!(summary.statements.len(), 1);
        assert_eq!(summary.statements[0].reference, "Bank.pdf");
        assert_eq!(summary.ledger_revision, store::hash(&before));
        assert!(summary.unresolved_exceptions.is_empty());
        let reopened = save_at(
            &paths,
            request,
            &saved.revision,
            &saved.evidence_revision,
            "reopened",
            "2026-02-01",
            &[],
            false,
        )
        .unwrap();
        assert_eq!(reopened.state, "reopened");
        assert_eq!(reopened.history_count, 2);
        assert_eq!(fs::read(&paths.ledger).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_coverage_pending_entries_sync_failures_and_active_imports_block_close() {
        let (root, paths, request) = fixture();
        let missing = inspect(&paths, &request);
        assert!(missing.blockers.iter().any(|b| b.stage == "reconcile"));
        assert!(save_at(
            &paths,
            request.clone(),
            &missing.revision,
            &missing.evidence_revision,
            "closed",
            "2026-02-01",
            &[],
            false
        )
        .unwrap_err()
        .contains("blocked"));
        assert!(!paths.close.exists());
        fs::write(
            &paths.ledger,
            LEDGER.replace("2026-01-10 *", "2026-01-10 !"),
        )
        .unwrap();
        let sync = vec![managed_import::ItemOutcome {
            item_id: "fake".into(),
            filename: "Synthetic.pdf".into(),
            state: "failed".into(),
            retry_safe: false,
            message: "Synthetic failure".into(),
            operation_id: None,
        }];
        let snapshot = capture(&paths, &request, "2026-02-01", &sync, true)
            .unwrap()
            .snapshot;
        assert!(snapshot.blockers.iter().any(|b| b.stage == "review"));
        assert_eq!(
            snapshot
                .blockers
                .iter()
                .filter(|b| b.stage == "import")
                .count(),
            2
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn evidence_changes_make_saved_close_stale_and_old_proof_cannot_publish() {
        let (root, paths, request) = fixture();
        reconcile(&paths, "2026-01-01", "2026-01-31");
        let initial = inspect(&paths, &request);
        let saved = save_at(
            &paths,
            request.clone(),
            &initial.revision,
            &initial.evidence_revision,
            "closed",
            "2026-02-01",
            &[],
            false,
        )
        .unwrap();
        fs::write(paths.root.join("Bank.pdf"), b"changed synthetic evidence").unwrap();
        let stale = inspect(&paths, &request);
        assert_eq!(stale.state, "stale");
        assert!(!stale.blockers.is_empty());
        let bytes = fs::read(&paths.close).unwrap();
        assert!(save_at(
            &paths,
            request.clone(),
            &saved.revision,
            &saved.evidence_revision,
            "closed",
            "2026-02-01",
            &[],
            false
        )
        .unwrap_err()
        .contains("changed"));
        assert_eq!(fs::read(&paths.close).unwrap(), bytes);
        fs::write(paths.root.join("Bank.pdf"), b"synthetic statement evidence").unwrap();
        fs::write(&paths.ledger, format!("{LEDGER}\n; external revision\n")).unwrap();
        assert_eq!(inspect(&paths, &request).state, "stale");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn partial_statement_period_cannot_close_whole_month_and_uncovered_accounts_require_explicit_scope(
    ) {
        let (root, paths, mut request) = fixture();
        reconcile(&paths, "2026-01-01", "2026-01-15");
        assert!(inspect(&paths, &request)
            .blockers
            .iter()
            .any(|b| b.message.contains("2026-01-16")));
        fs::write(&paths.ledger, format!("{LEDGER}2026-01-20 * \"Cash transfer\"\n  Assets:Cash 2 USD\n  Assets:Bank:A -2 USD\n")).unwrap();
        assert!(inspect(&paths, &request)
            .blockers
            .iter()
            .any(|b| b.message.contains("Assets:Cash: require")));
        request.exceptions.push(ScopeException {
            account: "Assets:Cash".into(),
            reason:
                "No external cash statement exists; physical cash verification recorded separately"
                    .into(),
        });
        assert!(!inspect(&paths, &request)
            .blockers
            .iter()
            .any(|b| b.message.contains("Assets:Cash: require")));
        let before_end = capture(&paths, &request, "2026-01-31", &[], false)
            .unwrap()
            .snapshot;
        assert!(before_end
            .blockers
            .iter()
            .any(|b| b.message.contains("has not ended")));
        fs::remove_dir_all(root).unwrap();
    }
}
