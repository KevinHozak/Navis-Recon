# Account and period reconciliation

## Evidence and legacy migration

The Statement Library provides an explicit reconciliation workspace. Map each PDF to a full Assets or Liabilities account, inclusive start/end dates, and currency/unit; institution-name suggestions never establish an account mapping. A single PDF can have several account mappings. Multiple PDFs in one month and overlapping periods remain separate records, and overlaps are disclosed without merging or deduplicating financial totals.

Record IDs hash statement content, account, covered dates and unit. Statement identity is the SHA-256 of the selected catalog PDF; the evidence reference retains its relative catalog path. Version-1 records live in `%LOCALAPPDATA%/Navis-Recon/reconciliation/<canonical-ledger-path-hash>/records.json`, outside Git. Changing statement bytes or mapping keys requires a new record. Re-select an existing record to edit its balances/status. Source PDFs and the ledger are never written by this workflow.

Migration is deliberately non-destructive: existing institution/month booleans and unknown checklist fields stay in their original document and display as **Legacy checked**. They do not create reconciliation records, imply receipt/import/review, or invent opening/closing evidence. Institution matching remains a PDF navigation suggestion only. Reconciliation never toggles those historical booleans.

## Lifecycle and strict reconciliation policy

Save a new mapping as **received**, then advance through **imported**, **reviewed**, and **reconciled**. Imported records are the user's explicit acknowledgment of import, not an automatic import command or proof that a legacy importer ran. Reconciled evidence may be reopened as reviewed. Each save validates identity and transitions natively; status cannot skip ahead.

Opening/closing balances are manually transcribed and reviewed by the user. Amounts use exact decimal strings with the native Money scale and range; no implicit conversion or foreign exchange is performed. Liability balances use the ledger's signed convention. The opening ledger balance is the exact account/unit balance immediately before the start date; closing includes all dated entries through the end date (end of day in this date-only ledger). Descendants are separate accounts, never silently rolled into the selected account. Parser-generated valid padding is included; unsupported or incomplete ledgers withhold comparisons.

Differences are statement minus ledger. Reconciled status requires both supplied differences to be exactly zero and every cleared transaction in the period explicitly matched. Pending flags `!` and `P` are disclosed and block completion; there is no rounding tolerance or explanation-based override. Cleared matches are user assertions, not automatic PDF transaction extraction. Account transactions outside the period remain visible as timing candidates and cannot be selected as in-period matches. Transfers are labeled candidates rather than hidden or treated as expenses. Match IDs include the exact ledger revision, so stale selections must be cleared and reviewed again.

Each save reevaluates the current ledger while holding source and PDF read handles that deny writes/deletion but permit concurrent financial-view reads, then publishes JSON only. Reads flag previously reconciled records stale if source revision or PDF identity changed or became unreadable; Refresh records rechecks freshness. The UI does not claim continuously monitored evidence. A stale record must be compared/reviewed again. Record summaries consider every explicit mapping for a statement; one reconciled account cannot hide another unresolved account.

## Persistence and conflicts

Checklist and reconciliation JSON use a raw-byte SHA-256 revision, a cooperative Windows exclusive sidecar lock shared by writers, unique create-new staged files, file sync, write-through atomic replacement, and disk readback. Read-only document loads use the atomic JSON snapshot without taking the writer lock. Whole-document saves require the expected revision and reject stale values. The lock file may remain after exit; ownership is the open handle, not its presence. Reconciliation's namespace and store are private; checklist saves preserve the existing configured checklist location.

The checklist UI serializes saves and retains its last persisted checkmarks until the response/readback succeeds. Failure leaves the persisted value visible with an explicit error; Refresh reloads the current document before a retry. The reconciliation form retains a clearly marked unsaved edit on failure; Refresh records explicitly reloads persisted values and discards that unsaved edit. Navigation suppresses late responses; backend revision checks still apply if a save completes after navigation. There is no automatic retry of ambiguous saves.

The lock coordinates Navis-Recon writers. Noncooperating external editors do not honor it: observed changes before either compare are rejected, but no filesystem-wide CAS or cross-file power-loss atomicity is claimed. Ledger/PDF write-denying read handles prevent ordinary competing writers during final evaluation on Windows. Failed atomic publication leaves prior JSON unchanged. Filesystem, schema and readback failures remain explicit.

## Verification

Synthetic native tests cover unknown-field/BOM/legacy preservation, stale saves, held locks, true replace denial, readback hashes, account disambiguation, multiple/overlapping statements, lifecycle gates, exact cutoff/currency/fractional/refund/negative-liability arithmetic, pending and transfer candidates, stale matches and stale source evidence, restart loading, and unchanged ledger bytes. Frontend tests exercise persisted-state rollback, saving/saved/failure behavior, overlap guards, late completion after disposal, malformed readback, and native IPC argument mapping without HTTP fallback. No household ledger, source PDF or credentials are test inputs.
