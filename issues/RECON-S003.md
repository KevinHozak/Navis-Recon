---
depends_on: ["RECON-S002"]
id: "RECON-S003"
parent: ["RECON-E001"]
priority: "high"
schema_version: 1
status: "open"
title: "Expose validated totals with provenance and coverage"
type: "enhancement"
---
## Outcome and current evidence

Summary totals do not expose evaluation coverage or source freshness.

## Target areas

ledger_parser.rs; OverviewWidget.jsx; apiClient.js

## Acceptance criteria

- [ ] Return ledger revision, read time, latest transaction date, valuation basis and validation status with the summary.
- [ ] Explain included accounts and exclusions through a drill-down that reconciles to headline totals.
- [ ] Show actionable unsupported-data or stale-source warnings; a failed read must not look like a zero balance. Verify synthetic complete, partial and unreadable ledgers.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E001](RECON-E001.md). Follows [RECON-S002](RECON-S002.md) as the requested delivery gate.
