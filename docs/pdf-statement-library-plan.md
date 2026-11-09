# PDF statement library plan

Status: proposed; implementation has not started.

## Purpose

Give households a local place to find statements, see which reporting periods are covered, open the source PDF, and record review work. Today, document discovery and payment extraction are different tasks but can appear as the same problem. A statement should be useful in the library even when it has no payment-extraction rule.

This plan develops the statement-library proposal in [the incubator](incubator.md). It is a standalone feature sequence, without requiring the broader household-records hub first. It does not authorize imports, file cleanup, or implementation.

## First release

- Select local folders and explicitly scan them for PDFs, with optional recursion.
- Browse and filter by institution, ledger account, document kind, statement date, reporting period, review state, and file availability.
- Add or correct metadata manually. Keep suggested metadata visibly separate until confirmed.
- Open a selected PDF with the operating system's default viewer.
- Record notes and review history separately from the PDF.
- Identify byte-identical files and show every known location without deleting or consolidating them.
- Search filenames and confirmed metadata.
- Export a deliberately selected metadata index as CSV or JSON.

Sources remain in their existing folders. Scanning never moves, renames, decrypts, uploads, or modifies PDFs. No ledger entries or managed-import approvals are created by this feature. External AI remains disabled.

## User workflow

1. Add a folder and choose whether to include subfolders. Show the scan boundary before starting.
2. Scan with progress, cancellation, and per-file diagnostics. Partial results must be labeled incomplete.
3. Review discovered documents in a table. Unknown institution, account, or period stays unknown.
4. Open a source, confirm its metadata, and add notes or mark it reviewed.
5. Filter for missing files, unreadable documents, unreviewed statements, or possible duplicates.
6. Once an account's cadence is configured, inspect possible coverage gaps and their explanation.

Use the existing dark dashboard styling, cyan active states, clear borders, and concise status labels. Preserve the speed and focus of ledger review with a separate Library navigation entry.

## Data model

Use separate document, location, and review records. A content hash identifies bytes; a filename or reporting period does not prove document identity.

| Record | Main fields |
|---|---|
| Library root | Stable ID, selected canonical folder, recursion choice, enabled state, last scan summary |
| Document | Stable ID, SHA-256, byte size, document kind, institution, optional ledger-account reference, statement date, period start/end, metadata provenance |
| Location | Document ID, root ID, relative path, last-seen time, availability, last verified fingerprint |
| Review event | Document ID, event ID, timestamp, action, note, relevant document hash and ledger revision when applicable |
| Coverage expectation | Account reference, effective dates, explicit cadence, grace period, exclusions and user explanation |
| Extraction cache, later | Content hash, extractor/version, page coverage, local text, diagnostics; disposable and separately removable |

Document kinds initially distinguish statement, payment receipt, bill, payroll, transfer evidence, other, and unknown. Library membership does not make a document an import candidate.

Dates use ISO calendar dates and allow unknown values. Periods may cross months or years; require start <= end. Provenance records whether each metadata field was manually confirmed or suggested, with the rule/version and source page where available. Never derive a statement period from a filename without labeling it a suggestion.

## Independent states and meanings

- Availability: present, missing, changed, or inaccessible. Scan errors are not evidence of deletion.
- Readability: not checked, readable, encrypted, unsupported, or extraction failed.
- Review: unreviewed, needs attention, reviewed, or explicitly excluded.
- Reconciliation: optional link to the existing reconciliation workflow and its evidence. Opening or reviewing a PDF does not establish reconciliation.
- Archival: location/provenance only in the first release. Being in an archive folder does not establish import or reconciliation success.

A changed PDF becomes a new content revision requiring fresh review; previous notes remain attached to the original bytes. Explicit relinking can associate a moved file with its known content. Missing locations retain their metadata and history.

## Architecture and persistence

React renders the table, filters, metadata editor, and detail panel. Native Rust owns root registration, bounded filesystem access, hashing, persistence, and safe source opening through Tauri IPC. The packaged application requires no web server or development ports and has no Navis-Web dependency.

Proposed persistence: a versioned SQLite database in a private per-user application directory outside Git, with transactional metadata/history updates and migration backups. Separate ledger workspaces by canonical ledger identity; do not merge their account associations implicitly. Confirm the storage implementation against existing native dependencies during technical design before adding a dependency.

Register roots natively. Frontend commands use backend-issued root/document/location IDs, never arbitrary paths or executable commands. Revalidate canonical containment and file identity at scan/open time; reject reparse-point escapes. Do not expose the whole filesystem through an unrestricted asset protocol. Use a safe native opener for the exact registered PDF, without shell command construction.

Scan in bounded batches with stable ordering, cancellation, resource limits, and per-file errors. Check source identity around hashing/extraction so a changing file cannot receive stale metadata. Retry permission or cloud-placeholder failures only on a subsequent explicit scan; do not trigger uncontrolled downloads. No background watcher is required initially.

Database backups preserve confirmed metadata and review history. Derived text can be rebuilt, but manual annotations cannot; provide backup/restore guidance and a validated metadata restore path. A restore must revalidate file references and never overwrite PDFs or the ledger. Cache removal is distinct from removing library records or deleting source files.

