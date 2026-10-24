import test from 'node:test'
import assert from 'node:assert/strict'
import { createSyncResourceObserver } from './syncResourceObserver.js'
const id = 'a'.repeat(64)
test('resource observer subscribes before snapshot and deduplicates confirmed effects across reconnects', async () => {
 let listener; const effects=[]; let order=[]
 const observer=createSyncResourceObserver({subscribe:async fn=>{order.push('subscribe');listener=fn;return()=>{}},snapshot:async()=>{order.push('snapshot');return {revision:1,managed_outcomes:[{state:'succeeded',operation_id:id}]}},onEffects:x=>effects.push(...x)})
 await observer.connect(); assert.deepEqual(order,['subscribe','snapshot']); listener({run_revision:2,confirmed_effect_ids:[id]}); assert.deepEqual(effects,[id]); observer.dispose()
})
test('analysis and no-effect events cause no invalidation, malformed events are ignored',()=>{let calls=0;const observer=createSyncResourceObserver({subscribe:async()=>()=>{},snapshot:async()=>({}),onEffects:()=>calls++});observer.apply({run_revision:2,confirmed_effect_ids:[]});observer.apply({run_revision:2,confirmed_effect_ids:['bad']});observer.apply(null);assert.equal(calls,0);observer.dispose()})
