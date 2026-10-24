---
depends_on: ["RECON-S003"]
id: "RECON-S004"
parent: ["RECON-E002"]
priority: "high"
schema_version: 1
status: "done"
title: "Identify pending transactions by content and reject stale approvals"
type: "enhancement"
---
## Outcome and current evidence

Approval uses a line number and checks only that the current header contains a pending flag.

## Target areas

ledger_review.rs; LedgerReviewWidget.jsx; apiClient.js

## Acceptance criteria

- [x] Return a stable transaction identity plus original-content or ledger revision token for each pending item.
- [x] Before writing, verify the exact transaction still matches; insertion, reordering or edits must never approve a different pending transaction.
- [x] On conflict keep the item unapproved, offer refresh and preserve external edits; test concurrent and repeated approval requests.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E002](RECON-E002.md). Follows [RECON-S003](RECON-S003.md) as the requested delivery gate.

## Completion evidence

Implemented in 1.0.4: pending rows carry SHA-256 transaction identity and raw-ledger revision. Approvals compare the full revision and exact pending content under serialized Windows-exclusive access; stale, reordered, edited and repeated requests reject without changing bytes. UI retains conflicts with Refresh, guards duplicate submissions and late results, and reloads after every success.

Before in-place writes, a unique recovery copy is synced beside the source. Caught write errors attempt restoration. This is not crash-atomic; non-Windows approval fails closed. Complex posting selection and durable audit/undo remain S005/S006.

Verified using synthetic fixtures only: 71 Rust tests passed, 1 existing mounted-drive benchmark ignored; 30 frontend tests, lint, production web build, Rust formatting and whitespace checks passed. The first final Rust compile overlapped generated-asset replacement; it passed when rerun after the web build. Source audit completed with two bounded repair rounds. No independent external review, household-ledger access, live desktop UI test or executable rebuild was performed.
