---
depends_on: ["RECON-E003"]
id: "RECON-E004"
parent: []
priority: "normal"
schema_version: 1
status: "done"
title: "Evidence-backed statement reconciliation"
type: "epic"
---
## Outcome

Track the evidence behind reconciliation rather than only a checked month.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [x] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [x] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [x] [RECON-S011: Store reconciliation records by account and statement period](RECON-S011.md)
- [x] [RECON-S012: Compare statement closing balances with ledger balances](RECON-S012.md)
- [x] [RECON-S013: Make checklist saves conflict-aware and visibly durable](RECON-S013.md)


## Completion evidence (2026-10-25)

Implemented and verified S011, S012 and S013 in prepared application version 1.0.13. The Statement Library now supports explicit account/period evidence and exact balance comparison; legacy month flags remain visibly separate. Native records enforce lifecycle, source identity, zero-difference/matched-transaction gates, stale-evidence detection and private persistence. Checklist saves are revision-checked and visibly durable. Full policy and verification boundaries are documented in docs/reconciliation.md and linked from architecture.md.

All three child records contain acceptance-specific evidence. Tests exercise synthetic migration, account disambiguation, overlapping/multiple statements, exact cutoff/currency/negative/fractional balances, unresolved/pending/stale gates, failed publication, conflicts, readback and overlapping frontend requests. No household financial data, credentials or source PDFs were read, added to Git or included in public evidence. The standalone Rust IPC runtime and ledger amounts are preserved.

Verified on prepared version 1.0.13, Node 24.14.0 / Cargo 1.96.0: cargo test --locked -- --test-threads=1 passed 171 tests (1 existing mounted-drive benchmark ignored); npm test passed 85 tests; lint, production frontend build, Rust formatting and whitespace checks passed. Synthetic ledgers, disposable JSON and fake PDF bytes only. Source audit completed; no independent external review, live desktop UI exercise, release executable rebuild, household-data access or remote publication. Changes remain local and uncommitted. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.
