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

Successful approval and undo history is stored in the Windows user's private `%LOCALAPPDATA%/Navis-Recon/approval-history/<sha256-canonical-ledger-path>/` directory. It is namespaced by canonical ledger identity; the resolved store must be outside a Git checkout, and failure to resolve a safe private store blocks mutations. Journal records include raw transaction before/after bytes for transaction-scoped undo and never expose those snapshots or private paths over normal IPC. Intents and committed records are each written to unique, create-new staging files, flushed with `sync_all`, then published with Windows write-through semantics without replacing an existing destination. Both approval and Undo use the same audited-write sequence: publish the intent, create and sync a uniquely owned whole-ledger recovery copy beside the source, write and sync the ledger, publish the committed audit record, then remove the intent and owned recovery copy. A failed audit publication triggers restoration and sync through the still-exclusive source handle. The full original copy is retained until audit publication or verified restoration; failed/partial restoration reports an `outcome uncertain; recovery required` error with its actionable path and keeps both copy and intent. Once the ledger and committed audit record are durable, approval/Undo returns success even if cleanup fails; it emits a local cleanup warning and retains the intent and recovery copy when intent removal fails. On restart, while holding exclusive ledger access, before-revision intents are removed before new writes are allowed; failure to remove one blocks mutations. After-revision intents are finalized once, and matching committed intents are validated against the exact committed operation before best-effort cleanup. Cleanup failure does not block history reads or later unrelated mutations. Other revisions and corrupt or inconsistent records block automatic recovery without rewriting the ledger. Recovery copies are owned by the operation that created them; cleanup never follows a path supplied by a persisted record. A process stop after a source write can leave an orphaned recovery copy even when later history reconciliation succeeds; orphaned copies may need manual review and cleanup. This protocol makes no power-loss atomicity claim spanning the ledger and private history files.

Undo accepts only a committed approval record ID. It locates one exact matching transaction at transaction boundaries, verifies the recorded approval transition, and restores only that transaction's recorded before bytes while preserving unrelated source bytes. An edited or duplicated target conflicts. Successful undo is durably linked as a separate history record, so repeat undo remains rejected after restart.

---

## 🦀 Native Rust Accounting Engine (`src-tauri/src/`)

All ledger evaluation, line modifications, checklist operations, and sync orchestration are implemented natively in Rust via Tauri IPC commands.

### 1. Ledger Review & Triage (`ledger_review.rs`)
* `get_pending_ledger`: Locates exactly one explicit-money Expenses or Income posting anywhere in each pending transaction block. Each row carries raw block identity, source line, ledger revision, and categories eligible for its transaction date and posting unit. Malformed or unsupported pending structures reject the whole list with a source-line error.
* `get_ledger_categories`: Returns a sorted, unit-neutral catalog of category accounts found in valid top-level open directives; it does not invent a current date or decide whether a category is eligible. The per-row `permitted_categories` snapshot is authoritative and honors transaction date, exclusive close date, and optional open currency restrictions.
* `approve_ledger_transaction`: Requires line number, transaction identity, whole-ledger revision, and category. Under an app-wide approval lock it verifies the revision and exact block, re-runs the shared posting locator and validates the category against the same locked snapshot's dated account lifetimes, then changes only the header flag and exact account-token span. Duplicate category splits, inferred amounts, annotations, malformed rows and unsupported transaction structures require manual review. Before writing, a unique recovery copy is created and synced beside the ledger; caught write errors restore original bytes. Writes preserve unrelated bytes and line endings, but are not crash-atomic. Approvals are supported only on Windows desktop.

Review supports simple transaction blocks containing comments, blank separators, lowercase metadata keys with quoted text, TRUE/FALSE, valid dates, finite plain fixed decimals, or simple #tag tokens (each may have an outside semicolon comment), and explicit three-token postings (account, plain Money, unit), with an optional trailing semicolon comment. It does not balance postings or implement full Beancount. Category account lifetime directives must be top-level `YYYY-MM-DD open Expenses:... [UNIT]` / `Income:... [UNIT]` and matching `YYYY-MM-DD close ACCOUNT`; close date is exclusive. Events resolve in date order, while duplicate account/date lifecycle events are ambiguous regardless of intervening accounts. Valid non-category account lifetimes are allowed but never grant category permission. Metadata strings preserve source bytes; escapes and broader Beancount scalar forms are unsupported. Includes, plugins, malformed lifetimes, malformed transaction-like headers, and unsupported transaction forms fail closed; include files are never read.

