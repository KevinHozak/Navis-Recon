import { test } from 'node:test';
import assert from 'node:assert/strict';
import { addMoney, absMoney, compareMoney, filterMoneyByCurrency, formatMoney, groupMoneyByCurrency, groupTransactionsByPeriod, moneyToChartNumber, multiplyDivideMoney, negateMoney, parseMoney, sumMoney } from './money.js';

test('exact fractional accumulation and precision', () => {
  assert.equal(sumMoney(Array(10).fill('0.1')), '1.00');
  assert.equal(sumMoney(Array(10000).fill('0.0001')), '1.00');
  assert.equal(addMoney('999999999999999.999999999999999999', '0.000000000000000001'), '1000000000000000.00');
});
test('negative values, refunds, and exact formatting', () => {
  assert.equal(addMoney('-10.25', '12.50'), '2.25');
  assert.equal(absMoney('-0.000000000000000001'), '0.000000000000000001');
  assert.equal(formatMoney('-0.000000000000000001', 'USD'), 'USD -0.000000000000000001');
  assert.equal(formatMoney('1.235', 'USD', { rounding: 2 }), 'USD 1.24');
  assert.equal(formatMoney('-1.235', 'USD', { rounding: 2 }), 'USD -1.24');
});
test('rejects implicit numeric conversion and invalid decimal strings', () => {
  for (const value of [1, '1e3', '1,000', '1.0000000000000000001', '', '.5']) assert.throws(() => parseMoney(value));
  assert.equal(compareMoney('9007199254740993', '9007199254740992'), 1);
});
test('currency aggregation isolates units', () => {
  const rows = [{ currency: 'USD', amount: '10' }, { currency: 'EUR', amount: '20' }, { currency: 'USD', amount: '-2' }];
  assert.deepEqual(groupMoneyByCurrency(rows), { USD: '8.00', EUR: '20.00' });
  assert.deepEqual(filterMoneyByCurrency(rows, 'USD').map(r => r.amount), ['10', '-2']);
});
test('period grouping isolates mixed currencies sharing the same year', () => {
  const rows = [
    { date: '2026-01-01', currency: 'USD', amount: '10' },
    { date: '2026-07-01', currency: 'EUR', amount: '500' },
    { date: '2026-09-01', currency: 'USD', amount: '-2' },
  ];
  const group = groupTransactionsByPeriod(rows, 'USD', row => row.date.slice(0, 4));
  assert.deepEqual(group, [{ name: '2026', sum: '8.00', count: 2, currency: 'USD' }]);
});
test('rational estimates round half away from zero at scale 18', () => {
  assert.equal(multiplyDivideMoney('52', 52, 12), '225.333333333333333333');
  assert.equal(multiplyDivideMoney('1', 1, 3), '0.333333333333333333');
  assert.equal(multiplyDivideMoney('-1', 1, 2), '-0.50');
});
test('chart conversion is a numeric boundary only', () => {
  assert.equal(moneyToChartNumber('12.5'), 12.5);
  assert.equal(formatMoney('9007199254740993.123456789012345678'), 'USD 9007199254740993.123456789012345678');
  for (const invalid of [null, 1, 'oops', '1e4']) assert.throws(() => moneyToChartNumber(invalid));
  assert.equal(negateMoney('9007199254740993.123456789012345678'), '-9007199254740993.123456789012345678');
});
