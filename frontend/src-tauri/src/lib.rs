mod checklist;
mod config;
mod ledger_parser;
mod ledger_review;
mod money;
mod obligations;
mod records;
mod review_audit;
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
            ledger_review::get_ledger_approval_history,
            ledger_review::undo_ledger_approval,
            ledger_parser::get_ledger_summary,
            ledger_parser::get_ledger_source_state,
            ledger_parser::get_ledger_history,
            ledger_parser::get_ledger_transactions,
            obligations::get_recurring_obligations,
            obligations::get_subscription_registry,
            obligations::save_subscription_metadata,
            obligations::save_subscription_registry_payload,
            records::get_records,
            records::save_record,
            records::delete_record,
            records::get_records_radar,
            records::reveal_record_file,
            statements::get_statement_catalog,
            statements::open_statement_file,
            sync::start_ledger_sync,
            sync::get_sync_run,
            sync::commit_ledger_sync,
            sync::cancel_sync_run,
            ping
        ])
        .run(tauri::generate_context!())
        .expect("error while running Navis-Recon");
}

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}
