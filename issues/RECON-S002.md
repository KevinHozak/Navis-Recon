---
depends_on: ["RECON-S001"]
id: "RECON-S002"
parent: ["RECON-E001"]
priority: "high"
schema_version: 1
status: "done"
title: "Use decimal amounts and currency-aware aggregation"
type: "enhancement"
---
## Outcome and current evidence

Posting amounts currently use f64 and do not retain commodity identity.

## Target areas

ledger_parser.rs; IPC financial types; obligations.rs

## Acceptance criteria

- [x] Use exact decimal money representation across parsing, aggregation and IPC without changing ledger balances.
- [x] Preserve currency or commodity on every posting; only aggregate compatible units and require explicit valuation for conversions.
- [x] Prove fractional-cent accumulation, refunds, negative balances and mixed-currency isolation with synthetic fixtures; keep supported single-currency totals consistent.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E001](RECON-E001.md). Follows [RECON-S001](RECON-S001.md) as the requested delivery gate.

## Completion evidence (2026-10-23)

Implemented signed fixed-point amounts with 18 decimal places and string-only monetary IPC, preserving explicit posting units. Summary, history, pending review, drill-down, and recurring-obligation paths use decimal amounts. Totals and obligations are isolated by currency; the UI identifies selected-unit subtotals and unvalued holdings without implicit conversion. Rounded statistics are labeled and undefined or out-of-range percentages remain unavailable.

Synthetic coverage verifies fractional-cent accumulation, refunds, negative balances, independent unit balancing, commodity holdings, IPC string serialization, currency-specific obligation keys and metadata, rounding boundaries, and preserved monthly history. Version 1.0.2 passed 57 Rust tests (1 existing mounted-drive benchmark ignored), 7 frontend tests, lint, format checking, and the production frontend build on Node 24.14.0 / Rust 1.96.0. Source audit completed after two bounded repair rounds. A bundle-size warning remains; no independent external review, live household data verification, or packaged executable rebuild was performed.
