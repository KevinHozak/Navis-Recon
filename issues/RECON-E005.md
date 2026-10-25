---
depends_on: ["RECON-E004"]
id: "RECON-E005"
parent: []
priority: "normal"
schema_version: 1
status: "done"
title: "Accurate history and practical household cash-flow analysis"
type: "epic"
---
## Outcome

Explain changes and upcoming commitments using validated accounting data.

## Scope and evidence

Based on the source review in this chat. Findings are source observations, not verified household-data failures. Preserve the standalone Tauri desktop runtime and unrelated local work.

## Acceptance criteria

- [x] Every linked child story meets its unchecked acceptance criteria with recorded verification.
- [x] No household financial data, credentials or source PDFs are added to Git or public evidence.

## Delivery order

Epic and story dependencies encode the user-requested rollout order; they are planning gates, not claims that all work is technically inseparable.

## Stories in delivery order

- [x] [RECON-S014: Correct chronological monthly history and support date ranges](RECON-S014.md)
- [x] [RECON-S015: Explain monthly income, spending and net-worth changes](RECON-S015.md)
- [x] [RECON-S016: Forecast cash availability against confirmed and detected obligations](RECON-S016.md)


## Completion evidence (2026-10-25)

Implemented and verified S014, S015 and S016 in prepared version 1.0.14. Wealth now supports corrected chronological history, explicit ranges and currency-aware movement/category explanations with source drill-downs. Subscriptions adds explicit-liquid-account upcoming-bill scenarios built on the existing detector, with confirmed versus estimated commitments, guarded settings and disclosed evidence/assumptions. Native financial amounts remain exact decimal strings; unsupported data withholds results and no ledger amounts or source documents are mutated.

All three child stories contain acceptance-specific synthetic evidence. Missing early-period evidence is unavailable, transfers/card repayments are not double-counted as spending, imported payment occurrences are counted once under the documented matching rule, and cancelled/insufficient/stale cases remain explicit. docs/household-analysis.md records policy and limitations and is linked from architecture.md. No household financial data, credentials or source PDFs were read, added to Git or included in public evidence. The standalone Rust IPC runtime is preserved.

Verified on prepared application version 1.0.14, Node 24.14.0 / Cargo 1.96.0: cargo test --locked -- --test-threads=1 passed 180 tests (1 existing mounted-drive benchmark ignored); npm test passed 89 tests; npm run lint, npm run build, cargo fmt --check and git diff --check passed. Source audit completed after targeted repairs; no independent external review, live desktop UI exercise, release executable rebuild, household-ledger/statement access or remote publication. Changes remain local and uncommitted. Existing native dead-code and frontend bundle-size/dynamic-import warnings remain.
