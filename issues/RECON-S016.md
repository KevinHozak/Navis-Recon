---
depends_on: ["RECON-S015"]
id: "RECON-S016"
parent: ["RECON-E005"]
priority: "normal"
schema_version: 1
status: "done"
title: "Forecast cash availability against confirmed and detected obligations"
type: "enhancement"
---
## Outcome and current evidence

Recurring-obligation detection and renewal alerts already exist; combine them with available-cash visibility rather than rebuilding detection.

## Target areas

obligations.rs; SubscriptionsWidget.jsx; analysis IPC

## Acceptance criteria

- [x] Show a dated upcoming-bills view against explicitly selected liquid accounts, with user-controlled horizon and disclosed forecast assumptions.
- [x] Separate confirmed obligations from detected estimates and expose confidence, last observation, price changes, trial endings and stale data.
- [x] Do not silently treat investment assets as spendable cash or double-count imported payments; verify irregular cadence, cancelled subscriptions and insufficient-history cases.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E005](RECON-E005.md). Follows [RECON-S015](RECON-S015.md) as the requested delivery gate.


## Completion evidence (2026-10-25)

Implemented a dated cash/bills workspace in Subscriptions using the existing recurring-obligation analyzer. Users explicitly select bank/cash accounts, native unit, as-of date and 1–366 day horizon. Investment/retirement/property assets are not selected or liquidated. Cash remains unavailable without selected-account observations. Confirmed and optional detector-estimate scenarios disclose last observations, pattern-support confidence, stale data, price changes, active-trial endings, funding assumptions, pending ledger amounts and daily closing cash shortfalls.

Revision-checked forecast settings support confirmed manual/irregular/one-off commitments, funding maps and cancellations while preserving unrelated registry metadata. A general subscription edit cannot restore stale forecast settings; cancelled entries are excluded from active subscription summaries. Unknown payee identity and ambiguous/noncash funding withhold projections with warnings. Recorded future movements count once; matching payment/category occurrences suppress projections, and already-paid one-offs remain in opening cash. Unresolved confirmed overdue bills receive an explicit reserve; additional historical arrears are not invented. Same-day events share end-of-day balances rather than assuming an intraday sequence.

Synthetic tests verify detector reuse, future imported payment deduplication, already-paid one-offs, manual confirmations, cancellation, irregular/insufficient history, stale observations, price increases, trial endings, negative availability, unit isolation, investment exclusion, absent selected cash, overdue reserves, metadata readback/preservation and stale-setting conflicts. Policy and limitations, including the explicit three-day payment-match window and absence of automatic income/valuation, are documented in docs/household-analysis.md.

Verified on prepared application version 1.0.14, Node 24.14.0 / Cargo 1.96.0: cargo test --locked -- --test-threads=1 passed 180 tests (1 existing mounted-drive benchmark ignored); npm test passed 89 tests; npm run lint, npm run build, cargo fmt --check and git diff --check passed. Source audit completed after targeted repairs; no independent external review, live desktop UI exercise, release executable rebuild, household-ledger/statement access or remote publication. Changes remain local and uncommitted. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.
