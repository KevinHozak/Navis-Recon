---
depends_on: ["RECON-S005"]
id: "RECON-S006"
parent: ["RECON-E002"]
priority: "high"
schema_version: 1
status: "open"
title: "Persist an approval audit trail and conflict-safe undo"
type: "enhancement"
---
## Outcome and current evidence

Approval currently has no user-visible persisted undo history.

## Target areas

ledger_review.rs; LedgerReviewWidget.jsx; local persistence

## Acceptance criteria

- [ ] Record before/after transaction identity, category change and timestamp only after a successful write; keep household content local and outside Git.
- [ ] Undo restores only this approval after verifying the current transaction matches its recorded after-state; concurrent edits block undo safely.
- [ ] Show in-flight, saved, conflict and failure states, prevent duplicate submission, and refresh dependent counts and totals after approval or undo. Verify restart recovery and failed writes.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E002](RECON-E002.md). Follows [RECON-S005](RECON-S005.md) as the requested delivery gate.
