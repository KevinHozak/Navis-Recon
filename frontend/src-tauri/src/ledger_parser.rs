use crate::money::Money;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

#[derive(Debug, Clone)]
pub(crate) struct Posting {
    pub(crate) account: String,
    pub(crate) amount: Money,
    pub(crate) currency: String,
}
#[derive(Debug, Clone)]
pub(crate) struct Transaction {
    pub(crate) date: String,
    pub(crate) payee: String,
    pub(crate) narration: String,
    pub(crate) postings: Vec<Posting>,
}
#[derive(Debug, Serialize)]
pub struct LedgerSummary {
    pub status: String,
    pub diagnostics: Vec<String>,
    pub currency: String,
    pub selected_currency: String,
    pub excluded_currencies: Vec<String>,
    pub unvalued_units: Vec<String>,
    pub totals_by_currency: BTreeMap<String, CurrencyTotals>,
    pub net_worth: Money,
    pub total_assets: Money,
    pub total_liabilities: Money,
    pub bank_accounts: BTreeMap<String, Money>,
    pub retirement: BTreeMap<String, Money>,
    pub investments: BTreeMap<String, Money>,
    pub credit_cards: BTreeMap<String, Money>,
    pub housing: Money,
    pub cash: Money,
    pub ledger_revision: String,
    pub read_at_unix_ms: u128,
    pub latest_transaction_date: String,
    pub valuation_basis: String,
    pub validation_status: String,
    pub coverage: Vec<AccountCoverage>,
    pub transaction_count: usize,
    pub posting_count: usize,
    pub account_count: usize,
    pub selected_included_account_count: usize,
    pub selected_excluded_account_count: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct AccountCoverage {
    pub account: String,
    pub currency: String,
    pub balance: Money,
    pub included_in_net_worth: bool,
    pub reason: String,
}
#[derive(Debug, Serialize)]
pub struct LedgerSourceState {
    pub expected_revision: String,
    pub observed_revision: Option<String>,
    pub state: String,
    pub checked_at_unix_ms: u128,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CurrencyTotals {
    pub total_assets: Money,
    pub total_liabilities: Money,
    pub net_worth: Money,
}
#[derive(Debug, Clone, Serialize)]
pub struct HistorySnapshot {
    pub name: String,
    pub currency: String,
    #[serde(rename = "Net Worth")]
    pub net_worth: Money,
    #[serde(rename = "401k")]
    pub retirement_401k: Money,
    #[serde(rename = "IRA Primary")]
    pub ira_primary: Money,
    #[serde(rename = "IRA Secondary")]
    pub ira_secondary: Money,
    #[serde(rename = "Stocks")]
    pub stocks: Money,
    #[serde(rename = "Stocks Secondary")]
    pub stocks_secondary: Money,
    #[serde(rename = "HSA")]
    pub hsa: Money,
}
#[derive(Debug, Serialize)]
pub struct LedgerTransaction {
    pub date: String,
    pub payee: String,
    pub narration: String,
    pub account: String,
    pub amount: Money,
    pub currency: String,
}

fn read_ledger_path(path: &str) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|e| format!("unavailable: {path}: line 1: failed to read ledger: {e}"))?;
    decode_ledger(path, bytes)
}
fn decode_ledger(path: &str, bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes)
        .map_err(|e| format!("unavailable: {path}: line 1: ledger is not valid UTF-8: {e}"))
}
pub(crate) fn parse_error_message(path: &str, diagnostics: Vec<String>) -> String {
    format!("partial: {path}: {}", diagnostics.join("; "))
}
fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| [4, 7].contains(&i) || c.is_ascii_digit())
    {
        return false;
    }
    let y = s[..4].parse::<i32>().unwrap_or(0);
    let m = s[5..7].parse::<u32>().unwrap_or(0);
    let d = s[8..].parse::<u32>().unwrap_or(0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 400 == 0 || (y % 4 == 0 && y % 100 != 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    y > 0 && d > 0 && d <= days
}
fn account_ok(s: &str) -> bool {
    let mut parts = s.split(':');
    matches!(
        parts.next(),
        Some("Assets" | "Liabilities" | "Equity" | "Income" | "Expenses")
    ) && parts.all(|p| {
        let mut chars = p.chars();
        matches!(chars.next(), Some(c) if c.is_ascii_uppercase() || c.is_ascii_digit())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '-')
    }) && s.contains(':')
}
fn quoted(s: &str) -> Option<(String, &str)> {
    let s = s.strip_prefix('"')?;
    let end = s.find('"')?;
    let value = &s[..end];
    if value.contains('\\') {
        return None;
    }
    if s[end + 1..]
        .chars()
        .next()
        .is_some_and(|c| !c.is_ascii_whitespace())
    {
        return None;
    }
    Some((value.to_string(), &s[end + 1..]))
}
fn header(line: &str) -> Option<(String, String, String)> {
    let (date, rest) = line.split_once(' ')?;
    if !valid_date(date) {
        return None;
    }
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('*').or_else(|| rest.strip_prefix('!'))?;
    if rest
        .chars()
        .next()
        .is_some_and(|c| !c.is_ascii_whitespace())
    {
        return None;
    }
    let rest = rest.trim_start();
    let (a, tail) = quoted(rest)?;
    let tail = tail.trim_start();
    if tail.is_empty() {
        return Some((date.into(), String::new(), a));
    }
    let (b, tail) = quoted(tail)?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some((date.into(), a, b))
}
fn location(line: usize, msg: &str) -> String {
    format!("line {line}: {msg}")
}
fn plain_decimal(s: &str) -> Option<Money> {
    let body = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    let mut dot = false;
    let mut digits = 0;
    for c in body.chars() {
        if c == '.' && !dot {
            dot = true;
        } else if c.is_ascii_digit() {
            digits += 1;
        } else {
            return None;
        }
    }
    if digits == 0 {
        return None;
    }
    Money::parse(s).ok()
}
fn parse_with_currency(contents: &str, currency: &str) -> Result<Vec<Transaction>, Vec<String>> {
    let lines: Vec<_> = contents.lines().collect();
    let mut txs = vec![];
    let mut errs = vec![];
    let mut i = 0;
    let mut seen_directive = false;
    let mut totals: BTreeMap<(String, String), Money> = BTreeMap::new();
    let mut ledger_magnitude = Money::default();
    let max_movement = Money::from_scaled(Money::MAX).expect("MAX is within i128");
    if !valid_currency(currency) {
        return Err(vec![location(
            1,
            "ledger_currency must be an uppercase unit token",
        )]);
    }
    while i < lines.len() {
        let line = lines[i];
        let trim = line.trim();
        if trim.is_empty() || trim.starts_with(';') {
            i += 1;
            continue;
        }
        if !(line.starts_with(' ') || line.starts_with('\t')) && line.starts_with("option ") {
            if line.contains("\\\"") {
                errs.push(location(i + 1, "escaped quotes are unsupported"));
                i += 1;
                continue;
            }
            let mut r = &line[7..];
            let (key, t) = match quoted(r) {
                Some(x) => x,
                None => {
                    errs.push(location(i + 1, "malformed option"));
                    i += 1;
                    continue;
                }
            };
            r = t.trim_start();
            let (val, end) = match quoted(r) {
                Some(x) => x,
                None => {
                    errs.push(location(i + 1, "malformed option value"));
                    i += 1;
                    continue;
                }
            };
            if !end.trim().is_empty() || key != "title" || val.is_empty() {
                errs.push(location(
                    i + 1,
                    "unsupported option (only a fully quoted title is supported)",
                ))
            }
            i += 1;
            continue;
        }
        if !(line.starts_with(' ') || line.starts_with('\t')) {
            if let Some((date, rest)) = trim.split_once(' ') {
                if valid_date(date) && rest.starts_with("open ") {
                    let fields: Vec<_> = rest.split_whitespace().collect();
                    if fields.len() < 2
                        || fields.len() > 3
                        || !account_ok(fields[1])
                        || (fields.len() == 3 && !valid_currency(fields[2]))
                    {
                        errs.push(location(i+1,"invalid open directive; expected YYYY-MM-DD open ACCOUNT [LEDGER_CURRENCY]"))
                    }
                    i += 1;
                    continue;
                }
            }
        }
        if !(line.starts_with(' ') || line.starts_with('\t')) {
            if line.contains("\\\"") {
                errs.push(location(i + 1, "escaped quotes are unsupported"));
                i += 1;
                continue;
            }
            if let Some((date, payee, narration)) = header(line) {
                let start = i + 1;
                let mut posts = vec![];
                let mut sums: BTreeMap<String, Money> = BTreeMap::new();
                let mut magnitude = Money::default();
                i += 1;
                while i < lines.len()
                    && ((lines[i].starts_with(' ') || lines[i].starts_with('\t'))
                        || lines[i].trim().is_empty()
                        || lines[i].trim_start().starts_with(';'))
                {
                    let raw = lines[i];
                    let t = raw.trim();
                    if t.is_empty() || t.starts_with(';') {
                        i += 1;
                        continue;
                    }
                    let f: Vec<_> = t.split_whitespace().collect();
                    let ln = i + 1;
                    if f.len() != 3 {
                        errs.push(location(ln,"posting requires account, finite amount, and explicit ledger currency; costs/prices/metadata are unsupported"));
                        i += 1;
                        continue;
                    }
                    if !account_ok(f[0]) {
                        errs.push(location(ln, "invalid or unsupported account"));
                        i += 1;
                        continue;
                    }
                    if f.len() == 3 && !valid_currency(f[2]) {
                        errs.push(location(
                            ln,
                            "posting currency must be an uppercase unit token",
                        ));
                        i += 1;
                        continue;
                    }
                    let amount = plain_decimal(f[1]);
                    if let Some(a) = amount {
                        let currency_unit = f[2].to_string();
                        let sum = sums.entry(currency_unit.clone()).or_default();
                        let Some(next_sum) = sum.checked_add(a) else {
                            errs.push(location(ln, "currency transaction sum overflow"));
                            i += 1;
                            continue;
                        };
                        *sum = next_sum;
                        let Some(next_magnitude) = magnitude.checked_add(a.abs()) else {
                            errs.push(location(ln, "transaction movement overflow"));
                            i += 1;
                            continue;
                        };
                        let Some(next_ledger_magnitude) = ledger_magnitude.checked_add(a.abs())
                        else {
                            errs.push(location(ln, "ledger movement overflow"));
                            i += 1;
                            continue;
                        };
                        magnitude = next_magnitude;
                        ledger_magnitude = next_ledger_magnitude;
                        let total = totals
                            .entry((f[0].into(), currency_unit.clone()))
                            .or_default();
                        let Some(next_total) = total.checked_add(a) else {
                            errs.push(location(ln, "account amount overflow"));
                            i += 1;
                            continue;
                        };
                        *total = next_total;
                        if magnitude > max_movement
                            || ledger_magnitude > max_movement
                            || total.abs() > max_movement
                        {
                            errs.push(location(
                                ln,
                                "transaction or account amount exceeds the supported 1e15 bound",
                            ));
                        } else {
                            posts.push(Posting {
                                account: f[0].into(),
                                amount: a,
                                currency: currency_unit,
                            })
                        }
                    } else {
                        errs.push(location(ln, "amount must be a finite plain decimal"))
                    }
                    i += 1;
                }
                if posts.len() < 2 {
                    errs.push(location(
                        start,
                        "transaction requires at least two explicit postings",
                    ))
                }
                if sums.values().any(|sum| !sum.is_zero()) {
                    errs.push(location(
                        start,
                        "transaction postings are not balanced exactly for each unit",
                    ))
                }
                if errs.is_empty() {
                    txs.push(Transaction {
                        date,
                        payee,
                        narration,
                        postings: posts,
                    })
                }
                seen_directive = true;
                continue;
            }
            let msg = if line.trim_start().starts_with("include ") {
                "include directives are unsupported"
            } else if line.contains("price ") {
                "price directives are unsupported"
            } else {
                "unsupported directive or malformed transaction header"
            };
            errs.push(location(i + 1, msg));
            i += 1;
            continue;
        }
        errs.push(location(
            i + 1,
            "unindented transaction posting or unsupported syntax",
        ));
        i += 1;
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    if !seen_directive {
        return Err(vec![format!("unavailable: {}", location(1, "ledger contains no supported financial entries; empty/comment-only ledger is unavailable"))]);
    }
    Ok(txs)
}
pub(crate) fn valid_currency(s: &str) -> bool {
    s.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
}
fn selected_unit_balances(t: &[Transaction], currency: &str) -> BTreeMap<String, Money> {
    let mut b = BTreeMap::new();
    for tx in t {
        for p in &tx.postings {
            if p.currency == currency {
                *b.entry(p.account.clone()).or_default() += p.amount;
            }
        }
    }
    b
}
#[cfg(test)]
fn account_balances(t: &[Transaction]) -> BTreeMap<String, Money> {
    let mut b = BTreeMap::new();
    for x in t {
        for p in &x.postings {
            *b.entry(p.account.clone()).or_default() += p.amount
        }
    }
    b
}
fn subtree_total(b: &BTreeMap<String, Money>, p: &str) -> Money {
    b.iter()
        .filter(|(a, _)| *a == p || a.starts_with(&format!("{p}:")))
        .map(|(_, v)| *v)
        .fold(Money::default(), |a, v| a + v)
}
fn leaf_balances(b: &BTreeMap<String, Money>, p: &str) -> BTreeMap<String, Money> {
    let desc: BTreeSet<&str> = b
        .keys()
        .filter_map(|a| a.starts_with(&format!("{p}:")).then_some(a.as_str()))
        .collect();
    b.iter()
        .filter(|(a, v)| {
            !v.is_zero()
                && (**a == p || a.starts_with(&format!("{p}:")))
                && !desc.iter().any(|d| d.starts_with(&format!("{a}:")))
        })
        .map(|(a, v)| (a.strip_prefix(&format!("{p}:")).unwrap_or(a).into(), *v))
        .collect()
}
fn history_snapshot(
    name: String,
    balances: &BTreeMap<String, Money>,
    currency: &str,
) -> HistorySnapshot {
    let net_worth = balances
        .iter()
        .filter(|(account, _)| account.starts_with("Assets") || account.starts_with("Liabilities"))
        .map(|(_, amount)| amount)
        .fold(Money::default(), |a, v| a + *v);
    HistorySnapshot {
        name,
        currency: currency.to_string(),
        net_worth,
        retirement_401k: *balances
            .get(crate::config::get("retirement_401k_account"))
            .unwrap_or(&Money::default()),
        ira_primary: *balances
            .get(crate::config::get("ira_primary_account"))
            .unwrap_or(&Money::default()),
        ira_secondary: *balances
            .get(crate::config::get("ira_secondary_account"))
            .unwrap_or(&Money::default()),
        stocks: *balances
            .get("Assets:Investments:Stocks")
            .unwrap_or(&Money::default()),
        stocks_secondary: *balances
            .get(crate::config::get("stocks_secondary_account"))
            .unwrap_or(&Money::default()),
        hsa: subtree_total(balances, "Assets:Retirement:HSA"),
    }
}
fn next_month(month: &str) -> String {
    let year = month[..4].parse::<i32>().unwrap_or_default();
    let month_number = month[5..7].parse::<u32>().unwrap_or(1);
    if month_number == 12 {
        format!("{}-01", year + 1)
    } else {
        format!("{year:04}-{:02}", month_number + 1)
    }
}
fn history_from_transactions_for_currency(
    transactions: &[Transaction],
    currency: &str,
) -> Vec<HistorySnapshot> {
    let transactions: Vec<Transaction> = transactions
        .iter()
        .flat_map(|tx| {
            let mut copy = tx.clone();
            copy.postings.retain(|p| p.currency == currency);
            std::iter::once(copy)
        })
        .collect();
    let mut balances = BTreeMap::new();
    let mut monthly = BTreeMap::new();
    for transaction in transactions {
        for posting in &transaction.postings {
            *balances.entry(posting.account.clone()).or_default() += posting.amount;
        }
        monthly.insert(
            transaction.date[..7].to_string(),
            history_snapshot(transaction.date[..7].to_string(), &balances, currency),
        );
    }
    let Some(first) = monthly.keys().next().cloned() else {
        return Vec::new();
    };
    let last = monthly.keys().next_back().cloned().unwrap_or(first.clone());
    let mut results = Vec::new();
    let mut current = first;
    let mut previous = None;
    while current <= last {
        if let Some(value) = monthly.get(&current) {
            previous = Some(value.clone());
        }
        if let Some(value) = previous.clone() {
            results.push(value);
        }
        current = next_month(&current);
    }
    results
        .into_iter()
        .rev()
        .take(24)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
fn history_from_transactions(transactions: &[Transaction]) -> Vec<HistorySnapshot> {
    history_from_transactions_for_currency(transactions, crate::config::get("ledger_currency"))
}
fn currency_totals(transactions: &[Transaction]) -> BTreeMap<String, CurrencyTotals> {
    let units: BTreeSet<String> = transactions
        .iter()
        .flat_map(|t| t.postings.iter().map(|p| p.currency.clone()))
        .collect();
    units
        .into_iter()
        .map(|unit| {
            let balances = selected_unit_balances(transactions, &unit);
            let assets = subtree_total(&balances, "Assets");
            let liabilities = subtree_total(&balances, "Liabilities");
            (
                unit,
                CurrencyTotals {
                    total_assets: assets,
                    total_liabilities: liabilities,
                    net_worth: assets + liabilities,
                },
            )
        })
        .collect()
}
fn summary_from_transactions_for_currency(
    transactions: &[Transaction],
    selected: &str,
    revision: String,
    read_at_unix_ms: u128,
) -> LedgerSummary {
    let balances = selected_unit_balances(transactions, selected);
    let assets = subtree_total(&balances, "Assets");
    let liabilities = subtree_total(&balances, "Liabilities");
    let units = transactions
        .iter()
        .flat_map(|t| t.postings.iter().map(|p| p.currency.clone()))
        .collect::<BTreeSet<_>>();
    let all_balances = all_unit_balances(transactions);
    let mut coverage = Vec::new();
    for ((account, currency), balance) in &all_balances {
        let selected_unit = currency == selected;
        let included_reason = match account.split(':').next().unwrap_or("") {
            "Assets" => Some("included_selected_unit_asset"),
            "Liabilities" => Some("included_selected_unit_liability"),
            _ => None,
        };
        let included = selected_unit && included_reason.is_some();
        let reason = if !selected_unit {
            "unvalued_unit"
        } else if let Some(reason) = included_reason {
            reason
        } else {
            "outside_net_worth"
        };
        coverage.push(AccountCoverage {
            account: account.clone(),
            currency: currency.clone(),
            balance: *balance,
            included_in_net_worth: included,
            reason: reason.into(),
        });
    }
    let summary = LedgerSummary {
        status: "validated".into(),
        diagnostics: vec![],
        currency: selected.into(),
        selected_currency: selected.into(),
        excluded_currencies: units
            .iter()
            .filter(|u| u.as_str() != selected)
            .cloned()
            .collect(),
        unvalued_units: units
            .iter()
            .filter(|u| u.as_str() != selected)
            .cloned()
            .collect(),
        totals_by_currency: currency_totals(transactions),
        net_worth: assets + liabilities,
        total_assets: assets,
        total_liabilities: liabilities,
        bank_accounts: leaf_balances(&balances, "Assets:Bank"),
        retirement: leaf_balances(&balances, "Assets:Retirement"),
        investments: leaf_balances(&balances, "Assets:Investments"),
        credit_cards: leaf_balances(&balances, "Liabilities:CreditCard"),
        housing: subtree_total(&balances, "Assets:Housing"),
        cash: subtree_total(&balances, "Assets:Cash"),
        ledger_revision: revision,
        read_at_unix_ms,
        latest_transaction_date: transactions
            .iter()
            .map(|tx| tx.date.as_str())
            .max()
            .unwrap_or("")
            .to_string(),
        valuation_basis: "native_units_no_conversion".into(),
        validation_status: "validated".into(),
        transaction_count: transactions.len(),
        posting_count: transactions.iter().map(|tx| tx.postings.len()).sum(),
        account_count: all_balances
            .keys()
            .map(|(account, _)| account)
            .collect::<BTreeSet<_>>()
            .len(),
        selected_included_account_count: coverage
            .iter()
            .filter(|c| c.currency == selected && c.included_in_net_worth)
            .count(),
        selected_excluded_account_count: coverage
            .iter()
            .filter(|c| c.currency == selected && !c.included_in_net_worth)
            .count(),
        coverage,
    };
    // Preserve a strict reconciliation invariant using the same direct-account coverage rows.
    let covered_assets = summary
        .coverage
        .iter()
        .filter(|c| {
            c.currency == selected
                && c.included_in_net_worth
                && c.reason == "included_selected_unit_asset"
        })
        .map(|c| c.balance)
        .fold(Money::default(), |a, b| a + b);
    let covered_liabilities = summary
        .coverage
        .iter()
        .filter(|c| {
            c.currency == selected
                && c.included_in_net_worth
                && c.reason == "included_selected_unit_liability"
        })
        .map(|c| c.balance)
        .fold(Money::default(), |a, b| a + b);
    debug_assert_eq!(covered_assets, summary.total_assets);
    debug_assert_eq!(covered_liabilities, summary.total_liabilities);
    summary
}
fn all_unit_balances(transactions: &[Transaction]) -> BTreeMap<(String, String), Money> {
    let mut balances = BTreeMap::new();
    for tx in transactions {
        for p in &tx.postings {
            *balances
                .entry((p.account.clone(), p.currency.clone()))
                .or_default() += p.amount;
        }
    }
    balances
}
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub(crate) fn validated_transactions() -> Result<Vec<Transaction>, String> {
    let path = crate::config::get("ledger_path");
    validated_transactions_from_path(path, crate::config::get("ledger_currency"))
}
fn validated_transactions_from_path(
    path: &str,
    currency: &str,
) -> Result<Vec<Transaction>, String> {
    let contents = read_ledger_path(path)?;
    parse_with_currency(&contents, currency).map_err(|e| {
        if e.first().is_some_and(|d| d.starts_with("unavailable:")) {
            let detail = e
                .iter()
                .map(|d| d.strip_prefix("unavailable: ").unwrap_or(d))
                .collect::<Vec<_>>()
                .join("; ");
            format!("unavailable: {path}: {detail}")
        } else {
            parse_error_message(path, e)
        }
    })
}
#[tauri::command]
pub fn get_ledger_summary() -> Result<LedgerSummary, String> {
    summary_from_path_with_clock(
        crate::config::get("ledger_path"),
        crate::config::get("ledger_currency"),
        unix_now_ms,
    )
}
fn unix_now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}
fn summary_from_path_with_clock(
    path: &str,
    selected: &str,
    clock: fn() -> u128,
) -> Result<LedgerSummary, String> {
    let bytes = fs::read(path)
        .map_err(|e| format!("unavailable: {path}: line 1: failed to read ledger: {e}"))?;
    let revision = sha256_hex(&bytes);
    let contents = decode_ledger(path, bytes)?;
    let transactions = parse_with_currency(&contents, selected).map_err(|e| {
        if e.first().is_some_and(|d| d.starts_with("unavailable:")) {
            let detail = e
                .iter()
                .map(|d| d.strip_prefix("unavailable: ").unwrap_or(d))
                .collect::<Vec<_>>()
                .join("; ");
            format!("unavailable: {path}: {detail}")
        } else {
            parse_error_message(path, e)
        }
    })?;
    let read_at_unix_ms = clock();
    Ok(summary_from_transactions_for_currency(
        &transactions,
        selected,
        revision,
        read_at_unix_ms,
    ))
}
#[tauri::command]
pub fn get_ledger_source_state(expected_revision: String) -> LedgerSourceState {
    let path = crate::config::get("ledger_path");
    source_state_from_path(path, expected_revision, unix_now_ms())
}
fn source_state_from_path(
    path: &str,
    expected_revision: String,
    checked_at_unix_ms: u128,
) -> LedgerSourceState {
    match fs::read(path) {
        Ok(bytes) => {
            let observed_revision = sha256_hex(&bytes);
            let state = if observed_revision == expected_revision {
                "current"
            } else {
                "changed"
            };
            LedgerSourceState {
                expected_revision,
                observed_revision: Some(observed_revision),
                state: state.into(),
                checked_at_unix_ms,
                error: None,
            }
        }
        Err(e) => LedgerSourceState {
            expected_revision,
            observed_revision: None,
            state: "unavailable".into(),
            checked_at_unix_ms,
            error: Some(format!("{path}: failed to read ledger: {e}")),
        },
    }
}
#[tauri::command]
pub fn get_ledger_history() -> Result<Vec<HistorySnapshot>, String> {
    let t = validated_transactions()?;
    Ok(history_from_transactions(&t))
}
#[tauri::command]
pub fn get_ledger_transactions(prefix: String) -> Result<Vec<LedgerTransaction>, String> {
    let mut out = validated_transactions()?
        .into_iter()
        .flat_map(|x| {
            x.postings
                .into_iter()
                .filter(|p| p.account == prefix || p.account.starts_with(&format!("{prefix}:")))
                .map(move |p| LedgerTransaction {
                    date: x.date.clone(),
                    payee: x.payee.clone(),
                    narration: x.narration.clone(),
                    account: p.account,
                    amount: p.amount,
                    currency: p.currency,
                })
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(out)
}

#[cfg(test)]
mod strict_subset_tests {
    use super::*;
    fn fixed_clock() -> u128 {
        1_800_000_000_123
    }
    fn ok(body: &str) {
        parse_with_currency(body, "USD").unwrap();
    }
    fn bad(body: &str) {
        assert!(
            parse_with_currency(body, "USD").is_err(),
            "accepted: {body}"
        )
    }
    #[test]
    fn supported_balanced_directives_and_explicit_currency() {
        let ledger="option \"title\" \"Synthetic\"\n2024-02-29 open Assets:Bank USD\n2024-02-29 * \"Shop\" \"Food\"\n  Assets:Bank -25.50 USD\n  Expenses:Food 25.50 USD\n";
        ok(ledger);
        let tx = parse_with_currency(ledger, "USD").unwrap();
        assert_eq!(tx[0].payee, "Shop");
        assert_eq!(tx[0].narration, "Food");
        assert_eq!(
            account_balances(&tx).get("Assets:Bank"),
            Some(&Money::parse("-25.5").unwrap())
        );
    }
    #[test]
    fn rejects_semantic_malformed_and_unsupported_entries() {
        for s in [
            "",
            "; comment",
            "2023-02-29 open Assets:Bank\n",
            "open Assets:Bank USD\n",
            "option \"title\" \"ok\" trailing\n",
            "option \"operating_currency\" \"USD\"\n",
            "plugin \"x\"\n",
            "include \"x\"\n",
            "2024-01-01 * \"x\" #tag\n  Assets:Bank -1\n  Expenses:Food 1\n",
            "2024-01-01 * \"x\" ^link\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n",
            "2024-01-01 * \"x\n  Assets:Bank -1\n  Expenses:Food 1\n",
            "2024-01-01 * unquoted\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n",
            "2024-02-30 * \"x\"\n  Assets:Bank -1\n  Expenses:Food 1\n",
            "2024-01-01 * \"x\"\nAssets:Bank -1\n  Expenses:Food 1\n",
            "2024-01-01 * \"x\"\n  Assets:Bad! -1\n  Expenses:Food 1\n",
            "2024-01-01 * \"x\"\n  Assets:Bank NaN\n  Expenses:Food NaN\n",
            "2024-01-01 * \"x\"\n  Assets:Bank inf\n  Expenses:Food -inf\n",
            "2024-01-01 * \"x\"\n  Assets:Bank 1e999\n  Expenses:Food -1e999\n",
            "2024-01-01 * \"x\"\n  Assets:Bank 1e2 USD\n  Expenses:Food -1e2 USD\n",
            "2024-01-01 * \"x\"\n  Assets:Bank -1\n  Expenses:Food 2\n",
            "2024-01-01 * \"x\"\n  Assets:Bank -1 USD {2 USD}\n  Expenses:Food 1 USD\n",
            "2024-01-01 * \"x\"\n  Assets:Bank 1.0 USD {2 USD}\n  Expenses:Food -1.0 USD\n",
            "2024-01-01 * \"x\"\n  Assets:Bank -1 USD\n  Expenses:Food 1 EUR\n",
            "2024-01-01 price USD 1 CAD\n",
        ] {
            bad(s)
        }
    }
    #[test]
    fn unindented_next_directive_ends_transaction_and_is_checked() {
        bad("2024-01-01 * \"x\"\n  Assets:Bank -1\n  Expenses:Food 1\n2024-01-02 nonsense\n");
    }
    #[test]
    fn nonselected_currency_is_accepted_and_balanced() {
        assert!(parse_with_currency(
            "2024-01-01 * \"x\"\n  Assets:Bank -1 EUR\n  Expenses:Food 1 EUR\n",
            "USD"
        )
        .is_ok())
    }
    #[test]
    fn history_carries_forward_empty_months() {
        let transactions = vec![
            Transaction {
                date: "2024-01-31".into(),
                payee: "a".into(),
                narration: "".into(),
                postings: vec![
                    Posting {
                        account: "Assets:Bank".into(),
                        currency: "USD".into(),
                        amount: Money::parse("10").unwrap(),
                    },
                    Posting {
                        account: "Equity:Opening".into(),
                        currency: "USD".into(),
                        amount: Money::parse("-10").unwrap(),
                    },
                ],
            },
            Transaction {
                date: "2024-03-02".into(),
                payee: "b".into(),
                narration: "".into(),
                postings: vec![
                    Posting {
                        account: "Assets:Bank".into(),
                        currency: "USD".into(),
                        amount: Money::parse("5").unwrap(),
                    },
                    Posting {
                        account: "Equity:Opening".into(),
                        currency: "USD".into(),
                        amount: Money::parse("-5").unwrap(),
                    },
                ],
            },
        ];
        let history = history_from_transactions(&transactions);
        assert_eq!(history.len(), 3);
        assert_eq!(history[1].name, "2024-01");
        assert_eq!(history[0].net_worth, Money::parse("10").unwrap());
        assert_eq!(history[1].net_worth, Money::parse("10").unwrap());
        assert_eq!(history[2].net_worth, Money::parse("15").unwrap());
    }
    #[test]
    fn fixed_tolerance_accepts_decimal_split_and_rejects_large_imbalance() {
        let valid = "2024-01-01 * \"split\"\n  Assets:Bank -0.10 USD\n  Expenses:Food 0.03 USD\n  Expenses:Food 0.07 USD\n";
        assert!(parse_with_currency(valid, "USD").is_ok());
        let bad =
            "2024-01-01 * \"large\"\n  Assets:Bank 100000000 USD\n  Expenses:Food -99999999 USD\n";
        let errors = parse_with_currency(bad, "USD").unwrap_err().join("; ");
        assert!(
            errors.contains("line 1: transaction postings are not balanced"),
            "{errors}"
        );
    }
    #[test]
    fn configured_currency_can_be_non_usd() {
        let euro = "2024-01-01 * \"euro\"\n  Assets:Bank -1 EUR\n  Expenses:Food 1 EUR\n";
        assert!(parse_with_currency(euro, "EUR").is_ok());
    }
    #[test]
    fn mixed_units_balance_independently_and_summary_keeps_zero_net_holdings() {
        let ledger = "2024-01-01 * \"units\"\n  Assets:Bank 10 USD\n  Equity:Opening -10 USD\n  Assets:Bank 10 EUR\n  Equity:Opening -10 EUR\n";
        let tx = parse_with_currency(ledger, "EUR").unwrap();
        assert_eq!(
            selected_unit_balances(&tx, "EUR").get("Assets:Bank"),
            Some(&Money::parse("10").unwrap())
        );
        let totals = currency_totals(&tx);
        assert_eq!(totals["USD"].total_assets, Money::parse("10").unwrap());
        assert_eq!(totals["EUR"].total_assets, Money::parse("10").unwrap());
        assert_eq!(
            history_from_transactions_for_currency(&tx, "EUR")[0].currency,
            "EUR"
        );
        assert!(parse_with_currency(
            "2024-01-01 * \"bad\"\n  Assets:Bank -10 USD\n  Expenses:Food 10 EUR\n",
            "USD"
        )
        .is_err());
    }
    #[test]
    fn production_summary_preserves_exact_selected_unit_and_serializes_money_as_strings() {
        let ledger = "2024-01-01 * \"units\"\n  Assets:Bank 1 EUR\n  Equity:Opening -1 EUR\n  Assets:Investments:VTI 9 VTI\n  Equity:Opening -9 VTI\n  Assets:Bank 1 USD\n  Equity:Opening -1 USD\n2024-01-02 * \"fractional\"\n  Assets:Bank 0.0001 USD\n  Equity:Opening -0.0001 USD\n2024-01-03 * \"refund\"\n  Assets:Bank -2 USD\n  Liabilities:CreditCard 2 USD\n";
        let tx = parse_with_currency(ledger, "EUR").unwrap();
        let summary = summary_from_transactions_for_currency(&tx, "EUR", "test-revision".into(), 1);
        assert_eq!(summary.total_assets.to_string(), "1");
        assert_eq!(summary.currency, "EUR");
        assert_eq!(
            summary.totals_by_currency["USD"].total_assets.to_string(),
            "-0.9999"
        );
        assert_eq!(
            summary.totals_by_currency["VTI"].total_assets.to_string(),
            "9"
        );
        assert!(summary.unvalued_units.contains(&"VTI".to_string()));
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["total_assets"], "1");
        assert_eq!(
            serde_json::to_value(history_from_transactions_for_currency(&tx, "EUR")[0].clone())
                .unwrap()["Net Worth"],
            "1"
        );
        assert_eq!(
            summary.totals_by_currency["USD"].net_worth.to_string(),
            "1.0001"
        );
    }
    #[test]
    fn production_path_summary_hashes_raw_bytes_and_reconciles_direct_account_coverage() {
        let path =
            std::env::temp_dir().join(format!("navis-recon-s003-{}-summary", std::process::id()));
        let source = b"2024-03-02 * \"later\"\n  Assets:Bank:Checking 10 USD\n  Equity:Opening -10 USD\n2024-01-01 * \"earlier\"\n  Assets:Bank 2 USD\n  Assets:Bank:Checking -2 USD\n  Assets:Bank:Zero 1 USD\n  Assets:Bank:Zero -1 USD\n  Liabilities:Card 1 USD\n  Equity:Opening -1 USD\n2024-01-03 * \"eur\"\n  Assets:Bank 3 EUR\n  Equity:Opening -3 EUR\n2024-01-04 * \"stock\"\n  Assets:Investments:VTI 4 VTI\n  Equity:Opening -4 VTI\n2024-01-05 * \"negative asset\"\n  Assets:Cash:Overdraft -4 USD\n  Equity:Opening 4 USD\n";
        fs::write(&path, source).unwrap();
        let name = path.to_string_lossy();
        let summary = summary_from_path_with_clock(&name, "USD", fixed_clock).unwrap();
        assert_eq!(summary.ledger_revision, sha256_hex(source));
        assert_eq!(
            sha256_hex(b"hello world"),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        assert_eq!(summary.read_at_unix_ms, fixed_clock());
        assert_eq!(summary.latest_transaction_date, "2024-03-02");
        assert_eq!(summary.valuation_basis, "native_units_no_conversion");
        assert_eq!((summary.transaction_count, summary.posting_count), (5, 14));
        let assets = summary
            .coverage
            .iter()
            .filter(|c| c.currency == "USD" && c.reason == "included_selected_unit_asset")
            .map(|c| c.balance)
            .fold(Money::default(), |a, b| a + b);
        let liabilities = summary
            .coverage
            .iter()
            .filter(|c| c.currency == "USD" && c.reason == "included_selected_unit_liability")
            .map(|c| c.balance)
            .fold(Money::default(), |a, b| a + b);
        assert_eq!(assets, summary.total_assets);
        assert_eq!(liabilities, summary.total_liabilities);
        assert_eq!(summary.net_worth, assets + liabilities);
        assert!(summary
            .coverage
            .iter()
            .any(|c| c.account == "Assets:Bank:Zero" && c.balance.is_zero()));
        assert!(summary
            .coverage
            .iter()
            .any(|c| c.account == "Assets:Cash:Overdraft"
                && c.balance.to_string() == "-4"
                && c.reason == "included_selected_unit_asset"));
        assert_eq!(summary.selected_included_account_count, 5);
        assert_eq!(summary.selected_excluded_account_count, 1);
        assert!(summary
            .coverage
            .iter()
            .any(|c| c.account == "Assets:Bank:Checking"));
        assert!(summary
            .coverage
            .iter()
            .any(|c| c.account == "Equity:Opening" && c.reason == "outside_net_worth"));
        assert!(summary
            .coverage
            .iter()
            .any(|c| c.currency == "EUR" && c.reason == "unvalued_unit"));
        assert!(summary
            .coverage
            .iter()
            .any(|c| c.currency == "VTI" && c.reason == "unvalued_unit"));
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["total_assets"], summary.total_assets.to_string());
        assert!(json["coverage"][0]["balance"].as_str().is_some());
        assert_eq!(
            summary_from_path_with_clock(&name, "USD", fixed_clock)
                .unwrap()
                .ledger_revision,
            summary.ledger_revision
        );
        fs::write(&path, [source.as_slice(), b"; changed bytes\n"].concat()).unwrap();
        assert_ne!(
            summary_from_path_with_clock(&name, "USD", fixed_clock)
                .unwrap()
                .ledger_revision,
            summary.ledger_revision
        );
        let _ = fs::remove_file(path);
    }
    #[test]
    fn production_path_summary_rejects_partial_invalid_utf8_empty_and_unreadable_sources() {
        let path =
            std::env::temp_dir().join(format!("navis-recon-s003-{}-failure", std::process::id()));
        fs::write(&path, "2024-01-01 * \"valid\"\n  Assets:Cash 1 USD\n  Equity:Opening -1 USD\ninclude \"missing.bean\"\n").unwrap();
        let name = path.to_string_lossy();
        assert!(summary_from_path_with_clock(&name, "USD", fixed_clock)
            .unwrap_err()
            .starts_with(&format!("partial: {name}: line 4:")));
        fs::write(&path, [0xff]).unwrap();
        assert!(summary_from_path_with_clock(&name, "USD", fixed_clock)
            .unwrap_err()
            .starts_with("unavailable:"));
        fs::write(&path, " ; empty\n").unwrap();
        assert!(summary_from_path_with_clock(&name, "USD", fixed_clock)
            .unwrap_err()
            .starts_with("unavailable:"));
        let missing = format!("{name}-missing");
        assert!(summary_from_path_with_clock(&missing, "USD", fixed_clock)
            .unwrap_err()
            .starts_with("unavailable:"));
        let _ = fs::remove_file(path);
    }
    #[test]
    fn freshness_state_contract_preserves_expected_revision_and_distinguishes_outcomes() {
        let path =
            std::env::temp_dir().join(format!("navis-recon-s003-{}-probe", std::process::id()));
        fs::write(&path, b"content").unwrap();
        let name = path.to_string_lossy();
        let revision = sha256_hex(b"content");
        let current = source_state_from_path(&name, revision.clone(), fixed_clock());
        assert_eq!(
            (current.state.as_str(), current.expected_revision.as_str()),
            ("current", revision.as_str())
        );
        assert_eq!(
            current.observed_revision.as_deref(),
            Some(revision.as_str())
        );
        let changed = source_state_from_path(&name, "old".into(), fixed_clock());
        assert_eq!(changed.state, "changed");
        assert_eq!(changed.expected_revision, "old");
        let missing = format!("{name}-missing");
        let unavailable = source_state_from_path(&missing, "keep".into(), fixed_clock());
        assert_eq!(unavailable.expected_revision, "keep");
        assert_eq!(unavailable.state, "unavailable");
        assert!(unavailable.observed_revision.is_none() && unavailable.error.is_some());
        let _ = fs::remove_file(path);
    }
    #[test]
    fn numerically_cancelling_mixed_units_are_rejected_with_source_line() {
        let err = parse_with_currency(
            "2024-01-01 * \"bad\"\n  Assets:Bank 10 USD\n  Expenses:Food -10 EUR\n",
            "USD",
        )
        .unwrap_err()
        .join("; ");
        assert!(
            err.contains("line 1: transaction postings are not balanced exactly for each unit"),
            "{err}"
        );
    }
    #[test]
    fn validates_file_status_contract_without_real_ledger_paths() {
        let path = std::env::temp_dir().join(format!("navis-recon-s001-{}", std::process::id()));
        let name = path.to_string_lossy().to_string();
        fs::write(
            &path,
            "2024-01-01 * \"ok\"\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n",
        )
        .unwrap();
        assert_eq!(
            validated_transactions_from_path(&name, "USD")
                .unwrap()
                .len(),
            1
        );
        fs::write(&path, [0xff]).unwrap();
        let utf8 = validated_transactions_from_path(&name, "USD").unwrap_err();
        assert!(
            utf8.starts_with(&format!("unavailable: {name}: line 1:")),
            "{utf8}"
        );
        fs::write(&path, " ; empty\n").unwrap();
        let empty = validated_transactions_from_path(&name, "USD").unwrap_err();
        assert!(
            empty.starts_with(&format!("unavailable: {name}: line 1:")),
            "{empty}"
        );
        fs::write(
            &path,
            "2024-01-01 * \"bad\"\n  Assets:Bank -1 USD\n  Expenses:Food 2 USD\n",
        )
        .unwrap();
        let semantic = validated_transactions_from_path(&name, "USD").unwrap_err();
        assert!(
            semantic.starts_with(&format!("partial: {name}: line 1:")),
            "{semantic}"
        );
        let _ = fs::remove_file(&path);
        let missing = format!("{name}-missing");
        let unavailable = validated_transactions_from_path(&missing, "USD").unwrap_err();
        assert!(
            unavailable.starts_with(&format!("unavailable: {missing}: line 1:")),
            "{unavailable}"
        );
    }
    #[test]
    fn supported_subset_rejects_inference_commodity_price_plugin_and_bad_separators() {
        for (source, expected) in [
            (
                "2024-01-01 * \"x\"\n  Assets:Bank -1 USD\n  Expenses:Food USD\n",
                "posting requires",
            ),
            ("2024-01-01 commodity USD\n", "unsupported directive"),
            ("2024-01-01 price USD 1 CAD\n", "price directives"),
            ("plugin \"x\"\n", "unsupported directive"),
            (
                "2024-01-01 *\"x\"\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n",
                "malformed transaction",
            ),
            ("option \"title\"\"x\"\n", "malformed option"),
            (
                "2024-01-01 * \"x\\\"y\"\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n",
                "escaped quotes are unsupported",
            ),
            ("2024-01-01 open Assets USD\n", "invalid open"),
            ("2024-01-01 open Assets:bad_name USD\n", "invalid open"),
        ] {
            let errors = parse_with_currency(source, "USD").unwrap_err().join("; ");
            assert!(
                errors.contains(expected),
                "expected {expected:?}, got {errors:?} for {source:?}"
            );
        }
    }
    #[test]
    fn malformed_amount_date_and_account_diagnostics_are_specific() {
        for (source, expected) in [
            (
                "2024-01-01 * \"x\"\n  Assets:Bank NaN USD\n  Expenses:Food 1 USD\n",
                "line 2: amount must be a finite plain decimal",
            ),
            (
                "2024-01-01 * \"x\"\n  Assets:Bank inf USD\n  Expenses:Food 1 USD\n",
                "line 2: amount must be a finite plain decimal",
            ),
            (
                "2024-02-30 * \"x\"\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n",
                "line 1: unsupported directive or malformed transaction header",
            ),
            (
                "2024-01-01 * \"x\"\n  Assets:Bad! -1 USD\n  Expenses:Food 1 USD\n",
                "line 2: invalid or unsupported account",
            ),
            (
                "2024-01-01 * \"x\"\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n",
                "",
            ),
        ] {
            let errors = parse_with_currency(source, "USD")
                .map(|_| String::new())
                .unwrap_or_else(|e| e.join("; "));
            if expected.is_empty() {
                assert!(errors.is_empty());
            } else {
                assert!(errors.contains(expected), "{errors}");
            }
        }
        let opens = parse_with_currency("2024-01-01 open Assets:Bank USD\n2024-01-01 * \"x\"\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n", "USD").unwrap();
        assert_eq!(opens.len(), 1);
    }
    #[test]
    fn diagnostics_and_unavailable_contract_include_source_and_line() {
        let errors = parse_with_currency(
            "2024-01-01 * \"x\"\n  Assets:Bank -1 USD\n  Expenses:Food 2 USD\n",
            "USD",
        )
        .unwrap_err();
        assert!(parse_error_message("synthetic.beancount", errors)
            .contains("synthetic.beancount: line 1:"));
        assert!(decode_ledger("synthetic.beancount", vec![0xff])
            .unwrap_err()
            .contains("unavailable: synthetic.beancount"));
        let missing = std::env::temp_dir().join(format!(
            "navis-recon-missing-{}.beancount",
            std::process::id()
        ));
        let error = read_ledger_path(missing.to_str().unwrap()).unwrap_err();
        assert!(error.contains("unavailable:") && error.contains("navis-recon-missing-"));
        assert!(error.contains("failed to read ledger"));
        assert!(
            parse_with_currency("; synthetic comment\n", "USD").unwrap_err()[0]
                .starts_with("unavailable: line 1:")
        );
    }
    #[test]
    fn include_before_or_after_transactions_rejects_entire_file() {
        let tx = "2024-01-01 * \"synthetic\"\n  Assets:Bank -1 USD\n  Expenses:Food 1 USD\n";
        bad(&format!("include \"before.beancount\"\n{tx}"));
        bad(&format!("{tx}include \"after.beancount\"\n"));
    }
    #[test]
    fn rejects_cumulative_account_overflow() {
        bad("2024-01-01 * \"x\"\n  Assets:Bank 1e308 USD\n  Expenses:Food -1e308 USD\n2024-01-02 * \"x\"\n  Assets:Bank 1e308 USD\n  Expenses:Food -1e308 USD\n2024-01-03 * \"x\"\n  Assets:Bank 1e308 USD\n  Expenses:Food -1e308 USD\n")
    }
}
