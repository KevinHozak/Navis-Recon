# Managed sync import protocol (RECON-S009)

## External analyzer boundary

### Local-only configured analysis (1.0.17)

Sync embeds the repository-owned `analyzer_bridge.py` and launches it through the bounded native Python runner. It passes the canonical inbox from the same configuration snapshot used for managed import, the selected ledger currency, and `sync_rules_path`. If the rules path is empty, it locates `finances_renaming_rules.json` beside the legacy `harvester_script`; the legacy Python processor and its commit/AI functions are never executed. Python with `pypdf` remains a prerequisite. No Google client is imported and no statement text is sent to an external service.

Optional `sync_ocr_enabled: "true"` enables installed local Poppler/Tesseract for empty-text pages among the first two pages. `sync_pdftoppm_path` and `sync_tesseract_path` select the executables. They render at 150 DPI in a disposable directory and run English OCR locally, with 45-second per-command limits inside the native runner's cancellable process tree. Temporary page images are removed after analysis. Missing executables, OCR failures and unreadable text are explicit diagnostics; no cloud fallback exists. OCR is off by default and never modifies a PDF or guesses missing rules.

Institution/date rules retain `keywords`, `date_patterns`, `date_selection`, and `target_template`. Date parsing uses a pattern's captured date rather than its full labeled match. A pattern capturing multiple different dates is ambiguous and rejected. For an import candidate, the rule additionally needs explicit local extraction:

```json
"local_extraction": {
  "amount_pattern": "Total:\\s+(-?[\\d,.]+)",
  "category": "Expenses:Utilities",
  "currency": "USD"
}
```

The amount pattern must match exactly once with exactly one capture group. Amounts use exact decimal strings, category must be an Income/Expenses account, and the rule currency must match the configured unit. No amount, category, or conversion is guessed. Missing extraction rules, multiple totals, mismatched currency, encrypted PDFs and unsupported files remain unapproved with explicit diagnostics. Existing date/rename rules alone therefore do not authorize financial postings. Analysis only reads the configured inbox and rules; Rust retains all source identity checks, reviewer approval, archive/ledger mutation and durable recovery behavior.

A rule may instead supply `review_only_reason`. After institution and date recognition, that rule emits an unmatched/manual-review diagnostic and cannot produce a candidate, even if it also contains `local_extraction`. Use this for split payroll or other multi-posting documents that the current single-category extractor cannot faithfully represent. A net deposit must not be classified as gross salary, and a historical statement already represented in the ledger must not be reimported. Keep household-specific rules in an ignored private file, such as `data/local-analysis-rules.json`, selected by `sync_rules_path`; shared legacy rules need not be modified.

Review-only acceptance (2026-10-26, v1.0.18): inspection of the historical scanned fixture found a corresponding existing split payroll posting and an incorrect OCR net-pay digit. A private institution/check-date rule therefore routes that format to multi-posting review instead of guessing an amount/category. The packaged app recognized the correct check date, returned one review diagnostic and zero candidates/eligible IDs, and left the copied ledger hash and empty archive unchanged. The no-candidate terminal state is expected, with no analyzer errors. Eight analyzer regressions passed, including refusal even when a review-only rule contains an amount mapping. Production executable and installer metadata both verified as 1.0.18. No real household financial postings or shared legacy rules were changed.

Packaged acceptance (2026-10-26): the actual 1.0.17 release analyzed a temporary inbox containing a synthetic text PDF and a copied historical scan. The configured synthetic local rule produced one eligible exact-decimal candidate; the locally OCR'd historical scan remained unmatched. Native reviewer-approved commit created one archive PDF and one balanced ledger append while preserving both inbox sources and all original ledger bytes. Repeating analysis/approval produced no second archive or append. After restarting the packaged executable, durable reconciliation reported the same single succeeded operation. The test config deliberately pointed `harvester_script` to a nonexistent file, proving the embedded adapter owned analysis. No external AI was used and the live household ledger matched its untouched smoke-test copy afterward.

Validation: seven local analyzer regressions passed (including a real synthetic PDF through the embedded `-c` invocation), 106 frontend tests passed, ESLint passed, 185 native tests passed with one existing mounted-drive benchmark ignored, formatting/whitespace passed, and the production executable plus NSIS installer both verified as 1.0.17. Installer execution was not part of this pass. The historical scan still needs an institution/date rule and an explicit local amount/category mapping before it can be imported; OCR alone does not establish financial meaning.

The observed legacy processor's `--analyze` output supplies candidate metadata. Its `--commit` can move or decrypt files, append logs and ledger text, and return generic process results. It does not supply durable per-item identities, source hash preconditions, idempotency, or recoverable outcomes. Navis-Recon does not invoke that commit path. No external processor code, credentials, private defaults, or household data are copied into this project.

## Approval and commit

