---
depends_on: ["RECON-S014"]
id: "RECON-S015"
parent: ["RECON-E005"]
priority: "normal"
schema_version: 1
status: "done"
title: "Explain monthly income, spending and net-worth changes"
type: "enhancement"
---
## Outcome and current evidence

The app exposes balances and account history; a reconciled monthly movement explanation is proposed.

## Target areas

ledger_parser.rs; CategoryDetailWidget.jsx; WealthDashboardWidget.jsx

## Acceptance criteria

- [x] Provide income, spending and category comparisons over selected periods using currency-aware validated postings.
- [x] Exclude internal transfers and credit-card repayments from double-counted spending; disclose the treatment of refunds, corrections and valuation changes.
- [x] Link each aggregate to source transactions and reconcile monthly net-worth movement to explained components or an explicit unexplained remainder.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E005](RECON-E005.md). Follows [RECON-S014](RECON-S014.md) as the requested delivery gate.


## Completion evidence (2026-10-25)

Implemented exact selected-unit monthly income, net spending, equity/adjustment postings and unexplained net-worth remainder. Explicit comparison windows return category amounts/deltas and source IDs; expanded monthly/category rows show dates, descriptions, pending flags and exact source account postings. Category values and differences are unavailable when a chosen period predates the first selected-unit evidence rather than inventing zero spending.

Internal asset transfers and credit-card repayments do not create Income/Expenses spending; card purchases and interest still do. Refunds and corrections retain their signed category effects. Equity/opening/capital adjustments are explicit; unsupported price/cost valuation and currency conversions are never inferred. Synthetic fixtures reconcile the exact monthly movement and source-category sums while covering refunds, corrections, transfers, repayment, purchases, equity changes, independent EUR postings and absent comparison evidence. No ledger balances are changed. Policy: docs/household-analysis.md.

Verified on prepared application version 1.0.14, Node 24.14.0 / Cargo 1.96.0: cargo test --locked -- --test-threads=1 passed 180 tests (1 existing mounted-drive benchmark ignored); npm test passed 89 tests; npm run lint, npm run build, cargo fmt --check and git diff --check passed. Source audit completed after targeted repairs; no independent external review, live desktop UI exercise, release executable rebuild, household-ledger/statement access or remote publication. Changes remain local and uncommitted. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.
