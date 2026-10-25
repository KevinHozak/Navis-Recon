import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createAnalysisController } from './analysisController.js'
const deferred = () => { let resolve; let reject; const promise = new Promise((yes,no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
test('date and unit changes invalidate old analysis, retain request identity, and clear unavailable totals', async () => {
  const first = deferred(); const states = []; const seen = []
  const controller = createAnalysisController({ load: request => { seen.push(request); return seen.length === 1 ? first.promise : Promise.resolve({ currency: request.currency, amount: '0.000000000000000001' }) }, onChange: state => states.push(state) })
  const pending = controller.run({ currency: 'USD' }); controller.invalidate(); await controller.run({ currency: 'EUR' }); first.resolve({ currency: 'USD' })
  assert.equal((await pending).stale, true); assert.equal(states.at(-1).data.currency, 'EUR'); assert.equal(states.at(-1).data.amount, '0.000000000000000001'); assert.deepEqual(seen, [{ currency: 'USD' }, { currency: 'EUR' }])
})
test('failed validated reads become unavailable rather than a stale or zero-balance result', async () => {
  const states = []; let fail = false
  const controller = createAnalysisController({ load: async () => { if (fail) throw Error('Ledger not validated'); return { opening_cash: '10' } }, onChange: state => states.push(state) })
  await controller.run({}); fail = true; await controller.run({})
  assert.equal(states.at(-1).data, null); assert.match(states.at(-1).error, /not validated/); assert.equal(states.at(-1).loading, false)
})
test('disposed and superseded requests cannot publish; activation permits a fresh snapshot', async () => {
  const gate = deferred(); const states = []
  const controller = createAnalysisController({ load: () => gate.promise, onChange: state => states.push(state) })
  const request = controller.run({}); controller.dispose(); const count = states.length; gate.reject(Error('late')); assert.equal((await request).stale,true); assert.equal(states.length,count)
  controller.activate(); await controller.run({}); assert.match(states.at(-1).error,/late/)
})
