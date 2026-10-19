---
depends_on: ["RECON-S011"]
id: "RECON-S012"
parent: ["RECON-E004"]
priority: "normal"
schema_version: 1
status: "open"
title: "Compare statement closing balances with ledger balances"
type: "enhancement"
---
## Outcome and current evidence

Statement files are cataloged, but no balance-comparison workspace was found in the inspected view.

## Target areas

ledger_parser.rs; reconciliation IPC; StatementLibraryWidget.jsx

## Acceptance criteria

- [ ] Accept or review statement opening and closing balances and compare with validated ledger balances at the exact statement cutoff.
- [ ] Show currency-aware difference and candidate unmatched transactions; transfers, pending items and timing differences remain explicit.
- [ ] Block reconciled status while the difference is unresolved, unless an explicit recorded explanation is permitted by the documented reconciliation policy. Never change ledger amounts to force agreement.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E004](RECON-E004.md). Follows [RECON-S011](RECON-S011.md) as the requested delivery gate.
