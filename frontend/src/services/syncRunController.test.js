import test from 'node:test'
import assert from 'node:assert/strict'
import { createSyncRunController as makeSyncRunController, createSelectionStore } from './syncRunController.js'
import { canAbandonReview, canCommitReview, canStartRun, canToggleMatch, managedOutcomes, retryableManagedItems, filterRecoveryOutcomes, showsReview } from './syncRunView.js'

test('filtered import recovery never retries hidden operations and keeps uncertain items read-only', () => {
  const rows = [
    {item_id:'one',filename:'one.pdf',state:'failed',retry_safe:true,message:'safe retry',operation_id:'a'.repeat(64)},
    {item_id:'two',filename:'two.pdf',state:'failed',retry_safe:true,message:'safe retry',operation_id:'b'.repeat(64)},
    {item_id:'three',filename:'three.pdf',state:'uncertain',retry_safe:false,message:'review',operation_id:'c'.repeat(64)},
    {item_id:'four',filename:'four.pdf',state:'succeeded',retry_safe:false,message:'done',operation_id:'d'.repeat(64)},
  ];
  assert.deepEqual(retryableManagedItems(filterRecoveryOutcomes(rows,'a'.repeat(64))).map(row=>row.item_id), ['one']);
  assert.deepEqual(retryableManagedItems(filterRecoveryOutcomes(rows,'c'.repeat(64))), []);
  assert.equal(filterRecoveryOutcomes(rows,'attention').length,3);
  assert.equal(filterRecoveryOutcomes(rows,'all').length,4);
});

test('mixed funding requires every selected receipt, persists across remount, and clears for a new run', async () => {
  const store=createSelectionStore(); const calls=[]
  let review={...snapshot('mixed',1,'review'),funding_accounts:['Assets:Retirement:HSA','Liabilities:CreditCard:Venture'],eligible_match_ids:['medical','food','unassigned'],matches:['medical','food','unassigned'].map(id=>({match_id:id,filename:id+'.pdf',new_name:id+'.pdf'}))}
  const api={listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>review,commitLedgerSync:async(id,payload)=>{calls.push({id,payload});return {...review,revision:2,sequence:2,status:'committing'}}}
  const make=()=>makeSyncRunController({api,selectionStore:store})
  let controller=make(); await controller.connect()
  assert.equal(await controller.commit(['medical']),false)
  assert.equal(controller.assignFunding('medical','Equity:Opening'),false)
  assert.equal(controller.assignFunding('medical','Assets:Retirement:HSA'),true)
  assert.equal(controller.assignFunding('food','Liabilities:CreditCard:Venture'),true)
  assert.equal(await controller.commit(['medical','food','unassigned']),false)
  assert.equal(await controller.commit(['medical','medical']),false)
  assert.equal(calls.length,0)
  controller.dispose();controller=make();await controller.connect()
  assert.equal(await controller.commit(['medical','food']),true)
  assert.deepEqual(calls[0].payload,[{matchId:'medical',fundingAccount:'Assets:Retirement:HSA'},{matchId:'food',fundingAccount:'Liabilities:CreditCard:Venture'}])
  assert.equal(controller.assignFunding('food',''),false)
  controller.dispose();review={...review,run_id:'next',revision:3};controller=make();await controller.connect()
  assert.equal(await controller.commit(['medical']),false)
  controller.dispose()
})

test('clearing funding blocks approval and stale removed accounts are not reused', () => {
  const store=createSelectionStore();const run={...snapshot('funding',1,'review'),matches:[{match_id:'m'}]}
  store.reconcile(run);assert.equal(store.assignFunding(run,'m','Assets:Bank:Checking'),true)
  assert.equal(store.funding(run).get('m'),'Assets:Bank:Checking')
  store.assignFunding(run,'m','');assert.equal(store.funding(run).size,0)
  store.assignFunding(run,'m','Assets:Bank:Checking');store.reconcile({...run,revision:2,funding_accounts:[]})
  assert.equal(store.funding(run).size,0)
  assert.equal(canCommitReview(run,true,false,1,false),false)
})

const deferred = () => { let resolve, reject; const promise = new Promise((r, j) => { resolve = r; reject = j }); return { promise, resolve, reject } }
const snapshot = (run_id, revision, status = 'idle') => ({ run_id, revision, sequence: revision, status, analysis_outcome: status === 'review' || status === 'committing' || status === 'complete' ? 'success' : status === 'failed' || status === 'cancelled' ? 'failed' : null, logs: [], matches: [], unmatched: [], errors: [], funding_accounts:['Assets:Bank:Checking'], result: null })
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
  controller.assignFunding('picked', 'Assets:Bank:Checking')
  await controller.commit(['picked'])
  assert.deepEqual(calls[1], ['commit', 'r3', [{matchId:'picked',fundingAccount:'Assets:Bank:Checking'}]])
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

