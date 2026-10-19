---
depends_on: ["RECON-S004"]
id: "RECON-S005"
parent: ["RECON-E002"]
priority: "high"
schema_version: 1
status: "open"
title: "Validate the intended posting and preserve unrelated ledger text"
type: "enhancement"
---
## Outcome and current evidence

Approval currently edits the line immediately after the transaction header, which may not be the intended category posting.

## Target areas

ledger_review.rs; CategorySelector.jsx

## Acceptance criteria

- [ ] Locate the category posting within the transaction block and validate the selected account against permitted open categories.
- [ ] Reject ambiguous split postings or unsupported structures with an actionable explanation rather than guessing.
- [ ] Preserve amounts, comments, metadata, whitespace, line endings and other transactions; synthetic metadata-before-posting and split-posting cases verify exact targeted changes.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E002](RECON-E002.md). Follows [RECON-S004](RECON-S004.md) as the requested delivery gate.
