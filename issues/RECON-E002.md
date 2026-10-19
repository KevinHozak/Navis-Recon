---
depends_on: ["RECON-E001"]
id: "RECON-E002"
parent: []
priority: "high"
schema_version: 1
status: "open"
title: "Safe transaction approvals with conflict protection and undo"
type: "epic"
---
## Outcome

Approve the intended transaction while preserving ledger formatting and concurrent changes.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [ ] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [ ] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [ ] [RECON-S004: Identify pending transactions by content and reject stale approvals](RECON-S004.md)
- [ ] [RECON-S005: Validate the intended posting and preserve unrelated ledger text](RECON-S005.md)
- [ ] [RECON-S006: Persist an approval audit trail and conflict-safe undo](RECON-S006.md)
