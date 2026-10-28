//! Native, fail-closed subset of statement importing. The external analyzer is
//! advisory only; its legacy commit command is deliberately never launched.
use crate::{config, money::Money};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

// Reconciliation also takes exclusive Windows ledger handles. Serialize the
// application's managed readers and writers so a refresh cannot make an
// approved batch mistake our own temporary lock for an unavailable ledger.
static MANAGED_IO: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn managed_io() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    MANAGED_IO
        .lock()
        .map_err(|_| "Managed import coordination is unavailable".into())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    operation: String,
    identity: String,
    run_id: String,
    item_id: String,
    filename: String,
    date: String,
    source_hash: String,
    source_size: u64,
    destination: String,
    archive_hash: String,
    entry_hash: String,
    entry: String,
    ledger_before_hash: String,
    ledger_before_size: u64,
    ledger_backup: String,
    state: String,
    retry_safe: bool,
    message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duplicate_override: Option<DuplicateOverride>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    batch_override: Option<BatchOverride>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchOverride {
    candidate_ids: Vec<String>,
    ledger_revision: String,
    funding_account: String,
    review_context: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DuplicateOverride {
    candidate_ids: Vec<String>,
    ledger_revision: String,
    funding_account: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemOutcome {
    pub item_id: String,
    pub filename: String,
    pub state: String,
    pub retry_safe: bool,
    pub message: String,
    #[serde(default)]
    pub operation_id: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ImportConfig {
    pub(crate) inbox: PathBuf,
    archive: PathBuf,
    ledger: PathBuf,
    store: PathBuf,
    currency: String,
    counter: String,
}
impl ImportConfig {
    pub(crate) fn currency(&self) -> &str {
        &self.currency
    }
    /// Explicit dependency injection for native tests and isolated store operations.
    #[cfg(test)]
    pub(crate) fn test_ledger(&self) -> &Path {
        &self.ledger
    }
    pub(crate) fn injected(
        inbox: PathBuf,
        archive: PathBuf,
        ledger: PathBuf,
        store: PathBuf,
        currency: String,
        counter: String,
    ) -> Self {
        Self {
            inbox,
            archive,
            ledger,
            store,
            currency,
            counter,
        }
    }
}

/// Private coordinator-owned approval material. It is never accepted from IPC.
#[derive(Clone, Debug)]
pub(crate) struct PreparedPlan {
    item: serde_json::Value,
    source: PathBuf,
    source_hash: String,
    source_size: u64,
    destination: PathBuf,
    ledger_before: Vec<u8>,
    ledger_before_hash: String,
    entry: String,
    funding_account: Option<String>,
    duplicates: Vec<crate::sync_duplicates::DuplicateMatch>,
    duplicate_override: Option<DuplicateOverride>,
    batch_duplicates: Vec<crate::sync_duplicates::BatchMatch>,
    batch_override: Option<BatchOverride>,
    already_imported: bool,
}
impl PreparedPlan {
    pub(crate) fn already_imported(&self) -> bool {
        self.already_imported
    }
    pub(crate) fn set_batch_duplicates(
        &mut self,
        matches: Vec<crate::sync_duplicates::BatchMatch>,
    ) {
        self.batch_duplicates = matches;
    }
    pub(crate) fn duplicates(&self) -> &[crate::sync_duplicates::DuplicateMatch] {
        &self.duplicates
    }
    pub(crate) fn source_path(&self) -> &Path {
        &self.source
    }
}

pub(crate) fn decide_batch(
    mut plan: PreparedPlan,
    decision: Option<&str>,
    reviewed_context: Option<&str>,
    current_context: &str,
) -> Result<PreparedPlan, String> {
    if current_context.len() > 262144 {
        return Err("Batch review context exceeds supported bounds".into());
    }
    if decision.is_some_and(|s| s != "import_reviewed") {
        return Err("Skipped batch receipts must be deselected".into());
    }
    if !plan.batch_duplicates.is_empty() {
        if decision != Some("import_reviewed") || reviewed_context != Some(current_context) {
            return Err("Review batch overlaps for every selected receipt after finalizing funding and selection".into());
        }
        plan.batch_override = Some(BatchOverride {
            candidate_ids: plan.batch_duplicates.iter().map(|m| m.id.clone()).collect(),
            ledger_revision: plan.ledger_before_hash.clone(),
            funding_account: plan
                .funding_account
                .clone()
                .ok_or("Assign funding before batch review")?,
            review_context: current_context.into(),
        });
    }
    Ok(plan)
}

pub(crate) fn decide_duplicates(
    mut plan: PreparedPlan,
    decision: Option<&str>,
) -> Result<PreparedPlan, String> {
    if decision.is_some_and(|s| s != "import_separately") {
        return Err("Already-recorded receipts must be deselected, not imported".into());
    }
    if !plan.duplicates.is_empty() {
        if decision != Some("import_separately") {
            return Err("Review possible ledger duplicates and explicitly choose separate payment before approval".into());
        }
        plan.duplicate_override = Some(DuplicateOverride {
            candidate_ids: plan.duplicates.iter().map(|m| m.id.clone()).collect(),
            ledger_revision: plan.ledger_before_hash.clone(),
            funding_account: plan
                .funding_account
                .clone()
                .ok_or("Assign funding before duplicate review")?,
        });
    }
    Ok(plan)
}

pub(crate) fn runtime_config() -> Result<ImportConfig, String> {
    let inbox = canonical_root(config::get("sync_inbox_path"))?;
    let archive = canonical_root(config::get("statement_archive_path"))?;
    reject_reparse_ancestry(Path::new(config::get("ledger_path")))?;
    let ledger = fs::canonicalize(config::get("ledger_path"))
        .map_err(|_| "Managed ledger is unavailable".to_owned())?;
    let store = private_dir(&ledger)?;
    Ok(ImportConfig::injected(
        inbox,
        archive,
        ledger,
        store,
        config::get("ledger_currency").to_owned(),
        config::get("sync_counter_account").to_owned(),
    ))
}

pub(crate) fn bound_or_load_config<F>(
    bound: Option<ImportConfig>,
    load: F,
) -> Result<ImportConfig, String>
where
    F: FnOnce() -> Result<ImportConfig, String>,
{
    match bound {
        Some(cfg) => Ok(cfg),
        None => load(),
    }
}

pub(crate) fn prepare(
    item: &serde_json::Value,
    cfg: &ImportConfig,
) -> Result<PreparedPlan, String> {
    let _io = managed_io()?;
    prepare_inner(item, cfg, Some(&cfg.counter))
}

pub(crate) fn prepare_review(
    item: &serde_json::Value,
    cfg: &ImportConfig,
) -> Result<PreparedPlan, String> {
    let _io = managed_io()?;
    prepare_inner(item, cfg, None)
}

/// Read declared account names only after validating the complete ledger.
fn funding_accounts_in(
    bytes: &[u8],
    currency: &str,
    receipt_date: Option<&str>,
) -> Result<Vec<String>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Ledger is not valid UTF-8")?;
    crate::ledger_parser::parse_with_currency(text, currency)
        .map_err(|_| "Ledger does not validate in the supported subset")?;
    let mut accounts = std::collections::BTreeSet::new();
    for line in text
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
    {
        let fields: Vec<_> = line
            .split(';')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        if fields.len() >= 3
            && fields[1] == "open"
            && receipt_date.is_none_or(|date| fields[0] <= date)
            && account_root_is(fields[2], &["Assets", "Liabilities"])
            && !fields[2].ends_with(":Unsorted")
            && !fields[2].ends_with(":Unassigned")
            && fields.get(3).is_none_or(|unit| *unit == currency)
        {
            accounts.insert(fields[2].to_owned());
        }
    }
    Ok(accounts.into_iter().collect())
}

pub(crate) fn funding_accounts(cfg: &ImportConfig) -> Result<Vec<String>, String> {
    let _io = managed_io()?;
    funding_accounts_in(
        &fs::read(&cfg.ledger).map_err(|_| "Ledger is unavailable")?,
        &cfg.currency,
        None,
    )
}

/// Bind the reviewer's choice to the original analysis-time source and ledger
/// evidence. Never re-prepare against newer files while approving an old run.
pub(crate) fn assign_funding(
    mut plan: PreparedPlan,
    cfg: &ImportConfig,
    account: &str,
) -> Result<PreparedPlan, String> {
    let _io = managed_io()?;
    if !funding_accounts_in(
        &plan.ledger_before,
        &cfg.currency,
        plan.item["date"].as_str(),
    )?
    .iter()
    .any(|a| a == account)
    {
        return Err("Select an existing funding account; unassigned and placeholder accounts cannot be approved".into());
    }
    let operation = identity(&cfg.ledger, &plan.source_hash);
    let entry = build_entry(&plan.item, "", &operation, &cfg.currency, account)?;
    if let Ok(raw) = fs::read(cfg.store.join(format!("{operation}.json"))) {
        let prior: Receipt =
            serde_json::from_slice(&raw).map_err(|_| "Prior import receipt is invalid")?;
        if !receipt_valid(&prior, &format!("{operation}.json"), cfg) || prior.entry != entry {
            return Err("Existing import is bound to a different reviewed entry; reconcile it before changing funding".into());
        }
    }
    let mut candidate = plan.ledger_before.clone();
    if !candidate.is_empty() && !candidate.ends_with(b"\n") {
        candidate.push(b'\n');
    }
    candidate.extend_from_slice(entry.as_bytes());
    crate::ledger_parser::parse_with_currency(
        std::str::from_utf8(&candidate).map_err(|_| "Ledger is not valid UTF-8")?, &cfg.currency,
    ).map_err(|_| "Selected funding account or transaction does not validate for this receipt date and currency")?;
    plan.entry = entry;
    plan.funding_account = Some(account.to_owned());
    Ok(plan)
}

fn prepare_inner(
    item: &serde_json::Value,
    cfg: &ImportConfig,
    funding: Option<&str>,
) -> Result<PreparedPlan, String> {
    let filename = item["filename"]
        .as_str()
        .filter(|s| safe_name(s))
        .ok_or("Unsafe source filename")?;
    let new_name = item["new_name"]
        .as_str()
        .filter(|s| safe_name(s))
        .ok_or("Unsafe archive filename")?;
    if item["was_encrypted"].as_bool() != Some(false) || item["is_cleared"].as_bool().is_none() {
        return Err("Encrypted or unsupported statement flags".into());
    }
    let date = item["date"]
        .as_str()
        .ok_or("Analysis date is unsupported")?;
    if !valid_date(date) {
        return Err("Analysis date is invalid".into());
    }
    let source =
        fs::canonicalize(cfg.inbox.join(filename)).map_err(|_| "Approved source is unavailable")?;
    if !within(&source, &cfg.inbox) {
        return Err("Approved source escapes configured inbox".into());
    }
    let mut sf = exclusive(&source, false)?;
    let _source_range = RangeGuard::acquire(&sf)?;
    let mut bytes = Vec::new();
    sf.read_to_end(&mut bytes)
        .map_err(|_| "Unable to read approved source")?;
    drop(sf);
    let source_hash = hash(&bytes);
    let operation_id = identity(&cfg.ledger, &source_hash);
    let year = &date[..4];
    let destination = cfg
        .archive
        .join(format!("Statements {year}"))
        .join(new_name);
    reject_reparse_ancestry(&destination)?;
    let prior = cfg.store.join(format!("{operation_id}.json"));
    let prior_succeeded = fs::read(&prior)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Receipt>(&raw).ok())
        .is_some_and(|r| {
            receipt_valid(&r, &format!("{operation_id}.json"), cfg)
                && r.state == "succeeded"
                && fs::canonicalize(&r.destination)
                    .ok()
                    .is_some_and(|p| within(&p, &cfg.archive))
                && fs::read(&r.destination)
                    .is_ok_and(|b| b.len() as u64 == r.source_size && hash(&b) == r.archive_hash)
                && fs::read(&cfg.ledger).is_ok_and(|b| exact_entry_present(&b, r.entry.as_bytes()))
        });
    if destination.exists() && !prior_succeeded {
        return Err("Archive destination already exists".into());
    }
    let ledger_before = fs::read(&cfg.ledger).map_err(|_| "Ledger is unavailable")?;
    // Validate analyzer tokens without choosing a default funding account.
    let validated_entry = build_entry(
        item,
        "",
        &operation_id,
        &cfg.currency,
        funding.unwrap_or("Assets:FundingPending"),
    )?;
    let entry = if funding.is_some() {
        validated_entry
    } else {
        String::new()
    };
    if funding.is_none() {
        funding_accounts_in(&ledger_before, &cfg.currency, None)?;
    }
    if !entry.is_empty() {
        let mut candidate = ledger_before.clone();
        if !candidate.is_empty() && !candidate.ends_with(b"\n") {
            candidate.push(b'\n');
        }
        candidate.extend_from_slice(entry.as_bytes());
        let candidate = std::str::from_utf8(&candidate).map_err(|_| "Ledger is not valid UTF-8")?;
        crate::ledger_parser::parse_with_currency(candidate, &cfg.currency).map_err(|_| {
            "Generated transaction does not validate in the supported ledger subset"
        })?;
    }
    let duplicates = if funding.is_none() && !prior_succeeded {
        crate::sync_duplicates::candidates(&ledger_before, item, &cfg.currency)?
    } else {
        vec![]
    };
    Ok(PreparedPlan {
        item: item.clone(),
        source,
        source_hash,
        source_size: bytes.len() as u64,
        destination,
        ledger_before_hash: hash(&ledger_before),
        ledger_before,
        entry,
        funding_account: funding.map(str::to_owned),
        duplicates,
        duplicate_override: None,
        batch_duplicates: vec![],
        batch_override: None,
        already_imported: prior_succeeded,
    })
}

#[derive(Clone, Copy, Default)]
struct FaultPlan {
    before_journal: bool,
    after_intent: bool,
    between_archive_and_proof: bool,
    after_archive: bool,
    after_ledger: bool,
    before_outcome: bool,
}

fn hash(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
pub(crate) fn safe_name(s: &str) -> bool {
    if s.is_empty()
        || s.len() > 180
        || s.ends_with(['.', ' '])
        || s.contains(['/', '\\', ':'])
        || s.chars().any(|c| {
            c.is_control() || c == '<' || c == '>' || c == '"' || c == '|' || c == '?' || c == '*'
        })
    {
        return false;
    }
    let stem = s
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    !matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}
fn valid_date(s: &str) -> bool {
    crate::ledger_parser::valid_date(s)
}
fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 160
        && s.split(':').all(|p| {
            !p.is_empty()
                && p.as_bytes()[0].is_ascii_uppercase()
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
fn account_root_is(s: &str, roots: &[&str]) -> bool {
    token(s) && s.split(':').count() >= 2 && roots.contains(&s.split(':').next().unwrap_or(""))
}
#[cfg(windows)]
pub(crate) fn reject_reparse_ancestry(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_attributes() & 0x400 != 0) {
            return Err("Managed sync paths cannot contain reparse points".into());
        }
    }
    Ok(())
}
#[cfg(not(windows))]
pub(crate) fn reject_reparse_ancestry(_: &Path) -> Result<(), String> {
    Ok(())
}
fn canonical_root(p: &str) -> Result<PathBuf, String> {
    let p = Path::new(p);
    if !p.is_absolute() {
        return Err("Managed sync paths must be configured as absolute paths".into());
    }
    reject_reparse_ancestry(p)?;
    fs::canonicalize(p).map_err(|_| "Managed sync folder is unavailable".into())
}
fn within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}
fn namespace(ledger: &Path) -> String {
    hash(ledger.to_string_lossy().to_lowercase().as_bytes())
}
fn private_dir(ledger: &Path) -> Result<PathBuf, String> {
    #[cfg(not(windows))]
    {
        let _ = ledger;
        return Err("Managed imports require Windows exclusive file locks".into());
    }
    #[cfg(windows)]
    {
        let root = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("Private sync journal unavailable")?;
        let root = fs::canonicalize(root).map_err(|_| "Private sync journal unavailable")?;
        if root.ancestors().any(|p| p.join(".git").exists()) {
            return Err("Private sync journal must be outside Git checkouts".into());
        }
        let dir = root
            .join("Navis-Recon")
            .join("sync-imports")
            .join(namespace(ledger));
        let mut probe = dir.as_path();
        while !probe.exists() {
            probe = probe
                .parent()
                .ok_or("Private sync journal path is invalid")?;
        }
        let actual = fs::canonicalize(probe).map_err(|_| "Private sync journal unavailable")?;
        if actual.ancestors().any(|p| p.join(".git").exists()) {
            return Err("Private sync journal must be outside Git checkouts".into());
        }
        fs::create_dir_all(&dir).map_err(|_| "Private sync journal unavailable")?;
        let dir = fs::canonicalize(dir).map_err(|_| "Private sync journal unavailable")?;
        if dir.ancestors().any(|p| p.join(".git").exists()) {
            return Err("Private sync journal must be outside Git checkouts".into());
        }
        Ok(dir)
    }
}
#[cfg(windows)]
fn exclusive(path: &Path, write: bool) -> Result<fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(write)
        .share_mode(0)
        .open(path)
        .map_err(|_| "A source or ledger file is busy".to_owned())
}

