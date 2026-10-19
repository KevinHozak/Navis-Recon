---
depends_on: ["RECON-E002"]
id: "RECON-E003"
parent: []
priority: "high"
schema_version: 1
status: "open"
title: "Reliable and recoverable statement syncing"
type: "epic"
---
## Outcome

Give every sync a clear identity, honest outcome and safe recovery path.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [ ] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [ ] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [ ] [RECON-S007: Add backend-owned sync run identity and concurrency control](RECON-S007.md)
- [ ] [RECON-S008: Preserve review selections and report analysis failures honestly](RECON-S008.md)
- [ ] [RECON-S009: Make commits idempotent with durable per-file outcomes](RECON-S009.md)
- [ ] [RECON-S010: Add cancellation, bounded subprocess handling and sync receipts](RECON-S010.md)
