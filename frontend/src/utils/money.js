const SCALE = 10n ** 18n;
const MONEY_PATTERN = /^([+-]?)(\d+)(?:\.(\d+))?$/;

export function parseMoney(value) {
  if (typeof value !== 'string') throw new TypeError('Money must be a decimal string');
  const match = MONEY_PATTERN.exec(value);
  if (!match || (match[3]?.length ?? 0) > 18) throw new RangeError(`Invalid canonical money: ${value}`);
  const fraction = (match[3] || '').padEnd(18, '0');
  const scaled = BigInt(match[2]) * SCALE + BigInt(fraction || '0');
  return match[1] === '-' ? -scaled : scaled;
}

function scaledString(value, minFraction = 2, maxFraction = 18) {
  let n = value;
  const sign = n < 0n ? '-' : '';
  if (n < 0n) n = -n;
  const whole = n / SCALE;
  const fraction = (n % SCALE).toString().padStart(18, '0');
  let digits = fraction.replace(/0+$/, '');
  const places = Math.max(minFraction, Math.min(maxFraction, digits.length));
  digits = fraction.slice(0, places).padEnd(minFraction, '0');
  return places > 0 ? `${sign}${whole}.${digits}` : `${sign}${whole}`;
}

export const addMoney = (a, b) => scaledString(parseMoney(a) + parseMoney(b));
export const compareMoney = (a, b) => parseMoney(a) < parseMoney(b) ? -1 : parseMoney(a) > parseMoney(b) ? 1 : 0;
export const absMoney = (a) => scaledString(parseMoney(a) < 0n ? -parseMoney(a) : parseMoney(a));

export function formatMoney(value, currency = 'USD', { rounding = null } = {}) {
  let scaled = parseMoney(value);
  if (rounding !== null) {
    if (!Number.isInteger(rounding) || rounding < 0 || rounding > 18) throw new RangeError('rounding must be 0..18');
    const factor = 10n ** BigInt(18 - rounding);
    const sign = scaled < 0n ? -1n : 1n;
    const magnitude = scaled < 0n ? -scaled : scaled;
    scaled = sign * (((magnitude + factor / 2n) / factor) * factor);
    const exact = scaledString(scaled, rounding, rounding);
    return `${currency} ${exact}`;
  }
  return `${currency} ${scaledString(scaled)}`;
}

export function moneyToChartNumber(value) {
  parseMoney(value);
  return Number(value);
}
export function negateMoney(value) { return scaledString(-parseMoney(value)); }

export function multiplyDivideMoney(value, numerator, denominator) {
  const n = BigInt(numerator); const d = BigInt(denominator);
  if (d === 0n) throw new RangeError('division by zero');
  const product = parseMoney(value) * n;
  const sign = (product < 0n) !== (d < 0n) ? -1n : 1n;
  const magnitude = (product < 0n ? -product : product);
  const divisor = d < 0n ? -d : d;
  return scaledString(sign * ((magnitude + divisor / 2n) / divisor));
}

export function sumMoney(values) { return values.reduce((total, value) => addMoney(total, value), '0'); }
export function groupMoneyByCurrency(rows, getCurrency = row => row.currency, getAmount = row => row.amount) {
  return rows.reduce((groups, row) => {
    const currency = getCurrency(row);
    groups[currency] = addMoney(groups[currency] ?? '0', getAmount(row));
    return groups;
  }, {});
}
export function filterMoneyByCurrency(rows, currency, getCurrency = row => row.currency) {
  return rows.filter(row => getCurrency(row) === currency);
}
export function groupTransactionsByPeriod(rows, currency, keyForRow) {
  const groups = new Map();
  for (const row of filterMoneyByCurrency(rows, currency)) {
    const name = keyForRow(row);
    const group = groups.get(name) || { name, sum: '0', count: 0, currency };
    group.sum = addMoney(group.sum, row.amount);
    group.count += 1;
    groups.set(name, group);
  }
  return [...groups.values()].sort((a, b) => b.name.localeCompare(a.name));
}