test('source reveal rejects stale completion after disposal and reports live failures',async()=>{const gate=deferred();let errors=[];const controller=createSyncRunController({onChange:v=>errors.push(v.error),api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>snapshot(null,0),revealSyncSource:()=>gate.promise}});await controller.connect();const pending=controller.revealSource({sourceRef:'src:'+`a`.repeat(64)});controller.dispose();gate.resolve();assert.equal(await pending,false);const live=createSyncRunController({onChange:v=>errors.push(v.error),api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>snapshot(null,0),revealSyncSource:async()=>{throw Error('safe failure')}}});await live.connect();assert.equal(await live.revealSource({}),false);assert.match(errors.at(-1).message,/safe failure/);live.dispose()})


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
    controller.assignFunding('m', 'Assets:Bank:Checking')
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
  await controller.connect(); controller.assignFunding('m','Assets:Bank:Checking'); const pending=controller.commit(['m']); await controller.connect()
  gate.resolve({...review,revision:4,sequence:4,status:'committing'}); assert.equal(await pending,false)
  assert.equal(controller.getState().run.status,'complete'); assert.equal(commits,1); assert.equal(controller.getState().pending,false)
  controller.dispose()
})

test('failed command reconciliation leaves the controller unready and blocks duplicate commit', async () => {
  const gate=deferred(); let reads=0; let commits=0
  const review={...snapshot('run',3,'review'),matches:[{match_id:'m',filename:'a.pdf',new_name:'b.pdf'}]}
  const controller=createSyncRunController({api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>{reads++;if(reads===1)return review;if(reads===2)throw new Error('reconnect failed');throw new Error('snapshot unavailable')},commitLedgerSync:()=>{commits++;return gate.promise}}})
  await controller.connect(); controller.assignFunding('m','Assets:Bank:Checking'); const pending=controller.commit(['m']); await controller.connect()
  gate.reject(new Error('transport lost')); assert.equal(await pending,false)
  assert.equal(controller.getState().ready,false); assert.equal(await controller.commit(['m']),false); assert.equal(commits,1)
  controller.dispose()
})

test('malformed action response reconciles from native snapshot before clearing pending', async () => {
  let reads=0; const review={...snapshot('r',2,'review'),matches:[{match_id:'m',filename:'a',new_name:'b'}]}
  const complete={...review,revision:5,sequence:5,status:'complete'}
  const controller=createSyncRunController({api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>++reads===1?review:complete,commitLedgerSync:async()=>({status:'broken'})}})
  await controller.connect(); controller.assignFunding('m','Assets:Bank:Checking'); assert.equal(await controller.commit(['m']),false)
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

test('restart snapshot hydrates partial managed outcomes and only backend classified failures are retryable', async () => {
  const saved = [
    { item_id: 'done', filename: 'a.pdf', state: 'succeeded', retry_safe: false, message: 'Imported', operation_id: 'a'.repeat(64) },
    { item_id: 'safe', filename: 'b.pdf', state: 'failed', retry_safe: true, message: 'No effects', operation_id: 'b'.repeat(64) },
    { item_id: 'uncertain', filename: 'c.pdf', state: 'uncertain', retry_safe: true, message: 'Verify manually', operation_id: 'c'.repeat(64) },
  ]
  const controller = createSyncRunController({ api: {
    listenForSyncEvents: async () => () => {},
    getSyncRun: async () => ({ ...snapshot(null, 0), revision: 1, managed_outcomes: saved }),
  } })
  assert.equal(await controller.connect(), true)
  assert.deepEqual(managedOutcomes(controller.getState().run).map(item => item.item_id), ['done', 'safe', 'uncertain'])
  assert.deepEqual(retryableManagedItems(controller.getState().run).map(item => item.item_id), ['safe'])
  controller.dispose()
})


test('recovery hydrates while idle and refresh failures preserve the last verified list', async () => {
  const item = { item_id:'x', filename:'x.pdf', state:'failed', retry_safe:true, message:'No effects', operation_id:'d'.repeat(64) }; let fail=false; let changes=[]
  const controller=createSyncRunController({onChange:view=>changes.push(view),api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>snapshot(null,0),reconcileLedgerSync:async()=>{if(fail)throw new Error('store busy');return [item]}}})
  await controller.connect(); assert.deepEqual(controller.getState().recovery,[item]); fail=true; assert.equal(await controller.refreshOutcomes(),false)
  assert.deepEqual(controller.getState().recovery,[item]); assert.match(controller.getState().recoveryError.message,/store busy/); controller.dispose()
})

