---
depends_on: ["RECON-S001"]
id: "RECON-S002"
parent: ["RECON-E001"]
priority: "high"
schema_version: 1
status: "open"
title: "Use decimal amounts and currency-aware aggregation"
type: "enhancement"
---
## Outcome and current evidence

Posting amounts currently use f64 and do not retain commodity identity.

## Target areas

ledger_parser.rs; IPC financial types; obligations.rs

## Acceptance criteria

- [ ] Use exact decimal money representation across parsing, aggregation and IPC without changing ledger balances.
- [ ] Preserve currency or commodity on every posting; only aggregate compatible units and require explicit valuation for conversions.
- [ ] Prove fractional-cent accumulation, refunds, negative balances and mixed-currency isolation with synthetic fixtures; keep supported single-currency totals consistent.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E001](RECON-E001.md). Follows [RECON-S001](RECON-S001.md) as the requested delivery gate.
