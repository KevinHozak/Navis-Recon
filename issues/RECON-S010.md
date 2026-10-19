---
depends_on: ["RECON-S009"]
id: "RECON-S010"
parent: ["RECON-E003"]
priority: "high"
schema_version: 1
status: "open"
title: "Add cancellation, bounded subprocess handling and sync receipts"
type: "enhancement"
---
## Outcome and current evidence

The current bridge waits for subprocess completion without a user cancellation command or declared execution timeout.

## Target areas

sync.rs; SyncWidget.jsx; local sync persistence

## Acceptance criteria

- [ ] Cancel only the owned run process, drain its output and report cancelled or uncertain effects honestly; define bounded timeout handling.
- [ ] Persist a local redacted run receipt with counts, per-file outcomes, errors and timing; exclude credentials and household documents from repository artifacts.
- [ ] After confirmed commit, refresh ledger, statement catalog and affected badges; offer direct links to unresolved files. Verify hung, failed and cancelled fake processes without running a real import.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E003](RECON-E003.md). Follows [RECON-S009](RECON-S009.md) as the requested delivery gate.