test('recovery request generations ignore stale refresh results and disposal', async () => {
  const old=deferred(); const late=deferred(); let reads=0; const item=n=>({item_id:String(n),filename:`${n}.pdf`,state:'failed',retry_safe:false,message:'x',operation_id:String(n).repeat(64)})
  const controller=createSyncRunController({api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>snapshot(null,0),reconcileLedgerSync:()=>++reads===1?Promise.resolve([item(0)]):reads===2?old.promise:reads===3?Promise.resolve([item(2)]):late.promise}})
  await controller.connect(); const stale=controller.refreshOutcomes(); await controller.refreshOutcomes(); old.resolve([item(1)]); await stale; assert.equal(controller.getState().recovery[0].item_id,'2')
  const pending=controller.refreshOutcomes(); const before=controller.getState().recovery; controller.dispose(); late.resolve([item(3)]); await pending; assert.deepEqual(controller.getState().recovery,before)
})

test('retry reserves one mutation, blocks competing actions and reconciles after rejection without retrying', async () => {
  const gate=deferred(); let tries=0; let reads=0; const safe={item_id:'safe',filename:'x.pdf',state:'failed',retry_safe:true,message:'safe',operation_id:'e'.repeat(64)}
  const idle=snapshot(null,1)
  const api={listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>++reads===1?idle:snapshot(null,4),reconcileLedgerSync:async()=>[safe],retryLedgerSync:()=>{tries++;return gate.promise},startLedgerSync:async()=>snapshot('new',5,'analyzing')}
  const controller=createSyncRunController({api}); await controller.connect(); const retry=controller.retryOutcomes(); assert.equal(controller.getState().pending,true)
  assert.equal(await controller.retryOutcomes(),false); assert.equal(await controller.start(),false); assert.equal(await controller.commit(['m']),false); assert.equal(await controller.cancel(),false)
  gate.reject(new Error('transport unknown')); assert.equal(await retry,false); assert.equal(tries,1); assert.equal(controller.getState().pending,false); controller.dispose()
})

test('malformed durable outcome lists are explicit snapshot errors, not silently filtered', async () => {
  const views=[]; const controller=createSyncRunController({onChange:view=>views.push(view),api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>({...snapshot(null,0),managed_outcomes:[{item_id:'x',filename:'x.pdf',state:'failed',retry_safe:true,message:'x',operation_id:'z'}]})}})
  assert.equal(await controller.connect(),false); assert.match(views.at(-1).error.message,/malformed snapshot/); controller.dispose()
})


test('controller ignores selection toggles while a commit is pending', async () => {
  const gate=deferred(); const review={...snapshot('review',2,'review'),matches:[{match_id:'m',filename:'a.pdf',new_name:'b.pdf'}],eligible_match_ids:['m']}
  const views=[]; const controller=createSyncRunController({onChange:view=>views.push(view),api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>review,commitLedgerSync:()=>gate.promise}})
  await controller.connect(); assert.deepEqual([...views.at(-1).selectedIds],['m'])
  controller.assignFunding('m','Assets:Bank:Checking')
  const committing=controller.commit(['m']); controller.toggle('m'); assert.deepEqual([...views.at(-1).selectedIds],['m'])
  gate.resolve({...review,revision:3,sequence:3,status:'committing'}); await committing; controller.dispose()
})


test('cancelled failed analysis is a valid terminal snapshot and receipts fail visibly without replacing outcomes', async () => {
  const cancelled={...snapshot('cancelled-run',3,'cancelled'),analysis_outcome:'failed'}; let view;
  const controller=createSyncRunController({onChange:value=>view=value,api:{listenForSyncEvents:async()=>()=>{},getSyncRun:async()=>cancelled,reconcileLedgerSync:async()=>[{item_id:'known',filename:'safe.pdf',state:'succeeded',retry_safe:false,message:'confirmed',operation_id:'f'.repeat(64)}],getSyncReceipts:async()=>({receipts:[],error:'store_unavailable'})}})
  await controller.connect(); assert.equal(controller.getState().run.status,'cancelled'); assert.equal(controller.getState().recovery[0].state,'succeeded'); assert.equal(controller.getState().receiptError,'store_unavailable'); assert.equal(view.receipts.length,0); controller.dispose()
})
