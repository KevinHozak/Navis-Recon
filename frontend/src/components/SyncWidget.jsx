import { useEffect, useRef, useState } from 'react'
import { cancelSyncRun, commitLedgerSync, getSyncRun, getSyncOutcomes, reconcileLedgerSync, retryLedgerSync, isDesktopRuntime, listenForSyncEvents, startLedgerSync } from '../services/apiClient'
import { createSyncRunController } from '../services/syncRunController'
import { canAbandonReview, canCommitReview, canStartRun, canToggleMatch, managedOutcomes, retryableManagedItems, showsReview } from '../services/syncRunView'

const empty = { status: 'idle', logs: [], matches: [], unmatched: [], errors: [] }
const displayText = value => ['string', 'number', 'boolean'].includes(typeof value) ? String(value) : ''
const SyncWidget = () => {
  const [view, setView] = useState({ run: null, pending: false, ready: false })
  const scrollRef = useRef(null)
  const controllerRef = useRef(null)
  const run = view.run || empty
  const { status = 'idle', logs = [], matches = [], unmatched = [], errors = [] } = run

  useEffect(() => {
    const api = { getSyncRun, getSyncOutcomes, reconcileLedgerSync, retryLedgerSync, listenForSyncEvents, startLedgerSync, commitLedgerSync, cancelSyncRun }
    const controller = createSyncRunController({ api, onChange: setView })
    controllerRef.current = controller
    controller.connect()
    return () => { controller.dispose(); if (controllerRef.current === controller) controllerRef.current = null }
  }, [])

  useEffect(() => {
    if (status === 'analyzing' && scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight
  }, [logs, status])

  const selected = view.selectedIds || new Set()
  const outcomes = view.recovery || managedOutcomes(run)
  const safeOutcomes = retryableManagedItems(outcomes)

  const start = () => controllerRef.current?.start()
  const commit = () => {
    const picked = matches.filter(m => selected.has(m.match_id)).map(m => m.match_id)
    controllerRef.current?.commit(picked)
  }
  const cancel = () => controllerRef.current?.cancel()
  const toggle = matchId => controllerRef.current?.toggle(matchId)
  const terminal = ['complete', 'failed', 'cancelled'].includes(status)

  return <div className="w-full h-full flex flex-col gap-6">
    <div className="flex justify-between items-center bg-zinc-900/50 p-6 rounded-lg border border-white/5 shadow-2xl">
      <div><h2 className="text-2xl font-black uppercase tracking-widest text-white/90">Vault Sync</h2>
        <p className="text-white/40 text-sm mt-1">Analyze, review, and commit financial statements.</p></div>
      {(!run.run_id || terminal) && <button onClick={start} disabled={!canStartRun(run, view.ready, view.pending, isDesktopRuntime())} className="px-8 py-3 rounded font-bold uppercase tracking-wider transition-all bg-cyan-500/20 text-cyan-400 border border-cyan-500/40 disabled:opacity-40">{view.pending ? 'Starting...' : 'Start Analysis'}</button>}
      {status === 'analyzing' && <div className="px-8 py-3 rounded font-bold bg-zinc-800 text-white/50 border border-white/5">Analyzing...</div>}
      {status === 'review' && <div className="flex gap-3"><button onClick={commit} disabled={!canCommitReview(run, view.ready, view.pending, selected.size)} className="px-8 py-3 rounded font-bold bg-green-500/20 text-green-400 border border-green-500/40 disabled:opacity-40">{view.pending ? 'Committing...' : `Commit ${selected.size} Items`}</button><button onClick={cancel} disabled={!canAbandonReview(run, view.ready, view.pending)} className="px-5 py-3 rounded font-bold bg-zinc-800 text-white/70 disabled:opacity-40">Abandon Review</button></div>}
      {status === 'committing' && <div className="px-8 py-3 rounded font-bold bg-zinc-800 text-white/50">Committing...</div>}
    </div>
    {!view.ready && <div className="text-white/60 px-4" role="status">Connecting to the desktop sync service… <button onClick={() => controllerRef.current?.connect()} className="underline text-cyan-400">Reconnect</button></div>}
    {view.error && <div className="text-red-400 px-4" role="alert">Sync failed: {String(view.error.message || view.error)} <button onClick={() => controllerRef.current?.connect()} className="underline">Reconnect</button></div>}
    <section className="bg-zinc-950 rounded-lg border border-white/10 p-5 space-y-3" aria-label="Managed import recovery">
      <div className="flex items-center justify-between gap-4"><h3 className="text-lg font-black uppercase text-white/70">Import recovery</h3><div className="flex gap-2">
        <button onClick={() => controllerRef.current?.refreshOutcomes()} disabled={!view.ready || view.pending} className="px-3 py-2 rounded border border-white/10 text-white/70 disabled:opacity-40">Refresh outcomes</button>
        {safeOutcomes.length > 0 && <button onClick={() => controllerRef.current?.retryOutcomes()} disabled={!view.ready || view.pending || (run.run_id && !terminal && status !== 'idle')} className="px-3 py-2 rounded bg-cyan-500/20 border border-cyan-500/40 text-cyan-300 disabled:opacity-40">{view.pending ? 'Retrying…' : `Retry ${safeOutcomes.length} safe items`}</button>}
      </div></div>
      {view.recoveryError && <div className="text-red-300" role="alert">Unable to refresh recovery outcomes: {displayText(view.recoveryError.message) || 'Unknown recovery error'}</div>}
      {outcomes.length === 0 ? <div className="text-white/40 text-sm">No managed import outcomes recorded.</div> : <div className="space-y-2">{outcomes.map((item, index) => <div key={`${item.operation_id || item.item_id}-${index}`} className={`p-3 rounded border ${item.state === 'succeeded' ? 'border-green-500/20 bg-green-500/5 text-green-300' : item.state === 'uncertain' ? 'border-amber-500/30 bg-amber-500/5 text-amber-200' : 'border-red-500/20 bg-red-500/5 text-red-300'}`}><div className="font-medium">{displayText(item.filename) || 'Statement'} — {displayText(item.state)}</div><div className="text-xs opacity-70 mt-1">{displayText(item.message)}</div>{item.state === 'uncertain' && <div className="text-xs mt-1">Manual review required.</div>}</div>)}</div>}
    </section>
    <div className="flex-1 bg-zinc-950 rounded-lg border border-white/10 shadow-2xl overflow-hidden flex flex-col">
      {(status === 'idle' || status === 'analyzing' || status === 'failed' || status === 'cancelled') && <div className="flex-1 flex flex-col font-mono text-sm">
        <div className="bg-zinc-900 px-4 py-2 border-b border-white/10 text-white/30 text-xs uppercase">Analyzer Terminal</div>
        <div ref={scrollRef} className="flex-1 p-6 overflow-y-auto custom-scrollbar">
          {status === 'failed' && <div className="text-red-400 mb-3">Sync failed.</div>}{status === 'cancelled' && <div className="text-white/50 mb-3">Review abandoned.</div>}
          {logs.map((log, index) => <div key={index} className="text-white/70 break-all">&gt; {displayText(log.message)}</div>)}
          {errors.map((item, index) => <div key={`error-${index}`} className="text-red-400 break-all">&gt; Error: {displayText(item.message) || 'Unknown sync error'}</div>)}
          {status === 'analyzing' && <div className="text-white/50 animate-pulse mt-2">&gt; _</div>}
        </div></div>}
      {showsReview(status) && <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-8">
        {run.analysis_outcome && <div className={`${run.analysis_outcome === 'failed' ? 'text-red-400' : run.analysis_outcome === 'partial' ? 'text-amber-300' : 'text-cyan-300'} p-3 border border-white/10 rounded`}>Analysis {run.analysis_outcome}{run.analysis_outcome === 'partial' ? ` — ${matches.length} match(es), ${unmatched.length + errors.length} diagnostic(s). Review successful matches; unresolved items cannot be committed.` : ''}</div>}
        {status === 'failed' && <div className="bg-red-500/10 border border-red-500/30 text-red-400 p-4 rounded text-center font-bold uppercase">{run.analysis_outcome === 'failed' ? 'Analysis Failed' : 'Commit Failed'}</div>}
        {status === 'complete' && <div className="bg-green-500/10 border border-green-500/30 text-green-400 p-4 rounded text-center font-bold uppercase">Sync Completed Successfully</div>}

        {(status === 'review' || status === 'analyzing' || status === 'failed') && <div><h3 className="text-lg font-black uppercase text-accent mb-4 border-b border-white/10 pb-2">{status === 'analyzing' ? 'Matches so far' : status === 'failed' ? 'Retained matches' : 'Ready to Commit'} ({matches.length})</h3>
          {matches.length === 0 ? <div className="text-white/40 italic">No matches found.</div> : <div className="space-y-2">{matches.map((m) => <div key={m.match_id} className={`flex items-center justify-between p-4 rounded border ${selected.has(m.match_id) ? 'bg-zinc-900 border-accent/30' : 'bg-black border-white/5 opacity-50'}`}>
            <div className="flex items-center gap-4"><input type="checkbox" checked={selected.has(m.match_id)} onChange={() => toggle(m.match_id)} className="w-5 h-5 accent-accent" disabled={!canToggleMatch(run, view.ready, view.pending, m.match_id)}/><div><div className="font-bold text-white/90">{displayText(m.rule)} <span className="text-white/30 text-sm font-normal">({displayText(m.date)})</span></div><div className="text-xs text-white/50 font-mono mt-1">{displayText(m.filename)} -&gt; {displayText(m.new_name)}</div></div></div>
            <div className="text-right"><div className="font-mono text-accent">{typeof m.amount === 'number' && Number.isFinite(m.amount) ? `${m.amount.toFixed(2)} USD` : 'Amount unavailable'}</div><div className="text-xs text-white/50 mt-1">{displayText(m.category)} {m.is_cleared ? '(Cleared)' : '(Pending)'}</div></div>
          </div>)}</div>}</div>}
        {(unmatched.length || errors.length) > 0 && <div><h3 className="text-lg font-black uppercase text-red-400 mb-4 border-b border-white/10 pb-2">Requires Manual Attention</h3>
          {[...unmatched, ...errors].map((item, i) => <div key={i} className="p-4 mb-2 rounded border bg-red-950/20 border-red-900/30"><div className="font-bold text-red-400">{displayText(item.filename) || 'System Error'}</div><div className="text-sm text-red-400/70 mt-1">{displayText(item.message)}</div></div>)}</div>}
      </div>}
    </div>
  </div>
}
export default SyncWidget
