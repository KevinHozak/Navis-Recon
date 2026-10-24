---
depends_on: ["RECON-S005"]
id: "RECON-S006"
parent: ["RECON-E002"]
priority: "high"
schema_version: 1
status: "done"
title: "Persist an approval audit trail and conflict-safe undo"
type: "enhancement"
---
## Outcome and current evidence

Approval currently has no user-visible persisted undo history.

## Target areas

ledger_review.rs; LedgerReviewWidget.jsx; local persistence

## Acceptance criteria

- [x] Record before/after transaction identity, category change and timestamp only after a successful write; keep household content local and outside Git.
- [x] Undo restores only this approval after verifying the current transaction matches its recorded after-state; concurrent edits block undo safely.
- [x] Show in-flight, saved, conflict and failure states, prevent duplicate submission, and refresh dependent counts and totals after approval or undo. Verify restart recovery and failed writes.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E002](RECON-E002.md). Follows [RECON-S005](RECON-S005.md) as the requested delivery gate.

## Completion evidence

Implemented in 1.0.6: native approval and Undo use a private, per-ledger audit journal outside Git. History records immutable before/after transaction identities, categories and timestamps only after a confirmed source write. Durable intents recover confirmed after-states once; validated committed leftovers clean up best effort. Failed writes and audit publication failures restore original bytes where possible. Restoration uncertainty retains the complete recovery copy and blocks ambiguous recovery without automatically overwriting the ledger.

Undo requires one exact recorded after-state, restores only that transaction block, preserves unrelated edits and appends an inverse audit record. Missing, changed or duplicate blocks and duplicate Undo requests fail safely. API mutation invalidation refreshes dependent views and counts; request generation guards prevent stale results from replacing current state. History exposes display metadata rather than raw transaction snapshots over IPC.

Verified with synthetic ledgers and disposable stores: 104 Rust tests passed (1 existing mounted-drive benchmark ignored), 44 frontend tests passed; lint, production web build, Rust formatting and whitespace checks passed. Failure tests exercise source write, journal staging/publication, rollback, partial restoration, restart reconciliation and saved cleanup behavior. A read-only temp file did not simulate a true unlink denial on this Windows setup; injected cleanup and leftover-intent behavior were tested. Source audit completed two bounded repair rounds. No independent external review, household-ledger access, live desktop UI verification or executable rebuild was performed. Source writes retain the documented non-crash-atomic limitation.