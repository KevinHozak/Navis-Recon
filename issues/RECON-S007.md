---
depends_on: ["RECON-S006"]
id: "RECON-S007"
parent: ["RECON-E003"]
priority: "high"
schema_version: 1
status: "open"
title: "Add backend-owned sync run identity and concurrency control"
type: "enhancement"
---
## Outcome and current evidence

Analysis emits global events without run identity; the backend has no visible guard against overlapping runs.

## Target areas

sync.rs; apiClient.js; SyncWidget.jsx

## Acceptance criteria

- [ ] Assign a run ID and expose backend-owned idle, analyzing, review, committing, complete, failed and cancelled states.
- [ ] Reject conflicting analysis or commit starts and scope every event to its run; old events cannot mutate a newer run.
- [ ] View navigation or remount reconnects to the current run without starting another process. Verify interleaved events and duplicate starts with a fake harvester.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E003](RECON-E003.md). Follows [RECON-S006](RECON-S006.md) as the requested delivery gate.
