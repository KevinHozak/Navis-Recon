const HASH = /^[a-f\d]{64}$/i

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

const validRow = (row) => row && Number.isInteger(row.line_num) && row.line_num >= 0 &&
  Number.isSafeInteger(row.line_num) &&
  typeof row.transaction_id === 'string' && HASH.test(row.transaction_id) &&
  typeof row.ledger_revision === 'string' && HASH.test(row.ledger_revision)

export function createLedgerReviewController({ list, approve }) {
  let generation = 0
  let disposed = false
  let currentRows = []
  let listGeneration = 0
  const inFlight = new Set()
  const consumed = new Set()

  function activate() { disposed = false; generation += 1 }
  function dispose() { disposed = true; generation += 1 }

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

  function isInFlight(row) { return inFlight.has(transactionKey(row)) }

  async function approveRow(row, category) {
    const key = transactionKey(row)
    if (inFlight.has(key)) return { skipped: true }
    if (validRow(row) && consumed.has(row.ledger_revision)) return { approved: false, error: 'This ledger revision was already used; refresh pending transactions.' }
    if (disposed || !validRow(row) || !currentRows.some((item) => transactionKey(item) === key && item.ledger_revision === row.ledger_revision))
      return { approved: false, error: 'Refresh pending transactions before approving this row.' }
    const requestGeneration = generation
    inFlight.add(key)
    try {
      await approve(row.line_num, row.transaction_id, row.ledger_revision, category)
      consumed.add(row.ledger_revision)
      if (disposed) return { stale: true, approved: true }
      currentRows = []
      generation += 1 // invalidate every list started before the write
      return { approved: true, refreshRequired: true }
    } catch (error) {
      if (disposed || requestGeneration !== generation) return { stale: true }
      return { approved: false, error: error instanceof Error ? error.message : String(error) }
    } finally { inFlight.delete(key) }
  }

  return { activate, dispose, refresh, approve: approveRow, isInFlight }
}
