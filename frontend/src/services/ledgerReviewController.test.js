import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createLedgerReviewController, resolveApprovalCompletion, transactionKey } from './ledgerReviewController.js'

const row = (line_num = 0) => ({ line_num, transaction_id: 'b'.repeat(64), ledger_revision: 'a'.repeat(64), category: 'Expenses:Food' })
const deferred = () => {
  let resolve; let reject
  const promise = new Promise((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

test('duplicate transaction content keeps a disambiguating line identity', () => {
  assert.notEqual(transactionKey(row(0)), transactionKey(row(4)))
})

test('concurrent duplicate approval clicks invoke one mutation', async () => {
  const pending = deferred(); let calls = 0
  // Duplicate clicks are only possible for a row from the current snapshot.
  const valid = createLedgerReviewController({ list: async () => [row()], approve: () => { calls += 1; return pending.promise } })
  await valid.refresh()
  const first = valid.approve(row(), 'Expenses:Food')
  const second = await valid.approve(row(), 'Expenses:Other')
  assert.deepEqual(second, { skipped: true }); assert.equal(calls, 1)
  pending.resolve(); assert.deepEqual(await first, { approved: true, refreshRequired: true })
})

test('conflict keeps row available and refresh failures are errors, not empty success', async () => {
  // A row must come from a validated list snapshot before it can be approved.
  const offline = createLedgerReviewController({ list: async () => { throw new Error('offline') }, approve: async () => {} })
  const valid = createLedgerReviewController({ list: async () => [row()], approve: async () => { throw new Error('ledger changed') } })
  await valid.refresh()
  assert.deepEqual(await valid.approve(row(), 'Expenses:Food'), { approved: false, error: 'ledger changed' })
  const result = await offline.refresh()
  assert.equal(result.rows, null); assert.equal(result.error, 'offline')
})

test('older list results cannot replace a later refresh result', async () => {
  const first = deferred(); let count = 0
  const controller = createLedgerReviewController({ list: () => ++count === 1 ? first.promise : Promise.resolve([row(2)]), approve: async () => {} })
  const oldRequest = controller.refresh(); const latest = await controller.refresh()
  first.resolve([row(1)])
  assert.equal((await oldRequest).stale, true); assert.equal(latest.rows[0].line_num, 2)
})

test('dispose discards outstanding list result after unmount', async () => {
  const pending = deferred(); const controller = createLedgerReviewController({ list: () => pending.promise, approve: async () => {} })
  const request = controller.refresh(); controller.dispose(); pending.resolve([row()])
  assert.equal((await request).stale, true)
})

test('approval conflict completing after refresh is stale and cannot replace current rows', async () => {
  const conflict = deferred(); const latest = row(8)
  const controller = createLedgerReviewController({ list: async () => [latest], approve: () => conflict.promise })
  await controller.refresh()
  const outcome = controller.approve(latest, 'Expenses:Food')
  const refreshed = await controller.refresh()
  conflict.reject(new Error('old conflict'))
  assert.equal((await outcome).stale, true)
  assert.equal(refreshed.rows[0].line_num, 8)
})

test('successful write invalidates pre-write list and consumes its revision until a new list arrives', async () => {
  const listCall = deferred(); let calls = 0
  const firstRow = row(0); const nextRow = { ...row(1), ledger_revision: 'c'.repeat(64) }
  const controller = createLedgerReviewController({ list: () => ++calls === 1 ? Promise.resolve([firstRow]) : listCall.promise, approve: async () => {} })
  await controller.refresh()
  const olderList = controller.refresh()
  assert.deepEqual(await controller.approve(firstRow, 'Expenses:Food'), { approved: true, refreshRequired: true })
  listCall.resolve([nextRow])
  assert.equal((await olderList).stale, true)
  assert.match((await controller.approve(firstRow, 'Expenses:Food')).error, /already used/)
  await controller.refresh()
  assert.equal((await controller.approve(nextRow, 'Expenses:Food')).approved, true)
})

test('successful write invalidates a manual refresh started while mutation is pending and requires a fresh list', async () => {
  const oldList = deferred(); const mutation = deferred(); let calls = 0
  const original = row(); const fresh = { ...row(4), ledger_revision: 'c'.repeat(64) }
  const controller = createLedgerReviewController({
    list: () => ++calls === 1 ? Promise.resolve([original]) : calls === 2 ? oldList.promise : Promise.resolve([fresh]),
    approve: () => mutation.promise,
  })
  await controller.refresh()
  const write = controller.approve(original, 'Expenses:Food')
  const manualRefresh = controller.refresh()
  mutation.resolve()
  const outcome = await write
  assert.deepEqual(outcome, { approved: true, refreshRequired: true })
  oldList.resolve([{ ...original, line_num: 9 }])
  assert.equal((await manualRefresh).stale, true)
  assert.deepEqual((await controller.refresh()).rows, [fresh])
})

test('successful write after a list finishes still requires a fresh list', async () => {
  const original = row(); const pending = deferred()
  const controller = createLedgerReviewController({ list: async () => [original], approve: () => pending.promise })
  await controller.refresh()
  const write = controller.approve(original, 'Expenses:Food')
  await controller.refresh()
  pending.resolve()
  assert.deepEqual(await write, { approved: true, refreshRequired: true })
})

test('malformed or duplicate composite keys fail closed and non-Error rejections become strings', async () => {
  const malformed = createLedgerReviewController({ list: async () => [row(), { ...row(1), ledger_revision: 'not-a-hash' }], approve: async () => {} })
  assert.match((await malformed.refresh()).error, /malformed identity tokens/)
  const duplicated = row(0)
  const controller = createLedgerReviewController({ list: async () => [duplicated, { ...duplicated }], approve: async () => { throw 'native failure' } })
  assert.match((await controller.refresh()).error, /duplicate transaction keys/)
  const validController = createLedgerReviewController({ list: async () => [row()], approve: async () => { throw 'native failure' } })
  await validController.refresh()
  assert.equal((await validController.approve(row(), 'Expenses:Food')).error, 'native failure')
})

test('mixed revisions and unsafe source lines fail closed and remove snapshot authority', async () => {
  let rows = [row()]
  const controller = createLedgerReviewController({ list: async () => rows, approve: async () => {} })
  await controller.refresh()
  rows = [row(), { ...row(1), ledger_revision: 'c'.repeat(64) }]
  assert.match((await controller.refresh()).error, /mixed ledger revisions/)
  assert.match((await controller.approve(row(), 'Expenses:Food')).error, /Refresh pending/)
  rows = [{ ...row(Number.MAX_SAFE_INTEGER + 1) }]
  assert.match((await controller.refresh()).error, /malformed identity tokens/)
  assert.match((await controller.approve(row(), 'Expenses:Food')).error, /Refresh pending/)
})

test('stale rejected completion clears only its busy key without surfacing an error', () => {
  const busy = { '0:id': true, '2:other': true }
  const completion = resolveApprovalCompletion({ stale: true }, true, '0:id')
  assert.deepEqual(completion.clearBusy(busy), { '2:other': true })
  assert.equal(completion.error, null)
  assert.equal(completion.refreshRequired, false)
})

test('approval completing after dispose is stale', async () => {
  const pending = deferred(); const controller = createLedgerReviewController({ list: async () => [row()], approve: () => pending.promise })
  await controller.refresh()
  const outcome = controller.approve(row(), 'Expenses:Food')
  controller.dispose(); pending.reject(new Error('late failure'))
  assert.equal((await outcome).stale, true)
})
