//! Read-only household analysis from one validated byte snapshot. No ledger mutation.
use crate::{
    document_store as store,
    ledger_parser::{self, Transaction},
    money::Money,
    obligations::{self, RecurrenceCadence, SubscriptionMetadata, SubscriptionRegistryPayload},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeRequest {
    pub start: String,
    pub end: String,
    pub currency: String,
    pub comparison_start: Option<String>,
    pub comparison_end: Option<String>,
}
#[derive(Clone, Serialize, Debug)]
pub struct Source {
    pub id: String,
    pub date: String,
    pub description: String,
    pub pending: bool,
    pub internal_transfer: bool,
    pub postings: BTreeMap<String, Money>,
}
#[derive(Serialize, Debug)]
pub struct Category {
    pub account: String,
    pub current: Option<Money>,
    pub previous: Option<Money>,
    pub difference: Option<Money>,
    pub source_ids: Vec<String>,
    pub comparison_source_ids: Vec<String>,
}
#[derive(Serialize, Debug)]
pub struct Month {
    pub month: String,
    pub start: String,
    pub end: String,
    pub evidence_status: String,
    pub opening_net_worth: Option<Money>,
    pub closing_net_worth: Option<Money>,
    pub net_worth_change: Option<Money>,
    pub income: Money,
    pub spending: Money,
    pub equity_and_adjustments: Money,
    pub unexplained: Option<Money>,
    pub source_ids: Vec<String>,
}
#[derive(Serialize, Debug)]
pub struct Analysis {
    pub chart_history: Vec<ledger_parser::HistorySnapshot>,
    pub ledger_revision: String,
    pub currency: String,
    pub latest_transaction_date: Option<String>,
    pub months: Vec<Month>,
    pub categories: Vec<Category>,
    pub sources: Vec<Source>,
    pub comparison_start: Option<String>,
    pub comparison_end: Option<String>,
}
fn date_range(start: &str, end: &str) -> Result<(), String> {
    if !ledger_parser::valid_date(start)
        || !ledger_parser::valid_date(end)
        || start > end
        || obligations::days_between(start, end).is_none_or(|d| d > 36600)
    {
        return Err("Choose valid ordered dates spanning at most 100 years".into());
    }
    Ok(())
}
fn read_ledger() -> Result<(Vec<Transaction>, String), String> {
    let bytes = fs::read(crate::config::get("ledger_path")).map_err(|_| "Ledger unavailable")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Ledger is not UTF-8")?;
    let txs = ledger_parser::parse_with_currency(text, crate::config::get("ledger_currency"))
        .map_err(|e| format!("Ledger not validated: {}", e.join("; ")))?;
    Ok((txs, store::hash(&bytes)))
}
fn source(tx: &Transaction, i: usize, revision: &str, unit: &str) -> Source {
    let mut postings = BTreeMap::new();
    for p in tx.postings.iter().filter(|p| p.currency == unit) {
        *postings.entry(p.account.clone()).or_default() += p.amount;
    }
    let internal_transfer = !postings.is_empty()
        && postings
            .keys()
            .all(|a| a.starts_with("Assets:") || a.starts_with("Liabilities:"));
    Source {
        id: store::hash(format!("{revision}:{i}:{unit}").as_bytes()),
        date: tx.date.clone(),
        description: format!("{} {}", tx.payee, tx.narration).trim().into(),
        pending: tx.flag != '*',
        internal_transfer,
        postings,
    }
}
fn net(tx: &Transaction, unit: &str) -> Money {
    tx.postings
        .iter()
        .filter(|p| {
            p.currency == unit
                && (p.account.starts_with("Assets:") || p.account.starts_with("Liabilities:"))
        })
        .fold(Money::default(), |a, p| a + p.amount)
}
pub(crate) fn analyze(
    txs: &[Transaction],
    revision: String,
    r: RangeRequest,
) -> Result<Analysis, String> {
    date_range(&r.start, &r.end)?;
    if !ledger_parser::valid_currency(&r.currency) {
        return Err("Invalid currency/unit".into());
    }
    match (&r.comparison_start, &r.comparison_end) {
        (Some(a), Some(b)) => date_range(a, b)?,
        (None, None) => (),
        _ => return Err("Provide both comparison dates".into()),
    }
    let mut ordered = txs.to_vec();
    ordered.sort_by(|a, b| a.date.cmp(&b.date));
    let first = ordered
        .iter()
        .find(|t| t.postings.iter().any(|p| p.currency == r.currency))
        .map(|t| t.date.as_str());
    let mut month = format!("{}-01", &r.start[..7]);
    let mut months = Vec::new();
    while month <= r.end {
        let next = obligations::add_months(&month, 1).ok_or("Month overflow")?;
        if !ledger_parser::valid_date(&next) || next <= month {
            return Err("Range exceeds supported calendar years".into());
        }
        let end = obligations::add_days(&next, -1)
            .ok_or("Month overflow")?
            .min(r.end.clone());
        let start = month.clone().max(r.start.clone());
        let opening = ordered
            .iter()
            .filter(|t| t.date < start)
            .fold(Money::default(), |v, t| v + net(t, &r.currency));
        let mut change = Money::default();
        let mut income = Money::default();
        let mut spending = Money::default();
        let mut equity = Money::default();
        let mut ids = Vec::new();
        for (i, tx) in ordered
            .iter()
            .enumerate()
            .filter(|(_, t)| t.date >= start && t.date <= end)
        {
            if !tx.postings.iter().any(|p| p.currency == r.currency) {
                continue;
            }
            ids.push(source(tx, i, &revision, &r.currency).id);
            change += net(tx, &r.currency);
            for p in tx.postings.iter().filter(|p| p.currency == r.currency) {
                if p.account.starts_with("Income:") {
                    income -= p.amount;
                } else if p.account.starts_with("Expenses:") {
                    spending += p.amount;
                } else if p.account.starts_with("Equity:") {
                    equity -= p.amount;
                }
            }
        }
        let known = first.is_some_and(|d| d <= end.as_str());
        let opening_known = first.is_some_and(|d| d < start.as_str());
        months.push(Month {
            month: month[..7].into(),
            start,
            end,
            evidence_status: if !known {
                "no_currency_evidence"
            } else if ids.is_empty() {
                "carried_forward_no_entries"
            } else {
                "ledger_entries"
            }
            .into(),
            opening_net_worth: opening_known.then_some(opening),
            closing_net_worth: known.then_some(opening + change),
            net_worth_change: known.then_some(change),
            income,
            spending,
            equity_and_adjustments: equity,
            unexplained: known.then_some(change - (income - spending + equity)),
            source_ids: ids,
        });
        month = next;
    }
    let in_period = |d: &str, a: &str, b: &str| d >= a && d <= b;
    let mut categories: BTreeMap<String, (Money, Money, Vec<String>, Vec<String>)> =
        BTreeMap::new();
    let mut sources = Vec::new();
    for (i, tx) in ordered.iter().enumerate() {
        let current = in_period(&tx.date, &r.start, &r.end);
        let previous = r
            .comparison_start
            .as_ref()
            .zip(r.comparison_end.as_ref())
            .is_some_and(|(a, b)| in_period(&tx.date, a, b));
        if !(current || previous) || !tx.postings.iter().any(|p| p.currency == r.currency) {
            continue;
        }
        let row = source(tx, i, &revision, &r.currency);
        for (account, amount) in &row.postings {
            if !(account.starts_with("Income:") || account.starts_with("Expenses:")) {
                continue;
            }
            let value = if account.starts_with("Income:") {
                -*amount
            } else {
                *amount
            };
            let entry = categories.entry(account.clone()).or_default();
            if current {
                entry.0 += value;
                entry.2.push(row.id.clone());
            }
            if previous {
                entry.1 += value;
                entry.3.push(row.id.clone());
            }
        }
        sources.push(row);
    }
    let current_known = first.is_some_and(|d| d <= r.end.as_str());
    let comparison_known = first.is_some_and(|d| {
        r.comparison_end
            .as_ref()
            .is_some_and(|end| d <= end.as_str())
    });
    Ok(Analysis {
        chart_history: ledger_parser::history_for_range(
            &ordered,
            &r.currency,
            Some((&r.start, &r.end)),
        ),
        ledger_revision: revision,
        currency: r.currency.clone(),
        latest_transaction_date: ordered
            .iter()
            .rev()
            .find(|t| t.postings.iter().any(|p| p.currency == r.currency))
            .map(|t| t.date.clone()),
        months,
        categories: categories
            .into_iter()
            .map(
                |(account, (current, previous, source_ids, comparison_source_ids))| Category {
                    account,
                    current: current_known.then_some(current),
                    previous: comparison_known.then_some(previous),
                    difference: (current_known && comparison_known).then_some(current - previous),
                    source_ids,
                    comparison_source_ids,
                },
            )
            .collect(),
        sources,
        comparison_start: r.comparison_start,
        comparison_end: r.comparison_end,
    })
}
#[tauri::command]
pub fn get_household_analysis(request: RangeRequest) -> Result<Analysis, String> {
    let (txs, rev) = read_ledger()?;
    analyze(&txs, rev, request)
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastRequest {
    pub as_of: String,
    pub horizon_days: u32,
    pub currency: String,
    pub liquid_accounts: Vec<String>,
    pub include_estimates: bool,
}
#[derive(Serialize, Debug)]
pub struct ForecastEvent {
    pub date: String,
    pub key: String,
    pub payee: String,
    pub amount: Money,
    pub kind: String,
    pub confidence: String,
    pub last_observation: Option<String>,
    pub stale: bool,
    pub price_change: Option<obligations::PriceDelta>,
    pub trial_end: Option<String>,
    pub note: String,
    pub confirmed_balance: Money,
    pub estimated_balance: Money,
}
#[derive(Debug, Serialize)]
pub struct Forecast {
    pub ledger_revision: String,
    pub registry_revision: String,
    pub currency: String,
    pub as_of: String,
    pub end: String,
    pub liquid_candidates: Vec<String>,
    pub selected_accounts: Vec<String>,
    pub opening_cash: Option<Money>,
    pub pending_cash_movement: Money,
    pub events: Vec<ForecastEvent>,
    pub warnings: Vec<String>,
    pub settings: Vec<SubscriptionMetadata>,
    pub detected_obligations: Vec<obligations::RecurringObligation>,
}
fn liquid(a: &str) -> bool {
    a == "Assets:Cash"
        || a.starts_with("Assets:Cash:")
        || a == "Assets:Bank"
        || a.starts_with("Assets:Bank:")
        || a == "Assets:Checking"
        || a.starts_with("Assets:Checking:")
        || a == "Assets:Savings"
        || a.starts_with("Assets:Savings:")
}
fn registry_at(path: &Path) -> Result<(SubscriptionRegistryPayload, String), String> {
    let (bytes, rev) = store::read(path)?;
    let registry = if rev == "missing" {
        SubscriptionRegistryPayload::default()
    } else {
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))
            .map_err(|_| "Subscription registry unreadable; forecasts withheld")?
    };
    Ok((registry, rev))
}
fn boolean(m: Option<&SubscriptionMetadata>, key: &str) -> bool {
    m.and_then(|m| m.extra.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}
fn string<'a>(m: Option<&'a SubscriptionMetadata>, key: &str) -> Option<&'a str> {
    m.and_then(|m| m.extra.get(key))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastSettings {
    pub payee: String,
    pub account: String,
    pub currency: String,
    pub confirmed: bool,
    pub cancelled: bool,
    pub cash_account: String,
    pub amount: Option<Money>,
    pub due_date: Option<String>,
    pub cadence: Option<RecurrenceCadence>,
}
fn save_settings_at(
    path: &Path,
    settings: ForecastSettings,
    expected_revision: &str,
) -> Result<String, String> {
    if settings.payee.trim().is_empty()
        || !ledger_parser::account_ok(&settings.account)
        || !settings.account.starts_with("Expenses:")
        || !ledger_parser::valid_currency(&settings.currency)
        || (!settings.cash_account.is_empty() && !liquid(&settings.cash_account))
        || settings.amount.is_some_and(|v| v <= Money::default())
        || settings
            .due_date
            .as_ref()
            .is_some_and(|d| !ledger_parser::valid_date(d))
    {
        return Err("Use a payee, expense account, currency, positive bill amount, valid due date and bank/cash funding account".into());
    }
    if settings.confirmed
        && (settings.amount.is_none()
            || settings.due_date.is_none()
            || settings.cash_account.is_empty())
    {
        return Err(
            "Confirmed bills require an amount, due date and explicit funding account".into(),
        );
    }
    let _lock = store::lock(path)?;
    let (mut registry, revision) = registry_at(path)?;
    if revision != expected_revision {
        return Err("Subscription settings conflict: refresh first".into());
    }
    let key = obligations::obligation_currency_key(
        &settings.payee,
        &settings.account,
        &settings.currency,
    );
    let index = registry.subscriptions.iter().position(|m| {
        m.key == key
            || (settings.currency == "USD"
                && m.currency == "USD"
                && m.key == obligations::obligation_key(&settings.payee, &settings.account))
    });
    let entry = if let Some(i) = index {
        &mut registry.subscriptions[i]
    } else {
        registry.subscriptions.push(SubscriptionMetadata {
            key: key.clone(),
            payee: settings.payee.clone(),
            account: settings.account.clone(),
            currency: settings.currency.clone(),
            ..Default::default()
        });
        registry.subscriptions.last_mut().unwrap()
    };
    for (k, v) in [
        ("forecast_confirmed", serde_json::json!(settings.confirmed)),
        ("forecast_cancelled", serde_json::json!(settings.cancelled)),
        (
            "forecast_cash_account",
            serde_json::json!(settings.cash_account),
        ),
        ("forecast_amount", serde_json::json!(settings.amount)),
        ("forecast_due_date", serde_json::json!(settings.due_date)),
        ("forecast_cadence", serde_json::json!(settings.cadence)),
    ] {
        entry.extra.insert(k.into(), v);
    }
    store::save(
        path,
        &revision,
        &serde_json::to_vec_pretty(&registry).map_err(|e| e.to_string())?,
    )
}
#[tauri::command]
pub fn save_forecast_settings(
    settings: ForecastSettings,
    expected_revision: String,
) -> Result<String, String> {
    save_settings_at(
        Path::new(crate::config::get("subscriptions_path")),
        settings,
        &expected_revision,
    )
}

