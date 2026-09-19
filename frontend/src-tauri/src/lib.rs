mod checklist;
mod ledger_review;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            checklist::get_vault_checklist,
            checklist::save_vault_checklist,
            ledger_review::get_pending_ledger,
            ledger_review::get_ledger_categories,
            ledger_review::approve_ledger_transaction,
            ping
        ])
        .run(tauri::generate_context!())
        .expect("error while running Navis-Recon");
}

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}
