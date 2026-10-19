---
depends_on: ["RECON-S018"]
id: "RECON-S019"
parent: ["RECON-E006"]
priority: "normal"
schema_version: 1
status: "open"
title: "Guide monthly close and save a local evidence summary"
type: "enhancement"
---
## Outcome and current evidence

A guided monthly-close workflow is proposed after accounting, approvals, syncing, reconciliation and analysis are reliable.

## Target areas

overview and reconciliation views; local close persistence

## Acceptance criteria

- [ ] Guide collect statements, import, review, reconcile and summarize for a selected month, showing outstanding blockers and their owners or resolution links.
- [ ] Close requires validated current data and resolved reconciliation criteria; reopen or mark stale when underlying evidence changes.
- [ ] Save a local summary referencing statement evidence, ledger revision, unresolved exceptions and period metrics; no automatic remote publication. Verify a synthetic complete close, blocked close and post-close source change.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E006](RECON-E006.md). Follows [RECON-S018](RECON-S018.md) as the requested delivery gate.
