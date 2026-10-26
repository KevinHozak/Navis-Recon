---
depends_on: ["RECON-S016"]
id: "RECON-S017"
parent: ["RECON-E006"]
priority: "normal"
schema_version: 1
status: "done"
title: "Preserve signed balances and complete asset-allocation coverage"
type: "enhancement"
---
## Outcome and current evidence

Overview now preserves signed balances and includes cash and other assets, with exact allocation reconciliation and accessible text coverage.

## Target areas

OverviewWidget.jsx; financial formatting utilities

## Acceptance criteria

- [x] Preserve negative asset and refund signs; label liability presentation explicitly without losing underlying sign semantics.
- [x] Include cash and disclose any asset categories excluded from allocation; chart totals reconcile to the displayed allocation basis.
- [x] Use consistent currency formatting, accessible text alternatives and clear unavailable states. Verify negative assets, zero values, cash-only accounts and liabilities.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E006](RECON-E006.md). Follows [RECON-S016](RECON-S016.md) as the requested delivery gate.

## Verified delivery

Signed asset, debt and refund balances; cash and other-asset coverage; positive pie basis plus negative offsets reconcile exactly. Synthetic allocation cases and rendered exact-value/table checks passed, including cyan accent and unavailable/zero coverage. Native and frontend full regression checks passed.

Validation: frontend tests, ESLint and production Vite build passed on v1.0.15. Native regression: 185 passed, one existing mounted-drive benchmark ignored; final run used CARGO_INCREMENTAL=0 and --test-threads=1. cargo fmt --check and git diff --check passed. The first parallel native run had a checklist busy-lock failure; targeted checklist and full sequential runs passed. A later disk-space build interruption was resolved by removing only this goal's generated debug cache. See docs/monthly-close.md for policy and coverage. v1.0.15 is prepared locally; no E006 commit, push or release package is claimed.

## Final acceptance audit

Final source: v1.0.15, app fingerprint 1fe0a2d91cb0c2adf170766bfede49541eff8ebdaf534274d920d8938a8e843a. Frontend: 106 passed, ESLint clean, production build passed. Native: 185 passed and one existing mounted-drive test ignored. Final native run also proves stable ledger/PDF reads remain available while concurrent writes are denied. Filtered recovery now retries only visible safe operations; a regression test proves hidden operations are excluded and uncertain items stay read-only. Rendered renewal/catalog race checks preserve focused search controls and reject superseded reads. Formatting, whitespace and issue-store validation passed. All evidence uses synthetic inputs; temporary harness assets were removed. Source audit completed locally; no independent external review is claimed.
