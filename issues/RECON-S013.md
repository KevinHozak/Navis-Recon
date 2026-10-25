---
depends_on: ["RECON-S012"]
id: "RECON-S013"
parent: ["RECON-E004"]
priority: "normal"
schema_version: 1
status: "done"
title: "Make checklist saves conflict-aware and visibly durable"
type: "enhancement"
---
## Outcome and current evidence

Checklist toggles update UI optimistically and a failed save can leave the changed checkmark visible.

## Target areas

checklist.rs; VaultChecklist.jsx; StatementLibraryWidget.jsx

## Acceptance criteria

- [x] Use a revision token or equivalent compare-before-write contract to reject stale whole-document saves without losing concurrent updates.
- [x] Show saving and saved states; on failure restore the last persisted value or keep a clearly marked unsaved edit with retry.
- [x] Prevent rapid overlapping saves from overwriting newer choices and verify disk readback, write failure and conflict scenarios with disposable fixtures.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E004](RECON-E004.md). Follows [RECON-S012](RECON-S012.md) as the requested delivery gate.


## Completion evidence (2026-10-25)

Implemented raw-byte revision tokens and expected-revision saves for the checklist and reconciliation records. Writer handles serialize cooperating Windows processes; unique create-new staging, file sync, write-through atomic replacement and disk readback precede success. Read-only loads use atomic JSON snapshots without contending on the writer lock. Stale or busy writes preserve persisted choices and return explicit errors.

The checklist saver keeps persisted checkmarks visible until successful readback, publishes saving/saved/failure states, rejects rapid overlapping requests, preserves failed values, allows refresh/retry and suppresses late UI callbacks after disposal. Initial load failures now expose Refresh. The reconciliation form retains unsaved edits on failure; Refresh explicitly reloads persisted values. Native IPC carries expected revisions with no HTTP fallback.

Synthetic native tests verify disk hashes, stale saves, held writer locks, true Windows replacement denial and unchanged persisted bytes. Frontend saver tests verify failure rollback/retry, synchronous duplicate guards, blocked refresh acceptance during a save, saved state, malformed readback and unmounted completion; IPC tests verify the revision and exact-money wire contract. Sidecar locking coordinates app writers; noncooperating external editors and cross-file power-loss atomicity remain documented limits.

Verified on prepared version 1.0.13, Node 24.14.0 / Cargo 1.96.0: cargo test --locked -- --test-threads=1 passed 171 tests (1 existing mounted-drive benchmark ignored); npm test passed 85 tests; lint, production frontend build, Rust formatting and whitespace checks passed. Synthetic ledgers, disposable JSON and fake PDF bytes only. Source audit completed; no independent external review, live desktop UI exercise, release executable rebuild, household-data access or remote publication. Changes remain local and uncommitted. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.
