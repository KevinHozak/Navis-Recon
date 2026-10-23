---
depends_on: []
id: "RECON-S001"
parent: ["RECON-E001"]
priority: "high"
schema_version: 1
status: "done"
title: "Define and enforce supported ledger semantics"
type: "enhancement"
---
## Outcome and current evidence

The native reader recognizes simple numeric postings; currencies, prices, costs, inferred amounts and include directives are not evaluated by the current parser.

## Target areas

ledger_parser.rs; config.rs; docs/architecture.md

## Acceptance criteria

- [x] Document supported directives and explicitly detect unsupported constructs with source locations; do not silently omit them.
- [x] Prevent incomplete evaluation from appearing as a validated total; distinguish unavailable, partial and validated results.
- [x] Use synthetic include, inferred-posting, commodity, price and malformed-entry fixtures; define an explicit supported or rejected outcome for each. Preserve standalone Rust IPC runtime.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E001](RECON-E001.md). This is the first story in the requested delivery order.

## Completion evidence (2026-10-23)

Implemented the documented explicit USD subset and a shared validated reader for summary, history, drill-downs, and obligations. Unsupported syntax reports filename and line and withholds financial results; unreadable, invalid UTF-8, and empty sources report unavailable. Synthetic fixtures cover includes, inferred postings, commodities, prices, malformed entries, currency mismatch, balance tolerance, and preserved monthly carry-forward.

Verification on application version 1.0.1: cargo test --locked passed (43 passed, 1 existing ignored mounted-drive benchmark); frontend lint, production build, and cargo fmt --check passed. Existing atomic-file tests required execution outside the restrictive sandbox. Source audit completed after two repair rounds. No independent external review or packaged executable rebuild was performed. Floating-point amounts remain pending RECON-S002.
