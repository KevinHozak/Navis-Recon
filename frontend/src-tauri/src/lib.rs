mod checklist;
mod config;
mod document_store;
mod household_analysis;
mod ledger_parser;
mod ledger_review;
mod managed_import;
mod money;
mod monthly_close;
mod obligations;
mod reconciliation;
mod records;
mod review_audit;
mod statements;
mod sync;
mod sync_process;
mod sync_receipts;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let window_config = app.config().app.windows.first().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Navis-Recon main window configuration is missing",
                )
            })?;

            tauri::WebviewWindowBuilder::from_config(app.handle(), window_config)?
                .initialization_script(include_str!("../startup_migration.js"))
                .build()?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            checklist::get_vault_checklist,
            checklist::save_vault_checklist,
            reconciliation::get_reconciliations,
            reconciliation::save_reconciliation,
            reconciliation::compare_reconciliation,
            monthly_close::get_monthly_close,
            monthly_close::save_monthly_close,
            ledger_review::get_pending_ledger,
            ledger_review::get_ledger_categories,
            ledger_review::approve_ledger_transaction,
            ledger_review::get_ledger_approval_history,
            ledger_review::undo_ledger_approval,
            ledger_parser::get_ledger_summary,
            ledger_parser::get_ledger_source_state,
            ledger_parser::get_ledger_history,
            household_analysis::get_household_analysis,
            household_analysis::get_cash_forecast,
            household_analysis::save_forecast_settings,
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
            sync::get_sync_receipts,
            sync::get_sync_outcomes,
            sync::reconcile_ledger_sync,
            sync::retry_ledger_sync,
            sync::commit_ledger_sync,
            sync::cancel_sync_run,
            sync::reveal_sync_source,
            ping
        ])
        .run(tauri::generate_context!())
        .expect("error while running Navis-Recon");
}

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}
