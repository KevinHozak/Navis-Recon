---
depends_on: []
id: "RECON-E001"
parent: []
priority: "high"
schema_version: 1
status: "done"
title: "Accounting correctness and explainable balances"
type: "epic"
---
## Outcome

Establish a trustworthy read model before downstream workflows rely on its totals.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [x] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [x] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [x] [RECON-S001: Define and enforce supported ledger semantics](RECON-S001.md)
- [x] [RECON-S002: Use decimal amounts and currency-aware aggregation](RECON-S002.md)
- [x] [RECON-S003: Expose validated totals with provenance and coverage](RECON-S003.md)

## Completion verification (2026-10-25)

All three child stories are done with checked acceptance and recorded implementation evidence: RECON-S001 (supported semantics and fail-closed diagnostics), RECON-S002 (exact decimal money and currency isolation), and RECON-S003 (provenance, reconciled coverage, and source freshness/error states). Current source audit at a1f3baf confirmed the documented standalone Rust IPC paths and synthetic behavioral coverage; no remaining epic acceptance gaps were found. This update reconciles the epic with its already-delivered stories; no application source or version changes were needed.

Verification on application 1.0.11, Node 24.14.0 / Cargo 1.96.0:

- `cargo test --locked -- --test-threads=1`: 162 passed, 0 failed, 1 existing mounted-drive benchmark ignored. Initial sandboxed execution encountered file-publication failures and STATUS_INVALID_HANDLE; the authorized retry outside the restrictive sandbox passed.
- `cargo test --locked ledger_parser::`: 24 passed, covering unsupported includes/inference/commodity/price entries, source locations, independent currency balancing, provenance, exact account reconciliation, and complete/partial/unreadable sources.
- `npm test`: 81 passed, including exact decimal arithmetic, IPC provenance, coverage validation, failed-read suppression, freshness warnings, and response-order protection.
- `npm run lint`, `npm run build`, and `cargo fmt --check`: passed. Existing bundle-size/dynamic-import and native dead-code warnings remain.

Synthetic fixtures and disposable files only: no household ledger, credentials, or source PDFs were read or added to Git or public evidence. No independent external review, packaged executable rebuild, live household-data verification, or remote publication was performed.