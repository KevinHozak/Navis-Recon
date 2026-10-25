import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

const API_BASE = import.meta.env?.VITE_API_URL || 'http://localhost:8080'

export const isDesktopRuntime = () =>
  typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__)

const parseResponse = async (response) => {
  if (!response.ok) {
    throw new Error(`Request failed (${response.status})`)
  }
  return response.json()
}

/**
 * Shared client boundary for the HTTP development server and Tauri IPC.
 *
 * HTTP remains the default for the existing FastAPI endpoints. A caller can
 * provide an IPC command for desktop-only commands without coupling React
 * components to either transport.
 */
export async function request(path, options = {}) {
  const { ipcCommand, ipcArgs, ...fetchOptions } = options

  if (isDesktopRuntime() && ipcCommand) {
    return invoke(ipcCommand, ipcArgs)
  }

  const response = await fetch(`${API_BASE}${path}`, fetchOptions)
  return parseResponse(response)
}

export function ping() {
  if (!isDesktopRuntime()) return Promise.resolve('http')

  return request('/health', {
    ipcCommand: 'ping',
    ipcArgs: {},
  })
}

export function getVaultChecklist() {
  return desktopRequest('get_vault_checklist', {})
}
export function saveVaultChecklist(payload) {
  const { revision, ...document } = payload
  return desktopRequest('save_vault_checklist', { payload: document, expectedRevision: revision })
}
export function getReconciliations() { return desktopRequest('get_reconciliations', {}) }
export function saveReconciliation(draft, expectedRevision) { return desktopRequest('save_reconciliation', { draft, expectedRevision }) }
export function compareReconciliation(draft) { return desktopRequest('compare_reconciliation', { draft }) }

export function getPendingLedger() {
  return request('/api/ledger/pending', {
    ipcCommand: 'get_pending_ledger',
    ipcArgs: {},
  })
}

export function getLedgerCategories() {
  return request('/api/ledger/categories', {
    ipcCommand: 'get_ledger_categories',
    ipcArgs: {},
  })
}

export function approveLedgerTransaction(lineNum, transactionId, ledgerRevision, category) {
  return auditedMutation({
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ line_num: lineNum, transaction_id: transactionId, ledger_revision: ledgerRevision, category }),
    ipcCommand: 'approve_ledger_transaction',
    ipcArgs: { lineNum, transactionId, ledgerRevision, category },
  })
}

export function getLedgerApprovalHistory() {
  return desktopRequest('get_ledger_approval_history', {})
}

export function undoLedgerApproval(recordId) {
  return auditedMutation({ ipcCommand: 'undo_ledger_approval', ipcArgs: { recordId } })
}

const mutationListeners = new Set()
export function subscribeLedgerMutation(listener) { mutationListeners.add(listener); return () => mutationListeners.delete(listener) }
export function invalidateLedgerViews() { for (const listener of mutationListeners) { try { listener() } catch (error) { console.error('Ledger mutation subscriber failed:', error) } } }

function desktopRequest(command, args) {
  if (!isDesktopRuntime()) return Promise.reject(new Error('Audited ledger review requires the Tauri desktop shell.'))
  return invoke(command, args)
}

async function auditedMutation(options) {
  try {
    const { ipcCommand, ipcArgs } = options
    const result = await desktopRequest(ipcCommand, ipcArgs)
    invalidateLedgerViews()
    return result
  } catch (error) {
    // A rejected IPC call may have committed before its response was lost.
    invalidateLedgerViews()
    throw error
  }
}

export function getLedgerSummary() {
  return request('/api/ledger/summary', {
    ipcCommand: 'get_ledger_summary',
    ipcArgs: {},
  })
}

export function getLedgerSourceState(expectedRevision) {
  return request('/api/ledger/source-state', {
    ipcCommand: 'get_ledger_source_state',
    ipcArgs: { expectedRevision },
  })
}

export function getLedgerHistory() {
  return request('/api/ledger/history', {
    ipcCommand: 'get_ledger_history',
    ipcArgs: {},
  })
}

export function getLedgerTransactions(prefix) {
  return request(`/api/ledger/transactions?prefix=${encodeURIComponent(prefix)}`, {
    ipcCommand: 'get_ledger_transactions',
    ipcArgs: { prefix },
  })
}

export function startLedgerSync() {
  if (!isDesktopRuntime()) {
    return Promise.reject(new Error('Ledger sync requires the Tauri desktop shell.'))
  }
  return invoke('start_ledger_sync')
}

export function getSyncRun() {
  if (!isDesktopRuntime()) return Promise.reject(new Error('Ledger sync requires the Tauri desktop shell.'))
  return invoke('get_sync_run')
}

export function commitLedgerSync(runId, payload) {
  if (!isDesktopRuntime()) return Promise.reject(new Error('Ledger sync requires the Tauri desktop shell.'))
  return invoke('commit_ledger_sync', { runId, payload })
}

export function cancelSyncRun(runId) {
  if (!isDesktopRuntime()) return Promise.reject(new Error('Ledger sync requires the Tauri desktop shell.'))
  return invoke('cancel_sync_run', { runId })
}

export function revealSyncSource(refs) {
  if (!isDesktopRuntime()) return Promise.reject(new Error('Source reveal requires the Tauri desktop shell.'))
  if (!refs || typeof refs.sourceRef !== 'string' || !/^src:[a-f0-9]{64}$/i.test(refs.sourceRef)) return Promise.reject(new Error('Source reference is invalid.'))
  return invoke('reveal_sync_source', { sourceRef: refs.sourceRef, runId: refs.runId ?? null, itemId: refs.itemId ?? null, operationId: refs.operationId ?? null })
}

