use serde::Serialize;
use std::collections::BTreeMap;

use crate::ledger_parser::{parse_ledger, read_ledger, Transaction};

/// Cadence classifications for recurring obligations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceCadence {
    Weekly,
    Biweekly,
    Monthly,
    Quarterly,
    SemiAnnual,
    Annual,
}

impl RecurrenceCadence {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            RecurrenceCadence::Weekly => "weekly",
            RecurrenceCadence::Biweekly => "biweekly",
            RecurrenceCadence::Monthly => "monthly",
            RecurrenceCadence::Quarterly => "quarterly",
            RecurrenceCadence::SemiAnnual => "semi_annual",
            RecurrenceCadence::Annual => "annual",
        }
    }

    /// Projects the next expected occurrence date from the most recent transaction date.
    pub fn project_next(&self, last_date: &str) -> Option<String> {
        match self {
            RecurrenceCadence::Weekly => add_days(last_date, 7),
            RecurrenceCadence::Biweekly => add_days(last_date, 14),
            RecurrenceCadence::Monthly => add_months(last_date, 1),
            RecurrenceCadence::Quarterly => add_months(last_date, 3),
            RecurrenceCadence::SemiAnnual => add_months(last_date, 6),
            RecurrenceCadence::Annual => add_months(last_date, 12),
        }
    }
}

/// A detected recurring obligation extracted from the ledger.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecurringObligation {
    pub payee: String,
    pub account: String,
    pub cadence: RecurrenceCadence,
    pub median_amount: f64,
    pub mean_amount: f64,
    pub median_interval_days: i32,
    pub trailing_12m_count: usize,
    pub total_count: usize,
    pub last_date: String,
    pub next_projected_date: String,
}

