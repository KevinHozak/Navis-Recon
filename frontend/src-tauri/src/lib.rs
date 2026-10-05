mod checklist;
mod ledger_parser;
mod ledger_review;
mod statements;
mod sync;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            checklist::get_vault_checklist,
            checklist::save_vault_checklist,
            ledger_review::get_pending_ledger,
            ledger_review::get_ledger_categories,
            ledger_review::approve_ledger_transaction,
            ledger_parser::get_ledger_summary,
            ledger_parser::get_ledger_history,
            ledger_parser::get_ledger_transactions,
            statements::get_statement_catalog,
            statements::open_statement_file,
            sync::start_ledger_sync,
            sync::commit_ledger_sync,
            ping
        ])
        .run(tauri::generate_context!())
        .expect("error while running Navis-Recon");
}

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}
