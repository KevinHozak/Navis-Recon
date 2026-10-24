# Architecture: Navis-Recon

Navis-Recon is the dedicated, standalone financial command center for households. Designed as a high-precision interface for `Sovereign.beancount`, it operates as a packaged Tauri v2 desktop application powered by a native Rust accounting engine.

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
* **Ledger File:** `C:/Example/Sovereign.beancount`
* **Audit Checklist:** `C:/Example/checklist.json`

Checklist mutations use atomic replacement. Ledger review approvals use an app-wide serialized read/compare/write through the open source file handle; they do not use fixed-name staging files. On Windows the handle denies other opens requesting write or delete access until the write completes. Before changing bytes, the writer creates a unique same-directory recovery copy and flushes it with `sync_all`; caught errors trigger restoration through the still-exclusive handle. The in-place ledger write is not crash-atomic. Ledger approvals fail closed on non-Windows platforms because external writers cannot be excluded there.

---

## 🦀 Native Rust Accounting Engine (`src-tauri/src/`)

All ledger evaluation, line modifications, checklist operations, and sync orchestration are implemented natively in Rust via Tauri IPC commands.

### 1. Ledger Review & Triage (`ledger_review.rs`)
* `get_pending_ledger`: Scans `Sovereign.beancount` line by line for transactions marked with the ` ! ` flag. Each row carries a SHA-256 identity of its complete raw transaction block, its source line, and a SHA-256 revision of the whole raw ledger. Identical duplicate transactions remain distinct by line plus identity.
* `get_ledger_categories`: Scans `open Expenses:` and `open Income:` directives to compile an alphabetical, deduplicated list of valid categories for autocomplete.
* `approve_ledger_transaction`: Requires line number, transaction identity, whole-ledger revision, and category. Under an app-wide approval lock it opens the source for exclusive Windows sharing, verifies the revision and exact pending transaction block, then changes only the flag and first supported direct posting's account token. Any stale revision rejects the write and the UI retains the row with a refresh action. The complete pending list is refreshed after a successful approval because the revision changes. Complex posting selection belongs to RECON-S005. Before writing, a unique recovery copy is created and synced beside the ledger; caught write errors restore original bytes. Writes preserve unrelated bytes and line endings, but are not crash-atomic. Approvals are supported only on Windows desktop.

### 2. Ledger Parsing & Telemetry (`ledger_parser.rs`)
The native reader implements a deliberately small Beancount subset with exact decimal accounting. It accepts blank lines, full-line comments beginning with `;`, a fully quoted `option "title" "..."`, dated `YYYY-MM-DD open ACCOUNT [UNIT]` directives, and dated transactions with one or two quoted descriptions. Header and option tokens require whitespace separators. Quoted text cannot contain escaped quotes or backslashes; inline comments are accepted only as standalone comment lines, not trailing posting/header comments.

#### Amount precision and bounds

- Amounts use a signed fixed-point `Money` value with scale 18 (18 fractional decimal places); IPC serializes canonical decimal strings, never floating-point JSON numbers.
- Explicit plain-decimal posting amounts are bounded to absolute `1e15`; total ledger movement and per-account movement are bounded to `1e15` as well.
- A transaction must balance exactly for each unit independently. Units are never netted against one another.
- Summary totals, category balances, history rows, transaction rows, and obligation monetary values retain their unit and exact decimal string representation.
- The summary's selected-unit totals describe only that unit. Other held units are listed separately with per-unit asset, liability, and net-worth totals as unvalued holdings; no conversion is implied or performed without explicit valuation data.
- Obligation means, even-sample medians, and percentage metrics use scale-18 fixed-point arithmetic. A nonrepresentable result is rounded half away from zero at the eighteenth decimal place.
- The frontend converts exact strings to `Number` only at chart-coordinate boundaries. Display and tooltip values use the original exact decimal strings; display rounding is explicit and limited to forecast estimates.

Supported account roots are Assets, Liabilities, Equity, Income, and Expenses; accounts require at least one segment after the root, each segment starts with an ASCII uppercase letter or digit and continues with ASCII letters, digits, or hyphens. The parser rejects includes, inferred amounts, costs, prices, plugins, every option except title, tags/links, metadata, malformed dates/descriptions/accounts, unindented postings, and every other unsupported directive. This is subset validation, not certification by the official Beancount parser. An unsupported or malformed construct rejects the whole ledger for summary, history, account drilldown, and recurring-obligation analysis. Diagnostics include the configured filename and 1-based line number. Semantic failures return a `partial:` error with no partial totals/rows; read and UTF-8 failures return `unavailable:` with no totals. Empty or comment-only ledgers are unavailable rather than a validated zero. Successful summaries include `status: "validated"` and an empty diagnostics list. History, drilldown, and obligations only succeed after the same complete validation. The frontend displays parser/read diagnostics instead of hiding them behind network errors or stale totals. No ledger content is written by these telemetry paths.

* `get_ledger_summary`: Evaluates `Sovereign.beancount` into transaction postings and computes live real-time balances for:
  * Net Worth (`total_assets + total_liabilities`)
  * Total Assets and Liabilities
  * Categorized leaf breakdowns (`Assets:Bank`, `Assets:Retirement`, `Assets:Investments`, `Liabilities:CreditCard`, `Assets:Housing`, `Assets:Cash`).