/// Parse YYYY-MM-DD into (year, month, day).
pub fn parse_ymd(d: &str) -> Option<(i32, u32, u32)> {
    if d.len() < 10 {
        return None;
    }
    let y = d[0..4].parse().ok()?;
    let m = d[5..7].parse().ok()?;
    let day = d[8..10].parse().ok()?;
    Some((y, m, day))
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(y) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Converts a Gregorian (year, month, day) into days since Unix Epoch (1970-01-01).
pub fn to_days(y: i32, m: u32, d: u32) -> i32 {
    let mut y = y;
    let mut m = m as i32;
    if m <= 2 {
        y -= 1;
        m += 12;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + d as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Converts days since Unix Epoch (1970-01-01) back into Gregorian (year, month, day).
pub fn from_days(days: i32) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i32) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let final_y = if m <= 2 { y + 1 } else { y };
    (final_y, m, d)
}

/// Calculates the number of days from d1 to d2 (d2 - d1).
pub fn days_between(d1: &str, d2: &str) -> Option<i32> {
    let (y1, m1, day1) = parse_ymd(d1)?;
    let (y2, m2, day2) = parse_ymd(d2)?;
    Some(to_days(y2, m2, day2) - to_days(y1, m1, day1))
}

/// Adds an offset of days to a date string formatted as YYYY-MM-DD.
pub fn add_days(date_str: &str, days: i32) -> Option<String> {
    let (y, m, d) = parse_ymd(date_str)?;
    let total_days = to_days(y, m, d) + days;
    let (ny, nm, nd) = from_days(total_days);
    Some(format!("{:04}-{:02}-{:02}", ny, nm, nd))
}

/// Adds an offset of calendar months to a date string formatted as YYYY-MM-DD,
/// clamping days to the end of month if the target month has fewer days.
pub fn add_months(date_str: &str, months: i32) -> Option<String> {
    let (y, m, d) = parse_ymd(date_str)?;
    let total_m = (y * 12 + (m as i32 - 1)) + months;
    let ny = total_m.div_euclid(12);
    let nm = (total_m.rem_euclid(12) + 1) as u32;
    let max_d = days_in_month(ny, nm);
    let nd = d.min(max_d);
    Some(format!("{:04}-{:02}-{:02}", ny, nm, nd))
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Analyzes transactions and detects recurring obligations for `Expenses:...` accounts.
/// Trailing 12-month window is computed relative to `reference_date` (or the latest transaction date).
pub fn analyze_recurring_obligations(
    transactions: &[Transaction],
    reference_date: Option<&str>,
) -> Vec<RecurringObligation> {
    // Collect all chronological postings to Expenses:*
    let mut all_expense_postings: Vec<(String, String, String, f64)> = Vec::new();
    let mut latest_date_in_ledger = String::new();

    for tx in transactions {
        if tx.date > latest_date_in_ledger {
            latest_date_in_ledger = tx.date.clone();
        }
        for posting in &tx.postings {
            if posting.account.starts_with("Expenses:") {
                all_expense_postings.push((
                    tx.date.clone(),
                    tx.payee.clone(),
                    posting.account.clone(),
                    posting.amount,
                ));
            }
        }
    }

    let anchor_date = reference_date.unwrap_or(&latest_date_in_ledger);
    if anchor_date.is_empty() {
        return Vec::new();
    }

    // Trailing 12-month window: anchor_date - 365 days
    let t12_cutoff = match parse_ymd(anchor_date) {
        Some((y, m, d)) => {
            let anchor_days = to_days(y, m, d);
            let (cy, cm, cd) = from_days(anchor_days - 365);
            format!("{:04}-{:02}-{:02}", cy, cm, cd)
        }
        None => return Vec::new(),
    };

    // Group postings by (payee, account)
    let mut groups: BTreeMap<(String, String), Vec<(String, f64)>> = BTreeMap::new();
    for (d, p, a, amt) in &all_expense_postings {
        groups
            .entry((p.clone(), a.clone()))
            .or_default()
            .push((d.clone(), *amt));
    }

    let mut obligations = Vec::new();

    for ((payee, account), mut txs) in groups {
        txs.sort_by(|a, b| a.0.cmp(&b.0));
        let last_date = txs.last().unwrap().0.clone();

        // Must have at least 1 posting in the trailing 12-month window
        let t12_postings: Vec<(String, f64)> = txs
            .iter()
            .filter(|(d, _)| d.as_str() >= t12_cutoff.as_str() && d.as_str() <= anchor_date)
            .cloned()
            .collect();

        if t12_postings.is_empty() {
            continue;
        }

        let dates: Vec<String> = txs.iter().map(|(d, _)| d.clone()).collect();
        let mut diffs = Vec::new();
        for w in dates.windows(2) {
            if let Some(diff) = days_between(&w[0], &w[1]) {
                if diff > 0 {
                    diffs.push(diff);
                }
            }
        }

        if diffs.is_empty() {
            continue;
        }

        let mean_diff = diffs.iter().sum::<i32>() as f64 / diffs.len() as f64;
        let variance = diffs
            .iter()
            .map(|&d| {
                let diff_f = d as f64 - mean_diff;
                diff_f * diff_f
            })
            .sum::<f64>()
            / diffs.len() as f64;
        let cv = variance.sqrt() / mean_diff;

        diffs.sort_unstable();
        let median_interval = diffs[diffs.len() / 2];

        // Cadence classification
        let cadence = if (6..=8).contains(&median_interval) && t12_postings.len() >= 6 && cv <= 0.60
        {
            Some(RecurrenceCadence::Weekly)
        } else if (13..=16).contains(&median_interval)
            && t12_postings.len() >= 4
            && cv <= 0.50
        {
            Some(RecurrenceCadence::Biweekly)
        } else if (25..=35).contains(&median_interval)
            && t12_postings.len() >= 2
            && cv <= 0.50
        {
            Some(RecurrenceCadence::Monthly)
        } else if (75..=105).contains(&median_interval)
            && t12_postings.len() >= 2
            && cv <= 0.40
        {
            Some(RecurrenceCadence::Quarterly)
        } else if (160..=200).contains(&median_interval)
            && t12_postings.len() >= 1
            && cv <= 0.35
        {
            Some(RecurrenceCadence::SemiAnnual)
        } else if (330..=395).contains(&median_interval)
            && txs.len() >= 2
            && (diffs.len() == 1 || cv <= 0.25)
        {
            Some(RecurrenceCadence::Annual)
        } else {
            None
        };

        if let Some(cad) = cadence {
            let mut t12_amts: Vec<f64> = t12_postings.iter().map(|(_, a)| *a).collect();
            t12_amts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median_amount = t12_amts[t12_amts.len() / 2];
            let mean_amount = t12_amts.iter().sum::<f64>() / t12_amts.len() as f64;

            if let Some(next_projected_date) = cad.project_next(&last_date) {
                obligations.push(RecurringObligation {
                    payee,
                    account,
                    cadence: cad,
                    median_amount: round2(median_amount),
                    mean_amount: round2(mean_amount),
                    median_interval_days: median_interval,
                    trailing_12m_count: t12_postings.len(),
                    total_count: txs.len(),
                    last_date,
                    next_projected_date,
                });
            }
        }
    }

    // Sort obligations by next projected date ascending, then payee ascending
    obligations.sort_by(|a, b| {
        a.next_projected_date
            .cmp(&b.next_projected_date)
            .then_with(|| a.payee.cmp(&b.payee))
    });

    obligations
}

/// Tauri command to retrieve recurring obligations from the ledger.
#[tauri::command]
pub fn get_recurring_obligations() -> Result<Vec<RecurringObligation>, String> {
    let ledger_content = read_ledger()?;
    let transactions = parse_ledger(&ledger_content);
    Ok(analyze_recurring_obligations(&transactions, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger_parser::Posting;

    #[test]
    fn test_cadence_str() {
        assert_eq!(RecurrenceCadence::Weekly.as_str(), "weekly");
        assert_eq!(RecurrenceCadence::Biweekly.as_str(), "biweekly");
        assert_eq!(RecurrenceCadence::Monthly.as_str(), "monthly");
        assert_eq!(RecurrenceCadence::Quarterly.as_str(), "quarterly");
        assert_eq!(RecurrenceCadence::SemiAnnual.as_str(), "semi_annual");
        assert_eq!(RecurrenceCadence::Annual.as_str(), "annual");
    }

    #[test]
    fn test_date_utilities() {
        assert_eq!(parse_ymd("2026-04-15"), Some((2026, 4, 15)));
        assert_eq!(days_between("2026-04-01", "2026-05-01"), Some(30));
        assert_eq!(days_between("2025-05-15", "2026-05-15"), Some(365));
        assert_eq!(add_days("2026-04-15", 7), Some("2026-04-22".to_string()));
        assert_eq!(add_days("2026-04-15", 14), Some("2026-04-29".to_string()));
        assert_eq!(add_months("2026-01-31", 1), Some("2026-02-28".to_string()));
        assert_eq!(add_months("2026-04-15", 1), Some("2026-05-15".to_string()));
        assert_eq!(add_months("2026-04-15", 3), Some("2026-07-15".to_string()));
        assert_eq!(add_months("2026-04-15", 12), Some("2027-04-15".to_string()));
    }

    #[test]
    fn test_detect_monthly_subscription() {
        let txs = vec![
            Transaction {
                date: "2026-01-15".to_string(),
                payee: "Netflix".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    amount: 15.49,
                }],
            },
            Transaction {
                date: "2026-02-15".to_string(),
                payee: "Netflix".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    amount: 15.49,
                }],
            },
            Transaction {
                date: "2026-03-15".to_string(),
                payee: "Netflix".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    amount: 15.49,
                }],
            },
            Transaction {
                date: "2026-04-15".to_string(),
                payee: "Netflix".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    amount: 15.49,
                }],
            },
        ];

        let obligations = analyze_recurring_obligations(&txs, Some("2026-04-30"));
        assert_eq!(obligations.len(), 1);
        let nflx = &obligations[0];
        assert_eq!(nflx.payee, "Netflix");
        assert_eq!(nflx.account, "Expenses:Tech:Subscriptions");
        assert_eq!(nflx.cadence, RecurrenceCadence::Monthly);
        assert_eq!(nflx.median_amount, 15.49);
        assert_eq!(nflx.mean_amount, 15.49);
        assert_eq!(nflx.trailing_12m_count, 4);
        assert_eq!(nflx.last_date, "2026-04-15");
        assert_eq!(nflx.next_projected_date, "2026-05-15");
    }

    #[test]
    fn test_detect_annual_subscription() {
        let txs = vec![
            Transaction {
                date: "2024-03-10".to_string(),
                payee: "Costco Renewal".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    amount: 120.00,
                }],
            },
            Transaction {
                date: "2025-03-10".to_string(),
                payee: "Costco Renewal".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    amount: 120.00,
                }],
            },
            Transaction {
                date: "2026-03-10".to_string(),
                payee: "Costco Renewal".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    amount: 130.00,
                }],
            },
        ];

        let obligations = analyze_recurring_obligations(&txs, Some("2026-04-01"));
        assert_eq!(obligations.len(), 1);
        let costco = &obligations[0];
        assert_eq!(costco.payee, "Costco Renewal");
        assert_eq!(costco.cadence, RecurrenceCadence::Annual);
        assert_eq!(costco.median_amount, 130.00);
        assert_eq!(costco.total_count, 3);
        assert_eq!(costco.trailing_12m_count, 1);
        assert_eq!(costco.last_date, "2026-03-10");
        assert_eq!(costco.next_projected_date, "2027-03-10");
    }

    #[test]
    fn test_detect_biweekly_payroll_obligation() {
        let txs = vec![
            Transaction {
                date: "2026-01-02".to_string(),
                payee: "Benefits Provider".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Health:Medical".to_string(),
                    amount: 100.0,
                }],
            },
            Transaction {
                date: "2026-01-16".to_string(),
                payee: "Benefits Provider".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Health:Medical".to_string(),
                    amount: 100.0,
                }],
            },
            Transaction {
                date: "2026-01-30".to_string(),
                payee: "Benefits Provider".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Health:Medical".to_string(),
                    amount: 100.0,
                }],
            },
            Transaction {
                date: "2026-02-13".to_string(),
                payee: "Benefits Provider".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Health:Medical".to_string(),
                    amount: 100.0,
                }],
            },
        ];

        let obligations = analyze_recurring_obligations(&txs, Some("2026-02-28"));
        assert_eq!(obligations.len(), 1);
        let b = &obligations[0];
        assert_eq!(b.cadence, RecurrenceCadence::Biweekly);
        assert_eq!(b.last_date, "2026-02-13");
        assert_eq!(b.next_projected_date, "2026-02-27");
    }

    #[test]
    fn test_rejects_irregular_retail_purchases() {
        // Amazon purchases on random days with huge interval variance
        let txs = vec![
            Transaction {
                date: "2026-01-03".to_string(),
                payee: "Amazon".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    amount: 25.0,
                }],
            },
            Transaction {
                date: "2026-01-05".to_string(),
                payee: "Amazon".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    amount: 40.0,
                }],
            },
            Transaction {
                date: "2026-02-20".to_string(),
                payee: "Amazon".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    amount: 12.0,
                }],
            },
            Transaction {
                date: "2026-04-10".to_string(),
                payee: "Amazon".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    amount: 15.0,
                }],
            },
        ];

        let obligations = analyze_recurring_obligations(&txs, Some("2026-04-30"));
        assert!(obligations.is_empty());
    }

    #[test]
    fn test_live_ledger_recurring_obligations() {
        // If ledger is present on Google Drive, verify live detection
        if let Ok(obligations) = get_recurring_obligations() {
            assert!(!obligations.is_empty(), "Expected recurring obligations to be detected from live ledger");
            for o in &obligations {
                assert!(!o.payee.is_empty());
                assert!(o.account.starts_with("Expenses:"));
                assert!(o.median_amount > 0.0);
                assert!(!o.next_projected_date.is_empty());
            }
        }
    }
}
