---
depends_on: ["RECON-S008"]
id: "RECON-S009"
parent: ["RECON-E003"]
priority: "high"
schema_version: 1
status: "done"
title: "Make commits idempotent with durable per-file outcomes"
type: "enhancement"
---
## Outcome and current evidence

The bridge returns overall success from process exit and passes through result messages; durable per-item recovery is not established in this repo.

## Target areas

sync.rs; harvester bridge contract; SyncWidget.jsx

## Acceptance criteria

- [x] Inspect the external harvester contract before implementation and document whether external changes are required; do not assume it already prevents duplicate imports.
- [x] Validate approved source identities and destinations; persist succeeded, failed and uncertain outcomes keyed to run and item identity.
- [x] On retry or restart reconcile actual effects and retry only verified safe items; prevent duplicate ledger imports and accidental archive overwrites. Verify interrupted and partial commits using disposable files.

## Verification and privacy

Use synthetic ledgers, disposable files and a fake harvester where applicable. Do not use or publish real household financial data. Preserve formatting and unrelated work; no Navis-Web coupling, production web server, automatic financial mutation or remote publication is implied. Record observed outcomes, not static keyword assertions.

## Parent and sequence

Part of [RECON-E003](RECON-E003.md). Follows [RECON-S008](RECON-S008.md) as the requested delivery gate.

## Completion evidence

Implemented in 1.0.9. Before implementation, read-only inspection of the installed legacy analyzer/committer established that it supplied match metadata but no content precondition, durable item key, idempotent ledger marker or recoverable per-file effects; its move/log/append sequence could partially succeed. The native managed importer replaces production legacy --commit. External analysis code remains unchanged; no external idempotency is assumed. The supported subset rejects encrypted transformations and unsupported ledger syntax before mutation.

Analysis captures private immutable source/destination plans with content hashes, sizes and ledger evidence. Managed commits resolve backend match IDs, verify approved contents and destinations, append exact balanced Money transactions only after native validation, and use create-only archives under Statements YYYY. Durable private receipts and recovery copies stay outside Git, protected by journal/ledger/source exclusion. Succeeded, failed and uncertain outcomes retain run/item and durable operation identity. Sources are retained for manual cleanup; explicit sync_inbox_path and sync_counter_account configuration is required.

Restart reconciliation checks the exact owned archive and complete ledger operation block. Verified completed effects become succeeded without duplicate append. Verified no-effect intents become safely retryable; owned archive recovery may complete under locked checks. Ambiguous, changed or corrupt evidence stays uncertain and cannot be blindly replayed or overwrite an archive. Safe retry is explicit, ID-only and uses coordinator ownership; concurrent starts cannot replace an approved plan. The Sync view shows recovery outcomes even while idle, preserves useful results on refresh failure, and guards retry/pending/reconnect/disposal behavior.

Verified with synthetic disposable files and fake analyzers: 137 Rust tests passed (1 existing mounted-drive benchmark ignored), 72 frontend tests passed; lint, production web build, Rust formatting and whitespace checks passed. Web build completed before the full native suite. Source audit completed two bounded repair rounds. No independent external review, real household-ledger/production-harvester execution, live desktop UI verification or executable rebuild occurred. Supported-subset and recovery limits are documented in docs/managed-sync-import.md; no power-loss atomicity across separate ledger/archive/journal files is claimed.