### 2. Ledger Parsing & Telemetry (`ledger_parser.rs`)
The native reader implements a deliberately bounded Beancount subset with exact decimal accounting. It accepts blank lines, full-line comments beginning with `;`, fully quoted `title` and single-value `operating_currency` options (one currency per option line), dated `open` directives, `*`, `!`, and `P` transaction flags, explicit postings, `pad` and `balance` directives, and the three-string `custom "fava-option"` metadata directive. It also accepts one or two quoted transaction descriptions. Header and option tokens require whitespace separators. Quoted text cannot contain escaped quotes or backslashes; inline comments are accepted only as standalone comment lines, not trailing posting/header comments. Unsupported options and custom directives still fail closed.

Financial entries are processed by date, with balance assertions checked at the start of the day and transaction postings applied afterward. Balance assertions include descendant accounts. Their default tolerance uses the final printed decimal place, matching Beancount's default multiplier; explicit `~ TOLERANCE` values are exact. A pad can be used for a later assertion in its account subtree, generating a balanced synthetic transaction dated at the pad directive. Every assertion is rechecked after padding is inserted, including the asserted account scope. Mismatched assertions, unused or ambiguous pads, unknown assertion accounts, and unsupported tolerance forms reject the whole ledger. Synthetic padding contributes to telemetry totals and transaction counts; review and approval continue to locate only source text blocks, so generated entries cannot become approval candidates.

#### Amount precision and bounds

- Amounts use a signed fixed-point `Money` value with scale 18 (18 fractional decimal places); IPC serializes canonical decimal strings, never floating-point JSON numbers.
- Explicit plain-decimal posting amounts are bounded to absolute `1e15`; total ledger movement and per-account movement are bounded to `1e15` as well.
- A transaction must balance exactly for each unit independently. Units are never netted against one another.
- Summary totals, category balances, history rows, transaction rows, and obligation monetary values retain their unit and exact decimal string representation.
- The summary's selected-unit totals describe only that unit. Other held units are listed separately with per-unit asset, liability, and net-worth totals as unvalued holdings; no conversion is implied or performed without explicit valuation data.
- Obligation means, even-sample medians, and percentage metrics use scale-18 fixed-point arithmetic. A nonrepresentable result is rounded half away from zero at the eighteenth decimal place.
- The frontend converts exact strings to `Number` only at chart-coordinate boundaries. Display and tooltip values use the original exact decimal strings; display rounding is explicit and limited to forecast estimates.

Supported account roots are Assets, Liabilities, Equity, Income, and Expenses; accounts require at least one segment after the root, each segment starts with an ASCII uppercase letter or digit and continues with ASCII letters, digits, or hyphens. The parser rejects includes, inferred amounts, costs, prices, plugins, tags/links, unsupported metadata, malformed dates/descriptions/accounts, unindented postings, and every other unsupported directive. This is subset validation, not certification by the official Beancount parser. An unsupported or malformed construct rejects the whole ledger for summary, history, account drilldown, and recurring-obligation analysis. Diagnostics include the configured filename and 1-based line number. Semantic failures return a `partial:` error with no partial totals/rows; read and UTF-8 failures return `unavailable:` with no totals. Empty or comment-only ledgers are unavailable rather than a validated zero. Successful summaries include `status: "validated"` and an empty diagnostics list. History, drilldown, and obligations only succeed after the same complete validation. The frontend displays parser/read diagnostics instead of hiding them behind network errors or stale totals. No ledger content is written by these telemetry paths.

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
* `start_ledger_sync`: Reserves the sole in-memory current run while holding the coordinator lock, then starts `finance_processor.py --analyze` in a blocking worker. Runs have a backend ID, monotonic snapshot revision, per-run event sequence, and `idle`, `analyzing`, `review`, `committing`, `complete`, `failed`, or `cancelled` status.
* `get_sync_run`: Returns the authoritative snapshot, including retained logs, matches, unmatched items, errors, and the last result. `sync:state` carries a snapshot; all `sync:log`, `sync:match`, `sync:unmatched`, `sync:error`, and `sync:done` events wrap their existing payload under `payload` and carry backend `run_id`, `revision`, and `sequence`. Backend identity replaces any child-supplied identity. Older workers cannot update a later run.
* Analysis snapshots carry `analysis_outcome: null | success | partial | failed`. Exit zero with no diagnostics produces success, including an empty analysis. Filename-scoped errors or unmatched files produce partial review only when at least one match remains eligible. Eligibility excludes every match from a filename with an error/unmatched diagnostic and all matches sharing a filename with another match; diagnostics and excluded matches remain visible. Diagnostics without eligible matches, global errors, malformed JSON-like protocol records, malformed matches, output read failures, spawn/wait failures, and nonzero exits fail analysis; valid matches remain visible after failure. Plain stdout lines are logs. Stderr lines are retained as warning logs and do not make analysis fatal.
* Matches require nonempty `filename` and `new_name`; optional display fields are rendered only when scalar. Stable `match_id` values hash the canonical child payload after child identity fields are removed, so replayed matches deduplicate.
* `commit_ledger_sync`: Accepts only selected eligible match ID strings. Under the coordinator lock it verifies current run/review/positive analysis outcome, rejects duplicate, unknown, and blocked IDs, resolves immutable match payloads, and reserves `committing` before passing those payloads to the worker. The worker receives the analyzed payload with backend identity fields removed. Structured commit errors and nonzero exits produce `failed` without replacing the analysis outcome.
* `cancel_sync_run`: Abandons a current review or requests cancellation for the current analysis/commit. Analysis cancellation targets the run-owned direct child and remains pending until its worker reports terminal state. Managed commit checks cancellation between items; an in-flight ledger/archive operation is allowed to finish, and later items receive durable no-effect failure outcomes when possible. Confirmed item effects remain recorded. The analyzer is assigned to a private Windows Job Object with kill-on-close, bounded output and a configurable deadline (600 seconds by default, 1–3600 seconds). The worker retains run ownership until root and job accounting confirm termination. Run receipts are local, redacted, strict-schema records; selected IDs and prepared plans remain process memory only.
* `sync:resources_changed`: A run-scoped event is emitted after commit/retry when at least one managed item is confirmed succeeded, including partial outcomes. The App shell listens independently of Sync view mounting and refreshes ledger-derived badges and statement/checklist views.

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
* **SyncWidget.jsx**: Reconciles subscribed run events with the authoritative native snapshot on mount; the controller rejects older revisions and stale run events, and retains review results across view remounts.

