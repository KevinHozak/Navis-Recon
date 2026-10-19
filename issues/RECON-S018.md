---
depends_on: ["RECON-S017"]
id: "RECON-S018"
parent: ["RECON-E006"]
priority: "normal"
schema_version: 1
status: "open"
title: "Bring financial attention items and freshness into one overview"
type: "enhancement"
---
## Outcome and current evidence

Pending reviews, obligation alerts, records alerts and statement status exist in separate views.

## Target areas

OverviewWidget.jsx; App.jsx; apiClient.js

## Acceptance criteria

- [ ] Show prioritized missing statements, pending reviews, failed sync items, upcoming renewals and expiring records with source-specific freshness.
- [ ] Each item opens the relevant filtered view; distinguish a healthy empty queue from unavailable or stale data.
- [ ] Refresh affected views and badges after persisted actions using shared invalidation rather than independent stale snapshots; preserve focus and keyboard access.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E006](RECON-E006.md). Follows [RECON-S017](RECON-S017.md) as the requested delivery gate.