* `get_ledger_history`: Calculates monthly history snapshots for the trailing 24 months, tracking net worth, retirement accounts, stocks, and HSA metrics over time.
* `get_ledger_transactions`: Retrieves and sorts transactions by date matching an account prefix (e.g. for account-level drill-downs).

#### Summary provenance, coverage, and source freshness (RECON-S003)

Each successful `get_ledger_summary` response remains the existing S002 summary, with its exact monetary decimal strings and these additional root fields:

| Field | Meaning |
| --- | --- |
| `ledger_revision` | Lowercase SHA-256 hex digest of the source file's raw bytes. |
| `read_at_unix_ms` | UTC epoch milliseconds when the source snapshot was read. This is distinct from the last transaction date. |
| `latest_transaction_date` | Latest transaction date in `YYYY-MM-DD` form, or an empty string when no transaction exists. It does not determine freshness. |
| `valuation_basis` | `native_units_no_conversion`; the selected unit is reported without FX conversion. |
| `validation_status` | `validated` only for a complete successful parse. A failure rejects the summary and returns no financial totals. |
| `coverage` | Direct `{ account, currency, balance, included_in_net_worth, reason }` rows. Balances are signed decimal strings. Rows include zero balances and account parents and children. |
| `transaction_count`, `posting_count` | Counts in the validated source. |
| `account_count` | Count of unique account names across all units (a name repeated in several units counts once). |
| `selected_included_account_count`, `selected_excluded_account_count` | Counts of account/unit coverage rows in the selected currency only. |

Coverage reasons are `included_selected_unit_asset`, `included_selected_unit_liability`, `unvalued_unit`, and `outside_net_worth`. Each exact account/unit pair appears once; included asset and liability reasons must match their account roots, and selected-unit Assets/Liabilities cannot be marked outside net worth. Assets and liabilities retain their signed balances. The frontend sums only included rows in the selected unit, separately reconciles asset and liability subtotals, then checks the combined value against `total_assets`, `total_liabilities`, and `net_worth` using exact decimal arithmetic. A mismatch or malformed/missing provenance suppresses the financial display and reports that the summary cannot be validated. Excluded equity, income, and expense rows are identified as outside net worth. Other units are shown as excluded without valuation; their separate existing per-unit disclosures remain separate and are never combined.

The read-only `get_ledger_source_state(expected_revision)` IPC command returns `{ expected_revision, observed_revision, state, checked_at_unix_ms, error }`, where `state` is `current`, `changed`, or `unavailable`. Current and changed replies require a lowercase or uppercase SHA-256 hex observed revision and a null error; current must match the expected revision and changed must differ. Unavailable requires a null observed revision and useful error text. Probe timestamps and summary read timestamps are safe nonnegative epoch milliseconds within JavaScript Date's range; the latest transaction date must be a real calendar date (or empty when no transaction exists). Tauri receives the JavaScript argument as `expectedRevision`. The overview checks immediately after loading a summary, on window focus, and every 60 seconds while mounted. While a follow-up check runs, the UI keeps the last confirmed changed/unavailable state and diagnostic visible and adds checking progress; only a coherent current result clears a stale warning. Unknown states, malformed replies, or rejected checks conservatively show source unavailable and identify the last successful snapshot. Manual Refresh clears the displayed financial data while loading and reads a new complete summary; a failed refresh leaves no old totals displayed. The UI does not infer staleness from transaction age. Summary read/parse failures reject instead of returning a numeric zero; partial diagnostics point to the invalid source location, while unavailable diagnostics direct attention to the configured file path/access or explain an empty ledger.

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

### Exact monetary IPC and supported ledger input

Native ledger and pending-review amounts cross IPC as decimal strings with a separate unit token. Pending reviews require a plain decimal amount followed by an explicit uppercase unit (letters/digits/hyphens); malformed or out-of-range pending amounts return a line-numbered diagnostic. Approval continues to modify only the existing transaction status/category text.

Obligation medians and means use fixed 18-place decimal arithmetic. `median_is_estimate` and `mean_is_estimate` mark rounded results. Price deltas remain exact; `percentage_delta` may be `null` if the exact ratio exceeds supported Money range, and the rounded ratio is marked by `percentage_is_estimate`. The UI labels estimates and unavailable percentages. The supported financial ledger subset requires explicit currency on every posting, uppercase unit tokens, plain decimal values with at most 18 fractional digits and magnitude at most 1e15; it does not perform implicit commodity inference or currency conversion.
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
2. **Directory Structure:** No financial PDFs in the root. Everything resides inside `C:/Example/`:
   - `Statements/`: Archive of all income/expense reports.
   - `Policies/`: Insurance, Taxes, and long-term contracts.
   - `Sovereign.beancount`: Active tracking sheets and reconciliation reports.
3. **Audit First:** Every monthly reconcile requires cross-referencing between the **Master Checklist** and bank activity.
4. **Security:** Never store plaintext passwords. Use Bitwarden integration for all financial credentials.
