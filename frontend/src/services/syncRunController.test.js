import test from 'node:test'
import assert from 'node:assert/strict'
import { createSyncRunController, reviewSelection } from './syncRunController.js'

const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r }); return { promise, resolve } }
const snapshot = (run_id, revision, status = 'idle') => ({ run_id, revision, sequence: revision, status, logs: [], matches: [], unmatched: [], errors: [], result: null })
const event = run => ({ run_id: run.run_id, revision: run.revision, sequence: run.sequence, status: run.status, snapshot: run })

test('subscribes before snapshot and ignores the older snapshot after an event', async () => {
  const gate = deferred(); let handlers; const seen = []
  const controller = createSyncRunController({ api: {
    listenForSyncEvents: async value => { handlers = value; return () => {} },
    getSyncRun: () => gate.promise,
  }, onChange: state => seen.push(state.run) })
  const connecting = controller.connect()
  handlers.match(event({ ...snapshot('r1', 8, 'analyzing'), matches: [{ filename: 'one.pdf' }, { filename: 'two.pdf' }], logs: [{ message: 'earlier log' }] }))
  gate.resolve(snapshot('r1', 7, 'analyzing')); await connecting
  assert.equal(controller.getState().run.revision, 8)
  assert.deepEqual(controller.getState().run.matches.map(match => match.filename), ['one.pdf', 'two.pdf'])
  assert.deepEqual(controller.getState().run.logs, [{ message: 'earlier log' }])
  controller.dispose()
})

test('snapshot reconnect restores review and events from older runs are ignored', async () => {
  let handlers; const review = { ...snapshot('r2', 12, 'review'), matches: [{ filename: 'kept.pdf' }] }
  const controller = createSyncRunController({ api: { listenForSyncEvents: async value => { handlers = value; return () => {} }, getSyncRun: async () => review } })
  await controller.connect()
  handlers.match(event({ ...snapshot('r1', 11, 'review'), matches: [{ filename: 'stale.pdf' }] }))
  assert.deepEqual(controller.getState().run.matches, [{ filename: 'kept.pdf' }])
  assert.equal(controller.getState().run.status, 'review')
  controller.dispose()
})

test('duplicate clicks are guarded and review transitions use the bound run', async () => {
  const gate = deferred(); const calls = []; let handler
  const controller = createSyncRunController({ api: {
    listenForSyncEvents: async handlers => { handler = handlers; return () => {} },
    getSyncRun: async () => snapshot(null, 0),
    startLedgerSync: () => { calls.push('start'); return gate.promise },
    commitLedgerSync: async (id, rows) => { calls.push(['commit', id, rows]); return snapshot(id, 4, 'committing') },
  } })
  await controller.connect()
  const first = controller.start(); const second = controller.start()
  assert.equal(await second, false); assert.deepEqual(calls, ['start'])
  gate.resolve(snapshot('r3', 1, 'analyzing')); await first
  handler.done(event({ ...snapshot('r3', 2, 'review'), matches: [{ filename: 'picked.pdf' }] }))
  await controller.commit([{ filename: 'picked.pdf' }])
  assert.deepEqual(calls[1], ['commit', 'r3', [{ filename: 'picked.pdf' }]])
  controller.dispose()
})

test('events after disposal and late snapshot replies cannot update the view', async () => {
  const gate = deferred(); let handlers; let changes = 0
  const controller = createSyncRunController({ api: { listenForSyncEvents: async value => { handlers = value; return () => {} }, getSyncRun: () => gate.promise }, onChange: () => changes++ })
  const connecting = controller.connect(); const before = changes; controller.dispose()
  gate.resolve(snapshot('late', 99, 'review')); await connecting
  handlers.done(event(snapshot('late', 100, 'review')))
  assert.equal(controller.getState().run, null); assert.equal(changes, before)
})

test('disposal during asynchronous listener registration immediately removes the late registration', async () => {
  const registration = deferred(); let removals = 0
  const controller = createSyncRunController({ api: {
    listenForSyncEvents: () => registration.promise,
    getSyncRun: async () => snapshot('never', 1, 'review'),
  } })
  const connecting = controller.connect()
  controller.dispose()
  registration.resolve(() => { removals++ })
  await connecting
  assert.equal(removals, 1)
  assert.equal(controller.getState().run, null)
})


