import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createLatestRequestLoader } from './latestRequestLoader.js'
import { approveLedgerTransaction, subscribeLedgerMutation } from './apiClient.js'

const deferred = () => {
  let resolve; let reject
  const promise = new Promise((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

test('mutation subscription invalidates an older pending-count result through the API boundary', async () => {
  const hadWindow = Object.hasOwn(globalThis, 'window'); const originalWindow = globalThis.window
  const old = deferred(); let calls = 0; const values = []
  const loader = createLatestRequestLoader({ load: () => ++calls === 1 ? old.promise : Promise.resolve([{},{},{}]), map: (rows) => Array.isArray(rows) ? rows.length : null, update: (value) => values.push(value) })
  const unsubscribe = subscribeLedgerMutation(() => { void loader.refresh() })
  try {
    globalThis.window = { __TAURI_INTERNALS__: { invoke: async () => null } }
    const staleLoad = loader.refresh()
    await approveLedgerTransaction(1, 'a'.repeat(64), 'b'.repeat(64), 'Expenses:Food')
    old.resolve([{}])
    assert.deepEqual(await staleLoad, { stale: true })
    assert.deepEqual(values, [3])
  } finally { unsubscribe(); loader.dispose(); if (hadWindow) globalThis.window = originalWindow; else delete globalThis.window }
})

test('latest count loader ignores older success and error, and disposal prevents updates', async () => {
  const first = deferred(); const outcomes = []; let calls = 0
  const loader = createLatestRequestLoader({ load: () => ++calls === 1 ? first.promise : Promise.resolve([{},{},{}]), map: (rows) => Array.isArray(rows) ? rows.length : null, update: (value) => outcomes.push(value) })
  const older = loader.refresh(); assert.deepEqual(await loader.refresh(), { value: 3 }); first.resolve([{}]); assert.deepEqual(await older, { stale: true })
  const failed = deferred(); const next = createLatestRequestLoader({ load: () => failed.promise, update: (value) => outcomes.push(value) })
  const pending = next.refresh(); next.dispose(); failed.reject(new Error('late error')); assert.deepEqual(await pending, { stale: true }); assert.deepEqual(outcomes, [3])
})

test('older error cannot replace a newer successful count', async () => {
  const first = deferred(); let calls = 0; const values = []
  const loader = createLatestRequestLoader({ load: () => ++calls === 1 ? first.promise : Promise.resolve([{},{},{}]), map: (rows) => Array.isArray(rows) ? rows.length : null, update: (value) => values.push(value) })
  const older = loader.refresh(); assert.deepEqual(await loader.refresh(), { value: 3 }); first.reject(new Error('old failure'))
  assert.deepEqual(await older, { stale: true }); assert.deepEqual(values, [3])
})
