---
depends_on: ["RECON-S003"]
id: "RECON-S004"
parent: ["RECON-E002"]
priority: "high"
schema_version: 1
status: "open"
title: "Identify pending transactions by content and reject stale approvals"
type: "enhancement"
---
## Outcome and current evidence

Approval uses a line number and checks only that the current header contains a pending flag.

## Target areas

ledger_review.rs; LedgerReviewWidget.jsx; apiClient.js

## Acceptance criteria

- [ ] Return a stable transaction identity plus original-content or ledger revision token for each pending item.
- [ ] Before writing, verify the exact transaction still matches; insertion, reordering or edits must never approve a different pending transaction.
- [ ] On conflict keep the item unapproved, offer refresh and preserve external edits; test concurrent and repeated approval requests.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E002](RECON-E002.md). Follows [RECON-S003](RECON-S003.md) as the requested delivery gate.
