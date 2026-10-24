import test from 'node:test'
import assert from 'node:assert/strict'
import { createSyncRunController as makeSyncRunController, createSelectionStore } from './syncRunController.js'
import { canAbandonReview, canCommitReview, canStartRun, canToggleMatch, showsReview } from './syncRunView.js'

const deferred = () => { let resolve, reject; const promise = new Promise((r, j) => { resolve = r; reject = j }); return { promise, resolve, reject } }
const snapshot = (run_id, revision, status = 'idle') => ({ run_id, revision, sequence: revision, status, analysis_outcome: status === 'review' || status === 'committing' || status === 'complete' ? 'success' : status === 'failed' || status === 'cancelled' ? 'failed' : null, logs: [], matches: [], unmatched: [], errors: [], result: null })
const event = run => ({ run_id: run.run_id, revision: run.revision, sequence: run.sequence, status: run.status, snapshot: run })
const createSyncRunController = options => makeSyncRunController({ selectionStore: createSelectionStore(), ...options })

test('subscribes before snapshot and ignores the older snapshot after an event', async () => {
  const gate = deferred(); let handlers; const seen = []
  const controller = createSyncRunController({ api: {
    listenForSyncEvents: async value => { handlers = value; return () => {} },
    getSyncRun: () => gate.promise,
  }, onChange: state => seen.push(state.run) })
  const connecting = controller.connect()
  handlers.match(event({ ...snapshot('r1', 8, 'analyzing'), matches: [{ match_id:'one',filename: 'one.pdf',new_name:'one.pdf' }, { match_id:'two',filename: 'two.pdf',new_name:'two.pdf' }], logs: [{ message: 'earlier log' }] }))
  gate.resolve(snapshot('r1', 7, 'analyzing')); await connecting
  assert.equal(controller.getState().run.revision, 8)
  assert.deepEqual(controller.getState().run.matches.map(match => match.filename), ['one.pdf', 'two.pdf'])
  assert.deepEqual(controller.getState().run.logs, [{ message: 'earlier log' }])
  controller.dispose()
})

test('snapshot reconnect restores review and events from older runs are ignored', async () => {
  let handlers; const review = { ...snapshot('r2', 12, 'review'), matches: [{ match_id:'kept',filename: 'kept.pdf',new_name:'kept.pdf' }] }
  const controller = createSyncRunController({ api: { listenForSyncEvents: async value => { handlers = value; return () => {} }, getSyncRun: async () => review } })
  await controller.connect()
  handlers.match(event({ ...snapshot('r1', 11, 'review'), matches: [{ filename: 'stale.pdf' }] }))
  assert.deepEqual(controller.getState().run.matches, [{ match_id:'kept',filename: 'kept.pdf',new_name:'kept.pdf' }])
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
  handler.done(event({ ...snapshot('r3', 2, 'review'), matches: [{ match_id:'picked',filename: 'picked.pdf',new_name:'picked.pdf' }] }))
  assert.equal(await controller.commit([{ match_id:'invented' }]), false)
  assert.equal(await controller.commit([{ filename:'picked.pdf' }]), false)
  await controller.commit(['picked'])
  assert.deepEqual(calls[1], ['commit', 'r3', ['picked']])
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
  const newer = { ...snapshot('r', 4, 'review'), matches: [{ match_id:'a',filename: 'a.pdf',new_name:'a.pdf' }, { match_id:'b',filename: 'b.pdf',new_name:'b.pdf' }], logs: [{ message: 'x' }] }
  handlers.done(event(newer))
  handlers.match(event({ ...snapshot('r', 3, 'analyzing'), matches: [{ filename: 'a.pdf' }] }))
  assert.deepEqual(controller.getState().run, newer)
  controller.dispose()
})

