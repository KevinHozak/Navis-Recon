---
depends_on: ["RECON-S010"]
id: "RECON-S011"
parent: ["RECON-E004"]
priority: "normal"
schema_version: 1
status: "open"
title: "Store reconciliation records by account and statement period"
type: "enhancement"
---
## Outcome and current evidence

Reconciliation currently toggles an institution/month boolean and uses institution-name matching.

## Target areas

statements.rs; checklist.rs; VaultChecklist.jsx; StatementLibraryWidget.jsx

## Acceptance criteria

- [ ] Store explicit account, statement identity, covered period, currency, review status and evidence reference with stable IDs.
- [ ] Support received, imported, reviewed and reconciled transitions; preserve historical booleans as legacy status without inventing balance evidence.
- [ ] Allow explicit account mapping and disambiguation for multiple accounts at one institution. Verify migration, overlapping periods and multiple statements per month using synthetic data.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E004](RECON-E004.md). Follows [RECON-S010](RECON-S010.md) as the requested delivery gate.
