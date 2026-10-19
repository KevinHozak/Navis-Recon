---
depends_on: ["RECON-E003"]
id: "RECON-E004"
parent: []
priority: "normal"
schema_version: 1
status: "open"
title: "Evidence-backed statement reconciliation"
type: "epic"
---
## Outcome

Track the evidence behind reconciliation rather than only a checked month.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [ ] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [ ] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [ ] [RECON-S011: Store reconciliation records by account and statement period](RECON-S011.md)
- [ ] [RECON-S012: Compare statement closing balances with ledger balances](RECON-S012.md)
- [ ] [RECON-S013: Make checklist saves conflict-aware and visibly durable](RECON-S013.md)