test('selection store preserves deselection across incremental reordered snapshots and remounts', () => {
  const store = createSelectionStore(); const run = { run_id: 'r', revision: 2, matches: [{match_id:'a'}, {match_id:'b'}] }
  store.reconcile(run)
  assert.deepEqual([...store.values(run)], ['a','b'])
  store.toggle(run, 'a')
  const reordered = { ...run, revision:3, matches: [{match_id:'b'},{match_id:'a'},{match_id:'c'}] }
  store.reconcile(reordered)
  assert.deepEqual([...store.values(reordered)], ['b','c'])
  assert.deepEqual([...store.values(run)], ['b','c'])
  assert.deepEqual([...store.values({ run_id:'late-run', revision: 1, matches:[{match_id:'late'}] })], [])
  store.reconcile({ ...run, revision:8, matches:reordered.matches })
  assert.deepEqual([...store.values({ ...run, revision: 8 })], ['b','c'])
  const remounted = createSelectionStore(); const freshRun = { ...run, revision:9, matches:[{match_id:'b'},{match_id:'a'},{match_id:'c'}] }
  remounted.reconcile(freshRun)
  assert.deepEqual([...remounted.values(freshRun)], ['b','a','c'])
})

test('view states expose failed diagnostics read-only and enable only eligible partial matches', () => {
  assert.equal(showsReview('failed'), true)
  const failed = { status:'failed', analysis_outcome:'failed', eligible_match_ids:[] }
  assert.equal(canToggleMatch(failed, true, false, 'a'), false)
  const partial = { status:'review', analysis_outcome:'partial', eligible_match_ids:['b'] }
  assert.equal(canToggleMatch(partial, true, false, 'a'), false)
  assert.equal(canToggleMatch(partial, true, false, 'b'), true)
  assert.equal(canCommitReview(partial, true, false, 1), true)
  assert.equal(canAbandonReview({status:'committing'}, true, false), false)
  assert.equal(canStartRun({status:'committing',run_id:'r'}, true, false, true), false)
})

test('selection authority rejects stale controller publications and toggles through controller', async () => {
  const store = createSelectionStore()
  const make = (revision, runId, capture = () => {}) => createSyncRunController({selectionStore:store,onChange:capture,api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>({...snapshot(runId,revision,'review'),matches:[{match_id:runId,filename:'x',new_name:'y'}]})}})
  const newer = make(8,'new',()=>{}); await newer.connect()
  const older = make(2,'old',()=>{}); await older.connect()
  assert.deepEqual([...store.values({...snapshot('new',8,'review'),matches:[{match_id:'new'}]})],['new'])
  newer.toggle('new')
  assert.deepEqual([...store.values({...snapshot('new',8,'review'),matches:[{match_id:'new'}]})],[])
  older.dispose(); newer.dispose()
})

test('a remounted controller restores deselection from the same in-memory selection store', async () => {
  const store=createSelectionStore(); const review={...snapshot('persist',12,'review'),matches:[{match_id:'a',filename:'a',new_name:'b'}]}
  const make=()=>makeSyncRunController({selectionStore:store,api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>review}})
  const first=make(); await first.connect(); first.toggle('a'); first.dispose()
  const second=make(); let selected; const third=makeSyncRunController({selectionStore:store,onChange:view=>{selected=view.selectedIds},api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>review}})
  await second.connect(); assert.deepEqual([...store.values(review)],[]); await third.connect(); assert.deepEqual([...selected],[])
  second.dispose(); third.dispose()
})

test('controller preserves deselection through new matches, reorder, replay, content identity and terminal review', async () => {
  let handlers; const views=[]; const controller=createSyncRunController({onChange:view=>views.push(view),api:{listenForSyncEvents:async h=>{handlers=h;return()=>{}},getSyncRun:async()=>snapshot(null,0)}})
  await controller.connect()
  const a={match_id:'a1',filename:'a.pdf',new_name:'one.pdf'}, b={match_id:'b1',filename:'b.pdf',new_name:'one.pdf'}
  handlers.match(event({...snapshot('sel',1,'analyzing'),matches:[a],eligible_match_ids:['a1']}))
  controller.toggle('a1')
  handlers.match(event({...snapshot('sel',2,'analyzing'),matches:[a,b],eligible_match_ids:['a1','b1']}))
  assert.deepEqual([...views.at(-1).selectedIds],['b1'])
  handlers.match(event({...snapshot('sel',3,'analyzing'),matches:[b,a],eligible_match_ids:['a1','b1']}))
  assert.deepEqual([...views.at(-1).selectedIds],['b1'])
  handlers.done(event({...snapshot('sel',4,'review'),analysis_outcome:'partial',matches:[b,a,{match_id:'a2',filename:'a.pdf',new_name:'two.pdf'}],eligible_match_ids:['b1']}))
  assert.deepEqual([...views.at(-1).selectedIds],['b1'])
  assert.deepEqual([...views.at(-1).selectedIds].filter(id=>id==='a1'),[])
  controller.dispose()
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
  assert.equal(controller.getState().pending, true)
  firstHandler.done(event(snapshot('stale', 9, 'review')))
  await reconnecting
  actionGate.resolve(snapshot('old-action', 1, 'analyzing'))
  assert.equal(await action, false)
  assert.equal(controller.getState().run.revision, 0)
  assert.equal(controller.getState().pending, false)
  assert.equal(controller.getState().ready, true)
  assert.equal(changes.at(-1).error, undefined)
  controller.dispose()
})