test('snapshot events preserve whole state across reordered revisions and terminal delivery', async () => {
  let handlers
  const controller = createSyncRunController({ api: {
    listenForSyncEvents: async value => { handlers = value; return () => {} },
    getSyncRun: async () => snapshot(null, 0),
  } })
  await controller.connect()
  const newer = { ...snapshot('r', 4, 'review'), matches: [{ filename: 'a.pdf' }, { filename: 'b.pdf' }], logs: [{ message: 'x' }] }
  handlers.done(event(newer))
  handlers.match(event({ ...snapshot('r', 3, 'analyzing'), matches: [{ filename: 'a.pdf' }] }))
  assert.deepEqual(controller.getState().run, newer)
  controller.dispose()
})

test('review selection defaults all on entry/remount and preserves user deselection', () => {
  const run = { run_id: 'r', matches: [{}, {}, {}] }
  const initial = reviewSelection(run, { runId: null, values: new Set() })
  assert.deepEqual([...initial], [0, 1, 2])
  const manual = new Set([0, 2])
  assert.equal(reviewSelection(run, { runId: 'r', values: manual }), manual)
  assert.deepEqual([...reviewSelection(run, { runId: null, values: new Set() })], [0, 1, 2])
})

test('controller blocks actions until connection and reports setup failure', async () => {
  let changes = []
  const controller = createSyncRunController({ api: { listenForSyncEvents: async () => { throw new Error('registration failed') }, getSyncRun: async () => snapshot(null, 0) }, onChange: state => changes.push(state) })
  assert.equal(await controller.start(), false)
  assert.equal(await controller.connect(), false)
  assert.match(changes.at(-1).error.message, /registration failed/)
  assert.equal(controller.getState().ready, false)
  controller.dispose()
})


test('reconnect removes prior handlers and ignores a superseded snapshot reply', async () => {
  const snapshotGate = deferred(); let registrations = 0; let removals = 0
  const controller = createSyncRunController({ api: {
    listenForSyncEvents: async () => { registrations++; return () => { removals++ } },
    getSyncRun: () => registrations === 1 ? snapshotGate.promise : Promise.resolve(snapshot('new', 3, 'review')),
  } })
  const first = controller.connect()
  await Promise.resolve()
  const second = controller.connect()
  assert.equal(await second, true)
  snapshotGate.resolve(snapshot('old', 2, 'review'))
  assert.equal(await first, false)
  assert.equal(controller.getState().run.run_id, 'new')
  assert.equal(removals, 1)
  controller.dispose()
})

test('reconnect guards queued events and late action replies while retaining revision and pending ownership', async () => {
  const actionGate = deferred(); let firstHandler; let setups = 0
  const changes = []
  const controller = createSyncRunController({ api: {
    listenForSyncEvents: async handlers => { setups++; if (setups === 1) firstHandler = handlers; return () => {} },
    getSyncRun: async () => snapshot(null, 0),
    startLedgerSync: () => actionGate.promise,
  }, onChange: state => changes.push(state) })
  await controller.connect()
  const action = controller.start()
  const reconnecting = controller.connect()
  assert.equal(controller.getState().ready, false)
  assert.equal(controller.getState().pending, false)
  firstHandler.done(event(snapshot('stale', 9, 'review')))
  await reconnecting
  actionGate.resolve(snapshot('old-action', 1, 'analyzing'))
  assert.equal(await action, false)
  assert.equal(controller.getState().run.revision, 0)
  assert.equal(controller.getState().ready, true)
  assert.equal(changes.at(-1).error, undefined)
  controller.dispose()
})

test('malformed initial snapshot fails closed and stale event envelopes are rejected', async () => {
  let handlers; const changes = []
  const controller = createSyncRunController({ api: {
    listenForSyncEvents: async value => { handlers = value; return () => {} },
    getSyncRun: async () => ({ ...snapshot(null, 0), revision: 1.5 }),
  }, onChange: state => changes.push(state) })
  assert.equal(await controller.connect(), false)
  assert.equal(controller.getState().ready, false)
  assert.match(changes.at(-1).error.message, /malformed snapshot/)
  handlers.done({ ...event(snapshot('x', 2, 'review')), revision: 3 })
  assert.equal(controller.getState().run, null)
  assert.equal(await controller.start(), false)
  controller.dispose()
})
