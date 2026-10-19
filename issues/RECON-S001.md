---
depends_on: []
id: "RECON-S001"
parent: ["RECON-E001"]
priority: "high"
schema_version: 1
status: "open"
title: "Define and enforce supported ledger semantics"
type: "enhancement"
---
## Outcome and current evidence

The native reader recognizes simple numeric postings; currencies, prices, costs, inferred amounts and include directives are not evaluated by the current parser.

## Target areas

ledger_parser.rs; config.rs; docs/architecture.md

## Acceptance criteria

- [ ] Document supported directives and explicitly detect unsupported constructs with source locations; do not silently omit them.
- [ ] Prevent incomplete evaluation from appearing as a validated total; distinguish unavailable, partial and validated results.
- [ ] Use synthetic include, inferred-posting, commodity, price and malformed-entry fixtures; define an explicit supported or rejected outcome for each. Preserve standalone Rust IPC runtime.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E001](RECON-E001.md). This is the first story in the requested delivery order.