test('reconnect keeps a pending commit owned until its stale success or rejection settles', async () => {
  for (const reject of [false, true]) {
    const gate = deferred()
    const review = { ...snapshot('run', 3, 'review'), matches: [{ match_id:'m', filename:'a.pdf', new_name:'b.pdf' }] }
    const controller = createSyncRunController({ api: {
      listenForSyncEvents: async () => () => {},
      getSyncRun: async () => review,
      commitLedgerSync: () => gate.promise,
    } })
    await controller.connect()
    const pending = controller.commit(['m'])
    await controller.connect()
    assert.equal(controller.getState().pending, true)
    assert.equal(await controller.commit(['m']), false)
    reject ? gate.reject(new Error('commit failed')) : gate.resolve({ ...review, revision:4, sequence:4, status:'committing' })
    assert.equal(await pending, false)
    assert.equal(controller.getState().pending, false)
    assert.equal(controller.getState().run.status, 'review')
    controller.dispose()
  }
})

test('accepted commit reply after reconnect is reconciled to authoritative completion without retry', async () => {
  const gate=deferred(); let reads=0; let commits=0
  const review={...snapshot('run',3,'review'),matches:[{match_id:'m',filename:'a.pdf',new_name:'b.pdf'}]}
  const complete={...review,revision:6,sequence:6,status:'complete',analysis_outcome:'success'}
  const controller=createSyncRunController({api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>++reads===1?review:complete,commitLedgerSync:()=>{commits++;return gate.promise}}})
  await controller.connect(); const pending=controller.commit(['m']); await controller.connect()
  gate.resolve({...review,revision:4,sequence:4,status:'committing'}); assert.equal(await pending,false)
  assert.equal(controller.getState().run.status,'complete'); assert.equal(commits,1); assert.equal(controller.getState().pending,false)
  controller.dispose()
})

test('failed command reconciliation leaves the controller unready and blocks duplicate commit', async () => {
  const gate=deferred(); let reads=0; let commits=0
  const review={...snapshot('run',3,'review'),matches:[{match_id:'m',filename:'a.pdf',new_name:'b.pdf'}]}
  const controller=createSyncRunController({api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>{reads++;if(reads===1)return review;if(reads===2)throw new Error('reconnect failed');throw new Error('snapshot unavailable')},commitLedgerSync:()=>{commits++;return gate.promise}}})
  await controller.connect(); const pending=controller.commit(['m']); await controller.connect()
  gate.reject(new Error('transport lost')); assert.equal(await pending,false)
  assert.equal(controller.getState().ready,false); assert.equal(await controller.commit(['m']),false); assert.equal(commits,1)
  controller.dispose()
})

test('malformed action response reconciles from native snapshot before clearing pending', async () => {
  let reads=0; const review={...snapshot('r',2,'review'),matches:[{match_id:'m',filename:'a',new_name:'b'}]}
  const complete={...review,revision:5,sequence:5,status:'complete'}
  const controller=createSyncRunController({api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>++reads===1?review:complete,commitLedgerSync:async()=>({status:'broken'})}})
  await controller.connect(); assert.equal(await controller.commit(['m']),false)
  assert.equal(controller.getState().run.status,'complete'); assert.equal(controller.getState().pending,false)
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
