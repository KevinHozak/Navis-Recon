---
depends_on: ["RECON-S007"]
id: "RECON-S008"
parent: ["RECON-E003"]
priority: "high"
schema_version: 1
status: "done"
title: "Preserve review selections and report analysis failures honestly"
type: "enhancement"
---
## Outcome and current evidence

Every incoming match currently selects all accumulated matches; the done handler enters review for both success and failure.

## Target areas

SyncWidget.jsx; apiClient.js

## Acceptance criteria

- [x] Use stable match IDs and preserve manual deselection as new matches arrive.
- [x] Distinguish successful, partial and failed analysis, retain useful diagnostics, and define when reviewed successful items may be committed.
- [x] Disable reset and conflicting actions during commit; listener registration and teardown cannot lose ownership or leak after unmount. Verify incremental results and failed analysis.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E003](RECON-E003.md). Follows [RECON-S007](RECON-S007.md) as the requested delivery gate.

## Completion evidence

Implemented in 1.0.8: backend-owned run-scoped content IDs deduplicate replay and resolve selected IDs to retained harvester payloads under the commit reservation lock. Manual selection survives incremental arrivals, reorder, replay, reconnect and view remount in the application process. New run authority resets selections; stale owners cannot replace newer authority.

Analysis now distinguishes success, partial and failed. Partial review permits only eligible matches after successful process exit. File errors/unmatched results and conflicting source matches block affected files; global/protocol/read/spawn/nonzero failures block commit, retaining useful matches and diagnostics. Empty clean analysis succeeds without commit items. Stderr warnings remain visible logs. Failed commit preserves the analysis outcome.

Failed retained results display read-only. Partial counts, diagnostics and stable-ID checkboxes remain visible; start/commit/abandon controls respect backend phase and pending ownership. Pending commands reconcile native state after reconnect before releasing their guard; failed reconciliation disables actions. Listener partial setup, abort and disposal guards remain verified.

Verified with synthetic fixtures and fake Python children: 119 Rust tests passed (1 existing mounted-drive benchmark ignored), 65 frontend tests passed; lint, production web build, Rust formatting and whitespace checks passed. Web build completed before the full native suite. Source audit completed two bounded repair rounds. No independent external review, real household-ledger/production-harvester execution, live desktop UI verification or executable rebuild occurred. State and selection remain in memory; durable per-file outcomes, receipts and active cancellation/timeouts remain later stories.