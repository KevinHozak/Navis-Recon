import { useEffect, useRef, useState } from 'react';
import { getMonthlyClose, saveMonthlyClose } from '../services/apiClient';
import { subscribeResourceChanges } from '../services/resourceChanges';
import { createMonthlyCloseController } from '../services/monthlyCloseState';
import { formatMoney } from '../utils/money';

const previousMonth = () => { const date = new Date(); return new Date(Date.UTC(date.getFullYear(), date.getMonth() - 1, 1)).toISOString().slice(0, 7); };
const stages = [['collect', 'Collect statements'], ['import', 'Import'], ['review', 'Review'], ['reconcile', 'Reconcile'], ['summarize', 'Summarize']];
export default function MonthlyCloseWidget({ onNavigate }) {
  const [month, setMonth] = useState(previousMonth);
  const [currency, setCurrency] = useState('USD');
  const [accounts, setAccounts] = useState([]);
  const [exceptions, setExceptions] = useState({});
  const [knownAccounts, setKnownAccounts] = useState([]);
  const [view, setView] = useState({ loading: true, snapshot: null, request: null, saving: false, error: null });
  const ref = useRef(null);
  const request = { month, currency, accounts: [...accounts].sort(), exceptions: Object.entries(exceptions).filter(([, reason]) => reason.trim()).map(([account, reason]) => ({ account, reason })).sort((a, b) => a.account.localeCompare(b.account)) };
  useEffect(() => {
    const controller = createMonthlyCloseController({ load: getMonthlyClose, save: saveMonthlyClose, onChange: next => { setView(next); if (next.snapshot) setKnownAccounts(next.snapshot.accounts); } });
    ref.current = controller;
    void controller.refresh({ month: previousMonth(), currency: 'USD', accounts: [], exceptions: [] });
    const refresh = () => { void controller.refresh(); };
    const stop = subscribeResourceChanges(ids => { if (['reviews', 'statements', 'sync'].some(id => ids.has(id))) refresh(); });
    window.addEventListener('focus', refresh);
    const timer = window.setInterval(refresh, 60_000);
    return () => { controller.dispose(); stop(); window.removeEventListener('focus', refresh); window.clearInterval(timer); ref.current = null; };
  }, []);
  const snapshot = view.snapshot;
  const scopeChanged = JSON.stringify(request) !== JSON.stringify(view.request);
  const canSave = snapshot && !view.loading && !view.saving && !scopeChanged;
  return <section aria-label="Monthly close" className="mt-8 rounded-lg border border-white/10 bg-zinc-950 p-6 space-y-5">
    <h2 className="text-xl font-bold">Monthly close</h2>
    <p className="text-sm text-white/60">Collect → import → review → reconcile → summarize. Every selected-unit asset and liability account needs full-month statement coverage or an explicit scope exception. Exceptions are retained in the private summary; they do not verify a balance. Checklist ticks alone cannot close a month.</p>
    <fieldset disabled={view.saving} className="space-y-4 disabled:opacity-60"><legend className="sr-only">Close period and account scope</legend>
      <div className="flex flex-wrap gap-4"><label className="text-sm">Month <input type="month" value={month} onChange={event => { setMonth(event.target.value); setAccounts([]); setExceptions({}); }} className="ml-2 rounded border border-white/20 bg-zinc-900 p-2" /></label><label className="text-sm">Unit <input aria-label="Close currency or unit" value={currency} onChange={event => { setCurrency(event.target.value); setAccounts([]); setExceptions({}); }} className="ml-2 w-24 rounded border border-white/20 bg-zinc-900 p-2" /></label></div>
      <p className="text-xs text-white/60">Select accounts requiring statements. For an account without external statements, leave it unselected and explain the scope exception.</p>
      {knownAccounts.map(account => <div key={account} className="rounded border border-white/10 p-3"><label className="flex gap-3 text-sm"><input type="checkbox" checked={accounts.includes(account)} onChange={event => { setAccounts(event.target.checked ? [...accounts, account] : accounts.filter(a => a !== account)); if (event.target.checked) setExceptions(previous => { const next = { ...previous }; delete next[account]; return next; }); }} />{account}</label>{!accounts.includes(account) && <label className="mt-2 block text-xs text-white/60">Scope exception reason (if applicable)<input value={exceptions[account] || ''} onChange={event => setExceptions(previous => ({ ...previous, [account]: event.target.value }))} className="mt-1 block w-full rounded border border-white/20 bg-zinc-900 p-2 text-white" /></label>}</div>)}
      <button type="button" onClick={() => void ref.current?.refresh(request)} disabled={view.loading} className="rounded border border-accent/40 px-4 py-2 text-accent disabled:opacity-50">{view.loading ? 'Checking evidence…' : 'Check close evidence'}</button>
    </fieldset>
    {scopeChanged && <p role="status" className="text-sm text-amber-200">Scope changed. Check close evidence before saving.</p>}
    {view.error && <p role="alert" className="text-sm text-red-300">{view.error}</p>}
    {snapshot && <><p role="status" className={snapshot.state === 'closed' && !scopeChanged && !view.loading ? 'text-emerald-200' : 'text-amber-200'}>Close state: {view.loading ? 'Checking current evidence' : scopeChanged ? 'Scope unverified' : snapshot.state} · ledger {snapshot.ledger_revision.slice(0, 12)} · {snapshot.history_count} saved history entries</p>
      {snapshot.last_summary && <button type="button" disabled={view.saving || view.loading} onClick={() => { const scope = snapshot.last_summary.scope; setMonth(scope.month); setCurrency(scope.currency); setAccounts(scope.accounts); setExceptions(Object.fromEntries(scope.exceptions.map(e => [e.account, e.reason]))); void ref.current?.refresh(scope); }} className="text-sm text-accent underline disabled:opacity-40">Load and verify last saved account scope</button>}
      <ol className="space-y-3">{stages.map(([id, title]) => { const blockers = snapshot.blockers.filter(blocker => blocker.stage === id); return <li key={id} className="rounded border border-white/10 p-3"><h3 className="font-semibold">{title} · {blockers.length ? `${blockers.length} blockers` : 'No blockers in checked scope'}</h3>{blockers.map((blocker, index) => <div key={`${blocker.message}-${index}`} className="mt-2 text-sm text-amber-200"><p>{blocker.message}</p><p className="text-xs text-white/60">Owner: {blocker.owner}</p><button type="button" onClick={() => onNavigate(blocker.view, {})} className="mt-1 underline text-accent">Open resolution view</button></div>)}</li>; })}</ol>
      {snapshot.metrics.map(metric => <dl key={metric.month} className="grid gap-2 text-sm sm:grid-cols-3"><div><dt>Income</dt><dd>{formatMoney(metric.income, currency)}</dd></div><div><dt>Spending</dt><dd>{formatMoney(metric.spending, currency)}</dd></div><div><dt>Net-worth movement</dt><dd>{metric.net_worth_change === null ? 'Unavailable' : formatMoney(metric.net_worth_change, currency)}</dd></div></dl>)}
      <div className="flex flex-wrap gap-3"><button type="button" disabled={!canSave || snapshot.blockers.length > 0 || snapshot.state === 'closed'} onClick={() => void ref.current?.persist('closed')} className="rounded border border-accent/40 bg-accent/10 px-4 py-2 text-accent disabled:opacity-40">{view.saving ? 'Saving…' : 'Close month and save local summary'}</button><button type="button" disabled={!canSave || !snapshot.last_summary || snapshot.state === 'reopened'} onClick={() => void ref.current?.persist('reopened')} className="rounded border border-white/20 px-4 py-2 disabled:opacity-40">Reopen saved close</button></div>
      {snapshot.last_summary && <details className="rounded border border-white/10 p-3"><summary className="cursor-pointer">Last saved summary · {snapshot.last_summary.action}</summary><p className="mt-2 text-xs text-white/60">Saved privately under Windows application data for this ledger. No automatic remote publication. This retained summary may be stale relative to current evidence.</p><pre className="mt-3 max-h-96 overflow-auto whitespace-pre-wrap break-words text-xs text-white/70">{JSON.stringify(snapshot.last_summary, null, 2)}</pre></details>}
    </>}
  </section>;
}
