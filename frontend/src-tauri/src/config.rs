//! Private runtime settings are kept outside tracked source files.
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};
static SETTINGS: OnceLock<BTreeMap<String, String>> = OnceLock::new();

pub fn get(key: &str) -> &str {
    SETTINGS
        .get_or_init(|| {
            let explicit = std::env::var_os("NAVIS_RECON_CONFIG").map(PathBuf::from);
            let path = explicit.clone().or_else(|| {
                let beside_exe = std::env::current_exe()
                    .ok()?
                    .parent()?
                    .join("config.local.json");
                if beside_exe.is_file() {
                    return Some(beside_exe);
                }
                std::env::current_dir()
                    .ok()?
                    .ancestors()
                    .map(|p| p.join("config.local.json"))
                    .find(|p| p.is_file())
            });
            match path {
                Some(path) => {
                    let text = std::fs::read_to_string(path)
                        .expect("Cannot read Navis-Recon local configuration");
                    serde_json::from_str(text.trim_start_matches('\u{feff}'))
                        .expect("Navis-Recon configuration must be a JSON object of string values")
                }
                None => BTreeMap::new(),
            }
        })
        .get(key)
        .map(String::as_str)
        .unwrap_or_else(|| default_value(key))
}

fn default_value(key: &str) -> &'static str {
    match key {
        "ledger_path" => "data/ledger.beancount",
        "ledger_currency" => "USD",
        "checklist_path" => "data/checklist.json",
        "subscriptions_path" => "data/subscriptions.json",
        "vault_root" => "data",
        "inventory_path" => "data/Records/inventory.json",
        "statements_path" => "data/Ledger/Statements",
        "statement_archive_path" => "data/Statements",
        "sync_inbox_path" => "data/Statements/Inbox",
        "sync_counter_account" => "Assets:Bank:Checking",
        "sync_analysis_timeout_seconds" => "600",
        "harvester_script" => "scripts/finance_processor.py",
        "retirement_401k_account" => "Assets:Retirement:401k",
        "ira_primary_account" => "Assets:Retirement:IRA:Primary",
        "ira_secondary_account" => "Assets:Retirement:IRA:Secondary",
        "stocks_secondary_account" => "Assets:Investments:Stocks:Secondary",
        _ => panic!("Unknown configuration key"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn defaults_do_not_include_personal_paths() {
        assert_eq!(super::default_value("ledger_path"), "data/ledger.beancount");
        assert_eq!(
            super::default_value("ira_primary_account"),
            "Assets:Retirement:IRA:Primary"
        );
    }
}