#[cfg(windows)]
fn journal_lock(dir: &Path) -> Result<fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(0)
        .open(dir.join("journal.lock"))
        .map_err(|_| "Managed sync journal is busy".to_owned())
}
#[cfg(not(windows))]
fn journal_lock(_: &Path) -> Result<fs::File, String> {
    Err("Managed imports require Windows exclusive file locks".into())
}

#[cfg(windows)]
struct RangeGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
    overlapped: windows_sys::Win32::System::IO::OVERLAPPED,
}
#[cfg(windows)]
impl RangeGuard {
    fn acquire(file: &fs::File) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{LockFileEx, LOCKFILE_EXCLUSIVE_LOCK};
        let mut g = Self {
            handle: file.as_raw_handle() as _,
            overlapped: Default::default(),
        };
        if unsafe {
            LockFileEx(
                g.handle,
                LOCKFILE_EXCLUSIVE_LOCK,
                0,
                u32::MAX,
                u32::MAX,
                &mut g.overlapped,
            )
        } == 0
        {
            return Err("A source or ledger file is busy".into());
        }
        Ok(g)
    }
}
#[cfg(windows)]
impl Drop for RangeGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
        unsafe {
            UnlockFileEx(self.handle, 0, u32::MAX, u32::MAX, &mut self.overlapped);
        }
    }
}
#[cfg(not(windows))]
struct RangeGuard;
#[cfg(not(windows))]
impl RangeGuard {
    fn acquire(_: &fs::File) -> Result<Self, String> {
        Err("Managed imports require Windows exclusive file locks".into())
    }
}
#[cfg(not(windows))]
fn exclusive(_: &Path, _: bool) -> Result<fs::File, String> {
    Err("Managed imports require Windows exclusive file locks".into())
}

fn persist(dir: &Path, r: &Receipt) -> Result<(), String> {
    let target = dir.join(format!("{}.json", r.identity));
    let stage = dir.join(format!(".stage-{}-{}.tmp", std::process::id(), now()));
    let bytes = serde_json::to_vec(r).map_err(|_| "Unable to serialize private sync receipt")?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)
        .map_err(|_| "Unable to stage private sync receipt")?;
    f.write_all(&bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| {
            let _ = fs::remove_file(&stage);
            "Unable to sync private sync receipt"
        })?;
    drop(f);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let a: Vec<u16> = stage.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                a.as_ptr(),
                b.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            let _ = fs::remove_file(stage);
            return Err("Unable to publish private sync receipt".into());
        }
    }
    #[cfg(not(windows))]
    {
        let _ = fs::remove_file(stage);
        return Err("Managed imports require Windows".into());
    }
    Ok(())
}
fn build_entry(
    item: &serde_json::Value,
    run: &str,
    item_id: &str,
    currency: &str,
    counter: &str,
) -> Result<String, String> {
    let date = item
        .get("date")
        .and_then(serde_json::Value::as_str)
        .ok_or("Analysis date is unsupported")?;
    let filename = item
        .get("new_name")
        .and_then(serde_json::Value::as_str)
        .ok_or("Analysis filename is unsupported")?;
    if !valid_date(date) || !safe_name(filename) {
        return Err("Analysis date or archive filename is invalid".into());
    }
    let category = item
        .get("category")
        .and_then(serde_json::Value::as_str)
        .ok_or("Analysis category is unsupported")?;
    if !account_root_is(category, &["Expenses", "Income"])
        || !account_root_is(counter, &["Assets", "Liabilities", "Equity"])
        || currency.is_empty()
        || !currency
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("Account or currency token is unsupported".into());
    }
    let amt = item
        .get("amount")
        .and_then(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_number().map(ToString::to_string))
        })
        .ok_or("Analysis amount is unsupported")?;
    let amount = Money::parse(&amt).map_err(|_| "Analysis amount is invalid")?;
    let payee = item
        .get("rule")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Statement");
    if payee.len() > 120
        || payee
            .chars()
            .any(|c| c == '"' || c == '\\' || c == '\n' || c == '\r' || c.is_control())
    {
        return Err("Analysis description is unsupported".into());
    }
    let marker = format!("navis_recon_import \"{}\"", item_id);
    if amount.is_zero() {
        return Ok(format!("; {marker} zero-amount\n"));
    }
    let counter_amount = -amount;
    let text = format!("{date} * \"{payee}\" \"{filename}\"\n  ; {marker}\n  {category} {amount} {currency}\n  {counter} {counter_amount} {currency}\n");
    let _ = run;
    Ok(text)
}
fn identity(ledger: &Path, source_hash: &str) -> String {
    hash(format!("{}\0{}", namespace(ledger), source_hash).as_bytes())
}

fn receipt_valid(r: &Receipt, filename: &str, cfg: &ImportConfig) -> bool {
    if let Some(review) = &r.batch_override {
        let is_hash = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
        if review.review_context.len() > 262144
            || review.candidate_ids.is_empty()
            || review.candidate_ids.len() > 100
            || review.candidate_ids.iter().any(|s| !is_hash(s))
            || !is_hash(&review.ledger_revision)
            || review
                .candidate_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != review.candidate_ids.len()
            || !account_root_is(&review.funding_account, &["Assets", "Liabilities"])
            || !r
                .entry
                .lines()
                .any(|line| line.split_whitespace().next() == Some(review.funding_account.as_str()))
        {
            return false;
        }
        let Ok(context) = serde_json::from_str::<Vec<(String, String)>>(&review.review_context)
        else {
            return false;
        };
        if context.is_empty()
            || context.len() > 1000
            || !context
                .iter()
                .any(|(id, account)| id == &r.item_id && account == &review.funding_account)
            || context.windows(2).any(|p| p[0] >= p[1])
            || context.iter().any(|(id, account)| {
                id.is_empty() || !account_root_is(account, &["Assets", "Liabilities"])
            })
        {
            return false;
        }
    }
    if let Some(review) = &r.duplicate_override {
        let is_hash = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
        if review.candidate_ids.is_empty()
            || review.candidate_ids.len() > 100
            || review.candidate_ids.iter().any(|s| !is_hash(s))
            || review
                .candidate_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != review.candidate_ids.len()
            || !is_hash(&review.ledger_revision)
            || !account_root_is(&review.funding_account, &["Assets", "Liabilities"])
            || !r
                .entry
                .lines()
                .any(|line| line.split_whitespace().next() == Some(review.funding_account.as_str()))
        {
            return false;
        }
    }
    let marker = format!("navis_recon_import \"{}\"", r.identity);
    let backup = fs::read(cfg.store.join(&r.ledger_backup)).ok();
    let backup_valid = backup
        .as_deref()
        .is_some_and(|b| b.len() as u64 == r.ledger_before_size && hash(b) == r.ledger_before_hash);
    let candidate_valid = backup.as_deref().is_some_and(|b| {
        let mut candidate = b.to_vec();
        if !candidate.is_empty() && !candidate.ends_with(b"\n") {
            candidate.push(b'\n');
        }
        candidate.extend_from_slice(r.entry.as_bytes());
        std::str::from_utf8(&candidate)
            .ok()
            .is_some_and(|s| crate::ledger_parser::parse_with_currency(s, &cfg.currency).is_ok())
    });
    r.version == 2
        && filename == format!("{}.json", r.identity)
        && r.identity == identity(&cfg.ledger, &r.source_hash)
        && safe_name(&r.filename)
        && valid_date(&r.date)
        && r.source_hash.len() == 64
        && r.source_hash.bytes().all(|b| b.is_ascii_hexdigit())
        && r.archive_hash == r.source_hash
        && r.entry_hash == hash(r.entry.as_bytes())
        && r.entry.contains(&marker)
        && r.entry.matches(&marker).count() == 1
        && r.ledger_before_hash.len() == 64
        && r.ledger_before_hash.bytes().all(|b| b.is_ascii_hexdigit())
        && r.ledger_backup == format!("{}.ledger-before.bin", r.identity)
        && Path::new(&r.destination).parent()
            == Some(
                cfg.archive
                    .join(format!("Statements {}", &r.date[..4]))
                    .as_path(),
            )
        && Path::new(&r.destination)
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(safe_name)
        && matches!(
            r.state.as_str(),
            "intent" | "archived" | "succeeded" | "failed" | "uncertain"
        )
        && r.operation == r.identity
        && backup_valid
        && candidate_valid
        && !r.item_id.is_empty()
        && r.item_id.len() <= 256
        && r.item_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':')
        && !r.run_id.is_empty()
        && r.run_id.len() <= 256
        && r.run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub(crate) fn source_for_operation(
    operation_id: &str,
    cfg: &ImportConfig,
) -> Result<PathBuf, String> {
    if operation_id.len() != 64 || !operation_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Durable source is unavailable".into());
    }
    let _journal_guard = journal_lock(&cfg.store).map_err(|_| "Durable source is unavailable")?;
    let journal = cfg.store.join(format!("{operation_id}.json"));
    let mut file = exclusive(&journal, false).map_err(|_| "Durable source is unavailable")?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| "Durable source is unavailable")?;
    drop(file);
    let receipt: Receipt =
        serde_json::from_slice(&bytes).map_err(|_| "Durable source is unavailable")?;
    if !receipt_valid(&receipt, &format!("{operation_id}.json"), cfg)
        || receipt.identity != operation_id
    {
        return Err("Durable source is unavailable".into());
    }
    if !safe_name(&receipt.filename) {
        return Err("Source file is unavailable".into());
    }
    let original_source = cfg.inbox.join(&receipt.filename);
    reject_reparse_ancestry(&original_source)
        .map_err(|_| "Source file is unavailable".to_string())?;
    let source = fs::canonicalize(&original_source).map_err(|_| "Source file is unavailable")?;
    if !within(&source, &cfg.inbox) || !source.is_file() {
        return Err("Source file is unavailable".into());
    }
    reject_reparse_ancestry(&source).map_err(|_| "Source file is unavailable".to_string())?;
    Ok(source)
}

fn exact_entry_present(ledger: &[u8], entry: &[u8]) -> bool {
    exact_entry_count(ledger, entry) == 1
}
fn exact_entry_count(ledger: &[u8], entry: &[u8]) -> usize {
    if entry.is_empty() {
        return 0;
    }
    ledger
        .windows(entry.len())
        .enumerate()
        .filter(|(start, bytes)| {
            *bytes == entry
                && (*start == 0 || ledger.get(start.wrapping_sub(1)) == Some(&b'\n'))
                && (entry.ends_with(b"\n")
                    || *start + entry.len() == ledger.len()
                    || ledger.get(*start + entry.len()) == Some(&b'\n'))
        })
        .count()
}

pub fn outcomes(ledger_path: &str) -> Vec<ItemOutcome> {
    let cfg = match runtime_config() {
        Ok(c) => c,
        Err(_) => {
            return vec![store_error(
                "Managed sync store or configuration is unavailable",
            )]
        }
    };
    match fs::canonicalize(ledger_path) {
        Ok(path) if path == cfg.ledger => outcomes_with_config(&cfg),
        _ => vec![store_error("Managed ledger is unavailable")],
    }
}

