---
depends_on: ["RECON-E001"]
id: "RECON-E002"
parent: []
priority: "high"
schema_version: 1
status: "done"
title: "Safe transaction approvals with conflict protection and undo"
type: "epic"
---
## Outcome

Approve the intended transaction while preserving ledger formatting and concurrent changes.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [x] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [x] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [x] [RECON-S004: Identify pending transactions by content and reject stale approvals](RECON-S004.md)
- [x] [RECON-S005: Validate the intended posting and preserve unrelated ledger text](RECON-S005.md)
- [x] [RECON-S006: Persist an approval audit trail and conflict-safe undo](RECON-S006.md)

## Completion verification (2026-10-25)

Current source audit at d71a9c1 (application 1.0.11) verified all three completed child stories against their acceptance criteria. RECON-E001 is done, and the local issue runner confirms three accepted children with no unresolved relationships. The implementation was already delivered; this update reconciles the epic status without application changes.

### Acceptance evidence

- S004: pending rows carry raw-content SHA-256 transaction identity and ledger revision. Locked approval checks both against the same exclusive source handle before changing bytes. Synthetic insertion, reordering, independent edit, repeated request, and concurrent same-row/distinct-row tests prove stale requests preserve the source. Frontend controller tests cover duplicate clicks, retained conflict errors, Refresh authority, failed refreshes, and late replies.
- S005: listing and locked approval share category-posting selection and dated open/close/currency eligibility checks. Production-path fixtures reject forged, closed, future, wrong-root, and wrong-unit categories, ambiguous splits, and unsupported metadata/headers. Exact-byte tests cover metadata-before-posting, category-after-bank layouts, comments, Unicode, tabs, CRLF/LF, absent final newline, and unrelated transactions; only the flag and category token change.
- S006: committed private history records contain before/after identities, category changes, and timestamps after synced source writes. Store resolution rejects Git checkout destinations; normal IPC exposes display metadata rather than raw snapshots. Undo requires exactly one unchanged recorded after-state, preserves later unrelated bytes, and rejects edited/duplicated blocks and repeated/concurrent requests. Fault-injected source/journal writes, rollback, partial restoration, cleanup leftovers, and restart reconciliation verify recovery behavior. Frontend tests verify saved/failure/stale completion handling and duplicate guards; API mutation tests verify dependent-view invalidation on success and uncertain failures, with App subscribers refreshing counts and remounting financial views.

### Checks and limits

- `cargo test --locked -- --test-threads=1`: 162 passed, 0 failed, 1 existing mounted-drive benchmark ignored. Executed outside the restrictive sandbox because its file-publication restrictions were established in the preceding E001 verification.
- `npm test`: 81 passed, including review-controller, category selection, mutation IPC, and dependent-view invalidation coverage.
- `npm run lint`, `npm run build`, and `cargo fmt --check`: passed on Node 24.14.0 / Cargo 1.96.0. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.

Synthetic ledgers and disposable stores only; no household financial data, credentials, or source PDFs were read or added to Git or public evidence. No application version bump, executable rebuild, live desktop UI exercise, independent external review, or remote publication was performed. Writes retain the documented limitation: no crash/power-loss atomicity guarantee spans the ledger and private journal; uncertain restoration retains a complete recovery copy for manual review.
