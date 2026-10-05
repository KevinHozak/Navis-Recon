use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

pub const LEDGER_PATH: &str = r"G:\My Drive\Navis-Vault\Ledger\Sovereign.beancount";

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
    #[serde(rename = "401k (K)")]
    pub retirement_401k_k: f64,
    #[serde(rename = "IRA (J)")]
    pub ira_j: f64,
    #[serde(rename = "IRA (K)")]
    pub ira_k: f64,
    #[serde(rename = "Stocks")]
    pub stocks: f64,
    #[serde(rename = "J-Stocks")]
    pub j_stocks: f64,
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

pub(crate) fn read_ledger() -> Result<String, String> {
    let bytes = fs::read(LEDGER_PATH).map_err(|error| format!("failed to read ledger: {error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("ledger is not valid UTF-8: {error}"))
}

fn is_date_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() >= 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

fn parse_header(line: &str) -> Option<(String, String, String)> {
    if !is_date_line(line) {
        return None;
    }
    let date = line[..10].to_string();
    let remainder = line[10..].trim_start();
    let marker = remainder.chars().next()?;
    if marker != '*' && marker != '!' {
        return None;
    }
    let description = remainder[marker.len_utf8()..].trim();
    let mut quoted = description.split('"');
    let first = quoted.next()?.trim();
    let quoted_text = quoted.next();
    let payee = quoted_text.unwrap_or(first).to_string();
    let narration = quoted
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches('"')
        .to_string();
    Some((date, payee, narration))
}

fn parse_posting(line: &str) -> Option<Posting> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 2 || !fields[0].contains(':') {
        return None;
    }
    let amount = fields[1].parse::<f64>().ok()?;
    Some(Posting {
        account: fields[0].to_string(),
        amount,
    })
}

pub(crate) fn parse_ledger(contents: &str) -> Vec<Transaction> {
    let lines: Vec<&str> = contents.lines().collect();
    let mut transactions = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let Some((date, payee, narration)) = parse_header(lines[index]) else {
            index += 1;
            continue;
        };
        let mut postings = Vec::new();
        index += 1;
        while index < lines.len() && !is_date_line(lines[index]) {
            if let Some(posting) = parse_posting(lines[index]) {
                postings.push(posting);
            }
            index += 1;
        }
        if !postings.is_empty() {
            transactions.push(Transaction {
                date,
                payee,
                narration,
                postings,
            });
        }
    }

    transactions
}

fn account_balances(transactions: &[Transaction]) -> BTreeMap<String, f64> {
    let mut balances = BTreeMap::new();
    for transaction in transactions {
        for posting in &transaction.postings {
            *balances.entry(posting.account.clone()).or_insert(0.0) += posting.amount;
        }
    }
    balances
}

fn subtree_total(balances: &BTreeMap<String, f64>, prefix: &str) -> f64 {
    balances
        .iter()
        .filter(|(account, _)| *account == prefix || account.starts_with(&format!("{prefix}:")))
        .map(|(_, amount)| amount)
        .sum()
}

fn leaf_balances(balances: &BTreeMap<String, f64>, prefix: &str) -> BTreeMap<String, f64> {
    let descendants: BTreeSet<&str> = balances
        .keys()
        .filter_map(|account| {
            if account.starts_with(&format!("{prefix}:")) {
                Some(account.as_str())
            } else {
                None
            }
        })
        .collect();
    balances
        .iter()
        .filter(|(account, amount)| {
            (**amount != 0.0)
                && (*account == prefix || account.starts_with(&format!("{prefix}:")))
                && !descendants
                    .iter()
                    .any(|descendant| descendant.starts_with(&format!("{account}:")))
        })
        .map(|(account, amount)| {
            let name = account
                .strip_prefix(&format!("{prefix}:"))
                .unwrap_or(account)
                .to_string();
            (name, round(*amount))
        })
        .collect()
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn snapshot(name: String, balances: &BTreeMap<String, f64>) -> HistorySnapshot {
    let net_worth = balances
        .iter()
        .filter(|(account, _)| account.starts_with("Assets") || account.starts_with("Liabilities"))
        .map(|(_, amount)| amount)
        .sum();
    HistorySnapshot {
        name,
        net_worth: round(net_worth),
        retirement_401k_k: round(*balances.get("Assets:Retirement:401k:K").unwrap_or(&0.0)),
        ira_j: round(*balances.get("Assets:Retirement:IRA:J").unwrap_or(&0.0)),
        ira_k: round(*balances.get("Assets:Retirement:IRA:K").unwrap_or(&0.0)),
        stocks: round(*balances.get("Assets:Investments:Stocks").unwrap_or(&0.0)),
        j_stocks: round(*balances.get("Assets:Investments:JStocks").unwrap_or(&0.0)),
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

#[tauri::command]
pub fn get_ledger_summary() -> Result<LedgerSummary, String> {
    let transactions = parse_ledger(&read_ledger()?);
    let balances = account_balances(&transactions);
    let total_assets = subtree_total(&balances, "Assets");
    let total_liabilities = subtree_total(&balances, "Liabilities");
    Ok(LedgerSummary {
        net_worth: round(total_assets + total_liabilities),
        total_assets: round(total_assets),
        total_liabilities: round(total_liabilities),
        bank_accounts: leaf_balances(&balances, "Assets:Bank"),
        retirement: leaf_balances(&balances, "Assets:Retirement"),
        investments: leaf_balances(&balances, "Assets:Investments"),
        credit_cards: leaf_balances(&balances, "Liabilities:CreditCard"),
        housing: round(subtree_total(&balances, "Assets:Housing")),
        cash: round(subtree_total(&balances, "Assets:Cash")),
    })
}

#[tauri::command]
pub fn get_ledger_history() -> Result<Vec<HistorySnapshot>, String> {
    let transactions = parse_ledger(&read_ledger()?);
    let mut balances = BTreeMap::new();
    let mut monthly = BTreeMap::new();
    for transaction in transactions {
        for posting in transaction.postings {
            *balances.entry(posting.account).or_insert(0.0) += posting.amount;
        }
        monthly.insert(
            transaction.date[..7].to_string(),
            snapshot(transaction.date[..7].to_string(), &balances),
        );
    }

    let Some(first) = monthly.keys().next().cloned() else {
        return Ok(Vec::new());
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
    Ok(results
        .into_iter()
        .rev()
        .take(24)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect())
}

#[tauri::command]
pub fn get_ledger_transactions(prefix: String) -> Result<Vec<LedgerTransaction>, String> {
    let transactions = parse_ledger(&read_ledger()?);
    let mut result = transactions
        .into_iter()
        .flat_map(|transaction| {
            transaction
                .postings
                .into_iter()
                .filter(|posting| posting.account.starts_with(&prefix))
                .map(move |posting| LedgerTransaction {
                    date: transaction.date.clone(),
                    payee: transaction.payee.clone(),
                    narration: transaction.narration.clone(),
                    account: posting.account,
                    amount: posting.amount,
                })
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| right.date.cmp(&left.date));
    Ok(result)
}