fn outcomes_with_config(cfg: &ImportConfig) -> Vec<ItemOutcome> {
    let _io = match managed_io() {
        Ok(guard) => guard,
        Err(message) => return vec![store_error(&message)],
    };
    let ledger = &cfg.ledger;
    let dir = &cfg.store;
    let _guard = match journal_lock(dir) {
        Ok(g) => g,
        Err(_) => return vec![store_error("Managed sync journal is busy")],
    };
    let mut ledger_file = match exclusive(ledger, false) {
        Ok(file) => file,
        Err(_) => return vec![store_error("Managed ledger is busy")],
    };
    let _ledger_range = match RangeGuard::acquire(&ledger_file) {
        Ok(range) => range,
        Err(_) => return vec![store_error("Managed ledger is busy")],
    };
    let mut ledger_bytes = Vec::new();
    if ledger_file
        .seek(SeekFrom::Start(0))
        .and_then(|_| ledger_file.read_to_end(&mut ledger_bytes))
        .is_err()
    {
        return vec![store_error("Managed ledger cannot be read")];
    }
    let mut v = Vec::new();
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return vec![store_error("Managed sync journal cannot be read")],
    };
    for entry in rd {
        let e = match entry {
            Ok(e) => e,
            Err(_) => return vec![store_error("Managed sync journal cannot be read")],
        };
        if !e.file_name().to_string_lossy().ends_with(".json") {
            continue;
        }
        let parsed = fs::read(e.path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Receipt>(&b).ok());
        let Some(r) = parsed else {
            v.push(store_error(
                "Private sync journal is corrupt; manual review required",
            ));
            continue;
        };
        let file_name = e.file_name().to_string_lossy().into_owned();
        if !receipt_valid(&r, &file_name, cfg) {
            v.push(ItemOutcome {
                item_id: "unverified".into(),
                filename: String::new(),
                state: "uncertain".into(),
                retry_safe: false,
                message: "Private sync receipt failed strict validation; manual review required"
                    .into(),
                operation_id: (r.identity.len() == 64
                    && r.identity.bytes().all(|b| b.is_ascii_hexdigit()))
                .then_some(r.identity),
            });
            continue;
        }
        let backup_ok = fs::read(dir.join(&r.ledger_backup)).is_ok_and(|b| {
            b.len() as u64 == r.ledger_before_size && hash(&b) == r.ledger_before_hash
        });
        let destination = PathBuf::from(&r.destination);
        let canonical_dest_ok = fs::canonicalize(&destination)
            .ok()
            .is_some_and(|p| within(&p, &cfg.archive));
        let archive_ok = canonical_dest_ok
            && fs::read(&destination)
                .is_ok_and(|b| b.len() as u64 == r.source_size && hash(&b) == r.archive_hash);
        let ledger_has_entry = exact_entry_present(&ledger_bytes, r.entry.as_bytes());
        let ledger_before = Some(ledger_bytes.as_slice()).is_some_and(|b| {
            b.len() as u64 == r.ledger_before_size && hash(b) == r.ledger_before_hash
        });
        let mut state = r.state.clone();
        let mut retry_safe = false;
        let mut message = match r.state.as_str() {
            "succeeded" => "Imported and verified".to_owned(),
            "failed" => "Import did not complete".to_owned(),
            "archived" => "Archive state is being reconciled".to_owned(),
            _ => "Operation requires reconciliation".to_owned(),
        };
        if !backup_ok {
            state = "uncertain".into();
            message = "Ledger recovery copy is missing or invalid".into();
        } else if r.state == "archived" && archive_ok && ledger_has_entry {
            let mut done = r.clone();
            done.state = "succeeded".into();
            done.message = "Recovered from exact archived and ledger effects".into();
            if persist(&dir, &done).is_ok() {
                state = done.state;
                message = done.message;
            } else {
                state = "uncertain".into();
                message = "Recovered outcome could not be journaled".into();
            }
        } else if r.state == "archived" && archive_ok && ledger_before {
            state = "uncertain".into();
            retry_safe = true;
            message =
                "Owned archive is verified; safe recovery can append the reviewed entry".into();
        } else if r.state == "intent" {
            let source = cfg.inbox.join(&r.filename);
            let source_ok = fs::read(source)
                .is_ok_and(|b| b.len() as u64 == r.source_size && hash(&b) == r.source_hash);
            if !destination.exists() && source_ok && ledger_before {
                let mut failed = r.clone();
                failed.state = "failed".into();
                failed.retry_safe = true;
                failed.message = "No import effects occurred; safe retry is available".into();
                if persist(&dir, &failed).is_ok() {
                    state = failed.state;
                    retry_safe = true;
                    message = failed.message;
                } else {
                    state = "uncertain".into();
                    message = "Safe retry state could not be journaled".into();
                }
            } else {
                state = "uncertain".into();
                message = "Interrupted operation effects are ambiguous".into();
            }
        } else if r.state == "succeeded" && (!archive_ok || !ledger_has_entry) {
            state = "uncertain".into();
            message = "Recorded effects no longer reconcile".into();
        } else if r.state == "failed" && r.retry_safe {
            retry_safe = !destination.exists()
                && Some(ledger_bytes.as_slice()).is_some_and(|b| {
                    b.len() as u64 == r.ledger_before_size && hash(b) == r.ledger_before_hash
                })
                && fs::read(cfg.inbox.join(&r.filename))
                    .is_ok_and(|b| b.len() as u64 == r.source_size && hash(&b) == r.source_hash);
            if !retry_safe {
                state = "uncertain".into();
                message = "Safe retry preconditions changed".into();
            } else {
                message = "No import effects occurred; safe retry is available".into();
            }
        } else if r.state == "archived"
            || r.state == "uncertain"
            || (r.state == "succeeded" && !archive_ok)
        {
            state = "uncertain".into();
            message = "Recorded effects do not reconcile".into();
        }
        v.push(ItemOutcome {
            item_id: r.item_id,
            filename: r.filename,
            state,
            retry_safe,
            message,
            operation_id: Some(r.identity),
        });
    }
    v
}

fn store_error(message: &str) -> ItemOutcome {
    ItemOutcome {
        item_id: "store".into(),
        filename: String::new(),
        state: "uncertain".into(),
        retry_safe: false,
        message: message.into(),
        operation_id: None,
    }
}

fn verified_prior_success(
    source_hash: &str,
    filename: &str,
    date: &str,
    new_name: &str,
    cfg: &ImportConfig,
) -> bool {
    let operation = identity(&cfg.ledger, source_hash);
    let Some(receipt) = fs::read(cfg.store.join(format!("{operation}.json")))
        .ok()
        .and_then(|raw| serde_json::from_slice::<Receipt>(&raw).ok())
    else {
        return false;
    };
    receipt_valid(&receipt, &format!("{operation}.json"), cfg)
        && receipt.state == "succeeded"
        && receipt.filename == filename
        && receipt.date == date
        && Path::new(&receipt.destination)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(new_name)
        && fs::canonicalize(&receipt.destination)
            .ok()
            .is_some_and(|p| within(&p, &cfg.archive))
        && fs::read(&receipt.destination).is_ok_and(|bytes| {
            bytes.len() as u64 == receipt.source_size && hash(&bytes) == receipt.archive_hash
        })
        && fs::read(&cfg.ledger)
            .is_ok_and(|bytes| exact_entry_present(&bytes, receipt.entry.as_bytes()))
}

fn batch_preflight(items: &[serde_json::Value], cfg: &ImportConfig) -> Vec<Option<String>> {
    let mut errors = vec![None; items.len()];
    let mut sources = std::collections::HashMap::<String, Vec<usize>>::new();
    let mut destinations = std::collections::HashMap::<String, Vec<usize>>::new();
    for (index, item) in items.iter().enumerate() {
        let filename = item["filename"].as_str().unwrap_or("");
        let new_name = item["new_name"].as_str().unwrap_or("");
        let date = item["date"].as_str().unwrap_or("");
        if !safe_name(filename)
            || !safe_name(new_name)
            || !valid_date(date)
            || item["was_encrypted"].as_bool() != Some(false)
            || item["is_cleared"].as_bool().is_none()
        {
            errors[index] = Some("Unsafe or unsupported approved match".into());
            continue;
        }
        if let Some(expected_source) = item
            .get("_prepared_source_path")
            .and_then(serde_json::Value::as_str)
        {
            let expected_hash = item
                .get("_prepared_source_hash")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let operation = identity(&cfg.ledger, expected_hash);
            let prior_path = cfg.store.join(format!("{operation}.json"));
            if prior_path.exists() {
                let prior: Receipt = match fs::read(&prior_path)
                    .ok()
                    .and_then(|raw| serde_json::from_slice(&raw).ok())
                {
                    Some(receipt) if receipt_valid(&receipt, &format!("{operation}.json"), cfg) => {
                        receipt
                    }
                    _ => {
                        errors[index] = Some("Prior import receipt is corrupt or invalid".into());
                        continue;
                    }
                };
                let is_success = prior.state == "succeeded"
                    && verified_prior_success(expected_hash, filename, date, new_name, cfg);
                let is_safe_failure = prior.state == "failed"
                    && prior.retry_safe
                    && !cfg
                        .archive
                        .join(format!("Statements {}", &date[..4]))
                        .join(new_name)
                        .exists()
                    && item
                        .get("_prepared_ledger_hash")
                        .and_then(serde_json::Value::as_str)
                        == Some(prior.ledger_before_hash.as_str());
                if !is_success && !is_safe_failure {
                    errors[index] =
                        Some("Prior import requires reconciliation before commit".into());
                    continue;
                }
            }
            let source = fs::canonicalize(cfg.inbox.join(filename));
            let source_check = (|| -> Result<(), String> {
                let operation = identity(&cfg.ledger, expected_hash);
                let entry = build_entry(
                    item,
                    "",
                    &operation,
                    &cfg.currency,
                    prepared_counter(item, &cfg.counter),
                )?;
                if item
                    .get("_prepared_entry")
                    .and_then(serde_json::Value::as_str)
                    != Some(entry.as_str())
                {
                    return Err("Approved ledger entry changed after review".into());
                }
                let destination = cfg
                    .archive
                    .join(format!("Statements {}", &date[..4]))
                    .join(new_name);
                if item
                    .get("_prepared_destination")
                    .and_then(serde_json::Value::as_str)
                    != Some(destination.to_string_lossy().as_ref())
                {
                    return Err("Approved destination changed after review".into());
                }
                reject_reparse_ancestry(&destination)?;
                match source {
                    Ok(source) => {
                        if source != Path::new(expected_source) || !within(&source, &cfg.inbox) {
                            return Err("Approved source path changed after review".into());
                        }
                        let mut file = exclusive(&source, false)?;
                        let _range = RangeGuard::acquire(&file)?;
                        let mut bytes = Vec::new();
                        file.read_to_end(&mut bytes)
                            .map_err(|_| "Approved source is unavailable")?;
                        if hash(&bytes) != expected_hash
                            || item
                                .get("_prepared_source_size")
                                .and_then(serde_json::Value::as_u64)
                                != Some(bytes.len() as u64)
                        {
                            return Err("Approved source changed after review".into());
                        }
                    }
                    Err(_)
                        if verified_prior_success(expected_hash, filename, date, new_name, cfg) => {
                    }
                    Err(_) => return Err("Approved source is unavailable".into()),
                }
                if destination.exists() {
                    let prior_path = cfg.store.join(format!("{operation}.json"));
                    let prior = fs::read(prior_path)
                        .ok()
                        .and_then(|raw| serde_json::from_slice::<Receipt>(&raw).ok());
                    let valid_prior = prior.is_some_and(|r| {
                        receipt_valid(&r, &format!("{operation}.json"), cfg)
                            && r.state == "succeeded"
                            && Path::new(&r.destination) == destination
                            && fs::canonicalize(&destination)
                                .ok()
                                .is_some_and(|p| within(&p, &cfg.archive))
                            && fs::read(&destination).is_ok_and(|b| hash(&b) == r.archive_hash)
                    });
                    if !valid_prior {
                        return Err("Archive destination already exists".into());
                    }
                }
                Ok(())
            })();
            if let Err(message) = source_check {
                errors[index] = Some(message);
                continue;
            }
        }
        let source_key = item
            .get("_prepared_source_path")
            .and_then(serde_json::Value::as_str)
            .map(|p| p.to_lowercase())
            .unwrap_or_else(|| cfg.inbox.join(filename).to_string_lossy().to_lowercase());
        let destination_key = cfg
            .archive
            .join(format!("Statements {}", &date[..4]))
            .join(new_name)
            .to_string_lossy()
            .to_lowercase();
        sources.entry(source_key).or_default().push(index);
        destinations.entry(destination_key).or_default().push(index);
    }
    for indexes in sources.values().chain(destinations.values()) {
        if indexes.len() > 1 {
            for index in indexes {
                errors[*index] =
                    Some("Selected matches contain duplicate source or archive destination".into());
            }
        }
    }
    errors
}

pub(crate) fn retry_operations_cancellable(
    operation_ids: Vec<String>,
    cancelled: impl FnMut() -> bool,
) -> Vec<ItemOutcome> {
    let cfg = match runtime_config() {
        Ok(c) => c,
        Err(_) => {
            return operation_ids
                .into_iter()
                .map(|_| store_error("Managed sync store is unavailable"))
                .collect()
        }
    };
    // Reconcile durable intents first; this promotes only exact observed effects or
    // records a no-effects state whose retry preconditions can be checked again.
    let _ = outcomes_with_config(&cfg);
    retry_with_config_cancellable(operation_ids, cfg, cancelled)
}

#[cfg(test)]
fn retry_with_config(operation_ids: Vec<String>, cfg: ImportConfig) -> Vec<ItemOutcome> {
    retry_with_config_cancellable(operation_ids, cfg, || false)
}

