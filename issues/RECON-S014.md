---
depends_on: ["RECON-S013"]
id: "RECON-S014"
parent: ["RECON-E005"]
priority: "normal"
schema_version: 1
status: "open"
title: "Correct chronological monthly history and support date ranges"
type: "enhancement"
---
## Outcome and current evidence

History accumulates transactions in file order; carried-forward months reuse a previous snapshot label.

## Target areas

ledger_parser.rs; WealthDashboardWidget.jsx

## Acceptance criteria

- [ ] Sort transactions chronologically with deterministic same-date handling before producing month-end balances.
- [ ] Emit the correct unique month label for every carried-forward snapshot; distinguish missing evidence from unchanged balances.
- [ ] Support explicit date ranges and verify out-of-order entries, empty months, year boundaries and the existing trailing-24-month behavior with synthetic fixtures.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E005](RECON-E005.md). Follows [RECON-S013](RECON-S013.md) as the requested delivery gate.
