use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

#[derive(Debug, Clone)]
pub(crate) struct Posting {
    pub(crate) account: String,
    pub(crate) amount: f64,
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
    pub net_worth: f64,
    pub total_assets: f64,
    pub total_liabilities: f64,
    pub bank_accounts: BTreeMap<String, f64>,
    pub retirement: BTreeMap<String, f64>,
    pub investments: BTreeMap<String, f64>,
    pub credit_cards: BTreeMap<String, f64>,
    pub housing: f64,
    pub cash: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct HistorySnapshot {
    pub name: String,
    #[serde(rename = "Net Worth")]
    pub net_worth: f64,
    #[serde(rename = "401k")]
    pub retirement_401k: f64,
    #[serde(rename = "IRA Primary")]
    pub ira_primary: f64,
    #[serde(rename = "IRA Secondary")]
    pub ira_secondary: f64,
    #[serde(rename = "Stocks")]
    pub stocks: f64,
    #[serde(rename = "Stocks Secondary")]
    pub stocks_secondary: f64,
    #[serde(rename = "HSA")]
    pub hsa: f64,
}
#[derive(Debug, Serialize)]
pub struct LedgerTransaction {
    pub date: String,
    pub payee: String,
    pub narration: String,
    pub account: String,
    pub amount: f64,
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
fn plain_decimal(s: &str) -> Option<f64> {
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
    s.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() <= 1.0e15)
}
fn parse_with_currency(contents: &str, currency: &str) -> Result<Vec<Transaction>, Vec<String>> {
    let lines: Vec<_> = contents.lines().collect();
    let mut txs = vec![];
    let mut errs = vec![];
    let mut i = 0;
    let mut seen_directive = false;
    let mut totals: BTreeMap<String, f64> = BTreeMap::new();
    let mut ledger_magnitude = 0.0;
    if currency != "USD" {
        return Err(vec![location(
            1,
            "unsupported ledger_currency; only USD is supported until currency-aware presentation is implemented",
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
                        || (fields.len() == 3 && fields[2] != currency)
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
                let mut sum = 0.0;
                let mut magnitude = 0.0;
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
                    if f.len() == 3 && f[2] != currency {
                        errs.push(location(ln, "posting currency must match ledger_currency"));
                        i += 1;
                        continue;
                    }
                    let amount = plain_decimal(f[1]);
                    if let Some(a) = amount {
                        sum += a;
                        magnitude += a.abs();
                        ledger_magnitude += a.abs();
                        let total = totals.entry(f[0].into()).or_insert(0.0);
                        *total += a;
                        if !sum.is_finite()
                            || !magnitude.is_finite()
                            || !ledger_magnitude.is_finite()
                            || !total.is_finite()
                            || magnitude > 1.0e15
                            || ledger_magnitude > 1.0e15
                            || total.abs() > 1.0e15
                        {
                            errs.push(location(
                                ln,
                                "transaction or account amount exceeds the supported 1e15 bound",
                            ));
                        } else {
                            posts.push(Posting {
                                account: f[0].into(),
                                amount: a,
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
                let tolerance = 1e-8;
                if sum.abs() > tolerance {
                    errs.push(location(
                        start,
                        "transaction postings are not balanced within the documented f64 tolerance",
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
fn account_balances(t: &[Transaction]) -> BTreeMap<String, f64> {
    let mut b = BTreeMap::new();
    for x in t {
        for p in &x.postings {
            *b.entry(p.account.clone()).or_insert(0.0) += p.amount
        }
    }
    b
}
fn subtree_total(b: &BTreeMap<String, f64>, p: &str) -> f64 {
    b.iter()
        .filter(|(a, _)| *a == p || a.starts_with(&format!("{p}:")))
        .map(|(_, v)| *v)
        .sum()
}
fn leaf_balances(b: &BTreeMap<String, f64>, p: &str) -> BTreeMap<String, f64> {
    let desc: BTreeSet<&str> = b
        .keys()
        .filter_map(|a| a.starts_with(&format!("{p}:")).then_some(a.as_str()))
        .collect();
    b.iter()
        .filter(|(a, v)| {
            **v != 0.0
                && (**a == p || a.starts_with(&format!("{p}:")))
                && !desc.iter().any(|d| d.starts_with(&format!("{a}:")))
        })
        .map(|(a, v)| {
            (
                a.strip_prefix(&format!("{p}:")).unwrap_or(a).into(),
                round(*v),
            )
        })
        .collect()
}
fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn history_snapshot(name: String, balances: &BTreeMap<String, f64>) -> HistorySnapshot {
    let net_worth = balances
        .iter()
        .filter(|(account, _)| account.starts_with("Assets") || account.starts_with("Liabilities"))
        .map(|(_, amount)| amount)
        .sum();
    HistorySnapshot {
        name,
        net_worth: round(net_worth),
        retirement_401k: round(
            *balances
                .get(crate::config::get("retirement_401k_account"))
                .unwrap_or(&0.0),
        ),
        ira_primary: round(
            *balances
                .get(crate::config::get("ira_primary_account"))
                .unwrap_or(&0.0),
        ),
        ira_secondary: round(
            *balances
                .get(crate::config::get("ira_secondary_account"))
                .unwrap_or(&0.0),
        ),
        stocks: round(*balances.get("Assets:Investments:Stocks").unwrap_or(&0.0)),
        stocks_secondary: round(
            *balances
                .get(crate::config::get("stocks_secondary_account"))
                .unwrap_or(&0.0),
        ),
        hsa: round(subtree_total(balances, "Assets:Retirement:HSA")),
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
fn history_from_transactions(transactions: &[Transaction]) -> Vec<HistorySnapshot> {
    let mut balances = BTreeMap::new();
    let mut monthly = BTreeMap::new();
    for transaction in transactions {
        for posting in &transaction.postings {
            *balances.entry(posting.account.clone()).or_insert(0.0) += posting.amount;
        }
        monthly.insert(
            transaction.date[..7].to_string(),
            history_snapshot(transaction.date[..7].to_string(), &balances),
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
    let t = validated_transactions()?;
    let b = account_balances(&t);
    let a = subtree_total(&b, "Assets");
    let l = subtree_total(&b, "Liabilities");
    Ok(LedgerSummary {
        status: "validated".into(),
        diagnostics: vec![],
        net_worth: round(a + l),
        total_assets: round(a),
        total_liabilities: round(l),
        bank_accounts: leaf_balances(&b, "Assets:Bank"),
        retirement: leaf_balances(&b, "Assets:Retirement"),
        investments: leaf_balances(&b, "Assets:Investments"),
        credit_cards: leaf_balances(&b, "Liabilities:CreditCard"),
        housing: round(subtree_total(&b, "Assets:Housing")),
        cash: round(subtree_total(&b, "Assets:Cash")),
    })
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
                })
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(out)
}

#[cfg(test)]
mod strict_subset_tests {
    use super::*;
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
        assert_eq!(account_balances(&tx).get("Assets:Bank"), Some(&-25.5));
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
            "2024-01-01 * \"x\"\n  Assets:Bank -1 CAD\n  Expenses:Food 1 CAD\n",
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
    fn mismatch_currency_rejected() {
        assert!(parse_with_currency(
            "2024-01-01 * \"x\"\n  Assets:Bank -1 EUR\n  Expenses:Food 1 EUR\n",
            "USD"
        )
        .is_err())
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
                        amount: 10.0,
                    },
                    Posting {
                        account: "Equity:Opening".into(),
                        amount: -10.0,
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
                        amount: 5.0,
                    },
                    Posting {
                        account: "Equity:Opening".into(),
                        amount: -5.0,
                    },
                ],
            },
        ];
        let history = history_from_transactions(&transactions);
        assert_eq!(history.len(), 3);
        assert_eq!(history[1].name, "2024-01");
        assert_eq!(history[0].net_worth, 10.0);
        assert_eq!(history[1].net_worth, 10.0);
        assert_eq!(history[2].net_worth, 15.0);
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
    fn configured_currency_is_usd_only() {
        let euro = "2024-01-01 * \"euro\"\n  Assets:Bank -1 EUR\n  Expenses:Food 1 EUR\n";
        let errors = parse_with_currency(euro, "EUR").unwrap_err().join("; ");
        assert!(errors.contains("only USD is supported"), "{errors}");
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
