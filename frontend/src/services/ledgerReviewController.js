const HASH = /^[a-f\d]{64}$/i
const validCategory = (value) => typeof value === 'string' && /^(Expenses|Income):[A-Z0-9][A-Za-z0-9-]*(?::[A-Z0-9][A-Za-z0-9-]*)*$/.test(value)

export function validateApprovalHistory(rows) {
  if (!Array.isArray(rows)) throw new Error('Approval history returned an invalid response.')
  const ids = new Set()
  if (rows.some((r) => {
    if (!r || typeof r.id !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(r.id) || ids.has(r.id) ||
      !['approve', 'undo'].includes(r.operation_type) || !Number.isSafeInteger(r.timestamp_unix_ms) || r.timestamp_unix_ms < 1 || r.timestamp_unix_ms > 8640000000000000 ||
      typeof r.before_hash !== 'string' || !HASH.test(r.before_hash) || typeof r.after_hash !== 'string' || !HASH.test(r.after_hash) || !validCategory(r.old_category) || !validCategory(r.new_category) ||
      typeof r.undone !== 'boolean' || !(r.undo_of === null || (typeof r.undo_of === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(r.undo_of))) ||
      (r.operation_type === 'approve' ? r.undo_of !== null : !r.undo_of)) return true
    ids.add(r.id)
    return false
  })) throw new Error('Approval history contains malformed records.')
  return rows
}

export function transactionKey(row) {
  return `${row.line_num}:${row.transaction_id}`
}

export function clearBusyKey(busy, key) {
  const next = { ...busy }
  delete next[key]
  return next
}

export function resolveApprovalCompletion(result, mounted, key) {
  return {
    clearBusy: (busy) => clearBusyKey(busy, key),
    stale: !mounted || Boolean(result?.stale),
    error: !mounted || result?.stale || result?.approved ? null : (result?.error || 'Approval was not completed. Refresh and try again.'),
    refreshRequired: Boolean(mounted && !result?.stale && result?.approved && result?.refreshRequired),
  }
}

export function resolveUndoCompletion(result, mounted) {
  return {
    clearUndoing: mounted,
    stale: !mounted || Boolean(result?.stale),
    error: !mounted || result?.stale || result?.saved || result?.skipped ? null : (result?.error || 'Undo was not completed. Refresh history and try again.'),
    saved: Boolean(mounted && !result?.stale && result?.saved),
    refreshRequired: Boolean(mounted && !result?.stale && result?.refreshRequired),
  }
}

const validRow = (row) => row && Number.isInteger(row.line_num) && row.line_num >= 0 &&
  Number.isSafeInteger(row.line_num) &&
  typeof row.transaction_id === 'string' && HASH.test(row.transaction_id) &&
  typeof row.ledger_revision === 'string' && HASH.test(row.ledger_revision)

export function createLedgerReviewController({ list, approve, history, undo }) {
  let generation = 0
  let disposed = false
  let currentRows = []
  let listGeneration = 0
  const inFlight = new Set()
  let historyGeneration = 0
  let historyLoading = false
  const undoInFlight = new Set()

  function activate() { disposed = false; generation += 1 }
  function dispose() { disposed = true; generation += 1; historyGeneration += 1; historyLoading = false }

  async function refresh() {
    const requestGeneration = ++generation
    const requestId = ++listGeneration
    try {
      const rows = await list()
      if (disposed || requestGeneration !== generation || requestId !== listGeneration) return { stale: true }
      if (!Array.isArray(rows) || rows.some((row) => !validRow(row))) throw new Error('Pending transaction list contains malformed identity tokens.')
      if (rows.length && rows.some((row) => row.ledger_revision !== rows[0].ledger_revision)) throw new Error('Pending transaction list contains mixed ledger revisions.')
      const keys = rows.map(transactionKey)
      if (new Set(keys).size !== keys.length) throw new Error('Pending transaction list contains duplicate transaction keys.')
      currentRows = rows
      return { rows }
    } catch (error) {
      if (disposed || requestGeneration !== generation || requestId !== listGeneration) return { stale: true }
      currentRows = []
      return { rows: null, error: error instanceof Error ? error.message : String(error) }
    }
  }

  async function refreshHistory() {
    const id = ++historyGeneration
    historyLoading = true
    try {
      const rows = validateApprovalHistory(await history())
      if (id === historyGeneration) historyLoading = false
      return disposed || id !== historyGeneration ? { stale: true } : { rows }
    } catch (error) {
      if (id === historyGeneration) historyLoading = false
      return disposed || id !== historyGeneration ? { stale: true } : { error: error instanceof Error ? error.message : String(error) }
    }
  }

  function isInFlight(row) { return inFlight.has(transactionKey(row)) }

  async function approveRow(row, category) {
    const key = transactionKey(row)
    if (inFlight.has(key)) return { skipped: true }
    if (disposed || !validRow(row) || !currentRows.some((item) => transactionKey(item) === key && item.ledger_revision === row.ledger_revision))
      return { approved: false, error: 'Refresh pending transactions before approving this row.' }
    const requestGeneration = generation
    inFlight.add(key)
    try {
      await approve(row.line_num, row.transaction_id, row.ledger_revision, category)
      if (disposed) return { stale: true, approved: true }
      currentRows = []
      generation += 1 // invalidate every list started before the write
      return { approved: true, refreshRequired: true }
    } catch (error) {
      if (disposed || requestGeneration !== generation) return { stale: true }
      return { approved: false, error: error instanceof Error ? error.message : String(error) }
    } finally { inFlight.delete(key) }
  }

  async function undoRecord(record) {
    if (disposed || !undo || record.undone || undoInFlight.size || undoInFlight.has(record.id)) return { skipped: true }
    undoInFlight.add(record.id)
    const requestGeneration = generation
    try {
      await undo(record.id)
      if (disposed) return { stale: true, saved: true }
      currentRows = []
      generation += 1
      historyGeneration += 1
      historyLoading = false
      return { saved: true, refreshRequired: true }
    } catch (error) {
      return disposed || requestGeneration !== generation ? { stale: true } : { saved: false, error: error instanceof Error ? error.message : String(error) }
    } finally { undoInFlight.delete(record.id) }
  }

  return { activate, dispose, refresh, refreshHistory, approve: approveRow, undo: undoRecord, isInFlight, isUndoing: (id) => undoInFlight.has(id), hasUndoInFlight: () => undoInFlight.size > 0, isHistoryLoading: () => historyLoading }
}
