---
depends_on: ["RECON-E002"]
id: "RECON-E003"
parent: []
priority: "high"
schema_version: 1
status: "done"
title: "Reliable and recoverable statement syncing"
type: "epic"
---
## Outcome

Give every sync a clear identity, honest outcome and safe recovery path.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [x] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [x] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [x] [RECON-S007: Add backend-owned sync run identity and concurrency control](RECON-S007.md)
- [x] [RECON-S008: Preserve review selections and report analysis failures honestly](RECON-S008.md)
- [x] [RECON-S009: Make commits idempotent with durable per-file outcomes](RECON-S009.md)
- [x] [RECON-S010: Add cancellation, bounded subprocess handling and sync receipts](RECON-S010.md)


## Completion verification (2026-10-25)

Audited current source based on bc0088a plus the local retry-cancellation repair, prepared as application 1.0.12. RECON-E002 is done. All four stories retain checked acceptance with current evidence:

- S007: native coordinator reservations and run/revision/sequence envelopes own analysis, review, commit and retry. Fake-child tests verify duplicate/conflicting starts, interleaved and late workers, current-run commit identity, and reconnect snapshots. Frontend tests verify subscribe-before-snapshot, remount without a new process, stale-envelope rejection and listener cleanup.
- S008: run-scoped content IDs and the shared in-memory selection store preserve deselection across incremental arrivals, replay, reorder, reconnect and remount. Native tests distinguish clean empty, partial, global/protocol/nonzero failures and exclude affected files from commit. Controller/view tests cover read-only failed results, disabled conflicting actions, pending command ownership, and failed reconciliation.
- S009: docs/managed-sync-import.md records the inspected legacy analyzer/committer boundary and the replacement native commit protocol; no external idempotency is assumed. Synthetic production-path tests verify prepared source/ledger identity, destination validation, create-only archives, exact balanced entries, durable succeeded/failed/uncertain outcomes, no duplicate append across runs, partial batches, interrupted intents/archives, restart reconciliation, safe retry, corrupt evidence and held-lock failures.
- S010: Windows fake-child tests verify bounded timeout/output, owned Job cancellation without stopping an unrelated process, descendants holding pipes, failed spawn/resume and uncertain termination. Strict redacted receipt tests and finalizer tests verify opaque IDs, counts/timing/diagnostic codes, immutable publication, restart listing and explicit persistence failures. Source-link tests inject the opener; App/resource-observer tests and source wiring verify confirmed-effect refresh and deduplication. The audit repaired missing retry cancellation; two new behavioral fixtures cover cancellation before work, between items, and archive-only recovery without losing completed effects.

Final verification on Node 24.14.0 / Cargo 1.96.0:

- `cargo test --locked -- --test-threads=1`: 164 passed, 0 failed, 1 existing mounted-drive benchmark ignored; run outside the restrictive sandbox for native file-publication behavior.
- `npm test`: 81 passed.
- `npm run lint`, `npm run build`, `cargo fmt --check`, and `git diff --check`: passed. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.
- All package, package-lock root, Cargo application/lock, and Tauri versions align at 1.0.12. The unversioned window title remains unchanged.

Synthetic ledgers, disposable files and fake children only; no household financial data, credentials or source PDFs were read or added to Git or public evidence. No production harvester/import, actual Explorer invocation, new packaged executable, live desktop interaction or independent external review was performed. Changes remain local and uncommitted. Selection remains in-process; recovery outcomes and receipts are durable. Native filesystem cancellation is cooperative between items, with no hard IO deadline or power-loss atomicity claim. Prior Windows symlink-creation coverage remains explicitly limited as recorded in S010.
