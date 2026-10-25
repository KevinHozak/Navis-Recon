---
depends_on: ["RECON-S013"]
id: "RECON-S014"
parent: ["RECON-E005"]
priority: "normal"
schema_version: 1
status: "done"
title: "Correct chronological monthly history and support date ranges"
type: "enhancement"
---
## Outcome and current evidence

History accumulates transactions in file order; carried-forward months reuse a previous snapshot label.

## Target areas

ledger_parser.rs; WealthDashboardWidget.jsx

## Acceptance criteria

- [x] Sort transactions chronologically with deterministic same-date handling before producing month-end balances.
- [x] Emit the correct unique month label for every carried-forward snapshot; distinguish missing evidence from unchanged balances.
- [x] Support explicit date ranges and verify out-of-order entries, empty months, year boundaries and the existing trailing-24-month behavior with synthetic fixtures.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E005](RECON-E005.md). Follows [RECON-S013](RECON-S013.md) as the requested delivery gate.


## Completion evidence (2026-10-25)

Implemented stable chronological history with deterministic same-date source ordering, correct unique labels and explicit carried-forward coverage. Other-unit-only entries cannot manufacture selected-unit evidence. The existing history command retains latest-24-ledger-month behavior; the Wealth view also provides explicit dates, unit and comparison dates through the native household analysis snapshot. Applying a range updates both charts and the monthly table from that same snapshot; reset invalidates pending range responses before restoring default charts.

Month boundaries clip to the exact requested dates. The table distinguishes no-currency-evidence months with unavailable balances from unchanged carried balances; charts omit pre-evidence dates and show carried coverage without implying statement completeness. Synthetic tests cover reversed/out-of-order entries, same-date ordering, empty months, year boundaries, partial cutoffs, unique labels, a 30-month fixture retaining the final 24 months, and invalid dates. Async-controller tests cover superseded and unmounted requests. Policy: docs/household-analysis.md.

Verified on prepared application version 1.0.14, Node 24.14.0 / Cargo 1.96.0: cargo test --locked -- --test-threads=1 passed 180 tests (1 existing mounted-drive benchmark ignored); npm test passed 89 tests; npm run lint, npm run build, cargo fmt --check and git diff --check passed. Source audit completed after targeted repairs; no independent external review, live desktop UI exercise, release executable rebuild, household-ledger/statement access or remote publication. Changes remain local and uncommitted. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.
