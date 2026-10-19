---
depends_on: ["RECON-S015"]
id: "RECON-S016"
parent: ["RECON-E005"]
priority: "normal"
schema_version: 1
status: "open"
title: "Forecast cash availability against confirmed and detected obligations"
type: "enhancement"
---
## Outcome and current evidence

Recurring-obligation detection and renewal alerts already exist; combine them with available-cash visibility rather than rebuilding detection.

## Target areas

obligations.rs; SubscriptionsWidget.jsx; analysis IPC

## Acceptance criteria

- [ ] Show a dated upcoming-bills view against explicitly selected liquid accounts, with user-controlled horizon and disclosed forecast assumptions.
- [ ] Separate confirmed obligations from detected estimates and expose confidence, last observation, price changes, trial endings and stale data.
- [ ] Do not silently treat investment assets as spendable cash or double-count imported payments; verify irregular cadence, cancelled subscriptions and insufficient-history cases.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E005](RECON-E005.md). Follows [RECON-S015](RECON-S015.md) as the requested delivery gate.