At sync start, Rust resolves one managed configuration snapshot. When each analyzer match arrives, Rust privately prepares a plan keyed by the backend generated `match_id`. The plan captures the canonical inbox source path, its SHA-256 and byte length, canonical archive destination, validated date, generated balanced entry, exact ledger-before bytes and hash, configured currency, and counter account. It never accepts a child-provided plan or path during commit. A source mismatch, changed ledger revision, missing/invalid flags, unsupported amount/account/currency/description, duplicate source or destination, unsafe Windows filename, occupied archive target, or a path outside its configured root fails closed.

Archive paths are `statement_archive_path/Statements <validated item date year>/<new_name>`. `new_name` is a safe basename; the existing `Institution YYYY-MM-DD.pdf` convention is supported without requiring a date prefix. Roots and source paths are canonicalized, reparse-point roots are rejected on Windows, and the final archive parent is checked again before create-new publication. Names reject traversal, separators, alternate data streams, reserved device names, controls, and Windows-trimmed dot/space aliases.

## Durable operation protocol

The operation identity is SHA-256 over the canonical ledger namespace and source content digest. It stays stable across runs and backend item IDs. The private `%LOCALAPPDATA%/Navis-Recon/sync-imports/<ledger namespace>/` store is outside Git checkouts. Version 2 receipts use strict schema and semantic checks; they bind the operation, source digest and size, date, destination, archive digest, exact generated entry, ledger-before digest and size, and synced whole-ledger backup. Temporary receipt files use `create_new`, flush to disk, then Windows `MoveFileExW` with replace-existing and write-through. A process-held exclusive journal handle serializes readers, reconciliation, retries, and commits; a leftover lock file alone does not retain a lock after process exit.

Before a selected batch starts, Rust validates every approved ID and prepared source, checks duplicate sources and destinations, verifies every ledger-before image, and parses the combined candidate entries. Each item then holds the journal lock and an exclusive ledger handle plus a Windows range lock before archive effects. The ledger revision advances only by a previously verified append from this same batch. For each operation, Rust syncs the archive with create-new semantics and persists an `archived` receipt as ownership proof. It then appends only the validated balanced entry (or a comment marker for a zero amount), flushes and verifies exactly one complete owned block, and persists `succeeded`. Original ledger bytes are preserved, including a missing final newline. The reviewed source is retained on every outcome, so an already imported source is recognized by content identity on later analysis and cannot create a duplicate append. Archive files are never overwritten, even when their bytes match.

The supported ledger parser deliberately rejects syntax outside its native subset. Writes fail closed outside Windows because the required exclusive file and range locking is not available there.

## Restart reconciliation and IPC

`ItemOutcome` keeps the established `item_id`, `filename`, `state`, `retry_safe`, and `message` fields; `operation_id` is an optional durable receipt ID. Store/configuration failures return an explicit `uncertain` outcome instead of an empty success.

| Command | Arguments | Result / behavior |
|---|---|---|
| `get_sync_run` | none | Current `SyncRun`; reconnect hydration includes durable managed outcomes. |
| `get_sync_outcomes` | none | `ItemOutcome[]`; verifies and reconciles durable receipts. |
| `reconcile_ledger_sync` | none | Same `ItemOutcome[]`; promotes only exact verified effects and marks no-effect intents retry-safe. |
| `commit_ledger_sync` | `{ runId: string, payload: string[] }` | Selects backend match IDs; native plans remain coordinator-owned. |
| `retry_ledger_sync` | `{ operationIds: string[] }` | Retries only durable receipt IDs. It rechecks journal, source when needed, backup, archive, and locked ledger state. |

After restart, an `intent` with the exact source, unchanged ledger, valid backup, and no archive effect becomes `failed` with `retry_safe: true`. If the archive was created but ownership proof was not durably recorded, reconciliation reports `uncertain` and does not infer ownership from matching bytes. An `archived` receipt with the exact owned archive and unchanged ledger is eligible for guarded completion; if its exact entry is already present once, reconciliation records success without appending again. Any missing/changed source, archive, backup, ledger-before state, duplicate or altered entry block, or corrupt/unknown receipt remains uncertain and is never overwritten automatically. A succeeded receipt is rechecked against the live archive and ledger on every read.

### Verification boundary

The native protocol is exercised only with synthetic temporary files, ledgers, and fake analyzer output. The app does not claim external processor idempotency. Source cleanup is intentionally manual; deleting a path after releasing its reviewed handle can remove a replacement file. The UI may offer retry only when `retry_safe` is true or when reconciliation identifies an owned archive that can be completed under the guarded retry command.


## Recovery API and supported subset

The desktop IPC commands are `get_sync_outcomes` and `reconcile_ledger_sync` (both return the complete `ItemOutcome[]` list), plus `retry_ledger_sync({ operationIds: string[] })`, which returns the immediate `SyncRun` snapshot and then publishes normal full-snapshot sync events while work completes. Recovery outcomes contain `item_id`, `filename`, `state` (`succeeded`, `failed`, or `uncertain`), `retry_safe`, `message`, and nullable `operation_id`. Retry accepts only durable 64-character hexadecimal operation IDs that reconciliation marked `retry_safe`; uncertain items require manual review. No HTTP financial-data fallback is supported.

