---
depends_on: ["RECON-S010"]
id: "RECON-S011"
parent: ["RECON-E004"]
priority: "normal"
schema_version: 1
status: "done"
title: "Store reconciliation records by account and statement period"
type: "enhancement"
---
## Outcome and current evidence

Reconciliation currently toggles an institution/month boolean and uses institution-name matching.

## Target areas

statements.rs; checklist.rs; VaultChecklist.jsx; StatementLibraryWidget.jsx

## Acceptance criteria

- [x] Store explicit account, statement identity, covered period, currency, review status and evidence reference with stable IDs.
- [x] Support received, imported, reviewed and reconciled transitions; preserve historical booleans as legacy status without inventing balance evidence.
- [x] Allow explicit account mapping and disambiguation for multiple accounts at one institution. Verify migration, overlapping periods and multiple statements per month using synthetic data.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E004](RECON-E004.md). Follows [RECON-S010](RECON-S010.md) as the requested delivery gate.


## Completion evidence (2026-10-25)

Implemented explicit account/unit and inclusive statement-period records with SHA-256 statement identity, stable mapping IDs, relative evidence references, reviewed decimal inputs, status and comparison provenance. A versioned private per-ledger store stays outside Git. The Statement Library supports explicit mapping and several records for one institution, PDF or month; overlapping periods are disclosed rather than merged. Native lifecycle gates enforce received → imported → reviewed → reconciled, with reopen to reviewed. Imported is user acknowledgment and never starts an import.

Legacy migration is non-destructive: the original institution/month booleans and unknown fields remain intact and display as Legacy checked; no balance evidence or reconciliation status is invented. New reconciliation records never toggle legacy booleans. Synthetic tests verify BOM/unknown-field/legacy preservation, same-PDF multiple-account IDs, overlapping periods, multiple statement files in one month, stable duplicate mapping identity, external evidence rejection and restart reads. Policy: docs/reconciliation.md.

Verified on prepared version 1.0.13, Node 24.14.0 / Cargo 1.96.0: cargo test --locked -- --test-threads=1 passed 171 tests (1 existing mounted-drive benchmark ignored); npm test passed 85 tests; lint, production frontend build, Rust formatting and whitespace checks passed. Synthetic ledgers, disposable JSON and fake PDF bytes only. Source audit completed; no independent external review, live desktop UI exercise, release executable rebuild, household-data access or remote publication. Changes remain local and uncommitted. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.
