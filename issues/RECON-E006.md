---
depends_on: ["RECON-E005"]
id: "RECON-E006"
parent: []
priority: "normal"
schema_version: 1
status: "done"
title: "Actionable financial overview and monthly-close experience"
type: "epic"
---
## Outcome

Present trustworthy state and connect the financial workflow from collection through close.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [x] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [x] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [x] [RECON-S017: Preserve signed balances and complete asset-allocation coverage](RECON-S017.md)
- [x] [RECON-S018: Bring financial attention items and freshness into one overview](RECON-S018.md)
- [x] [RECON-S019: Guide monthly close and save a local evidence summary](RECON-S019.md)

## Verified delivery

All three child stories completed and verified locally. No household data, credentials or source PDFs were added; tests used disposable synthetic files and mocked IPC. Standalone native commands own persistence; no Navis-Web coupling or remote publication was added.

Validation: frontend tests, ESLint and production Vite build passed on v1.0.15. Native regression: 185 passed, one existing mounted-drive benchmark ignored; final run used CARGO_INCREMENTAL=0 and --test-threads=1. cargo fmt --check and git diff --check passed. The first parallel native run had a checklist busy-lock failure; targeted checklist and full sequential runs passed. A later disk-space build interruption was resolved by removing only this goal's generated debug cache. See docs/monthly-close.md for policy and coverage. v1.0.15 is prepared locally; no E006 commit, push or release package is claimed.

## Final acceptance audit

Final source: v1.0.15, app fingerprint 1fe0a2d91cb0c2adf170766bfede49541eff8ebdaf534274d920d8938a8e843a. Frontend: 106 passed, ESLint clean, production build passed. Native: 185 passed and one existing mounted-drive test ignored. Final native run also proves stable ledger/PDF reads remain available while concurrent writes are denied. Filtered recovery now retries only visible safe operations; a regression test proves hidden operations are excluded and uncertain items stay read-only. Rendered renewal/catalog race checks preserve focused search controls and reject superseded reads. Formatting, whitespace and issue-store validation passed. All evidence uses synthetic inputs; temporary harness assets were removed. Source audit completed locally; no independent external review is claimed.