fn retry_with_config_cancellable(
    operation_ids: Vec<String>,
    cfg: ImportConfig,
    mut cancelled: impl FnMut() -> bool,
) -> Vec<ItemOutcome> {
    let _io = match managed_io() {
        Ok(guard) => guard,
        Err(message) => return vec![store_error(&message)],
    };
    let _journal = match journal_lock(&cfg.store) {
        Ok(g) => g,
        Err(_) => {
            return operation_ids
                .into_iter()
                .map(|_| store_error("Managed sync journal is busy"))
                .collect()
        }
    };
    let mut seen = std::collections::HashSet::new();
    let validated = operation_ids
        .iter()
        .map(|id| {
            if id.len() != 64
                || !id.bytes().all(|b| b.is_ascii_hexdigit())
                || !seen.insert(id.clone())
            {
                return Err("Retry selection is invalid or duplicated".to_owned());
            }
            let path = cfg.store.join(format!("{id}.json"));
            let receipt: Receipt = serde_json::from_slice(
                &fs::read(path).map_err(|_| "Retry operation is unavailable".to_owned())?,
            )
            .map_err(|_| "Private sync receipt is corrupt".to_owned())?;
            if !receipt_valid(&receipt, &format!("{id}.json"), &cfg) {
                return Err("Private sync receipt failed strict validation".into());
            }
            if receipt.state != "succeeded"
                && !((receipt.state == "failed" && receipt.retry_safe)
                    || receipt.state == "archived")
            {
                return Err("Operation is not in a retryable or recoverable state".into());
            }
            Ok(receipt)
        })
        .collect::<Vec<_>>();
    let mut cancellation_requested = false;
    operation_ids
        .into_iter()
        .zip(validated)
        .map(|(id, prepared)| {
            cancellation_requested |= cancelled();
            let filename = String::new();
            let valid_id = id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit());
            let result = (|| -> Result<ItemOutcome, String> {
                let mut receipt = prepared?;
                if receipt.state == "succeeded" {
                    let archive_ok = fs::canonicalize(&receipt.destination)
                        .ok()
                        .is_some_and(|p| within(&p, &cfg.archive))
                        && fs::read(&receipt.destination).is_ok_and(|b| {
                            b.len() as u64 == receipt.source_size
                                && hash(&b) == receipt.archive_hash
                        });
                    let ledger_ok = fs::read(&cfg.ledger)
                        .is_ok_and(|b| exact_entry_present(&b, receipt.entry.as_bytes()));
                    if archive_ok && ledger_ok {
                        return Ok(ItemOutcome {
                            item_id: receipt.item_id,
                            filename: receipt.filename,
                            state: "succeeded".into(),
                            retry_safe: false,
                            message: "Already confirmed; no duplicate append".into(),
                            operation_id: Some(receipt.identity),
                        });
                    }
                    return Err("Prior operation effects do not reconcile".into());
                }
                if cancellation_requested {
                    return Ok(ItemOutcome {
                        item_id: receipt.item_id,
                        filename: receipt.filename,
                        state: if receipt.state == "failed" {
                            "failed"
                        } else {
                            "uncertain"
                        }
                        .into(),
                        retry_safe: false,
                        message:
                            "Retry cancelled before this item started; prior durable state retained"
                                .into(),
                        operation_id: Some(receipt.identity),
                    });
                }
                if !((receipt.state == "failed" && receipt.retry_safe)
                    || receipt.state == "archived")
                {
                    return Err("Operation is not in a retryable or recoverable state".into());
                }
                #[cfg(windows)]
                let mut ledger_file = exclusive(&cfg.ledger, true)?;
                let _ledger_range = RangeGuard::acquire(&ledger_file)?;
                #[cfg(not(windows))]
                let mut ledger_file = return Err("Managed imports require Windows".into());
                let mut before = Vec::new();
                ledger_file
                    .seek(SeekFrom::Start(0))
                    .and_then(|_| ledger_file.read_to_end(&mut before))
                    .map_err(|_| "Ledger is unavailable")?;
                let destination = PathBuf::from(&receipt.destination);
                let archive_location_ok = fs::canonicalize(&destination)
                    .ok()
                    .is_some_and(|p| within(&p, &cfg.archive));
                if receipt.state == "archived"
                    && archive_location_ok
                    && fs::read(&receipt.destination).is_ok_and(|b| {
                        b.len() as u64 == receipt.source_size && hash(&b) == receipt.archive_hash
                    })
                    && exact_entry_present(&before, receipt.entry.as_bytes())
                {
                    receipt.state = "succeeded".into();
                    receipt.message = "Recovered exact archive and ledger effects".into();
                    persist(&cfg.store, &receipt)?;
                    return Ok(ItemOutcome {
                        item_id: receipt.item_id,
                        filename: receipt.filename,
                        state: "succeeded".into(),
                        retry_safe: false,
                        message: receipt.message,
                        operation_id: Some(receipt.identity),
                    });
                }
                let mut source_bytes = Vec::new();
                let _source_file = if receipt.state == "failed" {
                    let source = fs::canonicalize(cfg.inbox.join(&receipt.filename))
                        .map_err(|_| "Reviewed source is unavailable")?;
                    if !within(&source, &cfg.inbox) {
                        return Err("Reviewed source escaped inbox".into());
                    }
                    let mut file = exclusive(&source, false)?;
                    let range = RangeGuard::acquire(&file)?;
                    file.read_to_end(&mut source_bytes)
                        .map_err(|_| "Reviewed source is unavailable")?;
                    Some((file, range))
                } else {
                    None
                };
                let backup = fs::read(cfg.store.join(&receipt.ledger_backup))
                    .map_err(|_| "Ledger recovery copy is unavailable")?;
                if (receipt.state == "failed"
                    && (source_bytes.len() as u64 != receipt.source_size
                        || hash(&source_bytes) != receipt.source_hash))
                    || backup.len() as u64 != receipt.ledger_before_size
                    || hash(&backup) != receipt.ledger_before_hash
                    || before.len() as u64 != receipt.ledger_before_size
                    || hash(&before) != receipt.ledger_before_hash
                {
                    return Err("Safe retry preconditions changed; operation is uncertain".into());
                }
                if receipt.state == "failed" {
                    if destination.exists() {
                        return Err(
                            "Archive destination is occupied; operation is uncertain".into()
                        );
                    }
                    let parent = destination
                        .parent()
                        .ok_or("Archive destination is invalid")?;
                    reject_reparse_ancestry(parent)?;
                    fs::create_dir_all(parent).map_err(|_| "Unable to prepare archive folder")?;
                    let canonical_parent =
                        fs::canonicalize(parent).map_err(|_| "Archive folder is unavailable")?;
                    if !within(&canonical_parent, &cfg.archive) {
                        return Err("Archive folder escaped configured root".into());
                    }
                    let mut archive_file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&destination)
                        .map_err(|_| "Archive destination was claimed")?;
                    archive_file
                        .write_all(&source_bytes)
                        .and_then(|_| archive_file.sync_all())
                        .map_err(|_| "Archive write is uncertain")?;
                } else if !archive_location_ok
                    || !fs::read(&destination).is_ok_and(|b| {
                        b.len() as u64 == receipt.source_size && hash(&b) == receipt.archive_hash
                    })
                {
                    return Err("Archived operation bytes are not verifiable".into());
                }
                receipt.state = "archived".into();
                receipt.retry_safe = false;
                receipt.message = "Archive ownership verified".into();
                persist(&cfg.store, &receipt)?;
                if !receipt.entry.is_empty() {
                    if !before.is_empty() && !before.ends_with(b"\n") {
                        ledger_file
                            .seek(SeekFrom::End(0))
                            .and_then(|_| ledger_file.write_all(b"\n"))
                            .map_err(|_| "Ledger append is uncertain")?;
                    }
                    ledger_file
                        .seek(SeekFrom::End(0))
                        .and_then(|_| ledger_file.write_all(receipt.entry.as_bytes()))
                        .and_then(|_| ledger_file.sync_all())
                        .map_err(|_| "Ledger append is uncertain")?;
                }
                let mut after = Vec::new();
                ledger_file
                    .seek(SeekFrom::Start(0))
                    .and_then(|_| ledger_file.read_to_end(&mut after))
                    .map_err(|_| "Ledger verification failed")?;
                if !exact_entry_present(&after, receipt.entry.as_bytes())
                    || hash(receipt.entry.as_bytes()) != receipt.entry_hash
                {
                    return Err("Ledger operation block does not verify".into());
                }
                receipt.state = "succeeded".into();
                receipt.message = "Recovered and verified".into();
                persist(&cfg.store, &receipt)?;
                Ok(ItemOutcome {
                    item_id: receipt.item_id,
                    filename: receipt.filename,
                    state: "succeeded".into(),
                    retry_safe: false,
                    message: "Recovered and verified".into(),
                    operation_id: Some(receipt.identity),
                })
            })();
            result.unwrap_or_else(|e| ItemOutcome {
                item_id: if valid_id {
                    id.clone()
                } else {
                    "invalid".into()
                },
                filename,
                state: "uncertain".into(),
                retry_safe: false,
                message: e,
                operation_id: valid_id.then_some(id.clone()),
            })
        })
        .collect()
}

