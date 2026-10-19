---
depends_on: []
id: "RECON-E001"
parent: []
priority: "high"
schema_version: 1
status: "open"
title: "Accounting correctness and explainable balances"
type: "epic"
---
## Outcome

Establish a trustworthy read model before downstream workflows rely on its totals.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [ ] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [ ] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [ ] [RECON-S001: Define and enforce supported ledger semantics](RECON-S001.md)
- [ ] [RECON-S002: Use decimal amounts and currency-aware aggregation](RECON-S002.md)
- [ ] [RECON-S003: Expose validated totals with provenance and coverage](RECON-S003.md)
