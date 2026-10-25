import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createDocumentSaver } from './documentSaver.js'
const original = { revision: 'one', checked: false }
test('failed saves restore the persisted checkmark, report failure, and permit retry', async () => {
  const states = []; let fail = true
  const saver = createDocumentSaver({ persist: async () => { if (fail) throw Error('conflict: refresh'); return { revision: 'two', checked: true } }, onChange: s => states.push(s) })
  saver.accept(original); await saver.save({ ...original, checked: true })
  assert.equal(states.at(-1).snapshot.checked, false); assert.equal(states.at(-1).status, 'failed'); assert.match(states.at(-1).error, /conflict/)
  fail = false; await saver.save({ ...original, checked: true })
  assert.equal(states.at(-1).snapshot.checked, true); assert.equal(states.at(-1).snapshot.revision, 'two'); assert.equal(states.at(-1).status, 'saved')
})
test('rapid overlapping choices cannot dispatch a second stale whole-document save', async () => {
  let resolve; let calls = 0; const states = []
  const saver = createDocumentSaver({ persist: () => { calls++; return new Promise(r => { resolve = r }) }, onChange: s => states.push(s) })
  saver.accept(original); const request = saver.save({ checked: true })
  assert.equal(saver.isBusy(), true); assert.equal(states.at(-1).snapshot.checked, false)
  assert.deepEqual(await saver.save({ checked: false }), { skipped: true }); assert.equal(saver.accept({ revision: 'foreign' }), false); assert.equal(calls, 1)
  resolve({ revision: 'two', checked: true }); await request
  assert.equal(saver.isBusy(), false); assert.equal(states.at(-1).status, 'saved')
})
test('late save completion cannot update an unmounted checklist and malformed readback never claims saved', async () => {
  let resolve; const states = []
  const saver = createDocumentSaver({ persist: () => new Promise(r => { resolve = r }), onChange: s => states.push(s) })
  saver.accept(original); const request = saver.save({ checked: true }); saver.dispose(); const before = states.length
  resolve({ revision: 'two', checked: true }); await request; assert.equal(states.length, before)
  saver.activate(); saver.accept(original); const malformed = saver.save({ checked: true }); resolve(null); await malformed
  assert.equal(states.at(-1).status, 'failed'); assert.equal(states.at(-1).snapshot.checked, false)
})
