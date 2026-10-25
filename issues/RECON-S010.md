---
depends_on: ["RECON-S009"]
id: "RECON-S010"
parent: ["RECON-E003"]
priority: "high"
schema_version: 1
status: "done"
title: "Add cancellation, bounded subprocess handling and sync receipts"
type: "enhancement"
---
## Outcome and current evidence

The current bridge waits for subprocess completion without a user cancellation command or declared execution timeout.

## Target areas

sync.rs; SyncWidget.jsx; local sync persistence

## Acceptance criteria

- [x] Cancel only the owned run process, drain its output and report cancelled or uncertain effects honestly; define bounded timeout handling.
- [x] Persist a local redacted run receipt with counts, per-file outcomes, errors and timing; exclude credentials and household documents from repository artifacts.
- [x] After confirmed commit, refresh ledger, statement catalog and affected badges; offer direct links to unresolved files. Verify hung, failed and cancelled fake processes without running a real import.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E003](RECON-E003.md). Follows [RECON-S009](RECON-S009.md) as the requested delivery gate.

## Completion evidence

Implemented in 1.0.10. The Windows analyzer starts suspended in an owned kill-on-close Job with restricted handle inheritance, then resumes under a nonblocking bounded pipe pump. Cancellation is run-bound; timeout, output limits and termination/drain handling are bounded. The default analysis timeout is 600 seconds and accepts configuration from 1 to 3600 seconds. Unconfirmed termination keeps ownership blocked rather than claiming clean cancellation. Native managed import cancellation is cooperative: the current durable item can finish, confirmed successes remain confirmed, and later items are not started. Synchronous native filesystem calls are not force-stopped or claimed to have a hard IO deadline.

A separate private run-receipt store persists immutable redacted attempt summaries, opaque per-file IDs/outcomes, diagnostic codes, counts and timing. Raw filenames, paths, monetary values, household documents and child logs are excluded. Analysis, commit, retry, review abandonment and terminal failure/cancellation use the shared finalizer. Persistence failures are explicit without undoing verified financial effects; history loads independently after restart.

The App-wide observer subscribes before snapshot catch-up and deduplicates confirmed item effects across remounts. Confirmed successes refresh ledger views, statement catalog/checklist and badges even in partial failed or cancelled batches; no-effect analysis/cancellation does not invalidate financial data. The Sync view exposes cancelling/timeout states, receipt history/errors and guarded Show file actions. Opaque current-run or durable references resolve only through owned plans/strict receipts and canonical inbox checks; durable links work while idle after restart. Explorer receives literal arguments, and tests inject the opener.

Verified with fake Python processes and synthetic files: 156 Rust tests passed (1 existing mounted-drive benchmark ignored), 77 frontend tests passed; lint, production web build, Rust formatting and whitespace checks passed. Web build completed before full native testing. Focused source-link readback confirmed unknown/missing/tampered cases; the symlink escape subcase was unavailable because Windows denied symlink creation (error 1314), with no ACL change. Source audit completed two bounded repair rounds. No independent external review, production ledger/harvester run, actual Explorer launch, live desktop UI verification or executable rebuild occurred.

## Epic completion audit and repair (2026-10-25)

The E003 current-source audit found that the production retry worker did not observe run-bound cancellation even though the UI offered Cancel. Repaired in prepared version 1.0.12: retry latches cancellation between items, does not begin later archive/ledger effects, preserves confirmed successes and existing durable evidence, and reports the run/receipt as cancelled. Skipped items require reconciliation before retry safety is advertised. The Sync view now says Sync cancelled rather than incorrectly describing every cancellation as review abandonment.

New synthetic regression tests verify cancellation before the first retry item, after one confirmed success, and for an owned archive-only interrupted item; the latter retains both archive bytes and original ledger until an explicit later retry. Full final native suite: 164 passed, 0 failed, 1 existing mounted-drive benchmark ignored. Frontend: 81 passed; lint, production frontend build, Rust formatting and whitespace checks passed. Source audit found no remaining acceptance gaps after the repair. No production import, household data, Explorer launch, independent external review or new desktop executable build was used. Changes remain local and uncommitted.