fn forecast(
    txs: &[Transaction],
    revision: String,
    registry: SubscriptionRegistryPayload,
    registry_revision: String,
    r: ForecastRequest,
) -> Result<Forecast, String> {
    if !ledger_parser::valid_date(&r.as_of)
        || r.horizon_days == 0
        || r.horizon_days > 366
        || !ledger_parser::valid_currency(&r.currency)
    {
        return Err("Choose a valid as-of date, currency and horizon of 1–366 days".into());
    }
    let end =
        obligations::add_days(&r.as_of, r.horizon_days as i32).ok_or("Invalid forecast end")?;
    if !ledger_parser::valid_date(&end) {
        return Err("Forecast end exceeds supported calendar years".into());
    }
    let candidates: BTreeSet<String> = txs
        .iter()
        .flat_map(|t| t.postings.iter())
        .filter(|p| p.currency == r.currency && liquid(&p.account))
        .map(|p| p.account.clone())
        .collect();
    let selected: BTreeSet<_> = r.liquid_accounts.iter().cloned().collect();
    if selected.len() != r.liquid_accounts.len() || selected.iter().any(|a| !candidates.contains(a))
    {
        return Err("Select unique bank/cash accounts present in this currency; investment assets are not spendable cash".into());
    }
    let movement = |t: &Transaction| {
        t.postings
            .iter()
            .filter(|p| p.currency == r.currency && selected.contains(&p.account))
            .fold(Money::default(), |v, p| v + p.amount)
    };
    let opening = txs
        .iter()
        .filter(|t| t.date <= r.as_of)
        .fold(Money::default(), |v, t| v + movement(t));
    let pending = txs
        .iter()
        .filter(|t| t.date <= r.as_of && t.flag != '*')
        .fold(Money::default(), |v, t| v + movement(t));
    let historical: Vec<_> = txs.iter().filter(|t| t.date <= r.as_of).cloned().collect();
    let detected = obligations::analyze_recurring_obligations_with_registry(
        &historical,
        Some(&r.as_of),
        &registry,
    );
    let mut warnings = vec![];
    if selected.is_empty() {
        warnings.push("Select liquid accounts before relying on availability".into());
    }
    if !pending.is_zero() {
        warnings.push(
            "Opening cash includes pending ledger postings; bank availability may differ".into(),
        );
    }
    let latest = historical
        .iter()
        .filter(|t| {
            t.postings.iter().any(|p| {
                p.currency == r.currency && (selected.is_empty() || selected.contains(&p.account))
            })
        })
        .map(|t| t.date.as_str())
        .max();
    if latest.is_none_or(|d| obligations::days_between(d, &r.as_of).is_none_or(|days| days > 30)) {
        warnings.push("Ledger observations are over 30 days old or unavailable".into());
    }
    let mut events = Vec::new();
    let mut recorded = Vec::new();
    for (i, t) in txs
        .iter()
        .enumerate()
        .filter(|(_, t)| t.date > r.as_of && t.date <= end)
    {
        let amount = movement(t);
        if amount.is_zero() {
            continue;
        }
        recorded.push((i, t));
        events.push(ForecastEvent {
            date: t.date.clone(),
            key: format!("recorded:{i}"),
            payee: format!("{} {}", t.payee, t.narration).trim().into(),
            amount,
            kind: if t.flag == '*' {
                "recorded_cleared"
            } else {
                "recorded_pending"
            }
            .into(),
            confidence: "ledger_posting".into(),
            last_observation: Some(t.date.clone()),
            stale: false,
            price_change: None,
            trial_end: None,
            note: "Already in ledger; counted once in both scenarios".into(),
            confirmed_balance: Money::default(),
            estimated_balance: Money::default(),
        });
    }
    let detected_obligations = detected.clone();
    let mut used_payments = BTreeSet::new();
    let mut handled = BTreeSet::new();
    for ob in detected {
        handled.insert((ob.payee.clone(), ob.account.clone(), ob.currency.clone()));
        if ob.currency != r.currency {
            continue;
        }
        if ob.payee.trim().is_empty() {
            warnings.push(format!(
                "{}: no explicit payee identity; recurring projection withheld",
                ob.account
            ));
            continue;
        }
        let meta = ob.metadata.as_ref();
        if boolean(meta, "forecast_cancelled") {
            warnings.push(format!(
                "{}: cancelled; no future charge projected",
                ob.payee
            ));
            continue;
        }
        let confirmed = boolean(meta, "forecast_confirmed");
        let funding = string(meta, "forecast_cash_account")
            .map(str::to_owned)
            .or_else(|| {
                historical
                    .iter()
                    .filter(|t| {
                        t.payee == ob.payee
                            && t.date == ob.last_date
                            && t.postings
                                .iter()
                                .any(|p| p.account == ob.account && p.currency == r.currency)
                    })
                    .flat_map(|t| t.postings.iter())
                    .find(|p| {
                        p.currency == r.currency
                            && p.amount == -ob.latest_amount
                            && selected.contains(&p.account)
                    })
                    .map(|p| p.account.clone())
            });
        if funding.as_ref().is_none_or(|a| !selected.contains(a)) {
            warnings.push(format!(
                "{}: funding is not mapped to a selected liquid account; excluded",
                ob.payee
            ));
            continue;
        }
        let stale = obligations::days_between(&ob.last_date, &r.as_of).unwrap_or(i32::MAX)
            > ob.median_interval_days.saturating_mul(2).max(60);
        let amount = match string(meta, "forecast_amount") {
            Some(a) if confirmed => Money::parse(a)?,
            _ => ob.latest_amount,
        };
        if amount <= Money::default() {
            warnings.push(format!("{}: nonpositive charge excluded", ob.payee));
            continue;
        }
        let cadence = meta
            .and_then(|m| m.extra.get("forecast_cadence"))
            .filter(|v| !v.is_null())
            .map(|v| {
                serde_json::from_value::<RecurrenceCadence>(v.clone())
                    .map_err(|_| "Invalid forecast cadence")
            })
            .transpose()?
            .unwrap_or(ob.cadence);
        let mut due = string(meta, "forecast_due_date")
            .filter(|_| confirmed)
            .unwrap_or(&ob.next_projected_date)
            .to_owned();
        if !ledger_parser::valid_date(&due) {
            return Err("Invalid obligation due date".into());
        }
        if confirmed && due <= r.as_of {
            let paid = historical.iter().any(|t| {
                movement(t) < Money::default()
                    && t.payee == ob.payee
                    && obligations::days_between(&due, &t.date).is_some_and(|d| d.abs() <= 3)
                    && t.postings.iter().any(|p| {
                        p.account == ob.account
                            && p.currency == r.currency
                            && p.amount > Money::default()
                    })
            });
            if !paid {
                let reserve_date =
                    obligations::add_days(&r.as_of, 1).ok_or("Invalid overdue reserve date")?;
                let payment = recorded.iter().find(|(i, t)| {
                    !used_payments.contains(&(*i, ob.account.clone()))
                        && movement(t) < Money::default()
                        && t.payee == ob.payee
                        && obligations::days_between(&reserve_date, &t.date)
                            .is_some_and(|d| d.abs() <= 3)
                        && t.postings.iter().any(|p| {
                            p.account == ob.account
                                && p.currency == r.currency
                                && p.amount > Money::default()
                        })
                });
                if let Some((i, _)) = payment {
                    used_payments.insert((*i, ob.account.clone()));
                    warnings.push(format!(
                        "{}: recorded payment replaces unresolved overdue reserve",
                        ob.payee
                    ));
                } else {
                    events.push(ForecastEvent { date: reserve_date, key: format!("{}:overdue", ob.key), payee: ob.payee.clone(), amount: -amount, kind: "confirmed".into(), confidence: "user_confirmed_overdue".into(), last_observation: Some(ob.last_date.clone()), stale: true, price_change: ob.price_delta.clone(), trial_end: meta.filter(|m| m.is_trial).and_then(|m| m.trial_end_date.clone()), note: format!("Confirmed due date {due} has no matching payment; one overdue bill reserved next day. Additional historical arrears are not inferred."), confirmed_balance: Money::default(), estimated_balance: Money::default() });
                }
            }
        }
        let mut missed = 0;
        while due <= r.as_of && missed < 10000 {
            due = cadence.project_next(&due).ok_or("Invalid projected date")?;
            if !ledger_parser::valid_date(&due) {
                return Err("Projected date exceeds supported calendar".into());
            }
            missed += 1;
        }
        if due <= r.as_of {
            return Err("Too many elapsed projected cycles; review the due date".into());
        }
        if missed > 0 {
            warnings.push(format!("{}: {missed} elapsed projected cycles are not assumed paid; overdue arrears excluded",ob.payee));
        }
        while due <= end {
            let matching = recorded.iter().find(|(i, t)| {
                !used_payments.contains(&(*i, ob.account.clone()))
                    && movement(t) < Money::default()
                    && t.payee == ob.payee
                    && obligations::days_between(&due, &t.date).is_some_and(|d| d.abs() <= 3)
                    && t.postings.iter().any(|p| {
                        p.account == ob.account
                            && p.currency == r.currency
                            && p.amount > Money::default()
                    })
            });
            if let Some((i, _)) = matching {
                used_payments.insert((*i, ob.account.clone()));
                warnings.push(format!(
                    "{} on {due}: represented by a dated ledger payment; projection suppressed",
                    ob.payee
                ));
            } else {
                events.push(ForecastEvent{date:due.clone(),key:ob.key.clone(),payee:ob.payee.clone(),amount:-amount,kind:if confirmed{"confirmed"}else{"detected_estimate"}.into(),confidence:if stale{"low_stale"}else if ob.total_count>=6{"higher_pattern_support"}else{"limited_pattern_support"}.into(),last_observation:Some(ob.last_date.clone()),stale,price_change:ob.price_delta.clone(),trial_end:meta.and_then(|m|m.trial_end_date.clone()),note:format!("{} observations; {}-day median cadence; {} funding; trial renewal amount is not inferred",ob.total_count,ob.median_interval_days,funding.as_deref().unwrap_or("unmapped")),confirmed_balance:Money::default(),estimated_balance:Money::default()});
            }
            let next = cadence.project_next(&due).ok_or("Invalid projected date")?;
            if next <= due || !ledger_parser::valid_date(&next) {
                return Err("Forecast cadence did not advance".into());
            }
            due = next;
        }
    }
    let observed_groups: BTreeSet<_> = historical
        .iter()
        .flat_map(|t| {
            t.postings
                .iter()
                .filter(|p| {
                    p.currency == r.currency
                        && p.account.starts_with("Expenses:")
                        && p.amount > Money::default()
                })
                .map(|p| (t.payee.clone(), p.account.clone(), p.currency.clone()))
        })
        .collect();
    for (payee, account, currency) in observed_groups {
        if !handled.contains(&(payee.clone(), account.clone(), currency.clone()))
            && !registry.subscriptions.iter().any(|m| {
                m.payee == payee
                    && m.account == account
                    && m.currency == currency
                    && (boolean(Some(m), "forecast_confirmed")
                        || boolean(Some(m), "forecast_cancelled"))
            })
        {
            warnings.push(format!(
                "{payee} / {account}: insufficient or irregular history; no recurring projection"
            ));
        }
    }
    for meta in &registry.subscriptions {
        if meta.currency != r.currency
            || handled.contains(&(
                meta.payee.clone(),
                meta.account.clone(),
                meta.currency.clone(),
            ))
        {
            continue;
        }
        if boolean(Some(meta), "forecast_cancelled") {
            warnings.push(format!("{}: cancelled; excluded", meta.payee));
            continue;
        }
        if !boolean(Some(meta), "forecast_confirmed") {
            warnings.push(format!(
                "{}: insufficient recurring history and not confirmed; excluded",
                meta.payee
            ));
            continue;
        }
        let funding = string(Some(meta), "forecast_cash_account").unwrap_or("");
        if !selected.contains(funding) {
            warnings.push(format!(
                "{}: map confirmed bill to a selected liquid account",
                meta.payee
            ));
            continue;
        }
        let amount =
            Money::parse(string(Some(meta), "forecast_amount").ok_or("Confirmed amount missing")?)?;
        let mut due = string(Some(meta), "forecast_due_date")
            .ok_or("Confirmed due date missing")?
            .to_owned();
        if !ledger_parser::valid_date(&due) || amount <= Money::default() {
            return Err("Invalid confirmed bill".into());
        }
        let cadence = meta
            .extra
            .get("forecast_cadence")
            .filter(|v| !v.is_null())
            .map(|v| {
                serde_json::from_value::<RecurrenceCadence>(v.clone())
                    .map_err(|_| "Invalid confirmed cadence")
            })
            .transpose()?;
        // A confirmed payment already recorded by as-of is in opening cash, not another bill.
        let already_paid = historical.iter().any(|t| {
            movement(t) < Money::default()
                && t.payee == meta.payee
                && obligations::days_between(&due, &t.date).is_some_and(|d| d.abs() <= 3)
                && t.postings.iter().any(|p| {
                    p.account == meta.account
                        && p.currency == r.currency
                        && p.amount > Money::default()
                })
        });
        if due <= r.as_of && already_paid {
            warnings.push(format!(
                "{}: confirmed due payment already included in opening cash",
                meta.payee
            ));
            if let Some(c) = cadence {
                while due <= r.as_of {
                    due = c.project_next(&due).ok_or("Invalid cadence")?;
                    if !ledger_parser::valid_date(&due) {
                        return Err("Projected date exceeds supported calendar".into());
                    }
                }
            } else {
                continue;
            }
        }
        let original_due = due.clone();
        let overdue = due <= r.as_of;
        if overdue {
            due = obligations::add_days(&r.as_of, 1).ok_or("Invalid due date")?;
        }
        let mut first_occurrence = true;
        while due <= end {
            let matching = recorded.iter().find(|(i, t)| {
                !used_payments.contains(&(*i, meta.account.clone()))
                    && movement(t) < Money::default()
                    && t.payee == meta.payee
                    && obligations::days_between(&due, &t.date).is_some_and(|d| d.abs() <= 3)
                    && t.postings.iter().any(|p| {
                        p.account == meta.account
                            && p.currency == r.currency
                            && p.amount > Money::default()
                    })
            });
            if let Some((i, _)) = matching {
                used_payments.insert((*i, meta.account.clone()));
                warnings.push(format!(
                    "{} on {due}: dated ledger payment replaces confirmed projection",
                    meta.payee
                ));
            } else {
                events.push(ForecastEvent {
                    date: due.clone(),
                    key: meta.key.clone(),
                    payee: meta.payee.clone(),
                    amount: -amount,
                    kind: "confirmed".into(),
                    confidence: "user_confirmed_no_pattern".into(),
                    last_observation: historical
                        .iter()
                        .filter(|t| {
                            t.payee == meta.payee
                                && t.postings
                                    .iter()
                                    .any(|p| p.account == meta.account && p.currency == r.currency)
                        })
                        .map(|t| t.date.clone())
                        .max(),
                    stale: overdue,
                    price_change: None,
                    trial_end: if meta.is_trial {
                        meta.trial_end_date.clone()
                    } else {
                        None
                    },
                    note: if overdue {
                        "Overdue confirmed bill shown on next day; verify whether already paid"
                    } else {
                        "User confirmed amount/date; insufficient detector history"
                    }
                    .into(),
                    confirmed_balance: Money::default(),
                    estimated_balance: Money::default(),
                });
            }
            let Some(c) = cadence else {
                break;
            };
            let mut next = c
                .project_next(if overdue && first_occurrence {
                    &original_due
                } else {
                    &due
                })
                .ok_or("Invalid cadence")?;
            if overdue && first_occurrence {
                while next <= r.as_of {
                    next = c.project_next(&next).ok_or("Invalid cadence")?;
                    if !ledger_parser::valid_date(&next) {
                        return Err("Projected date exceeds supported calendar".into());
                    }
                }
            }
            first_occurrence = false;
            if next < due || (next == due && !overdue) || !ledger_parser::valid_date(&next) {
                return Err("Cadence did not advance".into());
            }
            due = next;
        }
    }
    events.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.key.cmp(&b.key)));
    let mut confirmed_balance = opening;
    let mut estimated_balance = opening;
    let mut start = 0;
    while start < events.len() {
        let mut end_index = start + 1;
        while end_index < events.len() && events[end_index].date == events[start].date {
            end_index += 1;
        }
        for e in &events[start..end_index] {
            if e.kind != "detected_estimate" {
                confirmed_balance += e.amount;
            }
            if e.kind != "detected_estimate" || r.include_estimates {
                estimated_balance += e.amount;
            }
        }
        for e in &mut events[start..end_index] {
            e.confirmed_balance = confirmed_balance;
            e.estimated_balance = estimated_balance;
        }
        start = end_index;
    }
    Ok(Forecast {
        ledger_revision: revision,
        registry_revision,
        currency: r.currency.clone(),
        as_of: r.as_of,
        end,
        liquid_candidates: candidates.into_iter().collect(),
        selected_accounts: r.liquid_accounts,
        opening_cash: (!selected.is_empty()
            && selected.iter().all(|a| {
                historical.iter().any(|t| {
                    t.postings
                        .iter()
                        .any(|p| p.account == *a && p.currency == r.currency)
                })
            }))
        .then_some(opening),
        pending_cash_movement: pending,
        events,
        warnings,
        detected_obligations,
        settings: registry
            .subscriptions
            .into_iter()
            .filter(|m| m.currency == r.currency)
            .collect(),
    })
}
#[tauri::command]
pub fn get_cash_forecast(request: ForecastRequest) -> Result<Forecast, String> {
    let (txs, rev) = read_ledger()?;
    let (registry, rr) = registry_at(Path::new(crate::config::get("subscriptions_path")))?;
    forecast(&txs, rev, registry, rr, request)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(s: &str) -> Vec<Transaction> {
        ledger_parser::parse_with_currency(s, "USD").unwrap()
    }
    fn range(start: &str, end: &str) -> RangeRequest {
        RangeRequest {
            start: start.into(),
            end: end.into(),
            currency: "USD".into(),
            comparison_start: None,
            comparison_end: None,
        }
    }
    fn forecast_request() -> ForecastRequest {
        ForecastRequest {
            as_of: "2026-03-15".into(),
            horizon_days: 60,
            currency: "USD".into(),
            liquid_accounts: vec!["Assets:Bank:Checking".into()],
            include_estimates: true,
        }
    }
    fn bill(payee: &str, account: &str, amount: &str, due: &str) -> SubscriptionMetadata {
        let mut m = SubscriptionMetadata {
            key: obligations::obligation_currency_key(payee, account, "USD"),
            payee: payee.into(),
            account: account.into(),
            currency: "USD".into(),
            ..Default::default()
        };
        m.extra
            .insert("forecast_confirmed".into(), serde_json::json!(true));
        m.extra
            .insert("forecast_amount".into(), serde_json::json!(amount));
        m.extra
            .insert("forecast_due_date".into(), serde_json::json!(due));
        m.extra.insert(
            "forecast_cash_account".into(),
            serde_json::json!("Assets:Bank:Checking"),
        );
        m
    }
    const RENT:&str="2025-12-31 * \"Opening\"\n  Assets:Bank:Checking 100 USD\n  Equity:Opening -100 USD\n2026-01-01 * \"Rent\" \"Synthetic transaction\"\n  Expenses:Rent 10 USD\n  Assets:Bank:Checking -10 USD\n2026-02-01 * \"Rent\" \"Synthetic transaction\"\n  Expenses:Rent 10 USD\n  Assets:Bank:Checking -10 USD\n2026-03-01 * \"Rent\" \"Synthetic transaction\"\n  Expenses:Rent 10 USD\n  Assets:Bank:Checking -10 USD\n";
    #[test]
    fn history_labels_ranges_chronology_missing_evidence_and_trailing_24() {
        let mut txs=parse("2024-03-03 * \"March\"\n  Assets:Cash 2 USD\n  Equity:Opening -2 USD\n2023-12-31 * \"December\"\n  Assets:Cash 10 USD\n  Equity:Opening -10 USD\n2024-03-03 * \"Same day\"\n  Assets:Cash 1 USD\n  Equity:Opening -1 USD\n");
        txs.reverse();
        let a = analyze(&txs, "revision".into(), range("2023-10-01", "2024-03-03")).unwrap();
        assert_eq!(a.months.len(), 6);
        assert_eq!(a.months[0].evidence_status, "no_currency_evidence");
        assert!(a.months[0].closing_net_worth.is_none());
        assert_eq!(a.months[3].month, "2024-01");
        assert_eq!(a.months[3].closing_net_worth.unwrap().to_string(), "10");
        assert_eq!(a.months[3].evidence_status, "carried_forward_no_entries");
        assert_eq!(
            a.chart_history
                .iter()
                .map(|h| h.name.as_str())
                .collect::<Vec<_>>(),
            vec!["2023-12", "2024-01", "2024-02", "2024-03"]
        );
        assert_eq!(a.chart_history.last().unwrap().net_worth.to_string(), "13");
        let b = analyze(&txs, "revision".into(), range("2024-01-15", "2024-03-02")).unwrap();
        assert_eq!(b.chart_history.last().unwrap().net_worth.to_string(), "10");
        let mut many = String::new();
        for n in 0..30 {
            let d = obligations::add_months("2022-01-01", n).unwrap();
            many += &format!("{d} * \"Monthly\"\n  Assets:Cash 1 USD\n  Equity:Opening -1 USD\n");
        }
        let txs = parse(&many);
        let h = ledger_parser::history_for_range(&txs, "USD", None);
        assert_eq!(h.len(), 24);
        assert_eq!(h[0].name, "2022-07");
        assert_eq!(h[23].net_worth.to_string(), "30");
        assert!(analyze(&txs, "r".into(), range("2026-02-30", "2026-03-01")).is_err());
    }
    #[test]
    fn movement_reconciles_sources_refunds_transfers_card_payments_and_units() {
        let txs=parse("2025-12-01 * \"Opening\"\n  Assets:Bank 1000 USD\n  Equity:Opening -1000 USD\n2025-12-20 * \"Food\"\n  Expenses:Food 10 USD\n  Assets:Bank -10 USD\n2026-01-01 * \"Salary\"\n  Income:Salary -100 USD\n  Assets:Bank 100 USD\n2026-01-02 * \"Food\"\n  Expenses:Food 30 USD\n  Assets:Bank -30 USD\n2026-01-03 * \"Refund\"\n  Expenses:Food -5 USD\n  Assets:Bank 5 USD\n2026-01-04 * \"Card purchase\"\n  Expenses:Travel 10 USD\n  Liabilities:Card -10 USD\n2026-01-05 * \"Repayment\"\n  Liabilities:Card 10 USD\n  Assets:Bank -10 USD\n2026-01-06 * \"Investment transfer\"\n  Assets:Investments 100 USD\n  Assets:Bank -100 USD\n2026-01-07 * \"Capital\"\n  Assets:Bank 50 USD\n  Equity:Opening -50 USD\n2026-01-08 * \"Correction\"\n  Assets:Bank -2 USD\n  Equity:Correction 2 USD\n2026-01-09 * \"Other currency\"\n  Assets:Bank 999 EUR\n  Income:Salary -999 EUR\n");
        let mut r = range("2026-01-01", "2026-01-31");
        r.comparison_start = Some("2025-12-01".into());
        r.comparison_end = Some("2025-12-31".into());
        let a = analyze(&txs, "revision".into(), r).unwrap();
        let m = &a.months[0];
        assert_eq!(m.income.to_string(), "100");
        assert_eq!(m.spending.to_string(), "35");
        assert_eq!(m.equity_and_adjustments.to_string(), "48");
        assert_eq!(m.net_worth_change.unwrap().to_string(), "113");
        assert!(m.unexplained.unwrap().is_zero());
        assert_eq!(m.opening_net_worth.unwrap().to_string(), "990");
        assert_eq!(m.closing_net_worth.unwrap().to_string(), "1103");
        assert_eq!(a.sources.iter().filter(|s| s.internal_transfer).count(), 2);
        let food = a
            .categories
            .iter()
            .find(|c| c.account == "Expenses:Food")
            .unwrap();
        assert_eq!(food.difference.unwrap().to_string(), "15");
        let sum = food
            .source_ids
            .iter()
            .map(|id| a.sources.iter().find(|s| s.id == *id).unwrap().postings["Expenses:Food"])
            .fold(Money::default(), |a, b| a + b);
        assert_eq!(Some(sum), food.current);
    }
    #[test]
    fn forecast_reuses_patterns_and_deduplicates_imported_future_payments() {
        let txs = parse(&format!(
            "{RENT}2026-04-01 * \"Rent\" \"Synthetic transaction\"\n  Expenses:Rent 12 USD\n  Assets:Bank:Checking -12 USD\n"
        ));
        let f = forecast(
            &txs,
            "r".into(),
            SubscriptionRegistryPayload::default(),
            "missing".into(),
            forecast_request(),
        )
        .unwrap();
        assert_eq!(f.opening_cash.unwrap().to_string(), "70");
        assert_eq!(
            f.events
                .iter()
                .filter(|e| e.kind == "recorded_cleared")
                .count(),
            1
        );
        assert_eq!(
            f.events
                .iter()
                .filter(|e| e.kind == "detected_estimate")
                .count(),
            1
        );
        assert_eq!(f.events.last().unwrap().confirmed_balance.to_string(), "58");
        assert_eq!(f.events.last().unwrap().estimated_balance.to_string(), "48");
        assert!(f.warnings.iter().any(|w| w.contains("suppressed")));
        let mut r = forecast_request();
        r.include_estimates = false;
        let f = forecast(
            &txs,
            "r".into(),
            SubscriptionRegistryPayload::default(),
            "missing".into(),
            r,
        )
        .unwrap();
        assert_eq!(f.events.last().unwrap().estimated_balance.to_string(), "58");
    }
    #[test]
    fn confirmed_oneoff_cancelled_insufficient_history_and_already_paid() {
        let txs=parse(&format!("{RENT}2026-03-10 * \"Oneoff\" \"Synthetic transaction\"\n  Expenses:Oneoff 5 USD\n  Assets:Bank:Checking -5 USD\n2026-01-03 * \"Irregular\" \"Synthetic transaction\"\n  Expenses:Random 1 USD\n  Assets:Bank:Checking -1 USD\n2026-03-13 * \"Irregular\" \"Synthetic transaction\"\n  Expenses:Random 1 USD\n  Assets:Bank:Checking -1 USD\n"));
        let mut cancelled = bill("Cancelled", "Expenses:Cancelled", "9", "2026-04-10");
        cancelled
            .extra
            .insert("forecast_cancelled".into(), serde_json::json!(true));
        let reg = SubscriptionRegistryPayload {
            subscriptions: vec![
                bill("Annual", "Expenses:Insurance", "100", "2026-04-10"),
                bill("Oneoff", "Expenses:Oneoff", "5", "2026-03-10"),
                cancelled,
            ],
            ..Default::default()
        };
        let f = forecast(&txs, "r".into(), reg, "registry".into(), forecast_request()).unwrap();
        assert_eq!(f.events.iter().filter(|e| e.payee == "Annual").count(), 1);
        assert!(!f
            .events
            .iter()
            .any(|e| e.payee == "Oneoff" || e.payee == "Cancelled" || e.payee == "Irregular"));
        assert!(f.warnings.iter().any(|w| w.contains("irregular history")));
        assert!(f
            .events
            .iter()
            .any(|e| e.confirmed_balance < Money::default()));
    }
    #[test]
    fn stale_trial_price_change_currency_and_investment_exclusion() {
        let txs=parse(&format!("{}2026-03-02 * \"Price\" \"Synthetic transaction\"\n  Expenses:Rent 5 USD\n  Assets:Bank:Checking -5 USD\n2026-03-01 * \"Investments\" \"Synthetic transaction\"\n  Assets:Investments 500 USD\n  Equity:Opening -500 USD\n",RENT.replace("2026-03-01 * \"Rent\" \"Synthetic transaction\"\n  Expenses:Rent 10 USD\n  Assets:Bank:Checking -10 USD","2026-03-01 * \"Rent\" \"Synthetic transaction\"\n  Expenses:Rent 15 USD\n  Assets:Bank:Checking -15 USD")));
        let mut meta = SubscriptionMetadata {
            key: obligations::obligation_currency_key("Rent", "Expenses:Rent", "USD"),
            payee: "Rent".into(),
            account: "Expenses:Rent".into(),
            currency: "USD".into(),
            is_trial: true,
            trial_end_date: Some("2026-04-15".into()),
            ..Default::default()
        };
        meta.extra.insert(
            "forecast_cash_account".into(),
            serde_json::json!("Assets:Bank:Checking"),
        );
        let reg = SubscriptionRegistryPayload {
            subscriptions: vec![meta],
            ..Default::default()
        };
        let f = forecast(
            &txs,
            "r".into(),
            reg.clone(),
            "rr".into(),
            forecast_request(),
        )
        .unwrap();
        assert!(f
            .events
            .iter()
            .any(|e| e.trial_end.as_deref() == Some("2026-04-15")));
        assert!(f
            .events
            .iter()
            .any(|e| e.price_change.as_ref().is_some_and(|p| p.is_increase)));
        assert!(!f
            .liquid_candidates
            .iter()
            .any(|a| a.contains("Investments")));
        let mut r = forecast_request();
        r.liquid_accounts = vec!["Assets:Investments".into()];
        assert!(forecast(&txs, "r".into(), reg.clone(), "rr".into(), r).is_err());
        let mut r = forecast_request();
        r.as_of = "2026-09-15".into();
        let f = forecast(&txs, "r".into(), reg, "rr".into(), r).unwrap();
        assert!(f.events.iter().any(|e| e.stale));
    }
    #[test]
    fn forecast_settings_preserve_metadata_and_reject_conflicts() {
        let dir = std::env::temp_dir().join(format!(
            "recon-forecast-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("subscriptions.json");
        let mut m = bill("Rent", "Expenses:Rent", "10", "2026-04-01");
        m.notes = Some("Preserve me".into());
        let reg = SubscriptionRegistryPayload {
            subscriptions: vec![m],
            ..Default::default()
        };
        fs::write(&path, serde_json::to_vec(&reg).unwrap()).unwrap();
        let rev = store::read(&path).unwrap().1;
        let settings = ForecastSettings {
            payee: "Rent".into(),
            account: "Expenses:Rent".into(),
            currency: "USD".into(),
            confirmed: true,
            cancelled: true,
            cash_account: "Assets:Bank:Checking".into(),
            amount: Some(Money::parse("10.000000000000000001").unwrap()),
            due_date: Some("2026-04-01".into()),
            cadence: Some(RecurrenceCadence::Monthly),
        };
        let new_rev = save_settings_at(&path, settings.clone(), &rev).unwrap();
        assert_ne!(new_rev, rev);
        assert_eq!(
            registry_at(&path).unwrap().0.subscriptions[0]
                .notes
                .as_deref(),
            Some("Preserve me")
        );
        let bytes = fs::read(&path).unwrap();
        assert!(save_settings_at(&path, settings, &rev)
            .unwrap_err()
            .contains("conflict"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    #[test]
    fn forecasts_do_not_mix_units_or_invent_cash_and_reserve_confirmed_overdue_bills() {
        let txs=parse(&format!("{RENT}2026-03-01 * \"EUR funds\"\n  Assets:Bank:Checking 999 EUR\n  Equity:Opening -999 EUR\n"));
        let mut r = forecast_request();
        r.liquid_accounts.clear();
        let f = forecast(
            &txs,
            "r".into(),
            SubscriptionRegistryPayload::default(),
            "rr".into(),
            r,
        )
        .unwrap();
        assert!(f.opening_cash.is_none());
        let f = forecast(
            &txs,
            "r".into(),
            SubscriptionRegistryPayload::default(),
            "rr".into(),
            forecast_request(),
        )
        .unwrap();
        assert_eq!(f.opening_cash.unwrap().to_string(), "70");
        assert!(f.events.iter().all(|e| e.amount.to_string() == "-10"));
        let mut confirmed = bill("Rent", "Expenses:Rent", "10", "2026-03-10");
        confirmed
            .extra
            .insert("forecast_cadence".into(), serde_json::json!("monthly"));
        let reg = SubscriptionRegistryPayload {
            subscriptions: vec![confirmed],
            ..Default::default()
        };
        let f = forecast(&txs, "r".into(), reg, "rr".into(), forecast_request()).unwrap();
        assert!(f
            .events
            .iter()
            .any(|e| e.confidence == "user_confirmed_overdue" && e.note.contains("2026-03-10")));
    }
    #[test]
    fn category_comparisons_do_not_invent_zero_before_first_currency_evidence() {
        let txs = parse(RENT);
        let mut r = range("2020-01-01", "2020-01-31");
        r.comparison_start = Some("2026-01-01".into());
        r.comparison_end = Some("2026-01-31".into());
        let a = analyze(&txs, "r".into(), r).unwrap();
        let rent = a
            .categories
            .iter()
            .find(|c| c.account == "Expenses:Rent")
            .unwrap();
        assert!(rent.current.is_none());
        assert!(rent.difference.is_none());
        assert_eq!(rent.previous.unwrap().to_string(), "10");
    }
}
