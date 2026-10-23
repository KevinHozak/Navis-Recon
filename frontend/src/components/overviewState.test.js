import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createOverviewController, validateLedgerSummary } from './overviewState.js'

const revision = char => char.repeat(64)
const sourceReply = (expected_revision, state) => ({ expected_revision, observed_revision: state === 'unavailable' ? null : state === 'current' ? expected_revision : revision('f'), state, checked_at_unix_ms: 1790000001000, error: null })
function fixture(overrides = {}) {
  const coverage = [
    { account: 'Assets:Cash', currency: 'USD', balance: '-2', included_in_net_worth: true, reason: 'included_selected_unit_asset' },
    { account: 'Assets:Cash:Wallet', currency: 'USD', balance: '5', included_in_net_worth: true, reason: 'included_selected_unit_asset' },
    { account: 'Assets:Empty', currency: 'USD', balance: '0', included_in_net_worth: true, reason: 'included_selected_unit_asset' },
    { account: 'Liabilities:Card', currency: 'USD', balance: '-3', included_in_net_worth: true, reason: 'included_selected_unit_liability' },
    { account: 'Equity:Opening', currency: 'USD', balance: '0', included_in_net_worth: false, reason: 'outside_net_worth' },
    { account: 'Assets:Cash', currency: 'EUR', balance: '900', included_in_net_worth: false, reason: 'unvalued_unit' },
    { account: 'Assets:VTI', currency: 'VTI', balance: '2', included_in_net_worth: false, reason: 'unvalued_unit' },
  ]
  return {
    status: 'validated', diagnostics: [], selected_currency: 'USD', currency: 'USD',
    ledger_revision: revision('a'), read_at_unix_ms: 1790000000000, latest_transaction_date: '2026-09-22',
    valuation_basis: 'native_units_no_conversion', validation_status: 'validated',
    total_assets: '3', total_liabilities: '-3', net_worth: '0', coverage,
    transaction_count: 2, posting_count: 7, account_count: 6,
    selected_included_account_count: 4, selected_excluded_account_count: 1,
    unvalued_units: ['EUR', 'VTI'], totals_by_currency: {}, bank_accounts: {}, retirement: {}, investments: {}, credit_cards: {}, housing: '0', cash: '0',
    ...overrides,
  }
}
function deferred() {
  let resolve
  let reject
  const promise = new Promise((res, rej) => { resolve = res; reject = rej })
  return { promise, resolve, reject }
}
const tick = () => new Promise(resolve => setImmediate(resolve))

test('validates exact selected-unit coverage with negative assets, liabilities, parents, zero rows and excluded units', () => {
  const result = validateLedgerSummary(fixture())
  assert.deepEqual(result, { valid: true, coveredAssets: '3.00', coveredLiabilities: '-3.00', coveredNetWorth: '0.00' })
})

test('rejects malformed coverage, mismatch, and unknown or missing provenance', () => {
  assert.match(validateLedgerSummary(fixture({ coverage: fixture().coverage.slice(0, 1) })).error, /counts|reconcile/)
  assert.match(validateLedgerSummary(fixture({ net_worth: '1' })).error, /reconcile/)
  assert.equal(validateLedgerSummary(fixture({ valuation_basis: 'fx' })).valid, false)
  assert.equal(validateLedgerSummary(fixture({ ledger_revision: undefined })).valid, false)
  const malformedSummaries = [
    { coverage: [...fixture().coverage, { ...fixture().coverage[2] }], selected_included_account_count: 5 },
    { coverage: fixture().coverage.map(row => row.account === 'Equity:Opening' && row.currency === 'USD' ? { ...row, reason: 'included_selected_unit_asset', included_in_net_worth: true } : row), selected_included_account_count: 5, selected_excluded_account_count: 0 },
    { coverage: fixture().coverage.map(row => row.account === 'Assets:Empty' ? { ...row, reason: 'outside_net_worth', included_in_net_worth: false } : row), selected_included_account_count: 3, selected_excluded_account_count: 2 },
    { coverage: fixture().coverage.map(row => row.account === 'Liabilities:Card' ? { ...row, reason: 'included_selected_unit_asset' } : row), total_assets: '0', total_liabilities: '0', net_worth: '0' },
    { coverage: fixture().coverage.map(row => row.account === 'Liabilities:Card' ? { ...row, reason: 'outside_net_worth', included_in_net_worth: false } : row), selected_included_account_count: 3, selected_excluded_account_count: 2, total_assets: '3', total_liabilities: '0', net_worth: '3' },
  ]
  for (const overrides of malformedSummaries) assert.equal(validateLedgerSummary(fixture(overrides)).valid, false)
  for (const read_at_unix_ms of [-1, 1.5, Number.MAX_SAFE_INTEGER, 8_640_000_000_000_001]) assert.equal(validateLedgerSummary(fixture({ read_at_unix_ms })).valid, false)
  for (const latest_transaction_date of ['2026-02-30', '2026-2-03', '']) assert.equal(validateLedgerSummary(fixture({ latest_transaction_date })).valid, latest_transaction_date === '')
})

test('failed partial or unreadable summary clears totals; later refresh can recover', async () => {
  const outcomes = [Promise.reject(new Error('partial: unsupported syntax at line 4')), Promise.reject(new Error('unavailable: configured file access denied')), Promise.resolve(fixture())]
  const controller = createOverviewController({ loadSummary: () => outcomes.shift(), checkSource: async rev => sourceReply(rev, 'current'), onChange: () => {} })
  await controller.refresh()
  assert.equal(controller.snapshot().summary, null)
  assert.match(controller.snapshot().error, /^partial:/)
  await controller.refresh()
  assert.equal(controller.snapshot().summary, null)
  assert.match(controller.snapshot().error, /^unavailable:/)
  await controller.refresh()
  assert.equal(controller.snapshot().summary.ledger_revision, revision('a'))
  await tick()
  assert.equal(controller.snapshot().source, 'current')
  controller.dispose()
})

