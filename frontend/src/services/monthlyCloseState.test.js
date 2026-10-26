import test from 'node:test';
import assert from 'node:assert/strict';
import { createMonthlyCloseController } from './monthlyCloseState.js';
const request = { month: '2026-01', currency: 'USD', accounts: ['Assets:Bank:A'], exceptions: [] };
const snapshot = overrides => ({ state: 'open', blockers: [], accounts: ['Assets:Bank:A'], metrics: [], ledger_revision: 'c'.repeat(64), revision: 'missing', evidence_revision: 'a'.repeat(64), ...overrides });
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };

test('close saves only the validated request and matching evidence/revision and prevents duplicate clicks', async () => {
  const pending = deferred(), calls = [];
  const controller = createMonthlyCloseController({ load: async () => snapshot(), save: (...args) => { calls.push(args); return pending.promise; }, onChange: () => {} });
  await controller.refresh(request);
  const save = controller.persist('closed');
  assert.equal(await controller.persist('closed'), false);
  assert.deepEqual(calls[0], [request, 'missing', 'a'.repeat(64), 'closed']);
  pending.resolve(snapshot({ state: 'closed', revision: 'b'.repeat(64) })); await save;
  assert.equal(controller.snapshot().snapshot.state, 'closed');
});

test('blocked closes never call persistence; scope requests supersede old reads', async () => {
  const first = deferred(); let loads = 0, saves = 0;
  const controller = createMonthlyCloseController({ load: () => ++loads === 1 ? first.promise : Promise.resolve(snapshot({ blockers: [{ stage: 'reconcile' }] })), save: async () => { saves++; }, onChange: () => {} });
  const old = controller.refresh(request);
  await controller.refresh({ ...request, month: '2026-02' });
  first.resolve(snapshot()); await old;
  assert.equal(controller.snapshot().request.month, '2026-02');
  assert.equal(await controller.persist('closed'), false);
  assert.equal(saves, 0);
});

test('a lost save reply is not retried and retains the failure while reconciling actual persisted state', async () => {
  let loads = 0, saves = 0;
  const controller = createMonthlyCloseController({ load: async () => snapshot({ state: ++loads === 1 ? 'open' : 'closed' }), save: async () => { saves++; throw Error('lost reply'); }, onChange: () => {} });
  await controller.refresh(request);
  assert.equal(await controller.persist('closed'), false);
  assert.equal(saves, 1);
  assert.equal(controller.snapshot().snapshot.state, 'closed');
  assert.match(controller.snapshot().error, /lost reply/);
});

test('resource invalidation arriving during save is checked after publication and can expose stale evidence', async () => {
  const pending = deferred(); let loads = 0;
  const controller = createMonthlyCloseController({ load: async () => snapshot({ state: ++loads === 1 ? 'open' : 'stale' }), save: () => pending.promise, onChange: () => {} });
  await controller.refresh(request);
  const save = controller.persist('closed');
  await controller.refresh();
  pending.resolve(snapshot({ state: 'closed' })); await save;
  assert.equal(controller.snapshot().snapshot.state, 'stale');
});

test('refresh preserves stable UI content while checking; malformed, failed and disposed reads cannot authorize closing', async () => {
  const pending = deferred(); let loads = 0, changes = 0;
  const controller = createMonthlyCloseController({ load: () => ++loads === 1 ? Promise.resolve(snapshot()) : pending.promise, save: async () => {}, onChange: () => { changes++; } });
  await controller.refresh(request);
  const refresh = controller.refresh();
  assert.equal(controller.snapshot().snapshot.state, 'open');
  assert.equal(await controller.persist('closed'), false);
  controller.dispose(); const before = changes; pending.resolve({}); await refresh;
  assert.equal(changes, before);
  const malformed = createMonthlyCloseController({ load: async () => ({}), save: async () => {}, onChange: () => {} });
  await malformed.refresh(request);
  assert.equal(malformed.snapshot().snapshot, null);
  assert.match(malformed.snapshot().error, /malformed/);
});