fn persist_pre_effect_failure(
    item: &serde_json::Value,
    run_id: &str,
    cfg: &ImportConfig,
    item_id: &str,
    filename: &str,
    message: &str,
) -> Option<ItemOutcome> {
    let source_hash = item.get("_prepared_source_hash")?.as_str()?.to_owned();
    let source_size = item.get("_prepared_source_size")?.as_u64()?;
    let date = item.get("date")?.as_str()?.to_owned();
    let new_name = item.get("new_name")?.as_str()?;
    let destination = PathBuf::from(item.get("_prepared_destination")?.as_str()?);
    let entry = item.get("_prepared_entry")?.as_str()?.to_owned();
    let backup: Vec<u8> =
        serde_json::from_value(item.get("_prepared_ledger_backup")?.clone()).ok()?;
    if source_hash.len() != 64
        || !source_hash.bytes().all(|b| b.is_ascii_hexdigit())
        || !safe_name(filename)
        || !safe_name(new_name)
        || !valid_date(&date)
        || destination
            != cfg
                .archive
                .join(format!("Statements {}", &date[..4]))
                .join(new_name)
        || entry.is_empty()
    {
        return None;
    }
    let operation_id = identity(&cfg.ledger, &source_hash);
    let _journal = journal_lock(&cfg.store).ok()?;
    let mut ledger = exclusive(&cfg.ledger, true).ok()?;
    let _ledger_range = RangeGuard::acquire(&ledger).ok()?;
    let mut current = Vec::new();
    ledger
        .seek(SeekFrom::Start(0))
        .and_then(|_| ledger.read_to_end(&mut current))
        .ok()?;
    let backup_hash = hash(&backup);
    let source_path = fs::canonicalize(cfg.inbox.join(filename)).ok();
    let source_matches = source_path.as_ref().is_some_and(|path| {
        within(path, &cfg.inbox)
            && exclusive(path, false)
                .and_then(|mut file| {
                    let _lock = RangeGuard::acquire(&file)?;
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes)
                        .map_err(|_| "Source read failed")?;
                    Ok(bytes.len() as u64 == source_size && hash(&bytes) == source_hash)
                })
                .unwrap_or(false)
    });
    let no_effects = !destination.exists() && current == backup;
    let receipt_path = cfg.store.join(format!("{operation_id}.json"));
    if receipt_path.exists() {
        return None;
    }
    let backup_name = format!("{operation_id}.ledger-before.bin");
    let backup_path = cfg.store.join(&backup_name);
    if backup_path.exists() {
        if fs::read(&backup_path).ok().as_deref() != Some(backup.as_slice()) {
            return None;
        }
    } else {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup_path)
            .ok()?;
        file.write_all(&backup).and_then(|_| file.sync_all()).ok()?;
    }
    let receipt = Receipt {
        version: 2,
        operation: operation_id.clone(),
        identity: operation_id.clone(),
        run_id: run_id.into(),
        item_id: item_id.into(),
        filename: filename.into(),
        date,
        source_hash: source_hash.clone(),
        source_size,
        destination: destination.to_string_lossy().into_owned(),
        archive_hash: source_hash,
        entry_hash: hash(entry.as_bytes()),
        entry,
        ledger_before_hash: backup_hash,
        ledger_before_size: backup.len() as u64,
        ledger_backup: backup_name,
        state: "failed".into(),
        retry_safe: no_effects && source_matches,
        message: message.into(),
        duplicate_override: serde_json::from_value(
            item.get("_prepared_duplicate_override")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .ok()?,
        batch_override: serde_json::from_value(
            item.get("_prepared_batch_override")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .ok()?,
    };
    if !receipt_valid(&receipt, &format!("{operation_id}.json"), cfg)
        || persist(&cfg.store, &receipt).is_err()
    {
        return None;
    }
    Some(ItemOutcome {
        item_id: item_id.into(),
        filename: filename.into(),
        state: "failed".into(),
        retry_safe: receipt.retry_safe,
        message: message.into(),
        operation_id: Some(operation_id),
    })
}

fn commit_with_config(
    items: Vec<serde_json::Value>,
    run_id: &str,
    cfg: ImportConfig,
    faults: FaultPlan,
) -> Vec<ItemOutcome> {
    commit_with_config_cancellable(items, run_id, cfg, faults, || false)
}

fn commit_with_config_cancellable(
    items: Vec<serde_json::Value>,
    run_id: &str,
    cfg: ImportConfig,
    faults: FaultPlan,
    mut cancelled: impl FnMut() -> bool,
) -> Vec<ItemOutcome> {
    let inbox = cfg.inbox;
    let archive = cfg.archive;
    let ledger = cfg.ledger;
    let store = cfg.store;
    let currency = cfg.currency;
    let counter = cfg.counter;
    let mut out = Vec::new();
    let mut preflight = batch_preflight(
        &items,
        &ImportConfig {
            inbox: inbox.clone(),
            archive: archive.clone(),
            ledger: ledger.clone(),
            store: store.clone(),
            currency: currency.clone(),
            counter: counter.clone(),
        },
    );
    let has_prepared_plans = items
        .iter()
        .any(|item| item.get("_prepared_source_hash").is_some());
    let mut expected_ledger = if has_prepared_plans {
        fs::read(&ledger).ok()
    } else {
        None
    };
    if has_prepared_plans {
        if let Some(base) = expected_ledger.as_ref() {
            let base_hash = hash(base);
            for (index, item) in items.iter().enumerate() {
                let prior_success = item
                    .get("_prepared_source_hash")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|source_hash| {
                        verified_prior_success(
                            source_hash,
                            item["filename"].as_str().unwrap_or(""),
                            item["date"].as_str().unwrap_or(""),
                            item["new_name"].as_str().unwrap_or(""),
                            &ImportConfig {
                                inbox: inbox.clone(),
                                archive: archive.clone(),
                                ledger: ledger.clone(),
                                store: store.clone(),
                                currency: currency.clone(),
                                counter: counter.clone(),
                            },
                        )
                    });
                if !prior_success
                    && item.get("_prepared_source_hash").is_some()
                    && item
                        .get("_prepared_ledger_exact")
                        .and_then(serde_json::Value::as_bool)
                        != Some(true)
                {
                    preflight[index] = Some("Ledger changed after analysis review".into());
                }
                if !prior_success
                    && item.get("_prepared_source_hash").is_some()
                    && (item
                        .get("_prepared_ledger_hash")
                        .and_then(serde_json::Value::as_str)
                        != Some(base_hash.as_str())
                        || item
                            .get("_prepared_ledger_size")
                            .and_then(serde_json::Value::as_u64)
                            != Some(base.len() as u64))
                {
                    preflight[index] = Some("Ledger changed after analysis review".into());
                }
            }
            let mut simulated = base.clone();
            for (index, item) in items.iter().enumerate() {
                if preflight[index].is_some() || item.get("_prepared_source_hash").is_none() {
                    continue;
                }
                let source_hash = item["_prepared_source_hash"].as_str().unwrap_or("");
                if verified_prior_success(
                    source_hash,
                    item["filename"].as_str().unwrap_or(""),
                    item["date"].as_str().unwrap_or(""),
                    item["new_name"].as_str().unwrap_or(""),
                    &ImportConfig {
                        inbox: inbox.clone(),
                        archive: archive.clone(),
                        ledger: ledger.clone(),
                        store: store.clone(),
                        currency: currency.clone(),
                        counter: counter.clone(),
                    },
                ) {
                    continue;
                }
                let entry = item
                    .get("_prepared_entry")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let mut candidate = simulated.clone();
                if !candidate.is_empty() && !candidate.ends_with(b"\n") {
                    candidate.push(b'\n');
                }
                candidate.extend_from_slice(entry.as_bytes());
                let valid = std::str::from_utf8(&candidate).ok().is_some_and(|text| {
                    crate::ledger_parser::parse_with_currency(text, &currency).is_ok()
                });
                if valid {
                    simulated = candidate;
                } else {
                    preflight[index] = Some(
                        "Combined approved entries do not validate in the supported ledger subset"
                            .into(),
                    );
                }
            }
        } else {
            for error in &mut preflight {
                *error = Some("Managed ledger is unavailable".into());
            }
        }
    }
    for (index, mut item) in items.into_iter().enumerate() {
        let id = item["_managed_item_id"]
            .as_str()
            .or_else(|| item["match_id"].as_str())
            .unwrap_or("")
            .to_owned();
        let filename = item["filename"].as_str().unwrap_or("").to_owned();
        let new_name = item["new_name"].as_str().unwrap_or("").to_owned();
        let operation_id = item
            .get("_prepared_source_hash")
            .and_then(serde_json::Value::as_str)
            .map(|source_hash| identity(&ledger, source_hash));
        if cancelled() {
            let durable = persist_pre_effect_failure(
                &item,
                run_id,
                &ImportConfig {
                    inbox: inbox.clone(),
                    archive: archive.clone(),
                    ledger: ledger.clone(),
                    store: store.clone(),
                    currency: currency.clone(),
                    counter: counter.clone(),
                },
                &id,
                &filename,
                "Import cancelled before this item started",
            );
            out.push(durable.unwrap_or(ItemOutcome {
                item_id: id,
                filename,
                state: "uncertain".into(),
                retry_safe: false,
                message:
                    "Import cancelled before this item started; durable outcome unavailable".into(),
                operation_id,
            }));
            continue;
        }
        if let Some(message) = preflight[index].as_ref() {
            let prepared = item.get("_prepared_source_hash").is_some();
            let durable = if prepared {
                persist_pre_effect_failure(
                    &item,
                    run_id,
                    &ImportConfig {
                        inbox: inbox.clone(),
                        archive: archive.clone(),
                        ledger: ledger.clone(),
                        store: store.clone(),
                        currency: currency.clone(),
                        counter: counter.clone(),
                    },
                    &id,
                    &filename,
                    message,
                )
            } else {
                None
            };
            out.push(durable.unwrap_or(ItemOutcome {
                item_id: id,
                filename,
                state: if prepared { "uncertain" } else { "failed" }.into(),
                retry_safe: false,
                message: if prepared {
                    format!("{message}; durable no-effect outcome unavailable")
                } else {
                    message.clone()
                },
                operation_id: if prepared {
                    None
                } else {
                    operation_id.filter(|op| store.join(format!("{op}.json")).is_file())
                },
            }));
            continue;
        }
        if let Some(expected) = expected_ledger.as_ref() {
            item["_prepared_ledger_hash"] = serde_json::json!(hash(expected));
            item["_prepared_ledger_size"] = serde_json::json!(expected.len());
        }
        let mut appended_entry: Option<String> = None;
        let result = (|| -> Result<(), String> {
            if !safe_name(&filename)
                || !safe_name(&new_name)
                || item["was_encrypted"].as_bool() != Some(false)
                || item["is_cleared"].as_bool().is_none()
            {
                return Err("Unsafe or duplicate selected item".into());
            }
            // A previously confirmed content identity is resolved before touching
            // the source path, which may have been manually removed after success.
            if let Some(reviewed_hash) = item
                .get("_prepared_source_hash")
                .and_then(serde_json::Value::as_str)
            {
                let reviewed_op = identity(&ledger, reviewed_hash);
                let reviewed_receipt_path = store.join(format!("{reviewed_op}.json"));
                if reviewed_receipt_path.exists() {
                    let prior: Receipt = serde_json::from_slice(
                        &fs::read(&reviewed_receipt_path)
                            .map_err(|_| "Private receipt is unavailable")?,
                    )
                    .map_err(|_| "Private receipt is corrupt")?;
                    let validation = ImportConfig {
                        inbox: inbox.clone(),
                        archive: archive.clone(),
                        ledger: ledger.clone(),
                        store: store.clone(),
                        currency: currency.clone(),
                        counter: counter.clone(),
                    };
                    if !receipt_valid(&prior, &format!("{reviewed_op}.json"), &validation) {
                        return Err("Prior import receipt failed strict validation".into());
                    }
                    if prior.state == "succeeded" {
                        if prior.filename != filename
                            || prior.date != item["date"].as_str().unwrap_or("")
                            || Path::new(&prior.destination)
                                .file_name()
                                .and_then(|n| n.to_str())
                                != Some(new_name.as_str())
                        {
                            return Err(
                                "Prior content identity belongs to a different approved item"
                                    .into(),
                            );
                        }
                        let destination = PathBuf::from(&prior.destination);
                        let archive_ok = fs::canonicalize(&destination)
                            .ok()
                            .is_some_and(|p| within(&p, &archive))
                            && fs::read(&destination).is_ok_and(|b| {
                                b.len() as u64 == prior.source_size
                                    && hash(&b) == prior.archive_hash
                            });
                        let ledger_ok = fs::read(&ledger)
                            .is_ok_and(|b| exact_entry_present(&b, prior.entry.as_bytes()));
                        if archive_ok && ledger_ok {
                            return Ok(());
                        }
                        return Err("Prior import receipt is uncertain; ledger or archive does not reconcile".into());
                    }
                }
            }
            let src = inbox.join(&filename);
            let source = fs::canonicalize(&src).map_err(|_| "Approved source is unavailable")?;
            if !within(&source, &inbox) {
                return Err("Approved source escapes configured inbox".into());
            }
            let mut sf = exclusive(&source, false)?;
            let _source_range = RangeGuard::acquire(&sf)?;
            let mut bytes = Vec::new();
            sf.read_to_end(&mut bytes)
                .map_err(|_| "Unable to read approved source")?;
            let sh = hash(&bytes);
            if item
                .get("_prepared_source_hash")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|h| h != sh)
                || item
                    .get("_prepared_source_size")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|n| n != bytes.len() as u64)
                || item
                    .get("_prepared_source_path")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|p| Path::new(p) != source)
            {
                return Err("Approved source changed after review".into());
            }
            let op = identity(&ledger, &sh);
            let receipt_path = store.join(format!("{op}.json"));
            if receipt_path.exists() {
                let r: Receipt = serde_json::from_slice(
                    &fs::read(&receipt_path).map_err(|_| "Private receipt is unavailable")?,
                )
                .map_err(|_| "Private receipt is corrupt")?;
                if !receipt_valid(
                    &r,
                    &format!("{op}.json"),
                    &ImportConfig {
                        inbox: inbox.clone(),
                        archive: archive.clone(),
                        ledger: ledger.clone(),
                        store: store.clone(),
                        currency: currency.clone(),
                        counter: counter.clone(),
                    },
                ) {
                    return Err("Prior import receipt failed strict validation".into());
                }
                if r.state == "succeeded" {
                    let destination = PathBuf::from(&r.destination);
                    let canonical_destination = fs::canonicalize(&destination).ok();
                    if !within(&destination, &archive)
                        || canonical_destination
                            .as_ref()
                            .is_none_or(|p| !within(p, &archive))
                        || !fs::read(&destination).is_ok_and(|b| {
                            b.len() as u64 == r.source_size && hash(&b) == r.archive_hash
                        })
                        || !fs::read(&ledger)
                            .is_ok_and(|b| exact_entry_present(&b, r.entry.as_bytes()))
                    {
                        return Err("Prior import receipt is uncertain; ledger or archive does not reconcile".into());
                    }
                    return Ok(());
                }
                if r.state != "failed" || !r.retry_safe {
                    return Err(
                        "Existing operation is uncertain and requires reconciliation".into(),
                    );
                }
            }
            let item_date = item["date"]
                .as_str()
                .filter(|d| valid_date(d))
                .ok_or("Analysis date is invalid")?;
            let dest = archive
                .join(format!("Statements {}", &item_date[..4]))
                .join(&new_name);
            if item
                .get("_prepared_destination")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|p| Path::new(p) != dest)
            {
                return Err("Approved archive destination changed after review".into());
            }
            let dest_parent = dest.parent().ok_or("Invalid archive destination")?;
            reject_reparse_ancestry(dest_parent)?;
            let entry = build_entry(
                &item,
                run_id,
                &op,
                &currency,
                prepared_counter(&item, &counter),
            )?;
            if item
                .get("_prepared_entry")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|e| e != entry)
            {
                return Err("Approved ledger entry changed after review".into());
            }
            let ledger_before = fs::read(&ledger).map_err(|_| "Ledger is unavailable")?;
            if item
                .get("_prepared_ledger_hash")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|h| h != hash(&ledger_before))
            {
                return Err("Ledger changed after review".into());
            }
            if item
                .get("_prepared_ledger_size")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|n| n != ledger_before.len() as u64)
            {
                return Err("Ledger changed after review".into());
            }
            if !entry.is_empty() {
                let mut candidate = ledger_before.clone();
                if !candidate.is_empty() && !candidate.ends_with(b"\n") {
                    candidate.push(b'\n');
                }
                candidate.extend_from_slice(entry.as_bytes());
                let candidate =
                    std::str::from_utf8(&candidate).map_err(|_| "Ledger is not valid UTF-8")?;
                crate::ledger_parser::parse_with_currency(candidate, &currency).map_err(|_| {
                    "Generated transaction does not validate in the supported ledger subset"
                })?;
            }
            let entry_hash = hash(entry.as_bytes());
            let archive_hash = sh.clone();
            let backup_name = format!("{op}.ledger-before.bin");
            if store.join(&backup_name).exists() {
                let previous = fs::read(store.join(&backup_name))
                    .map_err(|_| "Ledger recovery copy is unavailable")?;
                if previous != ledger_before {
                    return Err("Prior ledger recovery copy is uncertain".into());
                }
            } else {
                let mut backup = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(store.join(&backup_name))
                    .map_err(|_| "Unable to stage ledger recovery copy")?;
                backup
                    .write_all(&ledger_before)
                    .and_then(|_| backup.sync_all())
                    .map_err(|_| "Unable to sync ledger recovery copy")?;
                drop(backup);
            }
            let mut r = Receipt {
                batch_override: serde_json::from_value(
                    item.get("_prepared_batch_override")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                )
                .map_err(|_| "Invalid reviewed batch override")?,
                duplicate_override: serde_json::from_value(
                    item.get("_prepared_duplicate_override")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                )
                .map_err(|_| "Invalid reviewed duplicate override")?,
                version: 2,
                operation: op.clone(),
                identity: op,
                run_id: run_id.into(),
                item_id: id.clone(),
                filename: filename.clone(),
                date: item_date.to_owned(),
                source_hash: sh,
                source_size: bytes.len() as u64,
                destination: dest.to_string_lossy().into_owned(),
                archive_hash,
                entry_hash,
                entry: entry.clone(),
                ledger_before_hash: hash(&ledger_before),
                ledger_before_size: ledger_before.len() as u64,
                ledger_backup: backup_name,
                state: "intent".into(),
                retry_safe: false,
                message: String::new(),
            };
            let _journal_guard = journal_lock(&store)?;
            #[cfg(windows)]
            let mut lf = exclusive(&ledger, true)?;
            let _ledger_range = RangeGuard::acquire(&lf)?;
            #[cfg(not(windows))]
            let mut lf = return Err("Managed imports require Windows".into());
            let mut current = Vec::new();
            lf.seek(SeekFrom::Start(0))
                .and_then(|_| lf.read_to_end(&mut current))
                .map_err(|_| "Ledger reread failed")?;
            if current != ledger_before || hash(&current) != r.ledger_before_hash {
                return Err("Ledger changed after review".into());
            }
            if faults.before_journal {
                r.state = "failed".into();
                r.retry_safe = true;
                r.message = "No import effects occurred; safe retry is available".into();
                persist(&store, &r).map_err(|_| {
                    "Journal publication failed and safe failure could not be recorded"
                })?;
                return Err("Journal intent publication failed before effects".into());
            }
            persist(&store, &r)?;
            if faults.after_intent {
                return Err("Injected interruption after durable intent".into());
            }
            fs::create_dir_all(dest_parent).map_err(|_| "Unable to prepare archive folder")?;
            let dest_parent =
                fs::canonicalize(dest_parent).map_err(|_| "Archive folder unavailable")?;
            if !within(&dest_parent, &archive) || dest.exists() {
                r.state = "failed".into();
                r.message = "Archive destination exists or escapes configured archive".into();
                persist(&store, &r)?;
                return Err("Archive destination exists or escapes configured archive".into());
            }
            let mut df = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dest)
                .map_err(|_| "Archive destination was claimed by another process")?;
            df.write_all(&bytes)
                .and_then(|_| df.sync_all())
                .map_err(|_| {
                    r.state = "uncertain".into();
                    r.message = "Archive write outcome is uncertain".into();
                    let _ = persist(&store, &r);
                    "Archive write outcome is uncertain"
                })?;
            drop(df);
            if hash(&fs::read(&dest).map_err(|_| "Archive verification failed")?) != r.archive_hash
            {
                r.state = "uncertain".into();
                r.message = "Archive verification failed".into();
                let _ = persist(&store, &r);
                return Err("Archive verification failed; source retained".into());
            }
            if faults.between_archive_and_proof {
                return Err(
                    "Archive exists without durable ownership proof; outcome is uncertain".into(),
                );
            }
            // This durable phase is the ownership proof. A crash before it is
            // conservatively uncertain because matching bytes alone prove no provenance.
            r.state = "archived".into();
            persist(&store, &r)?;
            if faults.after_archive {
                return Err("Archive published; ledger outcome is uncertain".into());
            }
            if !entry.is_empty() {
                if !current.is_empty() && !current.ends_with(b"\n") {
                    lf.write_all(b"\n")
                        .map_err(|_| "Ledger separator write failed; outcome uncertain")?;
                }
                lf.write_all(entry.as_bytes())
                    .and_then(|_| lf.sync_all())
                    .map_err(|_| {
                        r.state = "uncertain".into();
                        r.message = "Ledger append outcome is uncertain".into();
                        let _ = persist(&store, &r);
                        "Ledger append outcome is uncertain"
                    })?;
            }
            if !entry.is_empty() {
                lf.seek(SeekFrom::Start(0))
                    .map_err(|_| "Ledger verification failed")?;
                let mut after = Vec::new();
                lf.read_to_end(&mut after)
                    .map_err(|_| "Ledger verification failed")?;
                if !exact_entry_present(&after, entry.as_bytes())
                    || hash(entry.as_bytes()) != r.entry_hash
                {
                    r.state = "uncertain".into();
                    r.message = "Ledger verification failed".into();
                    let _ = persist(&store, &r);
                    return Err("Ledger verification failed; source retained".into());
                }
                appended_entry = Some(entry.clone());
            }
            if faults.after_ledger {
                return Err("Ledger published; final outcome is uncertain".into());
            }
            r.state = "succeeded".into();
            r.message = "Imported and archived".into();
            if faults.before_outcome {
                return Err("Final outcome publication is uncertain".into());
            }
            persist(&store, &r).map_err(|_| "Final outcome publication is uncertain")?;
            // Retain the reviewed source. Removing by path after releasing an
            // exclusive handle can delete a replacement installed by another process.
            drop(sf);
            Ok(())
        })();
        let committed = result.is_ok();
        let (state, retry_safe, message) = match result {
            Ok(()) => ("succeeded", false, "Confirmed".to_owned()),
            Err(e) if e.contains("uncertain") || e.contains("reconcile") => ("uncertain", false, e),
            Err(e) => ("failed", false, e),
        };
        let operation_id = operation_id.filter(|op| store.join(format!("{op}.json")).is_file());
        if committed {
            if let (Some(expected), Some(entry)) = (expected_ledger.as_mut(), appended_entry) {
                if !expected.is_empty() && !expected.ends_with(b"\n") {
                    expected.push(b'\n');
                }
                expected.extend_from_slice(entry.as_bytes());
            }
        }
        out.push(ItemOutcome {
            item_id: id,
            filename,
            state: state.into(),
            retry_safe,
            message,
            operation_id,
        });
    }
    out
}

