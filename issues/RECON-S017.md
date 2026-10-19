---
depends_on: ["RECON-S016"]
id: "RECON-S017"
parent: ["RECON-E006"]
priority: "normal"
schema_version: 1
status: "open"
title: "Preserve signed balances and complete asset-allocation coverage"
type: "enhancement"
---
## Outcome and current evidence

Overview applies Math.abs to asset balances; the allocation chart omits the cash field.

## Target areas

OverviewWidget.jsx; financial formatting utilities

## Acceptance criteria

- [ ] Preserve negative asset and refund signs; label liability presentation explicitly without losing underlying sign semantics.
- [ ] Include cash and disclose any asset categories excluded from allocation; chart totals reconcile to the displayed allocation basis.
- [ ] Use consistent currency formatting, accessible text alternatives and clear unavailable states. Verify negative assets, zero values, cash-only accounts and liabilities.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E006](RECON-E006.md). Follows [RECON-S016](RECON-S016.md) as the requested delivery gate.
