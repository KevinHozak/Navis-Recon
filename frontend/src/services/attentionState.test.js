import test from 'node:test';
import assert from 'node:assert/strict';
import { attentionItems, createAttentionController } from './attentionState.js';
import { persistedAction, subscribeResourceChanges } from './resourceChanges.js';

const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };

test('attention maps every source to its specific resolution scope and prioritizes expired evidence', () => {
  assert.deepEqual(attentionItems('statements', { anomalies: [{ kind: 'missing_interval', institution: 'Example', message: 'Missing 2026-08' }] })[0].filters, { institution: 'Example', status: 'ALL' });
  assert.equal(attentionItems('reviews', [{ transaction_id: 'test' }])[0].view, 'reviews');
  assert.equal(attentionItems('sync', [{ item_id: 'x', operation_id: 'hash', filename: 'synthetic.pdf', state: 'failed' }])[0].filters.operationId, 'hash');
  const renewal = attentionItems('renewals', [{ key: 'k', payee: 'Example', account: 'Assets:Bank:A', currency: 'EUR', alerts: [{ message: 'Renews tomorrow' }] }])[0];
  assert.deepEqual(renewal.filters, { search: 'Example', currency: 'EUR', account: 'Assets:Bank:A', alertsOnly: true });
  const record = attentionItems('records', { items: [{ record: { id: 'r', title: 'Synthetic passport' }, bucket: 'expired', isActionRequired: true }] })[0];
  assert.equal(record.filters.recordId, 'r');
  assert.ok(record.priority < renewal.priority);
  assert.deepEqual(attentionItems('renewals', [{ metadata: { forecast_cancelled: true } }]), []);
  assert.equal(attentionItems('sync', { outcomes: [], run: { status: 'failed', run_id: 'synthetic-run' } })[0].title, 'Failed sync run · inspect diagnostics');
  assert.throws(() => attentionItems('sync', { outcomes: [], run: {} }), /status unavailable/);
});

test('healthy empty, unavailable, and stale sources remain distinct with independent successful read times', async () => {
  let fail = false, time = 100;
  const controller = createAttentionController({ loaders: { reviews: async () => { if (fail) throw Error('unreadable ledger'); return []; }, records: async () => { throw Error('missing registry'); } }, onChange: () => {}, now: () => time });
  await controller.refresh(['reviews', 'records']);
  assert.equal(controller.snapshot().reviews.status, 'current');
  assert.equal(controller.snapshot().reviews.count, 0);
  assert.equal(controller.snapshot().records.status, 'unavailable');
  fail = true; time = 200;
  await controller.refresh(['reviews']);
  assert.equal(controller.snapshot().reviews.status, 'stale');
  assert.equal(controller.snapshot().reviews.checkedAt, 100);
  assert.match(controller.snapshot().reviews.error, /unreadable/);
});

test('superseded refreshes and disposed responses cannot overwrite current source authority', async () => {
  const first = deferred(), second = deferred(); let call = 0, changes = 0;
  const controller = createAttentionController({ loaders: { reviews: () => (++call === 1 ? first.promise : second.promise) }, onChange: () => { changes++; } });
  const one = controller.refresh(['reviews']), two = controller.refresh(['reviews']);
  second.resolve([]); await two;
  first.resolve([{}]); await one;
  assert.equal(controller.snapshot().reviews.count, 0);
  const pending = deferred();
  const other = createAttentionController({ loaders: { reviews: () => pending.promise }, onChange: () => { changes++; } });
  const waiting = other.refresh(['reviews']); other.dispose(); const before = changes;
  pending.resolve([]); await waiting;
  assert.equal(changes, before);
});

test('unreadable sync store and malformed payloads fail closed rather than imply healthy sources', async () => {
  assert.throws(() => attentionItems('sync', [{ item_id: 'store', state: 'uncertain', message: 'Store unavailable' }]), /unavailable/);
  assert.throws(() => attentionItems('statements', {}), /Malformed/);
  const controller = createAttentionController({ loaders: { sync: async () => [{ state: 'unexpected' }] }, onChange: () => {} });
  await controller.refresh(['sync']);
  assert.equal(controller.snapshot().sync.status, 'unavailable');
});

test('persisted actions publish shared resource invalidation on success and uncertain rejection and unsubscribe cleanly', async () => {
  const events = []; const stop = subscribeResourceChanges(ids => events.push([...ids]));
  assert.equal(await persistedAction(['records'], async () => 'saved'), 'saved');
  await assert.rejects(persistedAction(['renewals'], async () => { throw Error('lost response'); }), /lost response/);
  stop(); await persistedAction(['reviews'], async () => {});
  assert.deepEqual(events, [['records'], ['renewals']]);
});