pub(crate) fn commit_prepared(
    plans: Vec<(String, PreparedPlan)>,
    run_id: &str,
    cfg: ImportConfig,
) -> Vec<ItemOutcome> {
    commit_prepared_cancellable(plans, run_id, cfg, || false)
}

pub(crate) fn commit_prepared_cancellable(
    plans: Vec<(String, PreparedPlan)>,
    run_id: &str,
    cfg: ImportConfig,
    cancelled: impl FnMut() -> bool,
) -> Vec<ItemOutcome> {
    let _io = match managed_io() {
        Ok(guard) => guard,
        Err(message) => return vec![store_error(&message)],
    };
    if plans.iter().any(|(_, plan)| {
        plan.funding_account.is_none()
            || (!plan.duplicates.is_empty() && plan.duplicate_override.is_none())
            || (!plan.batch_duplicates.is_empty() && plan.batch_override.is_none())
    }) {
        return plans
            .into_iter()
            .map(|(item_id, plan)| ItemOutcome {
                item_id,
                filename: plan.item["filename"].as_str().unwrap_or("").to_owned(),
                state: "failed".into(),
                retry_safe: false,
            message: "Every selected receipt requires funding and any necessary duplicate-review decision".into(),
                operation_id: None,
            })
            .collect();
    }
    let current_ledger = fs::read(&cfg.ledger).ok();
    let mut items = Vec::with_capacity(plans.len());
    for (id, plan) in plans {
        let mut item = plan.item;
        item["match_id"] = serde_json::json!(id);
        item["_prepared_source_hash"] = serde_json::json!(plan.source_hash);
        item["_prepared_source_size"] = serde_json::json!(plan.source_size);
        item["_prepared_source_path"] = serde_json::json!(plan.source.to_string_lossy());
        item["_prepared_destination"] = serde_json::json!(plan.destination.to_string_lossy());
        item["_prepared_ledger_hash"] = serde_json::json!(plan.ledger_before_hash);
        item["_prepared_ledger_size"] = serde_json::json!(plan.ledger_before.len());
        item["_prepared_ledger_exact"] = serde_json::json!(current_ledger
            .as_ref()
            .is_some_and(|bytes| bytes.as_slice() == plan.ledger_before.as_slice()));
        item["_prepared_entry"] = serde_json::json!(plan.entry);
        item["_prepared_funding_account"] = serde_json::json!(plan.funding_account);
        item["_prepared_duplicate_override"] = serde_json::json!(plan.duplicate_override);
        item["_prepared_batch_override"] = serde_json::json!(plan.batch_override);
        item["_prepared_ledger_backup"] = serde_json::json!(plan.ledger_before);
        items.push(item);
    }
    commit_with_config_cancellable(items, run_id, cfg, FaultPlan::default(), cancelled)
}

