import test from 'node:test';
import assert from 'node:assert/strict';
import { assetAllocation } from './assetAllocation.js';

function summary(entries, total) {
  return { selected_currency: 'USD', total_assets: total, coverage: entries.map(([account, balance, currency = 'USD', included = true]) => ({ account, balance, currency, included_in_net_worth: included })) };
}

test('cash-only allocation includes every cent and supplies an exact text row', () => {
  const result = assetAllocation(summary([['Assets:Cash:Wallet', '12.345']], '12.345'));
  assert.equal(result.chart[0].name, 'Cash');
  assert.equal(result.positiveBasis, '12.345');
  assert.equal(result.rows[0].balance, '12.345');
});

test('negative assets remain signed and reconcile alongside positive balances within one category', () => {
  const result = assetAllocation(summary([['Assets:Bank:A', '100'], ['Assets:Bank:B', '-130'], ['Assets:Other', '50']], '20'));
  assert.equal(result.positiveBasis, '150.00');
  assert.equal(result.negativeOffsets, '-130.00');
  assert.equal(result.signedTotal, '20.00');
  assert.equal(result.rows[0].balance, '-30.00');
  assert.equal(result.chart[0].value, '100.00');
  assert.equal(result.chart[1].name, 'Other assets');
});

test('zero and negative-only assets provide text coverage without misleading pie slices', () => {
  const result = assetAllocation(summary([['Assets:Cash', '0'], ['Assets:Bank:A', '-3']], '-3'));
  assert.deepEqual(result.chart, []);
  assert.equal(result.rows.length, 2);
  assert.equal(result.positiveBasis, '0.00');
});

test('liability debt and refunds, unvalued units and excluded balances never enter asset allocation', () => {
  const result = assetAllocation(summary([['Liabilities:Card:A', '-10'], ['Liabilities:Card:B', '5'], ['Assets:Cash', '7'], ['Assets:Cash:EUR', '80', 'EUR'], ['Assets:Excluded', '9', 'USD', false]], '7'));
  assert.equal(result.signedTotal, '7.00');
  assert.equal(result.rows.length, 1);
});

test('coverage discrepancy fails closed', () => {
  assert.throws(() => assetAllocation(summary([['Assets:Cash', '1']], '2')), /does not reconcile/);
});
