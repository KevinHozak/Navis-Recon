use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::ledger_parser::{validated_transactions, Transaction};
use crate::money::Money;

/// Cadence classifications for recurring obligations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

/// User-configurable metadata for a subscription / obligation in the registry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionMetadata {
    pub key: String,
    pub payee: String,
    pub account: String,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default)]
    pub billing_method: Option<String>,
    #[serde(default)]
    pub cancellation_url: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub is_trial: bool,
    #[serde(default)]
    pub trial_end_date: Option<String>,
    #[serde(default)]
    pub alert_lead_days: Option<u32>,
    #[serde(default)]
    pub custom_label: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
fn default_currency() -> String {
    "USD".to_string()
}

/// Top-level payload stored in `subscriptions.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionRegistryPayload {
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionMetadata>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Price delta metrics comparing the most recent charge against the rolling median baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceDelta {
    pub currency: String,
    pub latest_amount: Money,
    pub median_amount: Money,
    pub dollar_delta: Money,
    pub percentage_delta: Option<Money>,
    pub percentage_is_estimate: bool,
    pub is_increase: bool,
}

/// High-priority alerts for subscriptions (e.g. renewals due soon, expiring trials, price hikes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenewalAlertKind {
    TrialExpiring,
    AnnualRenewalDue,
    RenewalDue,
    PriceHike,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenewalAlert {
    pub kind: RenewalAlertKind,
    pub message: String,
    pub days_until: Option<i32>,
}

/// An enriched recurring obligation combining ledger telemetry, price-hike analysis,
/// renewal alerts, and user-defined registry annotations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecurringObligation {
    pub key: String,
    pub payee: String,
    pub account: String,
    pub currency: String,
    pub cadence: RecurrenceCadence,
    pub latest_amount: Money,
    pub median_amount: Money,
    pub mean_amount: Money,
    pub median_is_estimate: bool,
    pub mean_is_estimate: bool,
    pub price_delta: Option<PriceDelta>,
    pub alerts: Vec<RenewalAlert>,
    pub median_interval_days: i32,
    pub trailing_12m_count: usize,
    pub total_count: usize,
    pub last_date: String,
    pub next_projected_date: String,
    pub metadata: Option<SubscriptionMetadata>,
}

