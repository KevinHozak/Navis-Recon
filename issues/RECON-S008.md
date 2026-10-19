---
depends_on: ["RECON-S007"]
id: "RECON-S008"
parent: ["RECON-E003"]
priority: "high"
schema_version: 1
status: "open"
title: "Preserve review selections and report analysis failures honestly"
type: "enhancement"
---
## Outcome and current evidence

Every incoming match currently selects all accumulated matches; the done handler enters review for both success and failure.

## Target areas

SyncWidget.jsx; apiClient.js

## Acceptance criteria

- [ ] Use stable match IDs and preserve manual deselection as new matches arrive.
- [ ] Distinguish successful, partial and failed analysis, retain useful diagnostics, and define when reviewed successful items may be committed.
- [ ] Disable reset and conflicting actions during commit; listener registration and teardown cannot lose ownership or leak after unmount. Verify incremental results and failed analysis.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E003](RECON-E003.md). Follows [RECON-S007](RECON-S007.md) as the requested delivery gate.
