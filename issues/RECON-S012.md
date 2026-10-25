---
depends_on: ["RECON-S011"]
id: "RECON-S012"
parent: ["RECON-E004"]
priority: "normal"
schema_version: 1
status: "done"
title: "Compare statement closing balances with ledger balances"
type: "enhancement"
---
## Outcome and current evidence

Statement files are cataloged, but no balance-comparison workspace was found in the inspected view.

## Target areas

ledger_parser.rs; reconciliation IPC; StatementLibraryWidget.jsx

## Acceptance criteria

- [x] Accept or review statement opening and closing balances and compare with validated ledger balances at the exact statement cutoff.
- [x] Show currency-aware difference and candidate unmatched transactions; transfers, pending items and timing differences remain explicit.
- [x] Block reconciled status while the difference is unresolved, unless an explicit recorded explanation is permitted by the documented reconciliation policy. Never change ledger amounts to force agreement.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E004](RECON-E004.md). Follows [RECON-S011](RECON-S011.md) as the requested delivery gate.


## Completion evidence (2026-10-25)

Implemented native exact-decimal opening/closing comparison and a Statement Library workspace. Opening uses exact account/unit postings before the start date; closing includes entries through the inclusive end date, including validated padding. The UI accepts manually reviewed statement balances, reports statement-minus-ledger differences and revision, and exposes user-matched/unmatched candidates with pending, transfer and outside-period timing labels. Flags are retained in the shared native transaction read model without changing amounts.

The documented strict policy permits no difference override: both balances must be supplied and match exactly; all cleared period transactions must be explicitly matched; pending period entries block completion. Match IDs bind the ledger revision. Final save revalidates the ledger under an exclusive source read handle and holds the PDF evidence handle; it never writes either source. Previously reconciled evidence becomes stale on Refresh after changed/unreadable ledger or PDF evidence. All explicit account mappings must be current/reconciled before a statement leaves the unreviewed filter.

Synthetic tests verify exact cutoff inclusion/exclusion, fractional cents, refunds, mixed-unit isolation, negative liabilities, transfer/pending flags, invalid and stale selections, missing balances, nonzero-difference rejection, lifecycle gates, stale PDF/ledger revisions and unchanged ledger bytes. Policy: docs/reconciliation.md.

Verified on prepared version 1.0.13, Node 24.14.0 / Cargo 1.96.0: cargo test --locked -- --test-threads=1 passed 171 tests (1 existing mounted-drive benchmark ignored); npm test passed 85 tests; lint, production frontend build, Rust formatting and whitespace checks passed. Synthetic ledgers, disposable JSON and fake PDF bytes only. Source audit completed; no independent external review, live desktop UI exercise, release executable rebuild, household-data access or remote publication. Changes remain local and uncommitted. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.