/// Standard canonical key generator for an obligation: `"<payee>::<account>"`.
pub fn obligation_key(payee: &str, account: &str) -> String {
    format!("{}::{}", payee.trim(), account.trim())
}
pub fn obligation_currency_key(payee: &str, account: &str, currency: &str) -> String {
    format!("{}::{}::{}", payee.trim(), account.trim(), currency.trim())
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

/// Returns the current date in YYYY-MM-DD format (UTC).
pub fn current_date_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let days = (now.as_secs() / 86400) as i32;
    let (y, m, d) = from_days(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
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

fn read_utf8_sig(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read file: {error}"))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    String::from_utf8(bytes.to_vec()).map_err(|error| format!("file is not valid UTF-8: {error}"))
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
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
            "failed to replace file atomically: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    fs::rename(temp_path, path)
        .map_err(|error| format!("failed to replace file atomically: {error}"))
}

pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    if !parent.exists() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create parent directories: {error}"))?;
    }
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("temp.tmp");
    let temp_path = parent.join(format!("{file_name}.tmp"));

    fs::write(&temp_path, contents)
        .map_err(|error| format!("failed to write temp file: {error}"))?;
    if let Err(error) = replace_file(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    Ok(())
}

/// Reads the subscription registry payload from the specified file path (or default vault path).
pub fn load_subscription_registry(path: &Path) -> Result<SubscriptionRegistryPayload, String> {
    if !path.exists() {
        return Ok(SubscriptionRegistryPayload::default());
    }
    let json_text = read_utf8_sig(path)?;
    serde_json::from_str(&json_text)
        .map_err(|error| format!("failed to parse subscription registry: {error}"))
}

/// Saves the subscription registry payload atomically to the specified file path.
pub fn save_subscription_registry(
    path: &Path,
    payload: &SubscriptionRegistryPayload,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(payload)
        .map_err(|error| format!("failed to serialize subscription registry: {error}"))?;
    write_atomic(path, &bytes)
}

/// Analyzes price delta and checks for significant price increases (>= 0.50 currency units and >= 2.0%).
pub fn compute_price_delta(
    latest_amount: Money,
    median_amount: Money,
    currency: &str,
) -> Option<PriceDelta> {
    let dollar_delta = latest_amount - median_amount;
    let percentage_delta = if median_amount > Money::default() {
        Money::ratio_percent(dollar_delta, median_amount)
    } else {
        None
    };
    let is_increase = median_amount > Money::default()
        && dollar_delta >= Money::parse("0.50").ok()?
        && dollar_delta.at_least_ratio(median_amount, 2, 100);

    Some(PriceDelta {
        currency: currency.to_string(),
        latest_amount,
        median_amount,
        dollar_delta,
        percentage_delta,
        percentage_is_estimate: percentage_delta.is_some_and(|_| {
            median_amount > Money::default() && !Money::ratio_is_exact(dollar_delta, median_amount)
        }),
        is_increase,
    })
}

/// Generates renewal alerts based on cadence, projected next date, price changes, and registry metadata.
pub fn evaluate_renewal_alerts(
    cadence: RecurrenceCadence,
    anchor_date: &str,
    next_projected_date: &str,
    price_delta: Option<&PriceDelta>,
    metadata: Option<&SubscriptionMetadata>,
) -> Vec<RenewalAlert> {
    let mut alerts = Vec::new();

    // 1. Trial Expiration Alert
    if let Some(meta) = metadata {
        if meta.is_trial {
            if let Some(ref trial_end) = meta.trial_end_date {
                if let Some(days_until) = days_between(anchor_date, trial_end) {
                    let lead = meta.alert_lead_days.unwrap_or(7) as i32;
                    if days_until >= 0 && days_until <= lead {
                        alerts.push(RenewalAlert {
                            kind: RenewalAlertKind::TrialExpiring,
                            message: format!(
                                "Free trial ending in {days_until} days (on {trial_end})"
                            ),
                            days_until: Some(days_until),
                        });
                    } else if days_until < 0 {
                        alerts.push(RenewalAlert {
                            kind: RenewalAlertKind::TrialExpiring,
                            message: format!("Free trial expired {days_until} days ago"),
                            days_until: Some(days_until),
                        });
                    }
                }
            }
        }
    }

    // 2. Upcoming Renewal Alert (Annual or general configured lead days)
    if let Some(days_until) = days_between(anchor_date, next_projected_date) {
        let configured_lead = metadata
            .and_then(|m| m.alert_lead_days)
            .map(|d| d as i32)
            .unwrap_or(if cadence == RecurrenceCadence::Annual {
                30
            } else {
                7
            });

        if days_until >= 0 && days_until <= configured_lead {
            let kind = if cadence == RecurrenceCadence::Annual {
                RenewalAlertKind::AnnualRenewalDue
            } else {
                RenewalAlertKind::RenewalDue
            };
            let label = if cadence == RecurrenceCadence::Annual {
                "Annual renewal due"
            } else {
                "Renewal charge due"
            };
            alerts.push(RenewalAlert {
                kind,
                message: format!("{label} in {days_until} days ({next_projected_date})"),
                days_until: Some(days_until),
            });
        }
    }

    // 3. Price Hike Alert
    if let Some(delta) = price_delta {
        if delta.is_increase {
            alerts.push(RenewalAlert {
                kind: RenewalAlertKind::PriceHike,
                message: match delta.percentage_delta {
                    Some(percentage) => format!(
                        "Price increased by {} {} (+{}%) above median baseline",
                        delta.dollar_delta, delta.currency, percentage
                    ),
                    None => format!(
                        "Price increased by {} {} (percentage unavailable) above median baseline",
                        delta.dollar_delta, delta.currency
                    ),
                },
                days_until: None,
            });
        }
    }

    alerts
}

/// Analyzes transactions and detects recurring obligations for `Expenses:...` accounts.
/// Trailing 12-month window is computed relative to `reference_date` (or the latest transaction date).
#[allow(dead_code)]
pub fn analyze_recurring_obligations(
    transactions: &[Transaction],
    reference_date: Option<&str>,
) -> Vec<RecurringObligation> {
    analyze_recurring_obligations_with_registry(
        transactions,
        reference_date,
        &SubscriptionRegistryPayload::default(),
    )
}

/// Enriched analyzer that joins ledger transaction telemetry with registry metadata.
pub fn analyze_recurring_obligations_with_registry(
    transactions: &[Transaction],
    reference_date: Option<&str>,
    registry: &SubscriptionRegistryPayload,
) -> Vec<RecurringObligation> {
    // Map registry entries by canonical key
    let registry_map: BTreeMap<String, SubscriptionMetadata> = registry
        .subscriptions
        .iter()
        .map(|meta| (meta.key.clone(), meta.clone()))
        .collect();

    // Collect all chronological postings to Expenses:*
    let mut all_expense_postings: Vec<(String, String, String, String, Money)> = Vec::new();
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
                    posting.currency.clone(),
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
    let mut groups: BTreeMap<(String, String, String), Vec<(String, Money)>> = BTreeMap::new();
    for (d, p, a, c, amt) in &all_expense_postings {
        groups
            .entry((p.clone(), a.clone(), c.clone()))
            .or_default()
            .push((d.clone(), *amt));
    }

    let mut obligations = Vec::new();

    for ((payee, account, currency), mut txs) in groups {
        txs.sort_by(|a, b| a.0.cmp(&b.0));
        let last_date = txs.last().unwrap().0.clone();
        let latest_amount = txs.last().unwrap().1;

        // Must have at least 1 posting in the trailing 12-month window
        let t12_postings: Vec<(String, Money)> = txs
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
        let median_interval = if diffs.len() % 2 == 1 {
            diffs[diffs.len() / 2]
        } else {
            let mid = diffs.len() / 2;
            ((diffs[mid - 1] + diffs[mid]) as f64 / 2.0).round() as i32
        };

        // Cadence classification
        let cadence = if (6..=8).contains(&median_interval) && t12_postings.len() >= 6 && cv <= 0.60
        {
            Some(RecurrenceCadence::Weekly)
        } else if (13..=16).contains(&median_interval) && t12_postings.len() >= 4 && cv <= 0.50 {
            Some(RecurrenceCadence::Biweekly)
        } else if (25..=35).contains(&median_interval) && t12_postings.len() >= 2 && cv <= 0.50 {
            Some(RecurrenceCadence::Monthly)
        } else if (75..=105).contains(&median_interval) && t12_postings.len() >= 2 && cv <= 0.40 {
            Some(RecurrenceCadence::Quarterly)
        } else if (160..=200).contains(&median_interval) && t12_postings.len() >= 1 && cv <= 0.35 {
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
            let mut t12_amts: Vec<Money> = t12_postings.iter().map(|(_, a)| *a).collect();
            t12_amts.sort();
            let (median_amount, median_is_estimate) = if t12_amts.len() % 2 == 1 {
                (t12_amts[t12_amts.len() / 2], false)
            } else {
                let mid = t12_amts.len() / 2;
                let sum = t12_amts[mid - 1] + t12_amts[mid];
                (sum.rounded_div(2).unwrap(), sum.checked_div(2).is_none())
            };
            let mean_sum = t12_amts
                .iter()
                .copied()
                .fold(Money::default(), |a, b| a + b);
            let mean_amount = mean_sum.rounded_div(t12_amts.len() as i128).unwrap();
            let mean_is_estimate = mean_sum.checked_div(t12_amts.len() as i128).is_none();

            if let Some(next_projected_date) = cad.project_next(&last_date) {
                let key = obligation_currency_key(&payee, &account, &currency);
                let metadata = registry_map
                    .get(&key)
                    .or_else(|| {
                        if currency == "USD" {
                            registry_map.get(&obligation_key(&payee, &account))
                        } else {
                            None
                        }
                    })
                    .filter(|meta| meta.currency == currency)
                    .cloned();
                let price_delta = compute_price_delta(latest_amount, median_amount, &currency);
                let alerts = evaluate_renewal_alerts(
                    cad,
                    anchor_date,
                    &next_projected_date,
                    price_delta.as_ref(),
                    metadata.as_ref(),
                );

                obligations.push(RecurringObligation {
                    key,
                    payee,
                    account,
                    currency,
                    cadence: cad,
                    latest_amount,
                    median_amount,
                    mean_amount,
                    median_is_estimate,
                    mean_is_estimate,
                    price_delta,
                    alerts,
                    median_interval_days: median_interval,
                    trailing_12m_count: t12_postings.len(),
                    total_count: txs.len(),
                    last_date,
                    next_projected_date,
                    metadata,
                });
            }
        }
    }

    // Sort obligations: items with active alerts first, then next projected date ascending, then payee ascending
    obligations.sort_by(|a, b| {
        let a_has_alerts = !a.alerts.is_empty();
        let b_has_alerts = !b.alerts.is_empty();
        b_has_alerts
            .cmp(&a_has_alerts)
            .then_with(|| a.next_projected_date.cmp(&b.next_projected_date))
            .then_with(|| a.payee.cmp(&b.payee))
    });

    obligations
}

/// Tauri command to retrieve recurring obligations enriched with registry metadata and alerts.
#[tauri::command]
pub fn get_recurring_obligations() -> Result<Vec<RecurringObligation>, String> {
    let transactions = validated_transactions()?;
    let path = Path::new(crate::config::get("subscriptions_path"));
    let registry = if path.exists() {
        load_subscription_registry(path)?
    } else {
        SubscriptionRegistryPayload::default()
    };
    let today = current_date_utc();
    Ok(analyze_recurring_obligations_with_registry(
        &transactions,
        Some(&today),
        &registry,
    ))
}

/// Tauri command to retrieve raw subscription registry metadata payload from `subscriptions.json`.
#[tauri::command]
pub fn get_subscription_registry() -> Result<SubscriptionRegistryPayload, String> {
    load_subscription_registry(Path::new(crate::config::get("subscriptions_path")))
}

/// Tauri command to save or update an individual subscription's metadata in `subscriptions.json`.
#[tauri::command]
pub fn save_subscription_metadata(metadata: SubscriptionMetadata) -> Result<(), String> {
    let path = Path::new(crate::config::get("subscriptions_path"));
    let mut payload = if path.exists() {
        load_subscription_registry(path)?
    } else {
        SubscriptionRegistryPayload::default()
    };

    // Normalize metadata key if blank
    let mut entry = metadata;
    if entry.key.is_empty() {
        entry.key = obligation_currency_key(&entry.payee, &entry.account, &entry.currency);
    }

    if let Some(existing) = payload
        .subscriptions
        .iter_mut()
        .find(|item| item.key == entry.key)
    {
        *existing = entry;
    } else {
        payload.subscriptions.push(entry);
    }

    save_subscription_registry(path, &payload)
}

/// Tauri command to save the complete subscription registry payload.
#[tauri::command]
pub fn save_subscription_registry_payload(
    payload: SubscriptionRegistryPayload,
) -> Result<(), String> {
    save_subscription_registry(
        Path::new(crate::config::get("subscriptions_path")),
        &payload,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger_parser::Posting;

    #[test]
    fn recurring_obligations_are_grouped_by_currency_and_keep_currency_keys() {
        let mut txs = Vec::new();
        for (date, usd, eur) in [
            ("2026-01-01", "10", "20"),
            ("2026-02-01", "10", "20"),
            ("2026-03-01", "10", "20"),
        ] {
            txs.push(Transaction {
                flag: '*',
                date: date.into(),
                payee: "Service".into(),
                narration: "".into(),
                postings: vec![
                    Posting {
                        account: "Expenses:Services:Monthly".into(),
                        currency: "USD".into(),
                        amount: Money::parse(usd).unwrap(),
                    },
                    Posting {
                        account: "Expenses:Services:Monthly".into(),
                        currency: "EUR".into(),
                        amount: Money::parse(eur).unwrap(),
                    },
                ],
            });
        }
        let items = analyze_recurring_obligations(&txs, Some("2026-03-15"));
        assert_eq!(items.len(), 2);
        assert!(items
            .iter()
            .any(|x| x.key == "Service::Expenses:Services:Monthly::USD"
                && x.median_amount == Money::parse("10").unwrap()));
        assert!(items
            .iter()
            .any(|x| x.key == "Service::Expenses:Services:Monthly::EUR"
                && x.median_amount == Money::parse("20").unwrap()));
    }

    #[test]
    fn metadata_matches_only_its_currency_and_legacy_keys_remain_usd_only() {
        let mut txs = Vec::new();
        for date in ["2026-01-01", "2026-02-01", "2026-03-01"] {
            txs.push(Transaction {
                flag: '*',
                date: date.into(),
                payee: "Service".into(),
                narration: "".into(),
                postings: vec![
                    Posting {
                        account: "Expenses:Services:Monthly".into(),
                        currency: "USD".into(),
                        amount: Money::parse("10").unwrap(),
                    },
                    Posting {
                        account: "Expenses:Services:Monthly".into(),
                        currency: "EUR".into(),
                        amount: Money::parse("20").unwrap(),
                    },
                ],
            });
        }
        let usd_meta = SubscriptionMetadata {
            key: obligation_key("Service", "Expenses:Services:Monthly"),
            payee: "Service".into(),
            account: "Expenses:Services:Monthly".into(),
            currency: "USD".into(),
            notes: Some("legacy USD".into()),
            ..Default::default()
        };
        let eur_meta = SubscriptionMetadata {
            key: obligation_currency_key("Service", "Expenses:Services:Monthly", "EUR"),
            payee: "Service".into(),
            account: "Expenses:Services:Monthly".into(),
            currency: "EUR".into(),
            notes: Some("explicit EUR".into()),
            ..Default::default()
        };
        let registry = SubscriptionRegistryPayload {
            subscriptions: vec![usd_meta.clone(), eur_meta.clone()],
            ..Default::default()
        };
        let items =
            analyze_recurring_obligations_with_registry(&txs, Some("2026-03-15"), &registry);
        assert_eq!(items.len(), 2);
        assert_eq!(
            items
                .iter()
                .find(|x| x.currency == "USD")
                .unwrap()
                .metadata
                .as_ref()
                .unwrap()
                .notes
                .as_deref(),
            Some("legacy USD")
        );
        assert_eq!(
            items
                .iter()
                .find(|x| x.currency == "EUR")
                .unwrap()
                .metadata
                .as_ref()
                .unwrap()
                .notes
                .as_deref(),
            Some("explicit EUR")
        );
        let no_eur = SubscriptionRegistryPayload {
            subscriptions: vec![usd_meta],
            ..Default::default()
        };
        let items = analyze_recurring_obligations_with_registry(&txs, Some("2026-03-15"), &no_eur);
        assert!(items
            .iter()
            .find(|x| x.currency == "EUR")
            .unwrap()
            .metadata
            .is_none());
        let mismatch = SubscriptionRegistryPayload {
            subscriptions: vec![SubscriptionMetadata {
                currency: "USD".into(),
                ..eur_meta
            }],
            ..Default::default()
        };
        let items =
            analyze_recurring_obligations_with_registry(&txs, Some("2026-03-15"), &mismatch);
        assert!(items.iter().all(|x| x.metadata.is_none()));
    }

    #[test]
    fn even_money_median_rounding_is_explicit_at_fixed_scale() {
        let low = Money::default();
        let high = Money::parse("0.000000000000000001").unwrap();
        assert_eq!((low + high).rounded_div(2), Some(high));
    }

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
                flag: '*',
                date: "2026-01-15".to_string(),
                payee: "Netflix".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("15.49").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-02-15".to_string(),
                payee: "Netflix".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("15.49").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-03-15".to_string(),
                payee: "Netflix".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("15.49").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-04-15".to_string(),
                payee: "Netflix".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("15.49").unwrap(),
                }],
            },
        ];

        let obligations = analyze_recurring_obligations(&txs, Some("2026-04-30"));
        assert_eq!(obligations.len(), 1);
        let nflx = &obligations[0];
        assert_eq!(nflx.key, "Netflix::Expenses:Tech:Subscriptions::USD");
        assert_eq!(nflx.payee, "Netflix");
        assert_eq!(nflx.account, "Expenses:Tech:Subscriptions");
        assert_eq!(nflx.cadence, RecurrenceCadence::Monthly);
        assert_eq!(nflx.median_amount, Money::parse("15.49").unwrap());
        assert_eq!(nflx.mean_amount, Money::parse("15.49").unwrap());
        assert_eq!(nflx.trailing_12m_count, 4);
        assert_eq!(nflx.last_date, "2026-04-15");
        assert_eq!(nflx.next_projected_date, "2026-05-15");
    }

    #[test]
    fn test_detect_annual_subscription() {
        let txs = vec![
            Transaction {
                flag: '*',
                date: "2024-03-10".to_string(),
                payee: "Costco Renewal".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("120.00").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2025-03-10".to_string(),
                payee: "Costco Renewal".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("120.00").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-03-10".to_string(),
                payee: "Costco Renewal".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("130.00").unwrap(),
                }],
            },
        ];

        let obligations = analyze_recurring_obligations(&txs, Some("2026-04-01"));
        assert_eq!(obligations.len(), 1);
        let costco = &obligations[0];
        assert_eq!(costco.payee, "Costco Renewal");
        assert_eq!(costco.cadence, RecurrenceCadence::Annual);
        assert_eq!(costco.median_amount, Money::parse("130").unwrap());
        assert_eq!(costco.total_count, 3);
        assert_eq!(costco.trailing_12m_count, 1);
        assert_eq!(costco.last_date, "2026-03-10");
        assert_eq!(costco.next_projected_date, "2027-03-10");
    }

    #[test]
    fn test_detect_biweekly_payroll_obligation() {
        let txs = vec![
            Transaction {
                flag: '*',
                date: "2026-01-02".to_string(),
                payee: "Benefits Provider".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Health:Medical".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("100.0").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-01-16".to_string(),
                payee: "Benefits Provider".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Health:Medical".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("100.0").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-01-30".to_string(),
                payee: "Benefits Provider".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Health:Medical".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("100.0").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-02-13".to_string(),
                payee: "Benefits Provider".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Health:Medical".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("100.0").unwrap(),
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
    fn test_even_count_median_calculation() {
        let txs = vec![
            Transaction {
                flag: '*',
                date: "2026-01-01".to_string(),
                payee: "Streaming".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("10.0").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-02-01".to_string(),
                payee: "Streaming".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("20.0").unwrap(),
                }],
            },
        ];

        let obligations = analyze_recurring_obligations(&txs, Some("2026-02-15"));
        assert_eq!(obligations.len(), 1);
        let s = &obligations[0];
        assert_eq!(s.median_amount, Money::parse("15").unwrap());
        assert_eq!(s.mean_amount, Money::parse("15").unwrap());
    }

    #[test]
    fn test_price_increase_detection() {
        let txs = vec![
            Transaction {
                flag: '*',
                date: "2026-01-01".to_string(),
                payee: "Cloud Storage".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("10.00").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-02-01".to_string(),
                payee: "Cloud Storage".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("10.00").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-03-01".to_string(),
                payee: "Cloud Storage".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("15.00").unwrap(), // Price hiked from 10.00 to 15.00 (+50%)
                }],
            },
        ];

        let obligations = analyze_recurring_obligations(&txs, Some("2026-03-15"));
        assert_eq!(obligations.len(), 1);
        let o = &obligations[0];
        let delta = o.price_delta.as_ref().expect("Expected price delta");
        assert_eq!(delta.latest_amount, Money::parse("15").unwrap());
        assert_eq!(delta.median_amount, Money::parse("10").unwrap());
        assert_eq!(delta.dollar_delta, Money::parse("5").unwrap());
        assert_eq!(delta.percentage_delta, Some(Money::parse("50").unwrap()));
        assert!(delta.is_increase);

        // Check that a price hike alert was generated
        assert!(o
            .alerts
            .iter()
            .any(|a| a.kind == RenewalAlertKind::PriceHike));
    }

    #[test]
    fn price_delta_keeps_extreme_hikes_and_exact_thresholds() {
        let tiny = Money::parse("0.000000000000000001").unwrap();
        let hike =
            compute_price_delta(Money::parse("1000000000000000").unwrap(), tiny, "EUR").unwrap();
        assert!(hike.is_increase);
        assert_eq!(hike.currency, "EUR");
        assert_eq!(hike.percentage_delta, None);
        let alert = evaluate_renewal_alerts(
            RecurrenceCadence::Monthly,
            "2026-03-01",
            "2026-04-01",
            Some(&hike),
            None,
        )
        .into_iter()
        .find(|alert| alert.kind == RenewalAlertKind::PriceHike)
        .unwrap();
        assert!(alert.message.contains("EUR (percentage unavailable)"));
        assert!(!alert.message.contains("+percentage") && !alert.message.contains("unavailable%"));
        let carry = Money::ratio_percent(
            Money::parse("19999999999999.999999999999999999").unwrap(),
            Money::parse("1000000000000000").unwrap(),
        )
        .unwrap();
        assert_eq!(carry.to_string(), "2");
        let below = compute_price_delta(
            Money::parse("999999999999999.999999999999999999").unwrap(),
            Money::parse("1000000000000000").unwrap(),
            "USD",
        )
        .unwrap();
        assert_eq!(below.percentage_delta.unwrap().to_string(), "0");
        let negative = Money::ratio_percent(
            Money::parse("-19999999999999.999999999999999999").unwrap(),
            Money::parse("1000000000000000").unwrap(),
        )
        .unwrap();
        assert_eq!(negative.to_string(), "-2");
    }

    #[test]
    fn price_percentage_exactness_and_nonpositive_baselines() {
        let exact = compute_price_delta(
            Money::parse("9").unwrap(),
            Money::parse("8").unwrap(),
            "EUR",
        )
        .unwrap();
        assert_eq!(exact.percentage_delta, Some(Money::parse("12.5").unwrap()));
        assert!(!exact.percentage_is_estimate);

        let repeating = compute_price_delta(
            Money::parse("4").unwrap(),
            Money::parse("3").unwrap(),
            "EUR",
        )
        .unwrap();
        assert!(repeating.percentage_delta.is_some());
        assert!(repeating.percentage_is_estimate);

        for baseline in ["0", "-2"] {
            let delta = compute_price_delta(
                Money::parse("1").unwrap(),
                Money::parse(baseline).unwrap(),
                "USD",
            )
            .unwrap();
            assert_eq!(delta.percentage_delta, None);
            assert!(!delta.is_increase);
            let expected_delta = if baseline == "0" { "1" } else { "3" };
            assert_eq!(delta.dollar_delta, Money::parse(expected_delta).unwrap());
        }
    }

    #[test]
    fn recurring_statistics_flag_rounded_fractional_median_and_mean() {
        let txs = ["2026-01-01", "2026-02-01"]
            .into_iter()
            .zip(["0.000000000000000001", "0.000000000000000002"])
            .map(|(date, amount)| Transaction {
                flag: '*',
                date: date.into(),
                payee: "Tiny".into(),
                narration: "".into(),
                postings: vec![Posting {
                    account: "Expenses:Services:Monthly".into(),
                    currency: "EUR".into(),
                    amount: Money::parse(amount).unwrap(),
                }],
            })
            .collect::<Vec<_>>();
        let item = analyze_recurring_obligations(&txs, Some("2026-02-15"))
            .into_iter()
            .next()
            .unwrap();
        assert!(item.median_is_estimate);
        assert_eq!(item.median_amount.to_string(), "0.000000000000000002");
        assert!(item.mean_is_estimate);
        assert_eq!(item.currency, "EUR");
    }

    #[test]
    fn test_trial_and_renewal_radar() {
        let txs = vec![
            Transaction {
                flag: '*',
                date: "2025-05-01".to_string(),
                payee: "Annual Service".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("100.0").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-05-01".to_string(),
                payee: "Annual Service".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Tech:Subscriptions".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("100.0").unwrap(),
                }],
            },
        ];

        let mut registry = SubscriptionRegistryPayload::default();
        registry.subscriptions.push(SubscriptionMetadata {
            key: "Annual Service::Expenses:Tech:Subscriptions".to_string(),
            payee: "Annual Service".to_string(),
            account: "Expenses:Tech:Subscriptions".to_string(),
            currency: "USD".to_string(),
            billing_method: Some("Chase Visa".to_string()),
            cancellation_url: Some("https://example.com/cancel".to_string()),
            notes: Some("Cancel if not used".to_string()),
            is_trial: true,
            trial_end_date: Some("2026-05-10".to_string()),
            alert_lead_days: Some(14),
            custom_label: Some("Work Cloud".to_string()),
            extra: BTreeMap::new(),
        });

        // Anchor on 2026-05-05 (5 days before trial expiration)
        let obligations =
            analyze_recurring_obligations_with_registry(&txs, Some("2026-05-05"), &registry);

        assert_eq!(obligations.len(), 1);
        let item = &obligations[0];
        assert!(item.metadata.is_some());
        assert_eq!(
            item.metadata.as_ref().unwrap().cancellation_url,
            Some("https://example.com/cancel".to_string())
        );

        let trial_alert = item
            .alerts
            .iter()
            .find(|a| a.kind == RenewalAlertKind::TrialExpiring);
        assert!(trial_alert.is_some());
        assert_eq!(trial_alert.unwrap().days_until, Some(5));
    }

    #[test]
    fn test_registry_file_atomic_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_subscriptions_roundtrip.json");
        if test_file.exists() {
            let _ = fs::remove_file(&test_file);
        }

        let mut payload = SubscriptionRegistryPayload::default();
        payload.subscriptions.push(SubscriptionMetadata {
            key: "TestPayee::Expenses:Misc".to_string(),
            payee: "TestPayee".to_string(),
            account: "Expenses:Misc".to_string(),
            currency: "USD".to_string(),
            billing_method: Some("Amex".to_string()),
            cancellation_url: Some("https://test.com/cancel".to_string()),
            notes: Some("Test note".to_string()),
            is_trial: false,
            trial_end_date: None,
            alert_lead_days: Some(30),
            custom_label: None,
            extra: BTreeMap::new(),
        });

        save_subscription_registry(&test_file, &payload).expect("save should succeed");
        let loaded = load_subscription_registry(&test_file).expect("load should succeed");
        assert_eq!(loaded.subscriptions.len(), 1);
        assert_eq!(loaded.subscriptions[0].payee, "TestPayee");
        assert_eq!(
            loaded.subscriptions[0].cancellation_url,
            Some("https://test.com/cancel".to_string())
        );

        let _ = fs::remove_file(&test_file);
    }

    #[test]
    fn test_registry_corrupted_file_error_propagated() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_subscriptions_corrupted.json");
        fs::write(&test_file, b"{\"subscriptions\": [ invalid json ").unwrap();

        let result = load_subscription_registry(&test_file);
        assert!(
            result.is_err(),
            "Corrupted registry should return Err, not default"
        );

        let _ = fs::remove_file(&test_file);
    }

    #[test]
    fn test_rejects_irregular_retail_purchases() {
        // Amazon purchases on random days with huge interval variance
        let txs = vec![
            Transaction {
                flag: '*',
                date: "2026-01-03".to_string(),
                payee: "Amazon".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("25.0").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-01-05".to_string(),
                payee: "Amazon".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("40.0").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-02-20".to_string(),
                payee: "Amazon".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("12.0").unwrap(),
                }],
            },
            Transaction {
                flag: '*',
                date: "2026-04-10".to_string(),
                payee: "Amazon".to_string(),
                narration: "".to_string(),
                postings: vec![Posting {
                    account: "Expenses:Household:Supplies".to_string(),
                    currency: "USD".to_string(),
                    amount: Money::parse("15.0").unwrap(),
                }],
            },
        ];

        let obligations = analyze_recurring_obligations(&txs, Some("2026-04-30"));
        assert!(obligations.is_empty());
    }
}
