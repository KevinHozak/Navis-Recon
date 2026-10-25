import { useEffect, useRef, useState } from 'react'
import { getReconciliations, saveReconciliation, compareReconciliation } from '../services/apiClient'

const blank = statement => ({ id: '', statement_path: statement?.path || '', account: '', period_start: '', period_end: statement?.date || '', currency: 'USD', status: 'received', opening_balance: null, closing_balance: null, matched_transaction_ids: [] })
export default function ReconciliationWorkspace({ statement, onSaved }) {
  const [snapshot, setSnapshot] = useState(null)
  const [draft, setDraft] = useState(() => blank(statement))
  const [comparison, setComparison] = useState(null)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false)
  const owner = useRef({ generation: 0 })
  const pending = useRef(false)
  const savedCallback = useRef(onSaved)
  useEffect(() => { savedCallback.current = onSaved }, [onSaved])
  useEffect(() => {
    const lifetime = owner.current
    const generation = ++lifetime.generation
    getReconciliations().then(value => { if (owner.current.generation === generation) { setSnapshot(value); savedCallback.current?.(value) } }).catch(e => { if (owner.current.generation === generation) setError(String(e.message || e)) })
    return () => { lifetime.generation++ }
  }, [statement])
  async function action(kind) {
    if (pending.current) return
    pending.current = true; setBusy(true); setError(''); setMessage('')
    const generation = owner.current.generation
    try {
      if (kind === 'refresh') {
        const value = await getReconciliations()
        if (generation !== owner.current.generation) return
        setSnapshot(value); savedCallback.current?.(value); setComparison(null)
        setDraft(value.records.find(r => r.draft.id === draft.id)?.draft || blank(statement))
        setMessage('Persisted records reloaded; previous unsaved edit discarded.')
      } else if (kind === 'compare') {
        const value = await compareReconciliation(draft)
        if (generation === owner.current.generation) setComparison(value)
      } else {
        const value = await saveReconciliation(draft, snapshot.revision)
        if (generation !== owner.current.generation) return
        setSnapshot(value); savedCallback.current?.(value)
        const saved = value.records.find(r => r.draft.statement_path === draft.statement_path && r.draft.account === draft.account && r.draft.period_start === draft.period_start && r.draft.period_end === draft.period_end && r.draft.currency === draft.currency)
        if (saved) { setDraft(saved.draft); setComparison(saved.comparison) }
        setMessage('Saved and verified on disk')
      }
    } catch (e) { if (generation === owner.current.generation) setError(String(e.message || e)) }
    finally { pending.current = false; if (generation === owner.current.generation) setBusy(false) }
  }
  function change(key, value) { if (pending.current) return; setDraft(old => ({ ...old, [key]: value })); setComparison(old => key === 'matched_transaction_ids' && old ? { ...old, can_reconcile: false, candidates: old.candidates.map(c => ({ ...c, matched: value.includes(c.id) })) } : null); setMessage('Unsaved changes') }
  const related = snapshot?.records.filter(r => r.draft.statement_path === statement?.path) || []
  const overlaps = snapshot?.records.filter(r => r.draft.id !== draft.id && r.draft.account === draft.account && r.draft.currency === draft.currency && r.draft.period_start <= draft.period_end && r.draft.period_end >= draft.period_start) || []
  return <section className="rounded-xl border border-cyan-400/30 bg-zinc-950 p-5 space-y-4">
    <div className="flex justify-between gap-3"><h3 className="font-bold text-cyan-300">Statement reconciliation</h3><button type="button" disabled={busy} onClick={() => action('refresh')} className="text-xs underline">Refresh records</button></div>
    {error && <p role="alert" className="text-red-300 whitespace-pre-wrap">{error} Your edit remains unsaved.</p>}
    <p role="status" className="text-sm text-cyan-200">{busy ? 'Saving / checking…' : message}</p>
    {!statement ? <p className="text-white/50">Choose Reconcile beside a statement to map its account and period.</p> : <>
      <p className="text-sm text-white/70">{statement.filename}</p>
      <div className="flex flex-wrap gap-2"><button disabled={busy} type="button" className="border border-white/20 rounded px-2 py-1" onClick={() => { setDraft(blank(statement)); setComparison(null); setMessage('New account / period mapping') }}>New mapping</button>{related.map(r => <button disabled={busy} type="button" key={r.draft.id} className="border border-white/20 rounded px-2 py-1 text-xs" onClick={() => { setDraft(r.draft); setComparison(null); setMessage('Loaded persisted record; compare again to validate freshness') }}>{r.draft.account} · {r.draft.period_start} – {r.draft.period_end} · {snapshot.stale_ids.includes(r.draft.id) ? 'stale evidence' : r.draft.status}</button>)}</div>
      {overlaps.length > 0 && <p className="text-amber-300 text-sm">{overlaps.length} overlapping account/unit period records. These remain separate evidence; do not count statements twice.</p>}
      <fieldset disabled={busy} className="grid sm:grid-cols-2 lg:grid-cols-3 gap-3 disabled:opacity-60">
        <label className="text-sm">Account<input aria-label="Reconciliation account" className="block w-full bg-black border border-white/20 p-2 rounded" value={draft.account} onChange={e => change('account', e.target.value)} placeholder="Assets:Bank:Checking" /></label>
        <label className="text-sm">Period start<input aria-label="Period start" type="date" className="block w-full bg-black border border-white/20 p-2 rounded" value={draft.period_start} onChange={e => change('period_start', e.target.value)} /></label>
        <label className="text-sm">Period end (inclusive)<input aria-label="Period end" type="date" className="block w-full bg-black border border-white/20 p-2 rounded" value={draft.period_end} onChange={e => change('period_end', e.target.value)} /></label>
        <label className="text-sm">Currency / unit<input aria-label="Currency" className="block w-full bg-black border border-white/20 p-2 rounded" value={draft.currency} onChange={e => change('currency', e.target.value)} /></label>
        <label className="text-sm">Statement opening balance<input aria-label="Opening balance" inputMode="decimal" className="block w-full bg-black border border-white/20 p-2 rounded" value={draft.opening_balance ?? ''} onChange={e => change('opening_balance', e.target.value || null)} /></label>
        <label className="text-sm">Statement closing balance<input aria-label="Closing balance" inputMode="decimal" className="block w-full bg-black border border-white/20 p-2 rounded" value={draft.closing_balance ?? ''} onChange={e => change('closing_balance', e.target.value || null)} /></label>
        <label className="text-sm">Review status<select aria-label="Review status" className="block bg-black border border-white/20 p-2 rounded" value={draft.status} onChange={e => change('status', e.target.value)}>{['received','imported','reviewed','reconciled'].map(s => <option key={s}>{s}</option>)}</select></label>
      </fieldset>
      <p className="text-xs text-white/50">Exact account and unit; opening is before the start date, closing includes all activity through the end date. Use signed ledger balances for liabilities. Reconciliation requires zero differences and every cleared period transaction matched; pending entries block completion. Legacy checkmarks provide no balance evidence.</p>
      <div className="flex gap-3"><button type="button" disabled={busy} onClick={() => change('matched_transaction_ids', [])} className="text-xs underline">Clear transaction selections</button><button type="button" disabled={busy || !snapshot} onClick={() => action('save')} className="px-4 py-2 rounded bg-cyan-500/20 text-cyan-300 border border-cyan-400/40 disabled:opacity-40">Save record</button><button type="button" disabled={busy} onClick={() => action('compare')} className="px-4 py-2 rounded border border-white/20 disabled:opacity-40">Compare balances</button></div>
      {comparison && <div className="space-y-3 text-sm"><p className="font-mono">Ledger opening: {comparison.ledger_opening} {draft.currency} · difference: {comparison.opening_difference ?? 'not provided'}</p><p className="font-mono">Ledger closing: {comparison.ledger_closing} {draft.currency} · difference: {comparison.closing_difference ?? 'not provided'}</p><p className={comparison.can_reconcile ? 'text-emerald-300' : 'text-amber-300'}>{comparison.can_reconcile ? 'Eligible for reconciliation; save the next status.' : 'Resolve differences, pending items and unmatched period transactions.'}</p><p className="text-xs text-white/40 break-all">Ledger revision: {comparison.ledger_revision}</p>
        <div className="max-h-96 overflow-auto space-y-2">{comparison.candidates.map(c => <label key={c.id} className="flex gap-3 p-2 border border-white/10 rounded"><input type="checkbox" disabled={busy || !c.in_period || c.pending} checked={draft.matched_transaction_ids.includes(c.id)} onChange={e => change('matched_transaction_ids', e.target.checked ? [...draft.matched_transaction_ids, c.id] : draft.matched_transaction_ids.filter(id => id !== c.id))} /><span>{c.date} · {c.description || 'Ledger transaction'} · {c.amount} {c.currency} · {c.pending ? 'pending' : 'cleared'}{c.transfer ? ' · transfer candidate' : ''}{!c.in_period ? ' · outside period / timing candidate' : c.matched ? ' · matched' : ' · unmatched candidate'}</span></label>)}</div>
      </div>}
    </>}
  </section>
}