fn prepared_counter<'a>(item: &'a serde_json::Value, fallback: &'a str) -> &'a str {
    item.get("_prepared_funding_account")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[cfg(windows)]
    fn funding_fixture() -> (PathBuf, ImportConfig, serde_json::Value) {
        let (dir, mut cfg, item) = fixture();
        cfg.counter = "Assets:Bank:Legacy:Unsorted".into();
        fs::write(
            &cfg.ledger,
            concat!(
                "2020-01-01 open Assets:Bank:Checking USD\n",
                "2020-01-01 open Assets:Retirement:HSA USD\n",
                "2020-01-01 open Liabilities:CreditCard:Venture USD\n",
                "2020-01-01 open Assets:Bank:Legacy:Unsorted USD\n",
                "2020-01-01 open Equity:Opening USD\n",
                "2020-01-01 open Expenses:Food USD\n",
                "2020-01-01 open Assets:Foreign TWD\n",
                "2027-01-01 open Assets:Future USD\n",
            "2020-01-02 * \"Synthetic opening\"\n  Expenses:Food 0 USD\n  Assets:Bank:Checking 0 USD\n",
            ),
        )
        .unwrap();
        (dir, cfg, item)
    }

    #[cfg(windows)]
    #[test]
    fn duplicate_review_blocks_missing_or_skip_decisions_and_records_explicit_override() {
        let (_dir, cfg, item) = funding_fixture();
        let mut baseline = fs::read(&cfg.ledger).unwrap();
        baseline.extend_from_slice(b"2026-10-23 * \"Groceries\"\n  Expenses:Food 12.34 USD\n  Assets:Bank:Checking -12.34 USD\n");
        fs::write(&cfg.ledger, &baseline).unwrap();
        let plan = assign_funding(
            prepare_review(&item, &cfg).unwrap(),
            &cfg,
            "Assets:Retirement:HSA",
        )
        .unwrap();
        assert_eq!(plan.duplicates.len(), 1);
        assert!(decide_duplicates(plan.clone(), None).is_err());
        assert!(decide_duplicates(plan.clone(), Some("already_recorded")).is_err());
        assert!(decide_duplicates(plan.clone(), Some("forged")).is_err());
        assert_eq!(
            commit_prepared(
                vec![("blocked".into(), plan.clone())],
                "blocked",
                cfg.clone()
            )[0]
            .state,
            "failed"
        );
        assert_eq!(fs::read(&cfg.ledger).unwrap(), baseline);
        let reviewed = decide_duplicates(plan, Some("import_separately")).unwrap();
        let out = commit_prepared(vec![("approved".into(), reviewed)], "approved", cfg.clone());
        assert_eq!(out[0].state, "succeeded", "{out:?}");
        let id = identity(&cfg.ledger, &hash(b"synthetic statement bytes"));
        let raw = fs::read(cfg.store.join(format!("{id}.json"))).unwrap();
        let receipt: Receipt = serde_json::from_slice(&raw).unwrap();
        let audit = receipt.duplicate_override.unwrap();
        assert_eq!(audit.candidate_ids.len(), 1);
        assert_eq!(audit.ledger_revision, hash(&baseline));
        assert_eq!(audit.funding_account, "Assets:Retirement:HSA");
        assert!(prepare_review(&item, &cfg).unwrap().duplicates.is_empty()); // verified same-PDF success is no new payment.
    }

    #[cfg(windows)]
    #[test]
    fn review_has_no_default_funding_and_unassigned_or_invalid_accounts_fail_closed() {
        let (_dir, cfg, item) = funding_fixture();
        let before = fs::read(&cfg.ledger).unwrap();
        let plan = prepare_review(&item, &cfg).unwrap();
        assert!(plan.funding_account.is_none());
        assert!(plan.entry.is_empty());
        let options = funding_accounts(&cfg).unwrap();
        assert!(options.contains(&"Assets:Retirement:HSA".into()));
        assert!(!options.contains(&cfg.counter));
        assert!(!options.contains(&"Assets:Foreign".into()));
        for bad in [
            "",
            "Assets:Unknown",
            "Equity:Opening",
            "Expenses:Food",
            "Assets:Bank:Legacy:Unsorted",
            "Assets:Foreign",
            "Assets:Future",
        ] {
            assert!(assign_funding(plan.clone(), &cfg, bad).is_err(), "{bad}");
        }
        let out = commit_prepared(vec![("unassigned".into(), plan)], "blocked", cfg.clone());
        assert_eq!(out[0].state, "failed");
        assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
        assert_eq!(fs::read_dir(&cfg.archive).unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn mixed_review_uses_each_funding_account_and_cannot_retarget_a_prior_import() {
        let (_dir, cfg, first) = funding_fixture();
        fs::write(cfg.inbox.join("second.pdf"), b"second receipt bytes").unwrap();
        let mut second = first.clone();
        second["filename"] = serde_json::json!("second.pdf");
        second["new_name"] = serde_json::json!("second 2026-10-23.pdf");
        second["amount"] = serde_json::json!("56.78");
        let before = fs::read(&cfg.ledger).unwrap();
        let a = assign_funding(
            prepare_review(&first, &cfg).unwrap(),
            &cfg,
            "Assets:Retirement:HSA",
        )
        .unwrap();
        let b = prepare_review(&second, &cfg).unwrap();
        let blocked = commit_prepared(
            vec![("a".into(), a.clone()), ("b".into(), b.clone())],
            "mixed-blocked",
            cfg.clone(),
        );
        assert!(blocked.iter().all(|r| r.state == "failed"));
        assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
        let b = assign_funding(b, &cfg, "Liabilities:CreditCard:Venture").unwrap();
        let out = commit_prepared(vec![("a".into(), a), ("b".into(), b)], "mixed", cfg.clone());
        assert!(out.iter().all(|r| r.state == "succeeded"), "{out:?}");
        let after = fs::read(&cfg.ledger).unwrap();
        assert!(after.starts_with(&before));
        let appended = std::str::from_utf8(&after[before.len()..]).unwrap();
        assert!(appended.contains("Assets:Retirement:HSA -12.34 USD"));
        assert!(appended.contains("Liabilities:CreditCard:Venture -56.78 USD"));
        assert!(!appended.contains("Legacy:Unsorted"));
        assert!(assign_funding(
            prepare_review(&first, &cfg).unwrap(),
            &cfg,
            "Liabilities:CreditCard:Venture"
        )
        .is_err());
        let repeat = assign_funding(
            prepare_review(&first, &cfg).unwrap(),
            &cfg,
            "Assets:Retirement:HSA",
        )
        .unwrap();
        assert_eq!(
            commit_prepared(vec![("again".into(), repeat)], "repeat", cfg.clone())[0].state,
            "succeeded"
        );
        assert_eq!(fs::read(&cfg.ledger).unwrap(), after);
    }

    #[cfg(windows)]
    #[test]
    fn funding_binding_keeps_analysis_time_ledger_evidence() {
        let (_dir, cfg, item) = funding_fixture();
        let plan = prepare_review(&item, &cfg).unwrap();
        let mut changed = fs::read(&cfg.ledger).unwrap();
        changed.extend_from_slice(b"; unrelated concurrent edit\n");
        fs::write(&cfg.ledger, &changed).unwrap();
        let plan = assign_funding(plan, &cfg, "Assets:Retirement:HSA").unwrap();
        assert_eq!(
            commit_prepared(vec![("stale".into(), plan)], "stale", cfg.clone())[0].state,
            "failed"
        );
        assert_eq!(fs::read(&cfg.ledger).unwrap(), changed);
        assert_eq!(fs::read_dir(&cfg.archive).unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn managed_refresh_lock_cannot_make_a_funded_commit_report_ledger_unavailable() {
        let (_dir, cfg, item) = funding_fixture();
        let plan = assign_funding(
            prepare_review(&item, &cfg).unwrap(),
            &cfg,
            "Assets:Retirement:HSA",
        )
        .unwrap();
        let refresh = managed_io().unwrap();
        let ledger_handle = exclusive(&cfg.ledger, false).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker_cfg = cfg.clone();
        let worker = std::thread::spawn(move || {
            tx.send(commit_prepared(
                vec![("funded".into(), plan)],
                "coordinated",
                worker_cfg,
            ))
            .unwrap();
        });
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_millis(30)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        drop(ledger_handle);
        drop(refresh);
        let outcomes = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        worker.join().unwrap();
        assert_eq!(outcomes[0].state, "succeeded", "{outcomes:?}");
        assert_eq!(outcomes_with_config(&cfg)[0].state, "succeeded");
    }
    #[test]
    fn production_validation_rejects_windows_escape_names_and_bad_dates() {
        for name in ["../x.pdf", "C:secret.pdf", "CON.pdf", "bad. ", "x:y.pdf"] {
            assert!(!safe_name(name), "{name}");
        }
        assert!(safe_name("Bank 2026-10-23.pdf"));
        assert!(!valid_date("2026-2-03"));
        assert!(!valid_date("2026-13-03"));
        assert!(token("Expenses:Food"));
        assert!(!token("Expenses:Food\nAssets:Cash"));
    }
    #[test]
    fn generated_entry_uses_exact_balanced_decimal_and_stable_marker() {
        let item = serde_json::json!({"date":"2026-10-23", "filename":"source.pdf", "new_name":"Bank 2026-10-23.pdf", "rule":"Groceries", "category":"Expenses:Food", "amount":"-12.340000000000000001"});
        let entry = build_entry(&item, "run-a", "item-a", "USD", "Assets:Bank:Checking").unwrap();
        assert!(entry.contains("Expenses:Food -12.340000000000000001 USD"));
        assert!(entry.contains("Assets:Bank:Checking 12.340000000000000001 USD"));
        let again = build_entry(&item, "run-b", "item-a", "USD", "Assets:Bank:Checking").unwrap();
        assert_eq!(entry.lines().nth(1), again.lines().nth(1));
        let mut bad = item.clone();
        bad["category"] = serde_json::json!("Expenses:Food\nAssets:Cash");
        assert!(build_entry(&bad, "run-a", "item-a", "USD", "Assets:Bank:Checking").is_err());
    }

    #[cfg(windows)]
    fn fixture() -> (PathBuf, ImportConfig, serde_json::Value) {
        let dir = std::env::temp_dir().join(format!(
            "recon-managed-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let inbox = dir.join("inbox");
        let archive = dir.join("archive");
        let store = dir.join("private");
        fs::create_dir_all(&inbox).unwrap();
        fs::create_dir_all(&archive).unwrap();
        fs::create_dir_all(&store).unwrap();
        let ledger = dir.join("synthetic.beancount");
        fs::write(&ledger, "; synthetic ledger\n").unwrap();
        fs::write(inbox.join("source.pdf"), b"synthetic statement bytes").unwrap();
        let config = ImportConfig::injected(
            fs::canonicalize(inbox).unwrap(),
            fs::canonicalize(archive).unwrap(),
            ledger,
            store,
            "USD".into(),
            "Assets:Bank:Checking".into(),
        );
        let item = serde_json::json!({"_managed_item_id":"stable-item","date":"2026-10-23","filename":"source.pdf","new_name":"Bank 2026-10-23.pdf","rule":"Groceries","category":"Expenses:Food","amount":"12.34","was_encrypted":false,"is_cleared":true});
        (dir, config, item)
    }

    #[cfg(windows)]
    #[test]
    fn durable_config_loads_only_when_no_run_config_is_bound() {
        let (_dir, cfg, _) = fixture();
        let calls = std::cell::Cell::new(0);
        let bound = bound_or_load_config(Some(cfg.clone()), || {
            calls.set(calls.get() + 1);
            Err("loader should not run".into())
        })
        .unwrap();
        assert_eq!(bound.test_ledger(), cfg.test_ledger());
        assert_eq!(calls.get(), 0);

        let loaded = bound_or_load_config(None, || {
            calls.set(calls.get() + 1);
            Ok(cfg.clone())
        })
        .unwrap();
        assert_eq!(loaded.test_ledger(), cfg.test_ledger());
        assert_eq!(calls.get(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn durable_source_requires_valid_receipt_and_existing_regular_inbox_source() {
        let (dir, cfg, item) = fixture();
        let outcomes =
            commit_with_config(vec![item], "show-source", cfg.clone(), FaultPlan::default());
        assert_eq!(outcomes[0].state, "succeeded", "{outcomes:?}");
        let operation_id = identity(&cfg.ledger, &hash(b"synthetic statement bytes"));
        let resolved = source_for_operation(&operation_id, &cfg).unwrap();
        assert_eq!(
            resolved,
            fs::canonicalize(dir.join("inbox/source.pdf")).unwrap()
        );

        for bad_id in ["unknown-operation", &"f".repeat(64)] {
            assert!(source_for_operation(bad_id, &cfg).is_err());
        }
        let foreign_ledger = dir.join("foreign.beancount");
        fs::write(&foreign_ledger, b"; foreign synthetic ledger\n").unwrap();
        let foreign_cfg = ImportConfig::injected(
            cfg.inbox.clone(),
            cfg.archive.clone(),
            foreign_ledger,
            cfg.store.clone(),
            cfg.currency.clone(),
            cfg.counter.clone(),
        );
        assert!(source_for_operation(&operation_id, &foreign_cfg).is_err());

        let receipt_path = cfg.store.join(format!("{operation_id}.json"));
        let original = fs::read(&receipt_path).unwrap();
        let mut receipt: serde_json::Value = serde_json::from_slice(&original).unwrap();
        receipt["filename"] = serde_json::json!("../escape.pdf");
        fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(source_for_operation(&operation_id, &cfg).is_err());

        fs::write(&receipt_path, &original).unwrap();
        receipt = serde_json::from_slice(&original).unwrap();
        receipt["destination"] = serde_json::json!(dir
            .join("archive/../outside.pdf")
            .to_string_lossy()
            .to_string());
        fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(source_for_operation(&operation_id, &cfg).is_err());

        fs::write(&receipt_path, &original).unwrap();
        fs::remove_file(dir.join("inbox/source.pdf")).unwrap();
        assert!(source_for_operation(&operation_id, &cfg).is_err());
        let outside = dir.join("outside.pdf");
        fs::write(&outside, b"outside synthetic source").unwrap();
        let source_link = dir.join("inbox/source.pdf");
        match std::os::windows::fs::symlink_file(&outside, &source_link) {
            Ok(()) => {
                assert!(source_for_operation(&operation_id, &cfg).is_err());
            }
            Err(error) => {
                println!("SKIP source symlink escape: Windows rejected symlink creation ({error})")
            }
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    #[test]
    fn durable_source_resolves_failed_receipt_without_running_analysis() {
        let (_dir, cfg, item) = fixture();
        let outcomes = commit_with_config(
            vec![item],
            "show-source-failed",
            cfg.clone(),
            FaultPlan {
                before_journal: true,
                ..FaultPlan::default()
            },
        );
        assert_eq!(outcomes[0].state, "failed", "{outcomes:?}");
        let operation_id = identity(&cfg.ledger, &hash(b"synthetic statement bytes"));
        assert!(source_for_operation(&operation_id, &cfg).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn cancellation_before_first_item_has_no_effect_and_remains_retry_safe() {
        let (_dir, cfg, item) = fixture();
        let plan = prepare(&item, &cfg).unwrap();
        let before = fs::read(&cfg.ledger).unwrap();
        let outcomes = commit_prepared_cancellable(
            vec![("cancel-before".into(), plan)],
            "run-cancel-before",
            cfg.clone(),
            || true,
        );
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].state, "failed");
        assert!(outcomes[0].retry_safe);
        assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
        assert!(!cfg
            .archive
            .join("Statements 2026/Bank 2026-10-23.pdf")
            .exists());
    }

    #[cfg(windows)]
    #[test]
    fn cancellation_after_confirmed_item_preserves_first_success_and_skips_later_item() {
        let (dir, cfg, item) = fixture();
        fs::write(
            cfg.inbox.join("source-two.pdf"),
            b"second synthetic statement",
        )
        .unwrap();
        let mut second = item.clone();
        second["filename"] = serde_json::json!("source-two.pdf");
        second["new_name"] = serde_json::json!("Bank 2026-10-24.pdf");
        second["date"] = serde_json::json!("2026-10-24");
        let first_plan = prepare(&item, &cfg).unwrap();
        let second_plan = prepare(&second, &cfg).unwrap();
        let checks = std::sync::atomic::AtomicU64::new(0);
        let outcomes = commit_prepared_cancellable(
            vec![("first".into(), first_plan), ("second".into(), second_plan)],
            "run-cancel-after",
            cfg.clone(),
            || checks.fetch_add(1, Ordering::SeqCst) > 0,
        );
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].state, "succeeded");
        assert_eq!(outcomes[1].state, "failed");
        assert!(!outcomes[1].retry_safe, "later item shares the batch's pre-write ledger image and is never advertised retry-safe without reconciliation");
        let ledger = fs::read_to_string(&cfg.ledger).unwrap();
        assert!(ledger.contains("Bank 2026-10-23.pdf"));
        assert!(!ledger.contains("Bank 2026-10-24.pdf"));
        assert!(cfg
            .archive
            .join("Statements 2026/Bank 2026-10-23.pdf")
            .exists());
        assert!(!cfg
            .archive
            .join("Statements 2026/Bank 2026-10-24.pdf")
            .exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    #[test]
    fn analysis_prepared_plan_rejects_source_substitution_before_any_effect() {
        let (dir, cfg, item) = fixture();
        let plan = prepare(&item, &cfg).unwrap();
        let ledger_before = fs::read(&cfg.ledger).unwrap();
        fs::write(
            dir.join("inbox/source.pdf"),
            b"substituted bytes after review",
        )
        .unwrap();
        let destination = cfg
            .archive
            .join("Statements 2026")
            .join("Bank 2026-10-23.pdf");
        let outcomes = commit_prepared(
            vec![("backend-match-1".into(), plan)],
            "run-one",
            cfg.clone(),
        );
        assert_eq!(outcomes[0].state, "failed");
        assert!(!outcomes[0].retry_safe);
        assert!(outcomes[0]
            .operation_id
            .as_ref()
            .is_some_and(|id| id.len() == 64));
        let restarted = outcomes_with_config(&cfg);
        assert_eq!(restarted.len(), 1);
        assert_eq!(restarted[0].state, "failed");
        assert!(!restarted[0].retry_safe);
        assert_eq!(fs::read(&cfg.ledger).unwrap(), ledger_before);
        assert!(!destination.exists());
        assert_eq!(
            fs::read(dir.join("inbox/source.pdf")).unwrap(),
            b"substituted bytes after review"
        );
    }

    #[cfg(windows)]
    #[test]
    fn prepared_busy_ledger_failure_is_explicit_and_makes_no_durable_claim() {
        let (_dir, cfg, item) = fixture();
        let plan = prepare(&item, &cfg).unwrap();
        let before = fs::read(&cfg.ledger).unwrap();
        let held = exclusive(&cfg.ledger, true).unwrap();
        let outcomes = commit_prepared(
            vec![("busy-ledger-item".into(), plan)],
            "run-busy",
            cfg.clone(),
        );
        drop(held);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].state, "uncertain");
        assert!(!outcomes[0].retry_safe);
        assert_eq!(outcomes[0].operation_id, None);
        assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
        assert!(!cfg
            .archive
            .join("Statements 2026/Bank 2026-10-23.pdf")
            .exists());
    }

    #[cfg(windows)]
    #[test]
    fn analysis_plan_rejects_ledger_revision_change_before_effects() {
        let (_dir, cfg, item) = fixture();
        let plan = prepare(&item, &cfg).unwrap();
        let before = fs::read(&cfg.ledger).unwrap();
        fs::write(&cfg.ledger, b"; unrelated concurrent ledger change\n").unwrap();
        let outcomes = commit_prepared(
            vec![("backend-match-ledger".into(), plan)],
            "run",
            cfg.clone(),
        );
        assert_eq!(outcomes[0].state, "failed");
        assert_eq!(
            fs::read(&cfg.ledger).unwrap(),
            b"; unrelated concurrent ledger change\n"
        );
        assert!(!cfg
            .archive
            .join("Statements 2026/Bank 2026-10-23.pdf")
            .exists());
        assert_ne!(fs::read(&cfg.ledger).unwrap(), before);
    }

    #[cfg(windows)]
    #[test]
    fn prepared_commit_uses_institution_date_filename_and_content_identity_once() {
        let (_dir, cfg, item) = fixture();
        let plan = prepare(&item, &cfg).unwrap();
        let retry_plan = plan.clone();
        let source = cfg.inbox.join("source.pdf");
        let first = commit_prepared(vec![("match-a".into(), plan)], "run-one", cfg.clone());
        assert_eq!(first[0].state, "succeeded", "{}", first[0].message);
        let after_first = fs::read(&cfg.ledger).unwrap();
        fs::remove_file(source).unwrap();
        let second = commit_prepared(vec![("match-b".into(), retry_plan)], "run-two", cfg.clone());
        assert_eq!(second[0].state, "succeeded", "{}", second[0].message);
        assert_eq!(fs::read(&cfg.ledger).unwrap(), after_first);
        assert!(cfg
            .archive
            .join("Statements 2026/Bank 2026-10-23.pdf")
            .is_file());
        assert!(fs::read_to_string(&cfg.ledger)
            .unwrap()
            .contains("navis_recon_import"));
        let mut edited = fs::read(&cfg.ledger).unwrap();
        edited.extend_from_slice(b"\n; unrelated later ledger edit\n");
        fs::write(&cfg.ledger, edited).unwrap();
        let reconciled = outcomes_with_config(&cfg);
        assert_eq!(
            reconciled[0].state, "succeeded",
            "{}",
            reconciled[0].message
        );
        let mut forged = String::from_utf8(fs::read(&cfg.ledger).unwrap()).unwrap();
        forged = forged.replacen("Expenses:Food", "Expenses:Other", 1);
        fs::write(&cfg.ledger, forged).unwrap();
        let mismatch = outcomes_with_config(&cfg);
        assert_eq!(mismatch[0].state, "uncertain");

        let (_dir, cfg, item) = fixture();
        let imported = commit_with_config(
            vec![item],
            "run-missing-archive",
            cfg.clone(),
            FaultPlan::default(),
        );
        assert_eq!(imported[0].state, "succeeded");
        fs::remove_file(cfg.archive.join("Statements 2026/Bank 2026-10-23.pdf")).unwrap();
        let missing = outcomes_with_config(&cfg);
        assert_eq!(missing[0].state, "uncertain");
    }

    #[cfg(windows)]
    #[test]
    fn production_commit_helper_imports_once_across_runs_and_hydrates_receipt() {
        let (dir, cfg, item) = fixture();
        let ledger = cfg.ledger.clone();
        let store = cfg.store.clone();
        let first = commit_with_config(vec![item.clone()], "run-one", cfg, FaultPlan::default());
        assert_eq!(first[0].state, "succeeded", "{}", first[0].message);
        assert!(dir.join("inbox/source.pdf").exists());
        let before = fs::read(&ledger).unwrap();
        fs::write(dir.join("inbox/source.pdf"), b"synthetic statement bytes").unwrap();
        let mut second_item = item.clone();
        second_item["new_name"] = serde_json::json!("2026-10-23 Other.pdf");
        let again = commit_with_config(
            vec![second_item],
            "run-two",
            ImportConfig::injected(
                fs::canonicalize(dir.join("inbox")).unwrap(),
                fs::canonicalize(dir.join("archive")).unwrap(),
                ledger.clone(),
                store.clone(),
                "USD".into(),
                "Assets:Bank:Checking".into(),
            ),
            FaultPlan::default(),
        );
        assert_eq!(again[0].state, "succeeded", "{}", again[0].message);
        assert_eq!(fs::read(&ledger).unwrap(), before);
        let receipts = fs::read_dir(&store)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .count();
        assert_eq!(receipts, 1);
    }

    #[cfg(windows)]
    #[test]
    fn production_commit_helper_never_overwrites_archive_and_keeps_source() {
        let (dir, cfg, item) = fixture();
        let ledger = cfg.ledger.clone();
        let dest = cfg
            .archive
            .join("Statements 2026")
            .join("Bank 2026-10-23.pdf");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, b"unknown existing provenance").unwrap();
        let before = fs::read(&ledger).unwrap();
        let result = commit_with_config(vec![item], "run", cfg, FaultPlan::default());
        assert_eq!(result[0].state, "failed");
        assert_eq!(fs::read(&dest).unwrap(), b"unknown existing provenance");
        assert_eq!(fs::read(&ledger).unwrap(), before);
        assert!(dir.join("inbox/source.pdf").exists());
    }

    #[cfg(windows)]
    #[test]
    fn intent_precedes_effects_and_partial_effects_are_never_replayed() {
        let (dir, cfg, item) = fixture();
        let operation_id = identity(&cfg.ledger, &hash(b"synthetic statement bytes"));
        let before = fs::read(&cfg.ledger).unwrap();
        let failed = commit_with_config(
            vec![item.clone()],
            "run",
            cfg.clone(),
            FaultPlan {
                before_journal: true,
                ..FaultPlan::default()
            },
        );
        assert_eq!(failed[0].state, "failed");
        assert_eq!(fs::read(&dir.join("synthetic.beancount")).unwrap(), before);
        assert!(dir.join("inbox/source.pdf").exists());
        let retryable = outcomes_with_config(&cfg);
        assert_eq!(retryable[0].state, "failed");
        assert!(retryable[0].retry_safe);
        let retried = retry_with_config(vec![operation_id.clone()], cfg.clone());
        assert_eq!(retried[0].state, "succeeded", "{}", retried[0].message);
        let after = fs::read(&cfg.ledger).unwrap();
        assert!(String::from_utf8_lossy(&after).contains("navis_recon_import"));
        assert_eq!(
            retry_with_config(vec![operation_id], cfg.clone())[0].state,
            "succeeded"
        );
        assert_eq!(fs::read(&cfg.ledger).unwrap(), after);

        let (_dir2, cfg2, item2) = fixture();
        let before = fs::read(&cfg2.ledger).unwrap();
        let uncertain = commit_with_config(
            vec![item2.clone()],
            "run-ledger-fault",
            cfg2.clone(),
            FaultPlan {
                after_ledger: true,
                ..FaultPlan::default()
            },
        );
        assert_eq!(uncertain[0].state, "uncertain", "{}", uncertain[0].message);
        let after = fs::read(&cfg2.ledger).unwrap();
        assert_ne!(after, before);
        let recovered = outcomes_with_config(&cfg2);
        assert_eq!(recovered[0].state, "succeeded", "{}", recovered[0].message);
        let cfg = ImportConfig::injected(
            cfg2.inbox.clone(),
            cfg2.archive.clone(),
            cfg2.ledger.clone(),
            cfg2.store.clone(),
            cfg2.currency.clone(),
            cfg2.counter.clone(),
        );
        let retry = commit_with_config(vec![item2], "retry", cfg, FaultPlan::default());
        assert_eq!(retry[0].state, "succeeded");
        assert_eq!(fs::read(&cfg2.ledger).unwrap(), after);
    }

    #[cfg(windows)]
    #[test]
    fn retry_cancellation_preserves_effects_and_never_starts_later_items() {
        for cancel_before_first in [true, false] {
            let (_dir, cfg, item) = fixture();
            fs::write(
                cfg.inbox.join("source-two.pdf"),
                b"second synthetic statement",
            )
            .unwrap();
            let mut second = item.clone();
            second["filename"] = serde_json::json!("source-two.pdf");
            second["new_name"] = serde_json::json!("Bank 2026-10-24.pdf");
            second["date"] = serde_json::json!("2026-10-24");
            let plans = vec![
                ("first".into(), prepare(&item, &cfg).unwrap()),
                ("second".into(), prepare(&second, &cfg).unwrap()),
            ];
            let before = fs::read(&cfg.ledger).unwrap();
            let seeded = commit_prepared_cancellable(plans, "cancelled-seed", cfg.clone(), || true);
            assert!(seeded.iter().all(|o| o.state == "failed"));
            let ids = seeded
                .iter()
                .map(|o| o.operation_id.clone().unwrap())
                .collect();
            let mut checks = 0;
            let outcomes = retry_with_config_cancellable(ids, cfg.clone(), || {
                checks += 1;
                cancel_before_first || checks > 1
            });
            assert_eq!(
                outcomes[0].state,
                if cancel_before_first {
                    "failed"
                } else {
                    "succeeded"
                }
            );
            assert_eq!(outcomes[1].state, "failed");
            assert!(!outcomes[1].retry_safe); // Reconciliation must recheck the current ledger.
            assert!(outcomes[1].message.contains("cancelled"));
            assert!(!cfg
                .archive
                .join("Statements 2026/Bank 2026-10-24.pdf")
                .exists());
            if cancel_before_first {
                assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
                assert!(outcomes_with_config(&cfg).iter().all(|o| o.retry_safe));
            } else {
                let after = fs::read_to_string(&cfg.ledger).unwrap();
                assert!(after.contains("Bank 2026-10-23.pdf"));
                assert!(!after.contains("Bank 2026-10-24.pdf"));
                let durable = outcomes_with_config(&cfg);
                assert_eq!(durable.iter().filter(|o| o.state == "succeeded").count(), 1);
                assert!(durable
                    .iter()
                    .filter(|o| o.state != "succeeded")
                    .all(|o| !o.retry_safe));
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn retry_cancellation_retains_owned_archive_without_appending_ledger() {
        let (_dir, cfg, item) = fixture();
        let before = fs::read(&cfg.ledger).unwrap();
        let interrupted = commit_with_config(
            vec![item],
            "archive-interrupted",
            cfg.clone(),
            FaultPlan {
                after_archive: true,
                ..FaultPlan::default()
            },
        );
        assert_eq!(interrupted[0].state, "uncertain");
        let id = identity(&cfg.ledger, &hash(b"synthetic statement bytes"));
        let archive = cfg.archive.join("Statements 2026/Bank 2026-10-23.pdf");
        let archive_before = fs::read(&archive).unwrap();
        let outcome = retry_with_config_cancellable(vec![id.clone()], cfg.clone(), || true);
        assert_eq!(outcome[0].state, "uncertain");
        assert!(!outcome[0].retry_safe);
        assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
        assert_eq!(fs::read(&archive).unwrap(), archive_before);
        assert_eq!(
            retry_with_config(vec![id], cfg.clone())[0].state,
            "succeeded"
        );
    }

    #[cfg(windows)]
    #[test]
    fn restart_retry_promotes_no_effect_intent_and_finishes_owned_archive_once() {
        let (_dir, cfg, item) = fixture();
        let source_hash = hash(b"synthetic statement bytes");
        let operation_id = identity(&cfg.ledger, &source_hash);
        let interrupted = commit_with_config(
            vec![item.clone()],
            "run-interrupted",
            cfg.clone(),
            FaultPlan {
                after_intent: true,
                ..FaultPlan::default()
            },
        );
        assert_eq!(interrupted[0].state, "failed");
        let available = outcomes_with_config(&cfg);
        assert_eq!(available[0].state, "failed");
        assert!(available[0].retry_safe);
        let retried = retry_with_config(vec![operation_id.clone()], cfg.clone());
        assert_eq!(retried[0].state, "succeeded", "{}", retried[0].message);
        let ledger_once = fs::read(&cfg.ledger).unwrap();
        assert_eq!(
            retry_with_config(vec![operation_id.clone()], cfg.clone())[0].state,
            "succeeded"
        );
        assert_eq!(fs::read(&cfg.ledger).unwrap(), ledger_once);

        let (_dir, cfg, item) = fixture();
        let operation_id = identity(&cfg.ledger, &hash(b"synthetic statement bytes"));
        let interrupted = commit_with_config(
            vec![item],
            "run-archive-only",
            cfg.clone(),
            FaultPlan {
                after_archive: true,
                ..FaultPlan::default()
            },
        );
        assert_eq!(interrupted[0].state, "uncertain");
        let available = outcomes_with_config(&cfg);
        assert!(available[0].retry_safe, "{}", available[0].message);
        let recovered = retry_with_config(vec![operation_id], cfg.clone());
        assert_eq!(recovered[0].state, "succeeded", "{}", recovered[0].message);
        assert!(
            String::from_utf8_lossy(&fs::read(&cfg.ledger).unwrap()).contains("navis_recon_import")
        );

        let (dir, cfg, item) = fixture();
        let interrupted = commit_with_config(
            vec![item],
            "run-changed-source",
            cfg.clone(),
            FaultPlan {
                after_intent: true,
                ..FaultPlan::default()
            },
        );
        assert_eq!(interrupted[0].state, "failed");
        fs::write(
            dir.join("inbox/source.pdf"),
            b"changed after interrupted intent",
        )
        .unwrap();
        let uncertain = outcomes_with_config(&cfg);
        assert_eq!(uncertain[0].state, "uncertain");
        assert!(!uncertain[0].retry_safe);
    }

    #[cfg(windows)]
    #[test]
    fn corrupted_receipt_and_held_journal_or_ledger_locks_fail_closed() {
        let (dir, cfg, item) = fixture();
        let unavailable = ImportConfig::injected(
            cfg.inbox.clone(),
            cfg.archive.clone(),
            cfg.ledger.clone(),
            dir.join("missing-private-store"),
            cfg.currency.clone(),
            cfg.counter.clone(),
        );
        let unavailable_result = outcomes_with_config(&unavailable);
        assert_eq!(unavailable_result.len(), 1);
        assert_eq!(unavailable_result[0].state, "uncertain");
        let corrupt = cfg.store.join("deadbeef.json");
        fs::write(&corrupt, b"{not-json").unwrap();
        let listing = outcomes_with_config(&cfg);
        assert!(listing
            .iter()
            .any(|o| o.state == "uncertain" && o.message.contains("corrupt")));
        fs::remove_file(corrupt).unwrap();

        let before = fs::read(&cfg.ledger).unwrap();
        let held_journal = journal_lock(&cfg.store).unwrap();
        let result = commit_with_config(
            vec![item.clone()],
            "held-journal",
            cfg.clone(),
            FaultPlan::default(),
        );
        drop(held_journal);
        assert_eq!(result[0].state, "failed");
        assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
        assert!(!cfg
            .archive
            .join("Statements 2026/Bank 2026-10-23.pdf")
            .exists());

        let held_ledger = exclusive(&cfg.ledger, true).unwrap();
        let result =
            commit_with_config(vec![item], "held-ledger", cfg.clone(), FaultPlan::default());
        drop(held_ledger);
        assert_eq!(result[0].state, "failed");
        assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
        assert!(!cfg
            .archive
            .join("Statements 2026/Bank 2026-10-23.pdf")
            .exists());
        assert!(dir.join("inbox/source.pdf").exists());
    }

    #[cfg(windows)]
    #[test]
    fn archive_without_synced_ownership_proof_stays_uncertain_and_unretryable() {
        let (_dir, cfg, item) = fixture();
        let operation_id = identity(&cfg.ledger, &hash(b"synthetic statement bytes"));
        let before = fs::read(&cfg.ledger).unwrap();
        let result = commit_with_config(
            vec![item],
            "run",
            cfg.clone(),
            FaultPlan {
                between_archive_and_proof: true,
                ..FaultPlan::default()
            },
        );
        assert_eq!(result[0].state, "uncertain");
        let outcomes = outcomes_with_config(&cfg);
        assert_eq!(outcomes[0].state, "uncertain");
        assert!(!outcomes[0].retry_safe);
        assert_eq!(
            retry_with_config(vec![operation_id], cfg.clone())[0].state,
            "uncertain"
        );
        assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
    }

    #[cfg(windows)]
    #[test]
    fn production_commit_helper_keeps_partial_batch_outcomes_independent() {
        let (dir, cfg, item) = fixture();
        let mut bad = item.clone();
        bad["filename"] = serde_json::json!("../escape.pdf");
        let results = commit_with_config(vec![item, bad], "run", cfg, FaultPlan::default());
        assert_eq!(
            results.iter().map(|r| r.state.as_str()).collect::<Vec<_>>(),
            vec!["succeeded", "failed"]
        );
        assert!(dir.join("inbox/source.pdf").exists());
    }

    #[cfg(windows)]
    #[test]
    fn duplicate_batch_destinations_are_all_rejected_before_first_effect() {
        let (_dir, cfg, item) = fixture();
        let before = fs::read(&cfg.ledger).unwrap();
        let results = commit_with_config(
            vec![item.clone(), item],
            "run",
            cfg.clone(),
            FaultPlan::default(),
        );
        assert_eq!(
            results.iter().map(|r| r.state.as_str()).collect::<Vec<_>>(),
            vec!["failed", "failed"]
        );
        assert_eq!(fs::read(&cfg.ledger).unwrap(), before);
        assert!(!cfg
            .archive
            .join("Statements 2026/Bank 2026-10-23.pdf")
            .exists());
    }

    #[cfg(windows)]
    #[test]
    fn prepared_multi_item_batch_appends_against_rolling_locked_ledger_revision() {
        let (dir, cfg, first_item) = fixture();
        fs::write(cfg.inbox.join("second.pdf"), b"second synthetic statement").unwrap();
        let first = prepare(&first_item, &cfg).unwrap();
        let mut second_item = first_item.clone();
        second_item["filename"] = serde_json::json!("second.pdf");
        second_item["new_name"] = serde_json::json!("Credit 2026-10-23.pdf");
        second_item["category"] = serde_json::json!("Expenses:Travel");
        second_item["amount"] = serde_json::json!("7.00");
        let second = prepare(&second_item, &cfg).unwrap();
        let outcomes = commit_prepared(
            vec![("batch-one".into(), first), ("batch-two".into(), second)],
            "batch-run",
            cfg.clone(),
        );
        assert_eq!(
            outcomes
                .iter()
                .map(|outcome| outcome.state.as_str())
                .collect::<Vec<_>>(),
            vec!["succeeded", "succeeded"]
        );
        let ledger = fs::read_to_string(&cfg.ledger).unwrap();
        assert_eq!(ledger.matches("navis_recon_import").count(), 2);
        assert!(cfg
            .archive
            .join("Statements 2026/Bank 2026-10-23.pdf")
            .exists());
        assert!(cfg
            .archive
            .join("Statements 2026/Credit 2026-10-23.pdf")
            .exists());
        assert!(dir.join("inbox/second.pdf").exists());
        let receipts = outcomes_with_config(&cfg);
        assert_eq!(receipts.len(), 2);
        assert!(
            receipts.iter().all(|outcome| outcome.state == "succeeded"),
            "{receipts:?}"
        );
    }
}
