import { invoke } from '@tauri-apps/api/core'

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

export { API_BASE }
