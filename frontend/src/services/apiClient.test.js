import { test } from 'node:test'
import assert from 'node:assert/strict'
import { approveLedgerTransaction, getLedgerApprovalHistory, undoLedgerApproval, subscribeLedgerMutation, getLedgerSourceState, getLedgerSummary, getPendingLedger, startLedgerSync, commitLedgerSync, cancelSyncRun, registerSyncEventListeners } from './apiClient.js'

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

test('approval and undo invalidate automatically at the IPC boundary without exposing snapshots', async () => {
  const hadWindow = Object.hasOwn(globalThis, 'window'); const originalWindow = globalThis.window; const calls = []; let invalidations = 0
  const unsubscribe = subscribeLedgerMutation(() => { invalidations += 1 })
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => { calls.push({ command, args }); return [] } } }
    await getLedgerApprovalHistory(); await undoLedgerApproval('approval-123')
    assert.deepEqual(calls, [
      { command: 'get_ledger_approval_history', args: {} },
      { command: 'undo_ledger_approval', args: { recordId: 'approval-123' } },
    ])
    assert.equal(invalidations, 1)
    assert.deepEqual(calls[1].args, { recordId: 'approval-123' })
  } finally { unsubscribe(); if (hadWindow) globalThis.window = originalWindow; else delete globalThis.window }
})

test('approval auto-invalidates, isolates throwing listeners, and uncertain rejection still invalidates', async () => {
  const hadWindow = Object.hasOwn(globalThis, 'window'); const originalWindow = globalThis.window; let updates = 0; let shouldFail = false
  const removeBad = subscribeLedgerMutation(() => { throw new Error('subscriber') })
  const removeGood = subscribeLedgerMutation(() => { updates += 1 })
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async () => { if (shouldFail) throw new Error('uncertain'); return null } } }
    assert.equal(await approveLedgerTransaction(2, 'a'.repeat(64), 'b'.repeat(64), 'Expenses:Food'), null)
    assert.equal(updates, 1)
    shouldFail = true
    await assert.rejects(undoLedgerApproval('record-1'), /uncertain/)
    assert.equal(updates, 2)
    removeGood();
    shouldFail = false
    await approveLedgerTransaction(2, 'a'.repeat(64), 'b'.repeat(64), 'Expenses:Food')
    assert.equal(updates, 2)
  } finally { removeBad(); removeGood(); if (hadWindow) globalThis.window = originalWindow; else delete globalThis.window }
})

test('audited history and mutations reject outside desktop without HTTP fallback', async () => {
  const hadWindow = Object.hasOwn(globalThis, 'window'); const originalWindow = globalThis.window
  try {
    delete globalThis.window
    await assert.rejects(getLedgerApprovalHistory(), /Tauri desktop shell/)
    await assert.rejects(undoLedgerApproval('record-1'), /Tauri desktop shell/)
    await assert.rejects(approveLedgerTransaction(1, 'a'.repeat(64), 'b'.repeat(64), 'Expenses:Food'), /Tauri desktop shell/)
  } finally { if (hadWindow) globalThis.window = originalWindow; else delete globalThis.window }
})

test('sync IPC uses the native runId and payload argument mapping', async () => {
  const hadWindow = Object.hasOwn(globalThis, 'window'); const originalWindow = globalThis.window; const calls = []
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => { calls.push({ command, args }); return {} } } }
    await startLedgerSync(); await commitLedgerSync('sync-7', [{ filename: 'a.pdf' }]); await cancelSyncRun('sync-7')
    assert.deepEqual(calls, [
      { command: 'start_ledger_sync', args: {} },
      { command: 'commit_ledger_sync', args: { runId: 'sync-7', payload: [{ filename: 'a.pdf' }] } },
      { command: 'cancel_sync_run', args: { runId: 'sync-7' } },
    ])
  } finally { if (hadWindow) globalThis.window = originalWindow; else delete globalThis.window }
})

test('sync event registration cleans partial and late listeners, suppresses callbacks, and removes abort hooks', async () => {
  let removals = 0; let callback; let aborts = 0
  const signal = { aborted: false, addEventListener: (_name, fn) => { callback = fn }, removeEventListener: (_name, fn) => { if (fn === callback) aborts++ } }
  await assert.rejects(registerSyncEventListeners({ a: () => {}, b: () => {} }, async (name, handler) => {
    if (name === 'sync:b') throw new Error('listen failed')
    return () => { removals++; handler({ payload: 'late' }) }
  }, signal), /listen failed/)
  assert.equal(removals, 1); assert.equal(aborts, 1)

  let lateCallback; let lateRemoval = 0; let seen = 0
  const pending = registerSyncEventListeners({ a: () => { seen++ } }, () => new Promise(resolve => { lateCallback = resolve }), {
    aborted: false, addEventListener: (_name, fn) => { callback = fn }, removeEventListener: () => { aborts++ },
  })
  callback()
  lateCallback(() => { lateRemoval++ })
  const cleanup = await pending
  lateCallback // keep the test's captured registration explicit
  cleanup(); cleanup()
  assert.equal(lateRemoval, 1); assert.equal(seen, 0)
  assert.ok(aborts >= 3)
})
