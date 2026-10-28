//! Advisory, revision-bound duplicate candidates. Never decides or writes money.
use crate::{ledger_parser, money::Money};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DuplicateMatch {
    pub id: String,
    pub date: String,
    pub payee: String,
    pub narration: String,
    pub amount: String,
    pub currency: String,
    pub funding_accounts: Vec<String>,
    pub days_apart: i64,
    pub merchant_overlap: bool,
}

fn day_number(date: &str) -> Result<i64, String> {
    let parts = date
        .split('-')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Invalid duplicate-review date")?;
    if parts.len() != 3 || !(1..=12).contains(&parts[1]) || !(1..=31).contains(&parts[2]) {
        return Err("Invalid duplicate-review date".into());
    }
    let (year, month, day) = (parts[0], parts[1], parts[2]);
    let previous = year - 1;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let months = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    Ok(previous * 365 + previous / 4 - previous / 100
        + previous / 400
        + months[..(month - 1) as usize].iter().sum::<i64>()
        + day)
}

fn merchant_words(value: &str) -> BTreeSet<String> {
    value
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|s| {
            s.len() >= 3
                && ![
                    "payment",
                    "payments",
                    "receipt",
                    "statement",
                    "health",
                    "food",
                    "service",
                    "services",
                    "account",
                    "the",
                ]
                .contains(&s.as_str())
        })
        .collect()
}

pub(crate) fn candidates(
    bytes: &[u8],
    item: &Value,
    currency: &str,
) -> Result<Vec<DuplicateMatch>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Ledger is not valid UTF-8")?;
    let transactions = ledger_parser::parse_with_currency(text, currency)
        .map_err(|_| "Duplicate review needs a validated ledger")?;
    let amount = Money::parse(
        item["amount"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| item["amount"].as_number().map(ToString::to_string))
            .as_deref()
            .ok_or("Missing duplicate-review amount")?,
    )?;
    if amount.is_zero() {
        return Ok(vec![]);
    }
    let day = day_number(
        item["date"]
            .as_str()
            .ok_or("Missing duplicate-review date")?,
    )?;
    let root = if item["category"]
        .as_str()
        .is_some_and(|v| v.starts_with("Income:"))
    {
        "Income:"
    } else {
        "Expenses:"
    };
    let words = merchant_words(item["rule"].as_str().unwrap_or(""));
    let revision = Sha256::digest(bytes);
    let mut found = Vec::new();
    for (index, tx) in transactions.into_iter().enumerate() {
        let days_apart = (day_number(&tx.date)? - day).abs();
        if days_apart > 7 {
            continue;
        }
        let mut total = Money::default();
        for posting in tx
            .postings
            .iter()
            .filter(|p| p.currency == currency && p.account.starts_with(root))
        {
            total = total
                .checked_add(posting.amount)
                .ok_or("Duplicate-review amount exceeds supported bounds")?;
        }
        // Signed root totals cover split expenses; transfers, card repayments,
        // other currencies and opposite-sign refunds are not purchase matches.
        if total != amount {
            continue;
        }
        let funding_accounts = tx
            .postings
            .iter()
            .filter(|p| {
                p.currency == currency
                    && (p.account.starts_with("Assets:") || p.account.starts_with("Liabilities:"))
            })
            .map(|p| p.account.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let merchant_overlap =
            !words.is_disjoint(&merchant_words(&format!("{} {}", tx.payee, tx.narration)));
        let id = Sha256::digest([revision.as_slice(), index.to_string().as_bytes()].concat())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        found.push(DuplicateMatch {
            id,
            date: tx.date,
            payee: tx.payee.chars().take(1000).collect(),
            narration: tx.narration.chars().take(1000).collect(),
            amount: amount.to_string(),
            currency: currency.into(),
            funding_accounts,
            days_apart,
            merchant_overlap,
        });
        if found.len() > 100 {
            return Err("Too many possible duplicates; review this payment outside Sync".into());
        }
    }
    found.sort_by_key(|m| (m.days_apart, !m.merchant_overlap, m.id.clone()));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_signed_units_split_expenses_and_calendar_window_are_required() {
        let ledger=concat!(
            "2025-12-31 * \"Example Pharmacy\"\n  Expenses:Medical 10 USD\n  Expenses:Fees 2.34 USD\n  Assets:HSA -12.34 USD\n",
            "2026-01-01 * \"Refund\"\n  Expenses:Medical -12.34 USD\n  Assets:HSA 12.34 USD\n",
            "2026-01-01 * \"Other unit\"\n  Expenses:Medical 12.34 TWD\n  Assets:HSA -12.34 TWD\n",
            "2026-01-01 * \"Transfer\"\n  Assets:Checking 12.34 USD\n  Liabilities:Card -12.34 USD\n",
            "2025-12-23 * \"Too old\"\n  Expenses:Medical 12.34 USD\n  Assets:HSA -12.34 USD\n",
        );
        let item = json!({"date":"2026-01-01","amount":"12.340000000000000000","category":"Expenses:Medical","rule":"Example Pharmacy"});
        let found = candidates(ledger.as_bytes(), &item, "USD").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].date, "2025-12-31");
        assert_eq!(found[0].days_apart, 1);
        assert!(found[0].merchant_overlap);
        assert_eq!(found[0].funding_accounts, vec!["Assets:HSA"]);
        let changed = candidates(
            format!("{ledger}; revision changed\n").as_bytes(),
            &item,
            "USD",
        )
        .unwrap();
        assert_ne!(changed[0].id, found[0].id);
        assert_eq!(
            day_number("2024-03-01").unwrap() - day_number("2024-02-28").unwrap(),
            2
        );
    }
    #[test]
    fn merchant_or_funding_difference_is_disclosed_not_a_reason_to_hide_amount_match() {
        let ledger="2026-01-01 * \"Other merchant\"\n  Expenses:Other 12.34 USD\n  Assets:Legacy:Unsorted -12.34 USD\n";
        let item = json!({"date":"2026-01-08","amount":"12.34","category":"Expenses:Medical","rule":"Example Pharmacy"});
        let found = candidates(ledger.as_bytes(), &item, "USD").unwrap();
        assert_eq!(found.len(), 1);
        assert!(!found[0].merchant_overlap);
        assert!(found[0]
            .funding_accounts
            .contains(&"Assets:Legacy:Unsorted".into()));
        let mut later = item.clone();
        later["date"] = json!("2026-01-09");
        assert!(candidates(ledger.as_bytes(), &later, "USD")
            .unwrap()
            .is_empty());
    }
}
