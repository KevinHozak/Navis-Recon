# Architecture: Navis-Recon

Navis-Recon is the dedicated, standalone financial command center for the Hozak household. Designed as a high-precision interface for `Sovereign.beancount`, it operates as a packaged Tauri v2 desktop application powered by a native Rust accounting engine.

---

## 🏛️ System Overview & Architecture Flow

Unlike traditional client-server setups, Navis-Recon runs entirely locally without a Python web server or open HTTP ports in production:

```
┌────────────────────────────────────────────────────────┐
│               Frontend (React 19 + Vite 8)             │
│   • Views: OverviewWidget, LedgerReviewWidget, etc.   │
│   • apiClient: Invokes Tauri commands & listens to bus │
└───────────────────────────┬────────────────────────────┘
                            │ Tauri IPC (invoke / emit)
┌───────────────────────────▼────────────────────────────┐
│              Tauri Core (Rust Native Backend)          │
│   • src-tauri/src/lib.rs (Command Router & Ping)       │
│   • src-tauri/src/ledger_parser.rs (AST & Balances)    │
│   • src-tauri/src/ledger_review.rs (Triage & Approvals)│
│   • src-tauri/src/checklist.rs (Atomic JSON Storage)   │
│   • src-tauri/src/sync.rs (Harvester Subprocess & Bus) │
└─────────────┬───────────────────────────┬──────────────┘
              │ Direct File I/O           │ Subprocess Streaming
┌─────────────▼─────────────┐ ┌───────────▼──────────────┐
│       Navis-Vault         │ │      Codex Harvester     │
│  • Sovereign.beancount    │ │  • finance_processor.py │
│  • checklist.json         │ │  • Scans inbox/downloads │
└───────────────────────────┘ └──────────────────────────┘
```

---

## 📂 The Data Source (Vault Protocols)

The ultimate source of truth is stored in the Navis-Vault ledger manifold:
* **Ledger File:** `G:\My Drive\Navis-Vault\Ledger\Sovereign.beancount`
* **Audit Checklist:** `G:\My Drive\Navis-Vault\Ledger\checklist.json`

All ledger and checklist mutations use atomic write patterns (`.tmp` staging with native Windows `MoveFileExW` replacement flags) to guarantee zero corruption or partial writes across cloud sync cycles.

---

## 🦀 Native Rust Accounting Engine (`src-tauri/src/`)

All ledger evaluation, line modifications, checklist operations, and sync orchestration are implemented natively in Rust via Tauri IPC commands.

### 1. Ledger Review & Triage (`ledger_review.rs`)
* `get_pending_ledger`: Scans `Sovereign.beancount` line by line for transactions marked with the ` ! ` flag. Extracts date, payee, narration, suggested expense category, amount, and source line number.
* `get_ledger_categories`: Scans `open Expenses:` and `open Income:` directives to compile an alphabetical, deduplicated list of valid categories for autocomplete.
* `approve_ledger_transaction`: Safely modifies the transaction on disk by replacing the ` ! ` flag with ` * ` and updating the posting line's category. Uses atomic file replacement to preserve original whitespace, comments, and structure.

### 2. Ledger Parsing & Telemetry (`ledger_parser.rs`)
* `get_ledger_summary`: Evaluates `Sovereign.beancount` into transaction postings and computes live real-time balances for:
  * Net Worth (`total_assets + total_liabilities`)
  * Total Assets and Liabilities
  * Categorized leaf breakdowns (`Assets:Bank`, `Assets:Retirement`, `Assets:Investments`, `Liabilities:CreditCard`, `Assets:Housing`, `Assets:Cash`).
* `get_ledger_history`: Calculates monthly history snapshots for the trailing 24 months, tracking net worth, retirement accounts, stocks, and HSA metrics over time.
* `get_ledger_transactions`: Retrieves and sorts transactions by date matching an account prefix (e.g. for account-level drill-downs).

### 3. Vault Checklist (`checklist.rs`)
* `get_vault_checklist`: Reads and parses `checklist.json` (supporting optional UTF-8 BOM encoding) into structured weekly triage tasks and monthly statement grid items.
* `save_vault_checklist`: Serializes the checklist payload with pretty formatting and writes it atomically to disk.

### 4. Harvester & Sync Pipeline (`sync.rs`)
* `start_ledger_sync`: Spawns the Codex Harvester (`finance_processor.py --analyze`) in a background blocking task and streams JSON log/match events over Tauri event channels (`sync:log`, `sync:match`, `sync:unmatched`, `sync:error`, `sync:done`).
* `commit_ledger_sync`: Executes `finance_processor.py --commit`, piping approved file actions via stdin and returning execution results.

### 5. IPC Diagnostic (`lib.rs`)
* `ping`: Returns `"pong"` to allow the frontend to verify desktop IPC health.

---

## 🎨 The Frontend (Vite + React)

Located in `frontend/`, the UI is a React 19 single-page application styled with Tailwind CSS v4 and Framer Motion:

* **App.jsx**: The master layout and navigation shell. Connects to the Tauri IPC backend via `apiClient.js` and updates pending transaction badges.
* **apiClient.js**: The IPC boundary. Inspects the runtime (`window.__TAURI_INTERNALS__`) and routes calls through `@tauri-apps/api/core` (`invoke`) and `@tauri-apps/api/event` (`listen`).
* **OverviewWidget.jsx**: Executive net worth summary, asset allocation cards, and balance curves.
* **LedgerReviewWidget.jsx**: Tactical transaction triage queue with fast keyboard approvals and category overrides.
* **CategorySelector.jsx**: Searchable fuzzy category picker for quick reclassification.
* **VaultChecklist.jsx**: Monthly statement reconciliation grid and weekly cadence triage tracker.
* **WealthDashboardWidget.jsx**: Historical net worth chart and long-term trend analysis.
* **SyncWidget.jsx**: Interactive terminal streaming real-time event updates from the harvester bridge.

---

## 🚀 Execution & Packaging

* **Desktop Dev Mode**: `npm run desktop` inside `frontend/` (launches Vite + Tauri dev window).
* **Desktop Production Build**: `npm run desktop:build` inside `frontend/` (compiles Rust binaries, bundles release assets, and publishes `Navis-Recon.exe` to repository root).
* **Launch Desktop**: `start.bat` in root (launches packaged `Navis-Recon.exe`).
* **Terminate Desktop**: `stop.bat` in root (forcefully terminates active `navis-recon.exe` processes).

*(Note: Legacy FastAPI server files and batch runners are archived under `docs/archive/` and are not part of the active runtime).*

---

## 🛡️ Operational Mandates (Vault Protocol)

1. **Strict Chronology:** Statements must be archived using the format `[Institution] [YYYY-MM-DD].pdf`.
2. **Directory Structure:** No financial PDFs in the root. Everything resides inside `G:\My Drive\Navis-Vault\Ledger\`:
   - `Statements/`: Archive of all income/expense reports.
   - `Policies/`: Insurance, Taxes, and long-term contracts.
   - `Sovereign.beancount`: Active tracking sheets and reconciliation reports.
3. **Audit First:** Every monthly reconcile requires cross-referencing between the **Master Checklist** and bank activity.
4. **Security:** Never store plaintext passwords. Use Bitwarden integration for all financial credentials.

