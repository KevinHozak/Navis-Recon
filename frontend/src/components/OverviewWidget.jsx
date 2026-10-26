import { useEffect, useRef, useState } from 'react';
import { PieChart, Pie, Cell, Tooltip, ResponsiveContainer } from 'recharts';
import { RefreshCw } from 'lucide-react';
import { getLedgerSourceState, getLedgerSummary } from '../services/apiClient';
import { addMoney, formatMoney } from '../utils/money';
import { subscribeResourceChanges } from '../services/resourceChanges';
import { assetAllocation } from './assetAllocation';
import { createOverviewController, validateLedgerSummary } from './overviewState';

const reasonLabel = {
  included_selected_unit_asset: 'Included selected-unit asset',
  included_selected_unit_liability: 'Included selected-unit liability',
  unvalued_unit: 'Other unit; excluded with no valuation',
  outside_net_worth: 'Outside net worth (equity, income, or expense)',
};

function readTime(timestamp) {
  const date = new Date(timestamp);
  return Number.isNaN(date.getTime()) ? 'Unknown' : new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'medium' }).format(date);
}

export default function OverviewWidget({ onNavigateToReviews }) {
  const [view, setView] = useState({ summary: null, loading: true, error: null, source: 'unknown', sourceError: null });
  const controllerRef = useRef(null);

  useEffect(() => {
    const controller = createOverviewController({ loadSummary: getLedgerSummary, checkSource: getLedgerSourceState, onChange: setView });
    controllerRef.current = controller;
    const check = () => { void controller.check(); };
    window.addEventListener('focus', check);
    const interval = window.setInterval(check, 60_000);
    const stopResources = subscribeResourceChanges(ids => { if (ids.has('reviews')) void controller.refresh(); });
    void controller.refresh();
    return () => {
      window.removeEventListener('focus', check);
      window.clearInterval(interval);
      stopResources();
      controller.dispose();
      controllerRef.current = null;
    };
  }, []);

  const { summary, loading, error, source, sourceError, sourceChecking, sourceStale } = view;
  if (loading && !summary) return <div role="status" className="p-8 text-center text-accent uppercase font-mono animate-pulse">Loading validated ledger snapshot…</div>;
  if (!summary) {
    const message = error || 'No ledger snapshot is available.';
    const partial = message.toLowerCase().includes('partial:');
    const unavailable = message.toLowerCase().includes('unavailable:');
    return <section role="alert" className="rounded-lg border border-red-500/30 bg-zinc-950 p-6 text-red-300">
      <h2 className="font-bold">Ledger totals unavailable</h2>
      <p className="mt-2 text-sm">{partial ? 'The ledger contains unsupported or invalid data. Correct the entry at the indicated source location, then refresh.' : unavailable && message.toLowerCase().includes('no entries') ? 'The ledger contains no entries. Add valid ledger entries, then refresh.' : unavailable ? 'The configured ledger file could not be read. Check its configured path and file access, then refresh.' : 'The ledger could not be validated. Correct the reported issue, then refresh.'}</p>
      <pre className="mt-3 whitespace-pre-wrap break-words font-mono text-xs text-red-200/80">{message}</pre>
      <button type="button" onClick={() => void controllerRef.current?.refresh()} disabled={loading} className="mt-4 rounded border border-red-300/30 px-4 py-2 text-sm hover:bg-red-400/10 disabled:opacity-50">{loading ? 'Refreshing…' : 'Refresh'}</button>
    </section>;
  }

  const reconciliation = validateLedgerSummary(summary);
  if (!reconciliation.valid) return <section role="alert" className="rounded-lg border border-red-500/30 bg-zinc-950 p-6 text-red-300"><h2 className="font-bold">Coverage does not reconcile</h2><p className="mt-2 text-sm">{reconciliation.error} Refresh the ledger summary before relying on these totals.</p><button type="button" onClick={() => void controllerRef.current?.refresh()} disabled={loading} className="mt-4 rounded border border-red-300/30 px-4 py-2 text-sm hover:bg-red-400/10 disabled:opacity-50">{loading ? 'Refreshing…' : 'Refresh'}</button></section>;
  const currency = summary.selected_currency;
  const getSum = obj => Object.values(obj || {}).reduce((acc, val) => addMoney(acc, val), '0');
  const allocation = assetAllocation(summary);
  const assetData = allocation.chart;
  const renderCard = (title, icon, data, isLiability = false) => {
    if (!data || Object.keys(data).length === 0) return null;
    const total = getSum(data);
    const borderColor = isLiability ? 'border-red-500/20' : 'border-accent/20';
    const iconColor = isLiability ? 'text-red-500' : 'text-accent';
    return <div className={`p-6 bg-zinc-950 border ${borderColor} rounded shadow-lg hover:border-accent/50 transition-colors`}>
      <div className="flex items-center gap-2 mb-4"><span className={`${iconColor} text-xl`}>{icon}</span><h3 className="text-xl font-bold text-white uppercase tracking-wider">{title}{isLiability ? ' · signed balance' : ''}</h3></div>
      <p className="text-3xl font-bold text-white/90 mb-6">{formatMoney(total, currency)}</p>
      <div className="space-y-2">{isLiability && <p className="mb-3 text-xs text-white/60">Negative balances are debt; positive balances are credits or refunds.</p>}{Object.entries(data).map(([account, amount]) => <div key={account} className="flex justify-between text-xs font-mono border-b border-white/5 pb-1"><span className="text-white/50">{account}</span><span className="text-white/80">{formatMoney(amount, currency)}</span></div>)}</div>
    </div>;
  };
  const sourceMessage = source === 'changed'
    ? 'The ledger changed after this snapshot. These totals are stale.'
    : sourceStale
      ? 'This snapshot is stale or being replaced; wait for a successful refresh before relying on it.'
    : source === 'unavailable'
      ? 'The ledger source could not be checked. This is the last successfully read snapshot; its current status is unknown.'
      : source === 'current' ? 'Ledger source matches this snapshot.' : 'Ledger source freshness is not yet confirmed.';
  const sourceStyle = source === 'current' && !sourceChecking ? 'border-emerald-400/20 text-emerald-200' : source === 'unknown' && sourceChecking ? 'border-white/10 text-white/60' : 'border-amber-400/30 text-amber-200';
  const coverage = [...summary.coverage].sort((a, b) => a.account.localeCompare(b.account) || a.currency.localeCompare(b.currency));

  return <div className="space-y-8 animate-in fade-in duration-500">
    <div className="p-8 bg-zinc-900 border border-white/10 rounded-lg shadow-2xl relative overflow-hidden group flex flex-col md:flex-row items-center gap-8">
      <div className="absolute inset-0 bg-accent/5 opacity-0 group-hover:opacity-100 transition-opacity" />
      <div className="flex-1 z-10 w-full text-center md:text-left">
        <p className="text-xs font-mono text-white/50 uppercase tracking-widest mb-2">{currency} net worth subtotal · selected unit</p>
        <h2 className="text-5xl md:text-6xl font-black text-white tracking-tighter">{formatMoney(summary.net_worth, currency)}</h2>
        <div className={`mt-5 rounded border px-3 py-2 text-left text-xs ${sourceStyle}`} role={sourceStale || source === 'changed' || source === 'unavailable' ? 'alert' : 'status'}>
          <div className="flex flex-wrap items-center justify-between gap-3"><span><strong>{sourceStale || source === 'changed' ? 'Snapshot is stale' : source === 'current' ? 'Validated snapshot' : source === 'unavailable' ? 'Source unavailable' : 'Snapshot freshness'}</strong> · {sourceMessage}{sourceChecking ? ' Checking again…' : ''}</span><button type="button" onClick={() => void controllerRef.current?.refresh()} disabled={loading} className="inline-flex items-center gap-2 rounded border border-white/15 px-3 py-1.5 font-semibold text-white disabled:cursor-wait disabled:opacity-50"><RefreshCw size={14} aria-hidden="true" className={loading ? 'animate-spin' : ''} />{loading ? 'Refreshing…' : 'Refresh'}</button></div>
          {(source === 'unavailable' && sourceError) && <p className="mt-2 break-words text-amber-100/75">{sourceError}</p>}
          <dl className="mt-3 grid gap-x-4 gap-y-1 sm:grid-cols-2 text-white/65"><div><dt className="inline">Snapshot read: </dt><dd className="inline">{readTime(summary.read_at_unix_ms)}</dd></div><div><dt className="inline">Latest transaction: </dt><dd className="inline">{summary.latest_transaction_date || 'No transaction date'}</dd></div><div className="sm:col-span-2"><dt className="inline">Ledger revision: </dt><dd className="inline font-mono" title={summary.ledger_revision}>{summary.ledger_revision.slice(0, 12)}…</dd></div><div className="sm:col-span-2">Selected-unit native balances; no currency conversion.</div></dl>
        </div>
      </div>
      {assetData.length > 0 && <div className="w-full md:w-64 h-48 z-10"><ResponsiveContainer width="100%" height="100%"><PieChart><Pie data={assetData} cx="50%" cy="50%" innerRadius={50} outerRadius={80} paddingAngle={3} dataKey="chartValue" stroke="rgba(255,255,255,0.02)" strokeWidth={2}>{assetData.map((entry, index) => <Cell key={`cell-${index}`} fill={entry.color} />)}</Pie><Tooltip formatter={(value, name, item) => [formatMoney(item.payload.value, currency), name]} contentStyle={{ backgroundColor: '#09090b', borderColor: 'rgba(255,255,255,0.1)', color: '#fff', borderRadius: '8px' }} itemStyle={{ color: '#fff', fontWeight: 'bold' }} /></PieChart></ResponsiveContainer></div>}
    </div>

    <section aria-label="Asset allocation text alternative" className="rounded-lg border border-white/10 bg-zinc-950 p-5">
      <h3 className="font-semibold text-white">Asset allocation · {currency}</h3>
      <p className="mt-2 text-sm text-white/65">Chart basis: positive asset balances {formatMoney(allocation.positiveBasis, currency)}. Negative offsets {formatMoney(allocation.negativeOffsets, currency)} are excluded from pie geometry. Together they reconcile to signed assets {formatMoney(allocation.signedTotal, currency)}. All selected-unit asset categories are covered; other units have no conversion.</p>
      {assetData.length === 0 && <p role="status" className="mt-2 text-amber-200">No positive asset balances are available to chart.</p>}
      <div className="mt-4 overflow-x-auto"><table className="w-full text-left text-sm"><caption className="sr-only">Exact asset balances and chart coverage</caption><thead className="text-white/50"><tr><th scope="col">Category</th><th scope="col">Chart basis</th><th scope="col">Negative offsets</th><th scope="col">Signed balance</th></tr></thead><tbody>{allocation.rows.map(row => <tr key={row.name} className="border-t border-white/10"><th scope="row" className="py-2 font-normal">{row.name}</th><td>{formatMoney(row.positive, currency)}</td><td>{formatMoney(row.negative, currency)}</td><td>{formatMoney(row.balance, currency)}</td></tr>)}</tbody></table></div>
    </section>

    {(summary.unvalued_units || []).length > 0 && <div className="text-amber-300 text-xs font-mono space-y-1"><p>Showing {currency} subtotal only. No valuation is available for {summary.unvalued_units.join(', ')}; these units are excluded from the selected-unit total.</p>{summary.unvalued_units.map(unit => { const totals = summary.totals_by_currency?.[unit]; return totals ? <p key={unit}>{unit} separate net worth: {formatMoney(totals.net_worth, unit)} (assets {formatMoney(totals.total_assets, unit)}; liabilities {formatMoney(totals.total_liabilities, unit)}).</p> : null; })}</div>}

    <details className="rounded-lg border border-white/10 bg-zinc-950"><summary className="cursor-pointer list-none px-5 py-4 font-semibold text-white hover:text-accent">Account coverage <span className="ml-2 text-xs font-normal text-white/50">{summary.selected_included_account_count} included · {summary.selected_excluded_account_count} selected-unit exclusions · {summary.account_count} unique accounts across units</span></summary>
      <div className="border-t border-white/10 p-5"><p className="mb-4 text-xs text-white/55">Coverage lists direct account/unit balances, including zero balances and parent and child accounts. Counts refer to account/unit rows; unique account names are counted once across units.</p>
        <div className="overflow-x-auto"><table className="w-full min-w-[640px] text-left text-xs"><thead className="text-white/50"><tr><th className="pb-2 pr-4">Account</th><th className="pb-2 pr-4">Unit</th><th className="pb-2 pr-4 text-right">Exact balance</th><th className="pb-2 pr-4">Coverage</th><th className="pb-2">Reason</th></tr></thead><tbody>{coverage.map((row, index) => <tr key={`${row.account}-${row.currency}-${index}`} className="border-t border-white/5"><td className="py-2 pr-4 font-mono text-white/80">{row.account}</td><td className="py-2 pr-4 font-mono text-white/65">{row.currency}</td><td className="py-2 pr-4 text-right font-mono text-white/80">{row.balance} {row.currency}</td><td className={`py-2 pr-4 ${row.included_in_net_worth ? 'text-emerald-300' : 'text-amber-200'}`}>{row.included_in_net_worth ? 'Included' : 'Excluded'}</td><td className="py-2 text-white/55">{reasonLabel[row.reason]}</td></tr>)}</tbody></table></div>
        <div className="mt-4 grid gap-1 border-t border-white/10 pt-4 text-xs font-mono text-white/65 sm:grid-cols-3"><p>Assets: {reconciliation.coveredAssets} {currency}</p><p>Liabilities: {reconciliation.coveredLiabilities} {currency}</p><p>Net worth: {reconciliation.coveredNetWorth} {currency}</p></div>
      </div>
    </details>

    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-2 xl:grid-cols-3 gap-6">{renderCard('Bank Accounts', '🏦', summary.bank_accounts)}{renderCard('Retirement', '📈', summary.retirement)}{renderCard('Investments', '💹', summary.investments)}{renderCard('Credit Cards', '💳', summary.credit_cards, true)}{renderCard('Property', '🏠', { Housing: summary.housing })}{renderCard('Cash', '💵', { Cash: summary.cash })}</div>
    <button type="button" onClick={onNavigateToReviews} className="w-full mt-4 p-4 bg-accent/10 border border-accent/40 text-accent font-bold uppercase tracking-widest rounded hover:bg-accent hover:text-black transition-all flex items-center justify-center gap-2"><span>Process Pending Reviews</span><span>→</span></button>
  </div>;
}