test('changed, unavailable and malformed probe results are conservative', async () => {
  const results = [async rev => sourceReply(rev, 'changed'), async rev => ({ ...sourceReply(rev, 'unavailable'), error: 'probe detail' }), async rev => ({ ...sourceReply(rev, 'current'), expected_revision: 'wrong' })]
  const controller = createOverviewController({ loadSummary: async () => fixture(), checkSource: async rev => results.shift()(rev), onChange: () => {} })
  await controller.refresh(); await tick()
  assert.equal(controller.snapshot().source, 'changed')
  await controller.check();
  assert.equal(controller.snapshot().source, 'unavailable')
  await controller.check();
  assert.equal(controller.snapshot().source, 'unavailable')
  controller.dispose()
})

test('retains stale and unavailable warning while a follow-up probe is pending', async () => {
  const pending = deferred()
  let next = sourceReply(revision('a'), 'changed')
  const controller = createOverviewController({ loadSummary: async () => fixture(), checkSource: () => next instanceof Promise ? next : Promise.resolve(next), onChange: () => {} })
  await controller.refresh(); await tick()
  assert.equal(controller.snapshot().source, 'changed')
  next = pending.promise
  const checkingChanged = controller.check()
  assert.equal(controller.snapshot().source, 'changed')
  assert.equal(controller.snapshot().sourceChecking, true)
  pending.resolve(sourceReply(revision('a'), 'current'))
  await checkingChanged
  assert.equal(controller.snapshot().source, 'current')
  next = sourceReply(revision('a'), 'changed')
  await controller.check()
  next = { ...sourceReply(revision('a'), 'unavailable'), error: 'file became unavailable' }
  await controller.check()
  assert.equal(controller.snapshot().source, 'unavailable')
  assert.equal(controller.snapshot().sourceStale, true)
  next = sourceReply(revision('a'), 'current')
  await controller.check()
  assert.equal(controller.snapshot().sourceStale, false)

  next = { ...sourceReply(revision('a'), 'unavailable'), error: 'file is temporarily unavailable' }
  await controller.check()
  assert.equal(controller.snapshot().source, 'unavailable')
  const deferredUnavailable = deferred()
  next = deferredUnavailable.promise
  const checkingUnavailable = controller.check()
  assert.equal(controller.snapshot().source, 'unavailable')
  assert.equal(controller.snapshot().sourceError, 'file is temporarily unavailable')
  deferredUnavailable.resolve({ ...sourceReply(revision('a'), 'unavailable'), error: 'still unavailable' })
  await checkingUnavailable
  controller.dispose()
})

test('malformed freshness hashes, timestamps and current errors become unavailable', async () => {
  const bad = [
    rev => ({ ...sourceReply(rev, 'current'), observed_revision: revision('b') }),
    rev => ({ ...sourceReply(rev, 'current'), error: 'unexpected error' }),
    rev => ({ ...sourceReply(rev, 'changed'), observed_revision: 'not-a-hash' }),
    rev => ({ ...sourceReply(rev, 'current'), checked_at_unix_ms: Number.MAX_SAFE_INTEGER }),
  ]
  for (const makeReply of bad) {
    const controller = createOverviewController({ loadSummary: async () => fixture(), checkSource: async rev => makeReply(rev), onChange: () => {} })
    await controller.refresh(); await tick()
    assert.equal(controller.snapshot().source, 'unavailable')
    controller.dispose()
  }
})

test('ignores out-of-order summary and freshness responses across refresh generations', async () => {
  const summaries = [deferred(), deferred(), deferred()]
  const probeB = deferred()
  const probeC = deferred()
  const probes = [probeB, probeC]
  let index = 0
  const controller = createOverviewController({ loadSummary: () => summaries[index++].promise, checkSource: () => probes.shift().promise, onChange: () => {} })
  const oldSummary = controller.refresh()
  const newSummary = controller.refresh()
  summaries[1].resolve(fixture({ ledger_revision: revision('b') }))
  await newSummary
  summaries[0].resolve(fixture({ ledger_revision: revision('a') }))
  await oldSummary
  assert.equal(controller.snapshot().summary.ledger_revision, revision('b'))

  // A response tied to an earlier displayed snapshot cannot change a later one.
  const refreshWithPendingProbe = controller.refresh()
  summaries[2].resolve(fixture({ ledger_revision: revision('c') }))
  await refreshWithPendingProbe
  assert.equal(controller.snapshot().summary.ledger_revision, revision('c'))
  probeB.resolve(sourceReply(revision('b'), 'changed'))
  await tick()
  assert.equal(controller.snapshot().source, 'unknown')
  assert.equal(controller.snapshot().sourceChecking, true)
  probeC.resolve(sourceReply(revision('c'), 'current'))
  await tick()
  assert.equal(controller.snapshot().source, 'current')
  controller.dispose()
})

test('guards overlapping probes and suppresses callbacks after dispose', async () => {
  const probe = deferred()
  let probeCalls = 0
  let changes = 0
  const controller = createOverviewController({ loadSummary: async () => fixture(), checkSource: () => { probeCalls += 1; return probe.promise }, onChange: () => { changes += 1 } })
  await controller.refresh()
  await controller.check()
  assert.equal(probeCalls, 1)
  const beforeDispose = changes
  controller.dispose()
  probe.resolve(sourceReply(revision('a'), 'changed'))
  await tick()
  assert.equal(changes, beforeDispose)
})
