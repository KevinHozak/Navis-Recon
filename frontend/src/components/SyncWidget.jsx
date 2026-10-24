import { useEffect, useRef, useState } from 'react'
import { cancelSyncRun, commitLedgerSync, getSyncRun, isDesktopRuntime, listenForSyncEvents, startLedgerSync } from '../services/apiClient'
import { createSyncRunController, reviewSelection } from '../services/syncRunController'

const empty = { status: 'idle', logs: [], matches: [], unmatched: [], errors: [] }
const SyncWidget = () => {
  const [view, setView] = useState({ run: null, pending: false, ready: false })
  const [selection, setSelection] = useState({ runId: null, values: new Set() })
  const scrollRef = useRef(null)
  const controllerRef = useRef(null)
  const run = view.run || empty
  const { status = 'idle', logs = [], matches = [], unmatched = [], errors = [] } = run

  useEffect(() => {
    const api = { getSyncRun, listenForSyncEvents, startLedgerSync, commitLedgerSync, cancelSyncRun }
    const controller = createSyncRunController({ api, onChange: setView })
    controllerRef.current = controller
    controller.connect()
    return () => { controller.dispose(); if (controllerRef.current === controller) controllerRef.current = null }
  }, [])

  useEffect(() => {
    if (status === 'analyzing' && scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight
  }, [logs, status])

  const selected = reviewSelection(run, selection)

  const start = () => controllerRef.current?.start()
  const commit = () => {
    const picked = matches.filter((_, index) => selected.has(index))
    controllerRef.current?.commit(picked)
  }
  const cancel = () => controllerRef.current?.cancel()
  const toggle = index => setSelection(current => {
    const next = new Set(current.runId === run.run_id ? current.values : matches.map((_, i) => i))
    next.has(index) ? next.delete(index) : next.add(index)
    return { runId: run.run_id, values: next }
  })
  const terminal = ['complete', 'failed', 'cancelled'].includes(status)

  return <div className="w-full h-full flex flex-col gap-6">
    <div className="flex justify-between items-center bg-zinc-900/50 p-6 rounded-lg border border-white/5 shadow-2xl">
      <div><h2 className="text-2xl font-black uppercase tracking-widest text-white/90">Vault Sync</h2>
        <p className="text-white/40 text-sm mt-1">Analyze, review, and commit financial statements.</p></div>
      {(!run.run_id || terminal) && <button onClick={start} disabled={view.pending || !view.ready || !isDesktopRuntime()} className="px-8 py-3 rounded font-bold uppercase tracking-wider transition-all bg-cyan-500/20 text-cyan-400 border border-cyan-500/40 disabled:opacity-40">{view.pending ? 'Starting...' : 'Start Analysis'}</button>}
      {status === 'analyzing' && <div className="px-8 py-3 rounded font-bold bg-zinc-800 text-white/50 border border-white/5">Analyzing...</div>}
      {status === 'review' && <div className="flex gap-3"><button onClick={commit} disabled={!view.ready || view.pending || selected.size === 0} className="px-8 py-3 rounded font-bold bg-green-500/20 text-green-400 border border-green-500/40 disabled:opacity-40">{view.pending ? 'Committing...' : `Commit ${selected.size} Items`}</button><button onClick={cancel} disabled={!view.ready || view.pending} className="px-5 py-3 rounded font-bold bg-zinc-800 text-white/70 disabled:opacity-40">Abandon Review</button></div>}
      {status === 'committing' && <div className="px-8 py-3 rounded font-bold bg-zinc-800 text-white/50">Committing...</div>}
    </div>
    {!view.ready && <div className="text-white/60 px-4" role="status">Connecting to the desktop sync service… <button onClick={() => controllerRef.current?.connect()} className="underline text-cyan-400">Reconnect</button></div>}
    {view.error && <div className="text-red-400 px-4" role="alert">Sync failed: {String(view.error.message || view.error)} <button onClick={() => controllerRef.current?.connect()} className="underline">Reconnect</button></div>}
    <div className="flex-1 bg-zinc-950 rounded-lg border border-white/10 shadow-2xl overflow-hidden flex flex-col">
      {(status === 'idle' || status === 'analyzing' || status === 'failed' || status === 'cancelled') && <div className="flex-1 flex flex-col font-mono text-sm">
        <div className="bg-zinc-900 px-4 py-2 border-b border-white/10 text-white/30 text-xs uppercase">Analyzer Terminal</div>
        <div ref={scrollRef} className="flex-1 p-6 overflow-y-auto custom-scrollbar">
          {status === 'failed' && <div className="text-red-400 mb-3">Sync failed.</div>}{status === 'cancelled' && <div className="text-white/50 mb-3">Review abandoned.</div>}
          {logs.map((log, index) => <div key={index} className="text-white/70 break-all">&gt; {log.message || ''}</div>)}
          {errors.map((item, index) => <div key={`error-${index}`} className="text-red-400 break-all">&gt; Error: {item.message || 'Unknown sync error'}</div>)}
          {status === 'analyzing' && <div className="text-white/50 animate-pulse mt-2">&gt; _</div>}
        </div></div>}
      {(status === 'review' || status === 'committing' || status === 'complete') && <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-8">
        {status === 'complete' && <div className="bg-green-500/10 border border-green-500/30 text-green-400 p-4 rounded text-center font-bold uppercase">Sync Completed Successfully</div>}
        {status === 'review' && <div><h3 className="text-lg font-black uppercase text-accent mb-4 border-b border-white/10 pb-2">Ready to Commit ({matches.length})</h3>
          {matches.length === 0 ? <div className="text-white/40 italic">No matches found.</div> : <div className="space-y-2">{matches.map((m, i) => <div key={i} className={`flex items-center justify-between p-4 rounded border ${selected.has(i) ? 'bg-zinc-900 border-accent/30' : 'bg-black border-white/5 opacity-50'}`}>
            <div className="flex items-center gap-4"><input type="checkbox" checked={selected.has(i)} onChange={() => toggle(i)} className="w-5 h-5 accent-accent" disabled={!view.ready || view.pending}/><div><div className="font-bold text-white/90">{m.rule} <span className="text-white/30 text-sm font-normal">({m.date})</span></div><div className="text-xs text-white/50 font-mono mt-1">{m.filename} -&gt; {m.new_name}</div></div></div>
            <div className="text-right"><div className="font-mono text-accent">{typeof m.amount === 'number' && Number.isFinite(m.amount) ? `${m.amount.toFixed(2)} USD` : 'Amount unavailable'}</div><div className="text-xs text-white/50 mt-1">{m.category} {m.is_cleared ? '(Cleared)' : '(Pending)'}</div></div>
          </div>)}</div>}</div>}
        {(unmatched.length || errors.length) > 0 && <div><h3 className="text-lg font-black uppercase text-red-400 mb-4 border-b border-white/10 pb-2">Requires Manual Attention</h3>
          {[...unmatched, ...errors].map((item, i) => <div key={i} className="p-4 mb-2 rounded border bg-red-950/20 border-red-900/30"><div className="font-bold text-red-400">{item.filename || 'System Error'}</div><div className="text-sm text-red-400/70 mt-1">{item.message}</div></div>)}</div>}
      </div>}
    </div>
  </div>
}
export default SyncWidget
