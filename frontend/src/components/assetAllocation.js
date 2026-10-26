import { addMoney, compareMoney, moneyToChartNumber } from '../utils/money.js';

const categories = [
  ['Bank Accounts', 'Assets:Bank', '#00d4ff'],
  ['Cash', 'Assets:Cash', '#67e8f9'],
  ['Retirement', 'Assets:Retirement', '#00b4d8'],
  ['Investments', 'Assets:Investments', '#0096c7'],
  ['Property', 'Assets:Housing', '#0077b6'],
  ['Other assets', 'Assets', '#a78bfa'],
];

// Coverage contains direct balances, so parent and child accounts are counted once.
// Pie geometry uses positive balances only; negative offsets remain explicit.
export function assetAllocation(summary) {
  const rows = categories.map(([name, prefix, color]) => ({ name, prefix, color, positive: '0', negative: '0', balance: '0', count: 0 }));
  for (const account of summary.coverage) {
    if (!account.included_in_net_worth || account.currency !== summary.selected_currency || !account.account.startsWith('Assets:')) continue;
    const row = rows.find(category => account.account === category.prefix || account.account.startsWith(`${category.prefix}:`));
    row.count += 1;
    row.balance = addMoney(row.balance, account.balance);
    const field = compareMoney(account.balance, '0') < 0 ? 'negative' : 'positive';
    row[field] = addMoney(row[field], account.balance);
  }
  const positiveBasis = rows.reduce((sum, row) => addMoney(sum, row.positive), '0');
  const negativeOffsets = rows.reduce((sum, row) => addMoney(sum, row.negative), '0');
  const signedTotal = addMoney(positiveBasis, negativeOffsets);
  if (compareMoney(signedTotal, summary.total_assets) !== 0) throw new Error('Allocation coverage does not reconcile to total assets');
  return {
    rows: rows.filter(row => row.count > 0), positiveBasis, negativeOffsets, signedTotal,
    chart: rows.filter(row => compareMoney(row.positive, '0') > 0).map(row => ({ ...row, value: row.positive, chartValue: moneyToChartNumber(row.positive) })),
  };
}
