"use client";

import { useCallback, useEffect, useRef, useState } from 'react';
import CategorySelector from './CategorySelector';
import { approveLedgerTransaction, getLedgerApprovalHistory, undoLedgerApproval, getLedgerCategories, getPendingLedger } from '../services/apiClient';
import { createLedgerReviewController, resolveApprovalCompletion, resolveUndoCompletion, transactionKey } from '../services/ledgerReviewController';
import { formatMoney, negateMoney, compareMoney } from '../utils/money';

export default function LedgerReviewWidget() {
  const [pending, setPending] = useState([]);
  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState({});
  const [history, setHistory] = useState([]);
  const [historyError, setHistoryError] = useState('');
  const [historyLoading, setHistoryLoading] = useState(true);
  const [undoing, setUndoing] = useState('');
  const [saved, setSaved] = useState('');
  const mounted = useRef(false);
  const [controller] = useState(() => createLedgerReviewController({ list: getPendingLedger, approve: approveLedgerTransaction, history: getLedgerApprovalHistory, undo: undoLedgerApproval }));

  const fetchPending = useCallback(async ({ clearError = false } = {}) => {
    setLoading(true); if (clearError) setError('');
    const result = await controller.refresh();
    if (!mounted.current || result.stale) return;
    if (result.error) setError(result.error);
    else setPending(result.rows);
    setLoading(false);
  }, [controller]);

  const fetchHistory = useCallback(async () => {
    if (!mounted.current) return;
    setHistoryLoading(true);
    const result = await controller.refreshHistory();
    if (!mounted.current || result.stale) return;
    setHistoryLoading(false);
    if (result.error) setHistoryError(result.error); else setHistory(result.rows);
  }, [controller]);

  const refreshAll = useCallback(async ({ clearErrors = false } = {}) => { if (!mounted.current) return; if (clearErrors) { setError(''); setHistoryError(''); setSaved(''); } await fetchHistory(); if (mounted.current) await fetchPending({ clearError: clearErrors }); }, [fetchHistory, fetchPending]);

  useEffect(() => {
    controller.activate();
    let active = true;
    mounted.current = true;
    Promise.resolve().then(async () => { await fetchHistory(); if (active) await fetchPending(); });
    getLedgerCategories().then((items) => { if (active) setCategories(items); }).catch((e) => { if (active) setError(e instanceof Error ? e.message : String(e)); });
    return () => { active = false; mounted.current = false; controller.dispose(); };
  }, [controller, fetchHistory, fetchPending]);

  const handleApprove = async (tx) => {
    const key = transactionKey(tx);
    if (controller.isInFlight(tx)) return;
    setSaved('');
    setBusy((previous) => ({ ...previous, [key]: true }));
    const result = await controller.approve(tx, tx.category);
    const completion = resolveApprovalCompletion(result, mounted.current, key);
    if (!mounted.current) return;
    setBusy(completion.clearBusy);
    if (completion.stale) return;
    if (completion.error) { setError(completion.error); await refreshAll(); return; }
    if (completion.refreshRequired) { setSaved('Approval saved'); await refreshAll(); }
  };

  const handleUndo = async (record) => {
    if (controller.hasUndoInFlight() || record.undone) return;
    setUndoing(record.id); setSaved(''); setHistoryError('');
    const result = await controller.undo(record);
    const completion = resolveUndoCompletion(result, mounted.current);
    if (completion.clearUndoing) setUndoing('');
    if (completion.stale) return;
    if (completion.saved) setSaved('Undo saved'); else if (completion.error) setHistoryError(completion.error);
    if (completion.refreshRequired || completion.error) await refreshAll();
  };

  const handleCategoryChange = (key, value) => setPending((rows) => rows.map((tx) => transactionKey(tx) === key ? { ...tx, category: value } : tx));

  return (
    <div className="space-y-6">
      <h3 className="text-xl font-black text-white tracking-widest uppercase italic border-b border-white/10 pb-4">Pending Ledger Reviews</h3>
      {error && <div role="alert" className="p-4 border border-red-500/40 bg-red-950/30 text-red-200 rounded text-sm">{error} <button className="ml-3 underline" onClick={refreshAll}>Refresh pending transactions</button></div>}
      {loading && <div className="text-xs text-white/50 font-mono">Refreshing ledger review…</div>}
      {saved && <div role="status" className="text-sm text-emerald-300">{saved}</div>}
      {!loading && !error && pending.length === 0 && <div className="p-8 border border-white/5 bg-zinc-900/60 rounded text-center text-white/50 font-mono text-sm">No pending transactions in Sovereign.beancount</div>}
      {pending.length > 0 && <div className="grid grid-cols-1 gap-4">
        {pending.map((tx) => {
          const key = transactionKey(tx); const approving = Boolean(busy[key]); const permitted = tx.permitted_categories ?? categories;
          return <div key={key} className="p-4 bg-zinc-950 border border-white/10 rounded group hover:border-accent/30 transition-all flex justify-between items-center">
            <div><div className="text-[10px] font-mono text-white/40 mb-1">{tx.date}</div><div className="text-sm font-bold text-white tracking-wider">{tx.payee}</div><div className={`text-lg font-mono font-bold ${compareMoney(tx.amount, '0') < 0 ? 'text-green-400' : 'text-red-400'}`}>{formatMoney(negateMoney(tx.amount), tx.currency)}</div></div>
            <div className="flex items-center gap-4"><div className="flex flex-col gap-1"><label className="text-[9px] uppercase font-bold text-white/40 tracking-widest">AI Categorization</label><CategorySelector value={tx.category} onChange={(value) => handleCategoryChange(key, value)} categories={permitted} disabled={approving || loading || !permitted.length} />{!permitted.length && <span className="text-[10px] text-red-300">No eligible open category; review manually.</span>}</div>
              <button disabled={approving || loading || !permitted.includes(tx.category)} onClick={() => handleApprove(tx)} className="px-6 py-2 bg-accent/10 border border-accent/50 text-accent font-bold uppercase text-xs tracking-widest rounded hover:bg-accent hover:text-black transition-all disabled:opacity-50">{approving ? 'Saving…' : 'Approve'}</button>
            </div>
          </div>;
        })}
      </div>}
      <section className="space-y-3 border-t border-white/10 pt-5">
        <div className="flex items-center justify-between"><h4 className="text-sm font-bold uppercase tracking-widest text-white/70">Approval History</h4><button className="text-xs underline text-white/50" onClick={() => refreshAll({ clearErrors: true })}>Refresh</button></div>
        {historyLoading && <p className="text-xs text-white/50">Loading history…</p>}
        {historyError && <div role="alert" className="text-sm text-red-200">{historyError} <button className="underline" onClick={() => refreshAll({ clearErrors: true })}>Refresh history</button></div>}
        {!historyLoading && !historyError && history.length === 0 && <p className="text-sm text-white/40">No saved approvals yet.</p>}
        {history.map((record) => <div key={record.id} className="flex items-center justify-between gap-4 rounded border border-white/10 bg-zinc-950 p-3 text-sm">
          <div><div className="text-white/80">{record.operation_type === 'undo' ? 'Undo' : 'Approval'} · {record.old_category} → {record.new_category}</div><div className="text-xs text-white/40">{new Date(record.timestamp_unix_ms).toLocaleString()} · {record.before_hash.slice(0, 12)} → {record.after_hash.slice(0, 12)}</div></div>
          {record.operation_type === 'approve' && <button disabled={record.undone || controller.hasUndoInFlight()} onClick={() => handleUndo(record)} className="rounded border border-accent/40 px-3 py-1 text-accent disabled:opacity-40">{undoing === record.id ? 'Undoing…' : record.undone ? 'Undone' : 'Undo'}</button>}
        </div>)}
      </section>
    </div>
  );
}
