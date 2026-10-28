import { test } from 'node:test'
import assert from 'node:assert/strict'
import { approveLedgerTransaction, getLedgerApprovalHistory, undoLedgerApproval, subscribeLedgerMutation, getLedgerSourceState, getLedgerSummary, getPendingLedger, startLedgerSync, commitLedgerSync, cancelSyncRun, getSyncOutcomes, reconcileLedgerSync, retryLedgerSync, revealSyncSource, registerSyncEventListeners } from './apiClient.js'

test('monthly close sends explicit scope and both evidence/document revisions through native-only IPC', async () => {
  const { getMonthlyClose, saveMonthlyClose } = await import('./apiClient.js');
  const originalWindow = globalThis.window;
  const hadWindow = Object.hasOwn(globalThis, 'window');
  const calls = [];
  const request = { month: '2026-09', currency: 'USD', accounts: ['Assets:Bank:Example'], exceptions: [] };
  const response = { metrics: [{ spending: '0.000000000000000001' }] };
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => { calls.push({ command, args }); return response; } } };
    assert.equal((await getMonthlyClose(request)).metrics[0].spending, '0.000000000000000001');
    await saveMonthlyClose(request, 'document-revision', 'evidence-revision', 'closed');
    assert.deepEqual(calls, [{ command: 'get_monthly_close', args: { request } }, { command: 'save_monthly_close', args: { request, expectedRevision: 'document-revision', expectedEvidence: 'evidence-revision', action: 'closed' } }]);
    globalThis.window = {};
    await assert.rejects(getMonthlyClose(request), /desktop shell/);
    await assert.rejects(saveMonthlyClose(request, 'revision', 'proof', 'closed'), /desktop shell/);
  } finally { if (hadWindow) globalThis.window = originalWindow; else delete globalThis.window; }
});

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
    await startLedgerSync(); await commitLedgerSync('sync-7', [{matchId:'sync-7:match-hash',fundingAccount:'Assets:Bank:Checking'}]); await cancelSyncRun('sync-7')
    assert.deepEqual(calls, [
      { command: 'start_ledger_sync', args: {} },
      { command: 'commit_ledger_sync', args: { runId: 'sync-7', payload: [{matchId:'sync-7:match-hash',fundingAccount:'Assets:Bank:Checking'}] } },
      { command: 'cancel_sync_run', args: { runId: 'sync-7' } },
    ])
  } finally { if (hadWindow) globalThis.window = originalWindow; else delete globalThis.window }
})

test('source reveal maps only opaque references to native IPC', async()=>{ const had=Object.hasOwn(globalThis,'window'),old=globalThis.window,calls=[];try{globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args})}}};await revealSyncSource({sourceRef:`src:${'a'.repeat(64)}`,runId:'r',itemId:'i'});assert.deepEqual(calls,[{command:'reveal_sync_source',args:{sourceRef:`src:${'a'.repeat(64)}`,runId:'r',itemId:'i',operationId:null}}]);await assert.rejects(revealSyncSource({sourceRef:'C:\\secret'}),/invalid/)}finally{if(had)globalThis.window=old;else delete globalThis.window}})

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


test('managed recovery APIs use exact native IPC mappings and reject desktop fallback', async () => {
  const hadWindow=Object.hasOwn(globalThis,'window'); const original=globalThis.window; const calls=[]; const ids=['f'.repeat(64)]
  try { globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args}); return []}}}; await getSyncOutcomes(); await reconcileLedgerSync(); await retryLedgerSync(ids)
    assert.deepEqual(calls,[{command:'get_sync_outcomes',args:{}},{command:'reconcile_ledger_sync',args:{}},{command:'retry_ledger_sync',args:{operationIds:ids}}])
    await assert.rejects(retryLedgerSync(['bad']),/valid durable operation IDs/)
    globalThis.window={}; await assert.rejects(getSyncOutcomes(),/requires the Tauri desktop shell/); await assert.rejects(reconcileLedgerSync(),/requires the Tauri desktop shell/); await assert.rejects(retryLedgerSync(ids),/requires the Tauri desktop shell/)
  } finally { if(hadWindow)globalThis.window=original;else delete globalThis.window }
})


test('checklist CAS and reconciliation use desktop IPC with exact string balances and explicit revisions', async () => {
  const api = await import('./apiClient.js'); const had = Object.hasOwn(globalThis, 'window'); const old = globalThis.window; const calls = []
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => { calls.push({ command, args }); return { revision: 'new' } } } }
    await api.saveVaultChecklist({ revision: 'old', year: 2026, statements: [] })
    const draft = { account: 'Assets:Cash', closing_balance: '0.000000000000000001' }
    await api.compareReconciliation(draft); await api.saveReconciliation(draft, 'snapshot'); await api.getReconciliations()
    assert.deepEqual(calls, [
      { command: 'save_vault_checklist', args: { payload: { year: 2026, statements: [] }, expectedRevision: 'old' } },
      { command: 'compare_reconciliation', args: { draft } },
      { command: 'save_reconciliation', args: { draft, expectedRevision: 'snapshot' } },
      { command: 'get_reconciliations', args: {} },
    ])
    delete globalThis.window
    await assert.rejects(api.saveVaultChecklist({ revision: 'old' }), /Tauri desktop shell/)
    await assert.rejects(api.getReconciliations(), /Tauri desktop shell/)
  } finally { if (had) globalThis.window = old; else delete globalThis.window }
})


test('household analysis and forecasts send explicit ranges, accounts and decimal settings over native IPC only', async () => {
  const api = await import('./apiClient.js'); const had = Object.hasOwn(globalThis,'window'); const old = globalThis.window; const calls = []
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command,args) => { calls.push({command,args}); return null } } }
    const request = { start: '2026-01-01', end: '2026-02-28', currency: 'EUR' }; const forecast = { as_of: '2026-03-01', liquid_accounts: ['Assets:Bank'], horizon_days: 30, currency: 'EUR', include_estimates: false }; const settings = { amount: '12.000000000000000001' }
    await api.getHouseholdAnalysis(request); await api.getCashForecast(forecast); await api.saveForecastSettings(settings,'revision')
    assert.deepEqual(calls,[{command:'get_household_analysis',args:{request}},{command:'get_cash_forecast',args:{request:forecast}},{command:'save_forecast_settings',args:{settings,expectedRevision:'revision'}}])
    delete globalThis.window; await assert.rejects(api.getCashForecast(forecast), /Tauri desktop shell/)
  } finally { if(had) globalThis.window=old; else delete globalThis.window }
})
