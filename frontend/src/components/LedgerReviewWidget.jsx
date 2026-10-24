"use client";

import { useCallback, useEffect, useRef, useState } from 'react';
import CategorySelector from './CategorySelector';
import { approveLedgerTransaction, getLedgerCategories, getPendingLedger } from '../services/apiClient';
import { createLedgerReviewController, resolveApprovalCompletion, transactionKey } from '../services/ledgerReviewController';
import { formatMoney, negateMoney, compareMoney } from '../utils/money';

export default function LedgerReviewWidget() {
  const [pending, setPending] = useState([]);
  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState({});
  const [controller] = useState(() => createLedgerReviewController({ list: getPendingLedger, approve: approveLedgerTransaction }));
  const mounted = useRef(false);

  const fetchPending = useCallback(async () => {
    setLoading(true); setError('');
    const result = await controller.refresh();
    if (!mounted.current || result.stale) return;
    if (result.error) setError(result.error);
    else setPending(result.rows);
    setLoading(false);
  }, [controller]);

  useEffect(() => {
    mounted.current = true;
    controller.activate();
    Promise.resolve().then(() => { if (mounted.current) fetchPending(); });
    getLedgerCategories().then((items) => { if (mounted.current) setCategories(items); }).catch((e) => { if (mounted.current) setError(e instanceof Error ? e.message : String(e)); });
    return () => { mounted.current = false; controller.dispose(); };
  }, [controller, fetchPending]);

  const handleApprove = async (tx) => {
    const key = transactionKey(tx);
    if (controller.isInFlight(tx)) return;
    setBusy((previous) => ({ ...previous, [key]: true }));
    const result = await controller.approve(tx, tx.category);
    const completion = resolveApprovalCompletion(result, mounted.current, key);
    if (!mounted.current) return;
    setBusy(completion.clearBusy);
    if (completion.stale) return;
    if (completion.error) { setError(completion.error); return; }
    if (completion.refreshRequired) await fetchPending();
  };

  const handleCategoryChange = (key, value) => setPending((rows) => rows.map((tx) => transactionKey(tx) === key ? { ...tx, category: value } : tx));

  if (loading && pending.length === 0) return <div className="p-8 text-center text-accent uppercase font-mono animate-pulse">Scanning Ledger for Pending Transactions...</div>;

  return (
    <div className="space-y-6">
      <h3 className="text-xl font-black text-white tracking-widest uppercase italic border-b border-white/10 pb-4">Pending Ledger Reviews</h3>
      {error && <div role="alert" className="p-4 border border-red-500/40 bg-red-950/30 text-red-200 rounded text-sm">{error} <button className="ml-3 underline" onClick={fetchPending}>Refresh pending transactions</button></div>}
      {loading && <div className="text-xs text-white/50 font-mono">Refreshing ledger review…</div>}
      {!loading && !error && pending.length === 0 && <div className="p-8 border border-white/5 bg-zinc-900/60 rounded text-center text-white/50 font-mono text-sm">No pending transactions in Sovereign.beancount</div>}
      {pending.length > 0 && <div className="grid grid-cols-1 gap-4">
        {pending.map((tx) => {
          const key = transactionKey(tx); const approving = Boolean(busy[key]); const permitted = tx.permitted_categories ?? categories;
          return <div key={key} className="p-4 bg-zinc-950 border border-white/10 rounded group hover:border-accent/30 transition-all flex justify-between items-center">
            <div><div className="text-[10px] font-mono text-white/40 mb-1">{tx.date}</div><div className="text-sm font-bold text-white tracking-wider">{tx.payee}</div><div className={`text-lg font-mono font-bold ${compareMoney(tx.amount, '0') < 0 ? 'text-green-400' : 'text-red-400'}`}>{formatMoney(negateMoney(tx.amount), tx.currency)}</div></div>
            <div className="flex items-center gap-4"><div className="flex flex-col gap-1"><label className="text-[9px] uppercase font-bold text-white/40 tracking-widest">AI Categorization</label><CategorySelector value={tx.category} onChange={(value) => handleCategoryChange(key, value)} categories={permitted} disabled={approving || loading || !permitted.length} />{!permitted.length && <span className="text-[10px] text-red-300">No eligible open category; review manually.</span>}</div>
              <button disabled={approving || loading || !permitted.includes(tx.category)} onClick={() => handleApprove(tx)} className="px-6 py-2 bg-accent/10 border border-accent/50 text-accent font-bold uppercase text-xs tracking-widest rounded hover:bg-accent hover:text-black transition-all disabled:opacity-50">{approving ? 'Approving…' : 'Approve'}</button>
            </div>
          </div>;
        })}
      </div>}
    </div>
  );
}
