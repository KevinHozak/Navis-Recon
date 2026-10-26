---
depends_on: ["RECON-S018"]
id: "RECON-S019"
parent: ["RECON-E006"]
priority: "normal"
schema_version: 1
status: "done"
title: "Guide monthly close and save a local evidence summary"
type: "enhancement"
---
## Outcome and current evidence

Monthly close now checks current ledger/statement evidence and explicit account scope, then saves a private local close or reopen summary.

## Target areas

overview and reconciliation views; local close persistence

## Acceptance criteria

- [x] Guide collect statements, import, review, reconcile and summarize for a selected month, showing outstanding blockers and their owners or resolution links.
- [x] Close requires validated current data and resolved reconciliation criteria; reopen or mark stale when underlying evidence changes.
- [x] Save a local summary referencing statement evidence, ledger revision, unresolved exceptions and period metrics; no automatic remote publication. Verify a synthetic complete close, blocked close and post-close source change.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E006](RECON-E006.md). Follows [RECON-S018](RECON-S018.md) as the requested delivery gate.

## Verified delivery

Guided month/account scope, staged blockers with owners/resolution links, current full-month statement coverage and private close/reopen history implemented. Five native file-backed close tests cover complete persistence/readback, exact metrics, blocked pending/import/coverage cases, ledger/PDF changes, source locks, shared-PDF accounts, CAS rejection and non-waivable mapped evidence. Rendered close, stale-after-change, disabled reclose, reopen and retained draft scope passed.

Validation: frontend tests, ESLint and production Vite build passed on v1.0.15. Native regression: 185 passed, one existing mounted-drive benchmark ignored; final run used CARGO_INCREMENTAL=0 and --test-threads=1. cargo fmt --check and git diff --check passed. The first parallel native run had a checklist busy-lock failure; targeted checklist and full sequential runs passed. A later disk-space build interruption was resolved by removing only this goal's generated debug cache. See docs/monthly-close.md for policy and coverage. v1.0.15 is prepared locally; no E006 commit, push or release package is claimed.

## Final acceptance audit

Final source: v1.0.15, app fingerprint 1fe0a2d91cb0c2adf170766bfede49541eff8ebdaf534274d920d8938a8e843a. Frontend: 106 passed, ESLint clean, production build passed. Native: 185 passed and one existing mounted-drive test ignored. Final native run also proves stable ledger/PDF reads remain available while concurrent writes are denied. Filtered recovery now retries only visible safe operations; a regression test proves hidden operations are excluded and uncertain items stay read-only. Rendered renewal/catalog race checks preserve focused search controls and reject superseded reads. Formatting, whitespace and issue-store validation passed. All evidence uses synthetic inputs; temporary harness assets were removed. Source audit completed locally; no independent external review is claimed.
