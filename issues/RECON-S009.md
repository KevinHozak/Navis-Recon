---
depends_on: ["RECON-S008"]
id: "RECON-S009"
parent: ["RECON-E003"]
priority: "high"
schema_version: 1
status: "open"
title: "Make commits idempotent with durable per-file outcomes"
type: "enhancement"
---
## Outcome and current evidence

The bridge returns overall success from process exit and passes through result messages; durable per-item recovery is not established in this repo.

## Target areas

sync.rs; harvester bridge contract; SyncWidget.jsx

## Acceptance criteria

- [ ] Inspect the external harvester contract before implementation and document whether external changes are required; do not assume it already prevents duplicate imports.
- [ ] Validate approved source identities and destinations; persist succeeded, failed and uncertain outcomes keyed to run and item identity.
- [ ] On retry or restart reconcile actual effects and retry only verified safe items; prevent duplicate ledger imports and accidental archive overwrites. Verify interrupted and partial commits using disposable files.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E003](RECON-E003.md). Follows [RECON-S008](RECON-S008.md) as the requested delivery gate.