---

## 🚀 Execution & Packaging

* **Desktop Dev Mode**: `npm run desktop` inside `frontend/` (launches Vite + Tauri dev window).
* **Desktop Production Build**: `npm run desktop:build` inside `frontend/` (compiles Rust binaries, bundles release assets, and publishes `Navis-Recon.exe` to repository root).
* **Launch Desktop**: `start.bat` in root (launches packaged `Navis-Recon.exe`).
* **Terminate Desktop**: `stop.bat` in root (forcefully terminates active `navis-recon.exe` processes).

*(Note: Legacy FastAPI server files and batch runners are archived under `docs/archive/` and are not part of the active runtime).*

---

## 🛡️ Operational Mandates (Vault Protocol)

1. **Strict Chronology:** Statements use the basename format `Institution YYYY-MM-DD.pdf` and are stored under the validated statement year.
2. **Directory Structure:** No financial PDFs in the root. Everything resides inside `C:/Example/`:
   - `Statements/`: Archive of all income/expense reports.
   - `Policies/`: Insurance, Taxes, and long-term contracts.
   - `Sovereign.beancount`: Active tracking sheets and reconciliation reports.
3. **Audit First:** Every monthly reconcile requires cross-referencing between the **Master Checklist** and bank activity.
4. **Security:** Never store plaintext passwords. Use Bitwarden integration for all financial credentials.

### Harvester sync review outcome policy

Analysis outcome is separate from lifecycle status. Exit code zero with matches and no item errors or unmatched results is success; exit zero with valid matches plus item errors or unmatched results is partial and permits review of successful matches only; exit zero with errors but no matches is failed. Exit zero with no matches and no item errors is a successful empty review. Nonzero exit, spawn/wait/read failure, malformed structured output, or a global error lacking item context is failed, even when useful matches were already emitted. Stderr is retained as diagnostic log text and does not alone make an analysis partial. Plain stdout lines remain logs. Backend match IDs are scoped to the run and derived from the canonical child item after removing child identity; duplicate identical items collapse. During analysis, the coordinator privately binds each eligible match ID to a prepared native import plan containing source and ledger revisions. Commit accepts only selected backend IDs and executes the retained plans; the legacy external commit command is not launched. Per-operation receipts, backups, reconciliation, and safe retry survive process restarts. `get_sync_run`, `get_sync_outcomes`, `reconcile_ledger_sync`, and `retry_ledger_sync` expose durable results without exposing local paths. The bounded runner enforces cancellation and timeout; native synchronous file operations remain non-interruptible until the next item boundary. A process termination that cannot be confirmed keeps the coordinator reservation active.


### Managed sync import (RECON-S009)

The external finance processor is analysis-only. Its observed legacy --commit lacks durable item identity and idempotency, so production commit IPC routes through the native managed importer. See [managed-sync-import.md](managed-sync-import.md) for the supported subset, required `sync_inbox_path` and `sync_counter_account` settings, private journal location, recovery and retry protocol, IPC argument shapes, and platform limits. Encrypted transforms and ambiguous interrupted operations fail closed for manual review.
