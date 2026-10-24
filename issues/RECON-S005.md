---
depends_on: ["RECON-S004"]
id: "RECON-S005"
parent: ["RECON-E002"]
priority: "high"
schema_version: 1
status: "done"
title: "Validate the intended posting and preserve unrelated ledger text"
type: "enhancement"
---
## Outcome and current evidence

Approval currently edits the line immediately after the transaction header, which may not be the intended category posting.

## Target areas

ledger_review.rs; CategorySelector.jsx

## Acceptance criteria

- [x] Locate the category posting within the transaction block and validate the selected account against permitted open categories.
- [x] Reject ambiguous split postings or unsupported structures with an actionable explanation rather than guessing.
- [x] Preserve amounts, comments, metadata, whitespace, line endings and other transactions; synthetic metadata-before-posting and split-posting cases verify exact targeted changes.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E002](RECON-E002.md). Follows [RECON-S004](RECON-S004.md) as the requested delivery gate.

## Completion evidence

Implemented in 1.0.5: listing and locked approval share the category-posting locator, support metadata/comments before postings and category-after-bank layouts, and validate the selected Expenses/Income account against dated open/close events and its unit restrictions. Per-row permitted categories drive the disabled-aware native-button selector. Malformed headers, unsupported structures, ambiguous category splits and lifecycle events reject with actionable source-line diagnostics.

Synthetic production-path fixtures verify that only the actual header flag and category account token change; amounts, Unicode descriptions, metadata, comments, tabs, CRLF/LF, no-final-newline and unrelated transactions remain intact. S004 revision/content identity, concurrency exclusion and recovery-copy behavior are preserved. Review uses a documented conservative subset and does not balance postings or claim full Beancount validation.

Verified: 86 Rust tests passed (1 existing mounted-drive benchmark ignored), 32 frontend tests passed, lint, production web build, Rust formatting and whitespace checks passed. The web build completed before the full Rust suite. Source audit completed with two bounded repair rounds; no independent external review, household-ledger access, live desktop UI verification or executable rebuild was performed.
