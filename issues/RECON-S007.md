---
depends_on: ["RECON-S006"]
id: "RECON-S007"
parent: ["RECON-E003"]
priority: "high"
schema_version: 1
status: "done"
title: "Add backend-owned sync run identity and concurrency control"
type: "enhancement"
---
## Outcome and current evidence

Analysis emits global events without run identity; the backend has no visible guard against overlapping runs.

## Target areas

sync.rs; apiClient.js; SyncWidget.jsx

## Acceptance criteria

- [x] Assign a run ID and expose backend-owned idle, analyzing, review, committing, complete, failed and cancelled states.
- [x] Reject conflicting analysis or commit starts and scope every event to its run; old events cannot mutate a newer run.
- [x] View navigation or remount reconnects to the current run without starting another process. Verify interleaved events and duplicate starts with a fake harvester.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E003](RECON-E003.md). Follows [RECON-S006](RECON-S006.md) as the requested delivery gate.

## Completion evidence

Implemented in 1.0.7: the native coordinator reserves one current run before launching analysis or commit, owns its ID, global revision, event sequence and seven lifecycle states, and retains a full in-memory snapshot. Commit requires the current review run and a nonempty, nonduplicated selection belonging to its analysis results. Review abandonment is run-bound; active work retains ownership. All worker events carry backend identity and a complete snapshot, and late workers cannot alter newer runs or invalid phases.

The Sync view subscribes before fetching its snapshot, reconnects without launching another child process, rejects malformed or mismatched envelopes, and ignores old revisions and connection generations. Action guards prevent duplicate requests and keep controls disabled until backend state is available. Listener setup cleans up after partial failure, abort and unmount. Native workers drain stderr before completion, detect structured analysis/commit errors, and send commit stdin concurrently with output draining.

Verified using synthetic fixtures and fake Python children: 115 Rust tests passed (1 existing mounted-drive benchmark ignored), 57 frontend tests passed; lint, production web build, Rust formatting and whitespace checks passed. The web build completed before the full native suite. Source audit completed two bounded repair rounds; no independent external review, household-ledger/production-harvester execution, live desktop UI verification or executable rebuild occurred. State lasts for the current application process; durable outcomes, stable persisted selection and active cancellation/timeouts remain later-story scope.