## Local extraction and search

Deliver manual indexing first. Later add a dedicated local, read-only statement metadata extractor, reusing suitable bounded runner utilities from the embedded analyzer without invoking legacy processors or financial commit paths. The current payment analyzer's first-two-page behavior is not a promise of whole-document search.

Text search must disclose indexed page coverage. Offer local OCR only as an explicit option with time/page limits and cancellation; keep it disabled by default. Encrypted PDFs remain indexed by filename and metadata with an explicit unreadable state. No password storage, password guessing, or automatic decryption.

Extracted text and thumbnails are sensitive household data. Keep them private, excluded from Git and normal diagnostic output, removable independently, and out of exports by default. Do not claim application-level encryption until it is designed and verified; document the first release's reliance on the Windows account and disk protections.

## Duplicates, coverage, and ledger links

Exact duplicates share a content record with multiple locations. Similar institution/account/period combinations are possible duplicates requiring review; corrected statements can legitimately cover the same period with different bytes.

Coverage checks require a user-confirmed account association and cadence. Support monthly, quarterly, annual, irregular, and disabled expectations, with activation/closure dates and a grace period. Start with monthly support; other cadences must be explicitly unsupported until implemented. Describe an absent match as a possible gap, not a missing financial transaction. Unconfirmed periods and incomplete scans cannot establish complete coverage.

Link documents to existing statement checklist and reconciliation records using stable references. Retain existing reconciliation evidence and ledger-revision checks. A reference becoming stale must be visible. Managed-import durable receipts may later supply verified provenance through their owning backend; archive location or equal amounts alone are insufficient proof. The library must not rewrite import receipts or duplicate their state machine.

## Delivery sequence

| Phase | Deliverable | Exit criteria |
|---|---|---|
| 1: Foundation | Private schema, root registration, bounded scan, content/location identities | Restart persistence; cancellation; missing/inaccessible/changed files; duplicate locations; transactional recovery and migration tests |
| 2: Usable library | Table, filters, metadata editor, safe OS opening, notes/review history | End-to-end scan → find → open → annotate → restart on synthetic PDFs; source hashes unchanged |
| 3: Local discovery | Suggested institution/date/period metadata and optional text indexing | Ambiguous dates stay unresolved; page coverage disclosed; encrypted/error paths and extraction limits verified; no external requests |
| 4: Coverage and reconciliation | Confirmed account links, monthly expectations, possible-gap view, existing workflow links | Cross-year periods, closure/grace dates, corrected statements and incomplete scans produce explainable results; no automatic reconciliation |
| 5: Portability and release | Selected metadata export, backup/restore, packaged acceptance and user documentation | Export preview excludes full paths/text by default; restore preserves annotations and revalidates roots; installed app works without development services |

Each phase should become small local stories only after scope approval. This document creates no issue records and sets no delivery dates. Embedded preview, thumbnails, full-text search refinements, and additional cadences can follow the first release separately.

## Acceptance and verification

- Use synthetic PDFs and disposable directories for implementation tests. Real household files require separate authorization for acceptance work.
- Compare original PDF and ledger SHA-256 values before and after all read-only acceptance flows; confirm no source/archive/rule/config mutations outside the library's own explicitly configured state.
- Test traversal, reparse-point escape, stale IDs, files replaced during scan/open, inaccessible roots, corrupt PDFs, long Windows paths, encrypted documents, cancellation, and interrupted database updates.
- Verify exact duplicates across roots, moved files, corrected statements, manual metadata persistence, review invalidation on content changes, and incomplete-scan coverage warnings.
- Exercise packaged Windows behavior with cloud storage unavailable and optional extraction tools missing. Partial failures must preserve useful metadata and actionable diagnostics.
- Run relevant native/frontend checks and the existing managed-import/reconciliation regressions for any integration changes. No live import is part of library acceptance.

## Deferred scope

Embedded PDF viewing and annotation overlays; document uploads or managed storage; source moves/renames/deletions; cloud synchronization; household multi-user permissions; payment processing; automatic ledger imports; broad household-records modules; and application-level encryption are separate decisions.

## Decisions to confirm before implementation

1. Approve OS-default viewing for the first release, with embedded viewing deferred.
2. Approve private SQLite persistence and metadata backup/restore after dependency review.
3. Choose the first statement families for metadata extraction using synthetic examples, without embedding household mappings in code.
4. Confirm how existing checklist/reconciliation IDs should be referenced after inspecting their owning modules.
5. Decide whether local text search ships in phase 3 or follows the initial manual library; establish document/page/cache limits before enabling it.

## Related documents

- [Incubator](incubator.md): broader household-records exploration.
- [Architecture](architecture.md): standalone desktop and native IPC context.
- [Reconciliation](reconciliation.md): existing reconciliation semantics.
- [Monthly close](monthly-close.md): statement completeness and closing workflow.
- [Managed sync import](managed-sync-import.md): owning import protocol and source-integrity boundaries.
