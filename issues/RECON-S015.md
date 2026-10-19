---
depends_on: ["RECON-S014"]
id: "RECON-S015"
parent: ["RECON-E005"]
priority: "normal"
schema_version: 1
status: "open"
title: "Explain monthly income, spending and net-worth changes"
type: "enhancement"
---
## Outcome and current evidence

The app exposes balances and account history; a reconciled monthly movement explanation is proposed.

## Target areas

ledger_parser.rs; CategoryDetailWidget.jsx; WealthDashboardWidget.jsx

## Acceptance criteria

- [ ] Provide income, spending and category comparisons over selected periods using currency-aware validated postings.
- [ ] Exclude internal transfers and credit-card repayments from double-counted spending; disclose the treatment of refunds, corrections and valuation changes.
- [ ] Link each aggregate to source transactions and reconcile monthly net-worth movement to explained components or an explicit unexplained remainder.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E005](RECON-E005.md). Follows [RECON-S014](RECON-S014.md) as the requested delivery gate.