`reveal_sync_source({ sourceRef, runId, itemId, operationId })` accepts an opaque source reference only. Current-run references resolve against the coordinator snapshot and its bound inbox. Durable operation references use the bound config when present, or load the runtime config after restart when the coordinator is idle; they then pass strict receipt validation under the journal lock and must match the configured ledger namespace. Reveal does not start analysis or mutate sync state. Paths are never accepted from IPC. Reveal is desktop-only and checks for reparse-point ancestry before canonicalization, then requires a canonical regular file inside the inbox before opening Explorer.

Managed archive destinations follow the existing `Statements YYYY/<new_name>` layout under the configured archive root. Source filenames are preserved and need only pass safe filename validation; there is no institution-prefix requirement. The supported managed import subset remains limited to reviewed statements with a valid date, category, amount, currency, and supported cleared/encrypted flags. Recovery is journaled and reconciled against exact ledger and archive evidence; the implementation does not claim power-loss atomicity.


## Cancellation and current RECON-S010 coverage

Cancellation requests are tied to the current backend run ID. Analysis remembers a request made before child publication and targets the owned direct child; the run remains active until its worker reports terminal state. Managed commit checks cancellation between selected items. An in-flight journal/ledger/archive operation is allowed to finish, confirmed successes remain confirmed, and later items are recorded as no-effect retry-safe failures when the existing private journal permits it. A run-scoped resource event follows any commit or retry with a confirmed success, including partial batches.

Analysis cancellation and timeout handling use the bounded Windows Job Object runner described by S010. Cancellation requests are run-bound. Managed commit/retry checks cancellation between items; an in-flight synchronous native filesystem operation is allowed to finish, so cancellation latency includes that operation. The implementation does not detach or force-stop a thread while it is writing. An outcome that cannot be confirmed remains `uncertain`.

### Redacted run receipts (RECON-S010)

The sync screen reads the receipt list on connect and explicit refresh; a read error is displayed without clearing currently visible managed outcomes. The application resource observer subscribes before reading the current run snapshot and deduplicates confirmed operation IDs across remounts. It refreshes the ledger, statement catalog, and sidebar counts only after confirmed item effects.

The separate immutable run receipt store is `%LOCALAPPDATA%/Navis-Recon/sync-runs/`; it is independent from the private S009 item journal under `sync-imports`. Receipt listing does not load config or touch the ledger. The store resolves its canonical path and refuses a location inside a Git checkout. Each terminal phase uses a unique SHA-256-derived opaque receipt key. JSON is staged with `create_new`, flushed, and published with Windows `MoveFileExW` write-through without replacement. A collision or publication failure is reported as a fixed error code.

Version 1 uses strict `deny_unknown_fields` deserialization and semantic checks. Fields are version, opaque receipt/run IDs, operation kind (`analysis`, `commit`, `retry`), phase start/end/duration milliseconds, terminal state (`review`, `complete`, `failed`, `cancelled`), `timed_out`, six outcome counts, up to 1000 opaque per-item IDs with optional opaque operation IDs/state/retry safety, an `items_truncated` flag, and allowlisted diagnostic codes. Counts include all outcomes even when the per-item list is truncated. No names, paths, payees, amounts, accounts, child messages, raw errors, or household documents are stored. Raw IDs are SHA-256 hashed before serialization.

The list command reads at most 100 newest JSON receipts (512 KiB maximum each). It returns `{ receipts, error }`; read and corruption failures are explicit fixed codes instead of an empty success. Analysis review and later review abandonment each create their own receipt; commit/retry receipts also remain separate and never overwrite earlier phases. A crash before a terminal finalizer creates no completed run receipt; the S009 item journal remains the recovery source for in-flight effects. A receipt-store failure does not undo completed effects and appears as `receipt_error` on the terminal `SyncRun` snapshot.

`get_sync_receipts` takes no arguments and returns `{ receipts: RunReceipt[], error: string | null }`. `SyncRun` retains its existing fields and adds nullable `started_at_ms`, `elapsed_ms`, `receipt_id`, and `receipt_error`; serde defaults keep older clients compatible. Managed filesystem operations remain synchronous, so a cancellation arriving during a write is observed between items and can have that latency.

### Retry cancellation (1.0.12)

Retry uses the same run-bound cooperative cancellation as managed commit. A cancellation request is latched between items: the current item may finish, confirmed successes remain confirmed, and later items do not start archive or ledger effects. Skipped items retain their durable receipt rather than rewriting recovery evidence; their immediate response does not advertise retry safety until reconciliation rechecks the current source, archive, and ledger. Archive-only items remain uncertain when skipped. The retry run and redacted terminal receipt report cancelled, and confirmed successes still trigger resource refresh. The Sync view labels cancellation as such rather than claiming every cancellation abandoned a review.
