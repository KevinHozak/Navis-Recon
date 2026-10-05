import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

const API_BASE = import.meta.env.VITE_API_URL || 'http://localhost:8080'

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
  return request('/api/vault/checklist', {
    ipcCommand: 'get_vault_checklist',
    ipcArgs: {},
  })
}

export function saveVaultChecklist(payload) {
  return request('/api/vault/checklist', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
    ipcCommand: 'save_vault_checklist',
    ipcArgs: { payload },
  })
}

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

export function approveLedgerTransaction(lineNum, category) {
  return request('/api/ledger/approve', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ line_num: lineNum, category }),
    ipcCommand: 'approve_ledger_transaction',
    ipcArgs: { lineNum, category },
  })
}

export function getLedgerSummary() {
  return request('/api/ledger/summary', {
    ipcCommand: 'get_ledger_summary',
    ipcArgs: {},
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

export function commitLedgerSync(payload) {
  if (isDesktopRuntime()) {
    return invoke('commit_ledger_sync', { payload })
  }
  return request('/api/ledger/sync/commit', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  })
}

export async function listenForSyncEvents(handlers) {
  if (!isDesktopRuntime()) return () => {}
  const unlisteners = await Promise.all(
    Object.entries(handlers).map(([eventName, handler]) =>
      listen(`sync:${eventName}`, (event) => handler(event.payload))
    )
  )
  return () => Promise.all(unlisteners.map((unlisten) => unlisten()))
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
