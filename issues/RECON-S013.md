---
depends_on: ["RECON-S012"]
id: "RECON-S013"
parent: ["RECON-E004"]
priority: "normal"
schema_version: 1
status: "open"
title: "Make checklist saves conflict-aware and visibly durable"
type: "enhancement"
---
## Outcome and current evidence

Checklist toggles update UI optimistically and a failed save can leave the changed checkmark visible.

## Target areas

checklist.rs; VaultChecklist.jsx; StatementLibraryWidget.jsx

## Acceptance criteria

- [ ] Use a revision token or equivalent compare-before-write contract to reject stale whole-document saves without losing concurrent updates.
- [ ] Show saving and saved states; on failure restore the last persisted value or keep a clearly marked unsaved edit with retry.
- [ ] Prevent rapid overlapping saves from overwriting newer choices and verify disk readback, write failure and conflict scenarios with disposable fixtures.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E004](RECON-E004.md). Follows [RECON-S012](RECON-S012.md) as the requested delivery gate.