// Managed import recovery is a native-only boundary because it reads and may
// mutate the user's financial ledger and private receipt store.
export function getSyncReceipts() {
  if (!isDesktopRuntime()) return Promise.reject(new Error('Sync receipts require the Tauri desktop shell.'))
  return invoke('get_sync_receipts')
}

export function getSyncOutcomes() {
  if (!isDesktopRuntime()) return Promise.reject(new Error('Managed import recovery requires the Tauri desktop shell.'))
  return invoke('get_sync_outcomes')
}

export function reconcileLedgerSync() {
  if (!isDesktopRuntime()) return Promise.reject(new Error('Managed import recovery requires the Tauri desktop shell.'))
  return invoke('reconcile_ledger_sync')
}

export function retryLedgerSync(operationIds) {
  if (!isDesktopRuntime()) return Promise.reject(new Error('Managed import recovery requires the Tauri desktop shell.'))
  if (!Array.isArray(operationIds) || operationIds.some(id => typeof id !== 'string' || !/^[a-f0-9]{64}$/i.test(id))) return Promise.reject(new Error('Retry requires valid durable operation IDs.'))
  return invoke('retry_ledger_sync', { operationIds })
}

export async function registerSyncEventListeners(handlers, listenFn, signal) {
  const unlisteners = []
  let disposed = false
  const cleanup = () => {
    if (disposed) return
    disposed = true
    signal?.removeEventListener('abort', cleanup)
    unlisteners.splice(0).forEach((unlisten) => unlisten())
  }
  if (signal?.aborted) return cleanup
  signal?.addEventListener('abort', cleanup, { once: true })
  try {
    for (const [eventName, handler] of Object.entries(handlers)) {
      if (disposed) break
      const unlisten = await listenFn(`sync:${eventName}`, (event) => { if (!disposed) handler(event.payload) })
      if (disposed) unlisten()
      else unlisteners.push(unlisten)
    }
  } catch (error) {
    cleanup()
    throw error
  }
  signal?.removeEventListener('abort', cleanup)
  return cleanup
}

export function listenForSyncEvents(handlers, { signal } = {}) {
  if (!isDesktopRuntime()) return Promise.resolve(() => {})
  return registerSyncEventListeners(handlers, listen, signal)
}

export async function subscribeSyncResourceChanges(listener) {
  if (!isDesktopRuntime()) return () => {}
  return listen('sync:resources_changed', event => listener(event.payload))
}

export function getStatementCatalog(customPath = null) {
  return request(`/api/statements/catalog${customPath ? `?path=${encodeURIComponent(customPath)}` : ''}`, {
    ipcCommand: 'get_statement_catalog',
    ipcArgs: { customPath: customPath || null },
  })
}

export function openStatementFile(path) {
  if (isDesktopRuntime()) {
    return invoke('open_statement_file', { path })
  }
  return request('/api/statements/open', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
    ipcCommand: 'open_statement_file',
    ipcArgs: { path },
  })
}

export function getRecurringObligations() {
  return request('/api/obligations/recurring', {
    ipcCommand: 'get_recurring_obligations',
    ipcArgs: {},
  })
}

export function getSubscriptionRegistry() {
  return request('/api/obligations/registry', {
    ipcCommand: 'get_subscription_registry',
    ipcArgs: {},
  })
}

export function saveSubscriptionMetadata(metadata) {
  if (isDesktopRuntime()) {
    return invoke('save_subscription_metadata', { metadata })
  }
  return request('/api/obligations/metadata', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(metadata),
    ipcCommand: 'save_subscription_metadata',
    ipcArgs: { metadata },
  })
}

export function getRecords() {
  return request('/api/records', {
    ipcCommand: 'get_records',
    ipcArgs: {},
  })
}

export function saveRecord(record) {
  if (isDesktopRuntime()) {
    return invoke('save_record', { record })
  }
  return request('/api/records', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(record),
    ipcCommand: 'save_record',
    ipcArgs: { record },
  })
}

export function deleteRecord(id) {
  if (isDesktopRuntime()) {
    return invoke('delete_record', { id })
  }
  return request(`/api/records/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    ipcCommand: 'delete_record',
    ipcArgs: { id },
  })
}

export function getRecordsRadar(asOfDate = null) {
  return request(`/api/records/radar${asOfDate ? `?as_of=${encodeURIComponent(asOfDate)}` : ''}`, {
    ipcCommand: 'get_records_radar',
    ipcArgs: { asOfDate: asOfDate || null },
  })
}

export function revealRecordFile(filePath) {
  if (isDesktopRuntime()) {
    return invoke('reveal_record_file', { filePath })
  }
  return request('/api/records/reveal', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ file_path: filePath }),
    ipcCommand: 'reveal_record_file',
    ipcArgs: { filePath },
  })
}

export { API_BASE }

export function getHouseholdAnalysis(request) { return desktopRequest('get_household_analysis', { request }) }
export function getCashForecast(request) { return desktopRequest('get_cash_forecast', { request }) }
export function saveForecastSettings(settings, expectedRevision) { return desktopRequest('save_forecast_settings', { settings, expectedRevision }) }
