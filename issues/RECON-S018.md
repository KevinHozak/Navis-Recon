---
depends_on: ["RECON-S017"]
id: "RECON-S018"
parent: ["RECON-E006"]
priority: "normal"
schema_version: 1
status: "done"
title: "Bring financial attention items and freshness into one overview"
type: "enhancement"
---
## Outcome and current evidence

The overview combines attention sources with independent freshness and filtered navigation, backed by shared persisted-action invalidation.

## Target areas

OverviewWidget.jsx; App.jsx; apiClient.js

## Acceptance criteria

- [x] Show prioritized missing statements, pending reviews, failed sync items, upcoming renewals and expiring records with source-specific freshness.
- [x] Each item opens the relevant filtered view; distinguish a healthy empty queue from unavailable or stale data.
- [x] Refresh affected views and badges after persisted actions using shared invalidation rather than independent stale snapshots; preserve focus and keyboard access.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E006](RECON-E006.md). Follows [RECON-S017](RECON-S017.md) as the requested delivery gate.

## Verified delivery

Five prioritized attention sources expose independent read times/current/stale/unavailable states. Rendered record, renewal, failed-import, pending-review and missing-institution destinations passed. Actual mocked-IPC approval refreshed attention; shared-resource updates refreshed renewal/catalog rows while preserving keyboard focus and search filters, with late older reads ignored. Source failure retained visibly stale evidence rather than healthy fallback data.

Validation: frontend tests, ESLint and production Vite build passed on v1.0.15. Native regression: 185 passed, one existing mounted-drive benchmark ignored; final run used CARGO_INCREMENTAL=0 and --test-threads=1. cargo fmt --check and git diff --check passed. The first parallel native run had a checklist busy-lock failure; targeted checklist and full sequential runs passed. A later disk-space build interruption was resolved by removing only this goal's generated debug cache. See docs/monthly-close.md for policy and coverage. v1.0.15 is prepared locally; no E006 commit, push or release package is claimed.

## Final acceptance audit

Final source: v1.0.15, app fingerprint 1fe0a2d91cb0c2adf170766bfede49541eff8ebdaf534274d920d8938a8e843a. Frontend: 106 passed, ESLint clean, production build passed. Native: 185 passed and one existing mounted-drive test ignored. Final native run also proves stable ledger/PDF reads remain available while concurrent writes are denied. Filtered recovery now retries only visible safe operations; a regression test proves hidden operations are excluded and uncertain items stay read-only. Rendered renewal/catalog race checks preserve focused search controls and reject superseded reads. Formatting, whitespace and issue-store validation passed. All evidence uses synthetic inputs; temporary harness assets were removed. Source audit completed locally; no independent external review is claimed.
