import { test } from 'node:test'
import assert from 'node:assert/strict'
import { approveLedgerTransaction, getLedgerSourceState, getLedgerSummary, getPendingLedger } from './apiClient.js'

test('ledger API client preserves Tauri arguments and exact summary provenance at IPC boundary', async () => {
  const hadWindow = Object.hasOwn(globalThis, 'window')
  const originalWindow = globalThis.window
  const calls = []
  const summary = {
    ledger_revision: 'a'.repeat(64),
    total_assets: '12345678901234567890.01',
    total_liabilities: '-0.10',
    net_worth: '12345678901234567889.91',
    read_at_unix_ms: 1790000000123,
    latest_transaction_date: '2026-10-22',
  }
  const sourceState = {
    expected_revision: 'a'.repeat(64),
    observed_revision: 'a'.repeat(64),
    state: 'current',
    checked_at_unix_ms: 1790000000456,
    error: null,
  }
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => {
      calls.push({ command, args })
      return command === 'get_ledger_summary' ? summary : sourceState
    } } }
    const actualSummary = await getLedgerSummary()
    const actualSourceState = await getLedgerSourceState('a'.repeat(64))
    assert.deepEqual(calls, [
      { command: 'get_ledger_summary', args: {} },
      { command: 'get_ledger_source_state', args: { expectedRevision: 'a'.repeat(64) } },
    ])
    assert.equal(actualSummary.total_assets, '12345678901234567890.01')
    assert.equal(actualSummary.read_at_unix_ms, 1790000000123)
    assert.equal(actualSourceState.expected_revision, 'a'.repeat(64))
    assert.equal(actualSourceState.observed_revision, 'a'.repeat(64))
    assert.equal(actualSourceState.checked_at_unix_ms, 1790000000456)
  } finally {
    if (hadWindow) globalThis.window = originalWindow
    else delete globalThis.window
  }
})

test('ledger approval IPC includes line, stable transaction identity, revision, and category', async () => {
  const hadWindow = Object.hasOwn(globalThis, 'window')
  const originalWindow = globalThis.window
  const calls = []
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => { calls.push({ command, args }) } } }
    await approveLedgerTransaction(7, 'b'.repeat(64), 'a'.repeat(64), 'Expenses:Food')
    assert.deepEqual(calls, [{ command: 'approve_ledger_transaction', args: { lineNum: 7, transactionId: 'b'.repeat(64), ledgerRevision: 'a'.repeat(64), category: 'Expenses:Food' } }])
  } finally {
    if (hadWindow) globalThis.window = originalWindow
    else delete globalThis.window
  }
})

test('pending ledger IPC preserves per-row permission snapshot', async () => {
  const hadWindow = Object.hasOwn(globalThis, 'window')
  const originalWindow = globalThis.window
  const permitted = ['Expenses:Food', 'Income:Refund']
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async () => [{ line_num: 3, permitted_categories: permitted }] } }
    const rows = await getPendingLedger()
    assert.deepEqual(rows[0].permitted_categories, permitted)
  } finally {
    if (hadWindow) globalThis.window = originalWindow
    else delete globalThis.window
  }
})
