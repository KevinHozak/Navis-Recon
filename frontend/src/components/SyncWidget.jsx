import { useEffect, useRef, useState } from 'react'
import { cancelSyncRun, commitLedgerSync, getSyncRun, getSyncReceipts, getSyncOutcomes, reconcileLedgerSync, retryLedgerSync, revealSyncSource, isDesktopRuntime, listenForSyncEvents, startLedgerSync } from '../services/apiClient'
import { createSyncRunController } from '../services/syncRunController'
import { canAbandonReview, canCommitReview, canStartRun, canToggleMatch, managedOutcomes, retryableManagedItems, filterRecoveryOutcomes, showsReview } from '../services/syncRunView'

const empty = { status: 'idle', logs: [], matches: [], unmatched: [], errors: [] }
const displayText = value => ['string', 'number', 'boolean'].includes(typeof value) ? String(value) : ''
const SyncWidget = ({ initialFilter = {} }) => {
  const [view, setView] = useState({ run: null, pending: false, ready: false })
  const scrollRef = useRef(null)
  const controllerRef = useRef(null)
  const run = view.run || empty
  const { status = 'idle', logs = [], matches = [], unmatched = [], errors = [] } = run

  useEffect(() => {
    const api = { getSyncRun, getSyncReceipts, getSyncOutcomes, reconcileLedgerSync, retryLedgerSync, revealSyncSource, listenForSyncEvents, startLedgerSync, commitLedgerSync, cancelSyncRun }
    const controller = createSyncRunController({ api, onChange: setView })
    controllerRef.current = controller
    controller.connect()
    return () => { controller.dispose(); if (controllerRef.current === controller) controllerRef.current = null }
  }, [])

  useEffect(() => {
    if (status === 'analyzing' && scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight
  }, [logs, status])

  const selected = view.selectedIds || new Set()
  const funding = view.fundingAccounts || new Map()
  const duplicateDecisions = view.duplicateDecisions || new Map()
  const fundingComplete = [...selected].every(id => (run.funding_accounts || []).includes(funding.get(id)) && (!matches.find(m=>m.match_id===id)?.possible_duplicates?.length || duplicateDecisions.get(id)==='import_separately'))
  const outcomes = view.recovery || managedOutcomes(run)
  const [recoveryFilter, setRecoveryFilter] = useState(initialFilter.operationId || (initialFilter.recoveryOnly ? 'attention' : 'all'));
  const filteredOutcomes = filterRecoveryOutcomes(outcomes, recoveryFilter);
  const safeOutcomes = retryableManagedItems(filteredOutcomes)

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
      {status === 'analyzing' && <div className="flex gap-3 items-center"><div className="px-8 py-3 rounded font-bold bg-zinc-800 text-white/50 border border-white/5">{run.cancelling ? 'Cancelling...' : 'Analyzing...'}</div><button onClick={cancel} disabled={!view.ready || view.pending || run.cancelling} aria-label="Cancel analysis" className="px-5 py-3 rounded font-bold bg-red-500/10 text-red-300 border border-red-500/30 disabled:opacity-40">{run.cancelling ? 'Cancellation pending' : 'Cancel'}</button></div>}
      {status === 'review' && <div className="flex gap-3"><button onClick={commit} disabled={!canCommitReview(run, view.ready, view.pending, selected.size, fundingComplete)} className="px-8 py-3 rounded font-bold bg-green-500/20 text-green-400 border border-green-500/40 disabled:opacity-40">{view.pending ? 'Committing...' : `Commit ${selected.size} Items`}</button><button onClick={cancel} disabled={!canAbandonReview(run, view.ready, view.pending)} className="px-5 py-3 rounded font-bold bg-zinc-800 text-white/70 disabled:opacity-40">Abandon Review</button></div>}
      {status === 'committing' && <div className="flex gap-3 items-center"><div className="px-8 py-3 rounded font-bold bg-zinc-800 text-white/50">{run.cancelling ? 'Finishing current item...' : 'Committing...'}</div><button onClick={cancel} disabled={!view.ready || view.pending || run.cancelling} aria-label="Cancel import" className="px-5 py-3 rounded font-bold bg-red-500/10 text-red-300 border border-red-500/30 disabled:opacity-40">{run.cancelling ? 'Cancellation pending' : 'Cancel'}</button></div>}
    </div>
    {!view.ready && <div className="text-white/60 px-4" role="status">Connecting to the desktop sync serviceâ€¦ <button onClick={() => controllerRef.current?.connect()} className="underline text-cyan-400">Reconnect</button></div>}
    {view.error && <div className="text-red-400 px-4" role="alert">Sync failed: {String(view.error.message || view.error)} <button onClick={() => controllerRef.current?.connect()} className="underline">Reconnect</button></div>}
    <section className="bg-zinc-950 rounded-lg border border-white/10 p-5 space-y-3" aria-label="Recent sync attempts">
      <div className="flex justify-between items-center"><h3 className="text-lg font-black uppercase text-white/70">Recent attempts</h3><button onClick={() => controllerRef.current?.refreshReceipts()} disabled={!view.ready} className="px-3 py-2 rounded border border-white/10 text-white/70 disabled:opacity-40">Refresh</button></div>
      {view.receiptError && <div role="alert" className="text-red-300">Receipt history unavailable: {displayText(view.receiptError.message || view.receiptError)}</div>}
      {(view.receipts || []).slice(0, 8).map(receipt => <div key={receipt.receipt_id} className="text-sm text-white/60 border-t border-white/5 pt-2"><span className="text-white/80">{displayText(receipt.terminal_state)}</span> · {displayText(receipt.operation_kind)} · {receipt.counts?.matched || 0} matched, {receipt.counts?.succeeded || 0} succeeded, {receipt.counts?.failed || 0} failed · {Math.round((receipt.duration_ms || 0)/1000)}s</div>)}
      {!view.receiptError && !(view.receipts || []).length && <div className="text-white/40 text-sm">No sync attempts recorded.</div>}
    </section>
    <section className="bg-zinc-950 rounded-lg border border-white/10 p-5 space-y-3" aria-label="Managed import recovery">
      <div className="flex items-center justify-between gap-4"><h3 className="text-lg font-black uppercase text-white/70">Import recovery</h3><div className="flex gap-2">
        <button onClick={() => controllerRef.current?.refreshOutcomes()} disabled={!view.ready || view.pending} className="px-3 py-2 rounded border border-white/10 text-white/70 disabled:opacity-40">Refresh outcomes</button>
        {safeOutcomes.length > 0 && <button onClick={() => controllerRef.current?.retryOutcomes()} disabled={!view.ready || view.pending || (run.run_id && !terminal && status !== 'idle')} className="px-3 py-2 rounded bg-cyan-500/20 border border-cyan-500/40 text-cyan-300 disabled:opacity-40">{view.pending ? 'Retryingâ€¦' : `Retry ${safeOutcomes.length} safe items`}</button>}
      </div></div>
      {view.recoveryError && <div className="text-red-300" role="alert">Unable to refresh recovery outcomes: {displayText(view.recoveryError.message) || 'Unknown recovery error'}</div>}
      <button type="button" onClick={() => setRecoveryFilter(recoveryFilter === 'all' ? 'attention' : 'all')} className="text-xs text-accent underline">{recoveryFilter === 'all' ? 'Show failed or uncertain only' : 'Show all outcomes'}</button>
      {filteredOutcomes.length === 0 ? <div className="text-white/40 text-sm">No managed import outcomes in the current view.</div> : <div className="space-y-2">{filteredOutcomes.map((item, index) => <div key={`${item.operation_id || item.item_id}-${index}`} className={`p-3 rounded border ${item.state === 'succeeded' ? 'border-green-500/20 bg-green-500/5 text-green-300' : item.state === 'uncertain' ? 'border-amber-500/30 bg-amber-500/5 text-amber-200' : 'border-red-500/20 bg-red-500/5 text-red-300'}`}><div className="font-medium">{displayText(item.filename) || 'Statement'} â€” {displayText(item.state)}</div><div className="text-xs opacity-70 mt-1">{displayText(item.message)}</div>{item.state === 'uncertain' && <div className="text-xs mt-1">Manual review required.</div>}{['failed','uncertain'].includes(item.state) && item.operation_id && <button type="button" disabled={!view.ready || view.pending || run.cancelling} onClick={()=>controllerRef.current?.revealSource({sourceRef:`src:${item.operation_id}`,operationId:item.operation_id})} className="mt-2 text-xs underline disabled:opacity-40">Show file</button>}</div>)}</div>}
    </section>
    <div className="flex-1 bg-zinc-950 rounded-lg border border-white/10 shadow-2xl overflow-hidden flex flex-col">
      {(status === 'idle' || status === 'analyzing' || status === 'failed' || status === 'cancelled') && <div className="flex-1 flex flex-col font-mono text-sm">
        <div className="bg-zinc-900 px-4 py-2 border-b border-white/10 text-white/30 text-xs uppercase">Analyzer Terminal</div>
        <div ref={scrollRef} className="flex-1 p-6 overflow-y-auto custom-scrollbar">
          {status === 'failed' && <div className="text-red-400 mb-3">Sync failed.</div>}{status === 'cancelled' && <div className="text-white/50 mb-3">Sync cancelled.</div>}
          {logs.map((log, index) => <div key={index} className="text-white/70 break-all">&gt; {displayText(log.message)}</div>)}
          {errors.map((item, index) => <div key={`error-${index}`} className="text-red-400 break-all">&gt; Error: {displayText(item.message) || 'Unknown sync error'}</div>)}
          {status === 'analyzing' && <div className="text-white/50 animate-pulse mt-2">&gt; _</div>}
        </div></div>}
      {showsReview(status) && <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-8">
        {status === 'review' && <p className="text-sm text-amber-200" role="status">Choose funding and resolve any possible duplicates for each selected receipt. Unassigned or unresolved receipts cannot be committed; deselect them to approve other receipts.</p>}
        {run.analysis_outcome && <div className={`${run.analysis_outcome === 'failed' ? 'text-red-400' : run.analysis_outcome === 'partial' ? 'text-amber-300' : 'text-cyan-300'} p-3 border border-white/10 rounded`}>Analysis {run.analysis_outcome}{run.analysis_outcome === 'partial' ? ` â€” ${matches.length} match(es), ${unmatched.length + errors.length} diagnostic(s). Review successful matches; unresolved items cannot be committed.` : ''}</div>}
        {status === 'failed' && <div className="bg-red-500/10 border border-red-500/30 text-red-400 p-4 rounded text-center font-bold uppercase">{run.analysis_outcome === 'failed' ? 'Analysis Failed' : 'Commit Failed'}</div>}
        {status === 'complete' && <div className="bg-green-500/10 border border-green-500/30 text-green-400 p-4 rounded text-center font-bold uppercase">Sync Completed Successfully</div>}

        {(status === 'review' || status === 'analyzing' || status === 'failed') && <div><h3 className="text-lg font-black uppercase text-accent mb-4 border-b border-white/10 pb-2">{status === 'analyzing' ? 'Matches so far' : status === 'failed' ? 'Retained matches' : 'Ready to Commit'} ({matches.length})</h3>
          {matches.length === 0 ? <div className="text-white/40 italic">No matches found.</div> : <div className="space-y-2">{matches.map((m) => <div key={m.match_id} className={`flex flex-wrap items-center gap-4 justify-between p-4 rounded border ${selected.has(m.match_id) ? 'bg-zinc-900 border-accent/30' : 'bg-black border-white/5 opacity-50'}`}>
            <div className="flex min-w-0 items-center gap-4"><input type="checkbox" checked={selected.has(m.match_id)} onChange={() => toggle(m.match_id)} className="w-5 h-5 shrink-0 accent-accent" disabled={!canToggleMatch(run, view.ready, view.pending, m.match_id)}/><div className="min-w-0 break-words"><div className="font-bold text-white/90">{displayText(m.rule)} <span className="text-white/30 text-sm font-normal">({displayText(m.date)})</span></div><div className="text-xs text-white/50 font-mono mt-1 break-all">{displayText(m.filename)} -&gt; {displayText(m.new_name)}</div></div></div>
            <div className="text-right min-w-64 max-w-full shrink-0"><div className="font-mono text-accent">{displayText(m.amount) ? `${displayText(m.amount)} ${displayText(run.currency)}` : 'Amount unavailable'}</div><div className="text-xs text-white/50 mt-1">{displayText(m.category)} {m.is_cleared ? '(Cleared)' : '(Pending)'}</div>
              <label className="block text-xs text-white/70 mt-3">Funding account
                <select aria-label={`Funding account for ${m.filename}`} value={funding.get(m.match_id) || ''} onChange={event => controllerRef.current?.assignFunding(m.match_id, event.target.value)} disabled={status !== 'review' || !canToggleMatch(run, view.ready, view.pending, m.match_id)} className="block w-full mt-1 rounded border border-white/20 bg-zinc-950 text-white p-2 disabled:opacity-40">
                  <option value="">Unassigned — select an account</option>
                  {(run.funding_accounts || []).map(account => <option key={account} value={account}>{account}</option>)}
                </select>
              </label>
              {!funding.has(m.match_id) && <div className="text-xs text-amber-300 mt-1">Unassigned — approval blocked</div>}
              {m.source_ref && <button type="button" disabled={!view.ready || view.pending || run.cancelling} onClick={()=>controllerRef.current?.revealSource({sourceRef:m.source_ref,runId:run.run_id,itemId:m.match_id})} className="mt-2 text-xs underline disabled:opacity-40">Show file</button>}</div>
            {m.possible_duplicates?.length > 0 ? <section aria-label={`Possible duplicates for ${m.filename}`} className="w-full rounded border border-amber-500/30 bg-amber-500/5 p-3 text-sm text-amber-100">
              <h4 className="font-semibold">Possible ledger matches ({m.possible_duplicates.length})</h4>
              <p className="text-xs opacity-80 mt-1">Same signed amount and currency within seven days. These may be different payments; compare dates, merchant text and funding.</p>
              <ul className="max-h-64 overflow-y-auto space-y-2 mt-2">{m.possible_duplicates.map(item=><li key={item.id} className="border-t border-white/10 pt-2 break-words">
                <div>{displayText(item.date)} · {displayText(item.amount)} {displayText(item.currency)} · {displayText(item.payee) || displayText(item.narration) || 'Unnamed ledger payment'}</div>
                {item.payee && item.narration && <div className="text-xs opacity-80">{displayText(item.narration)}</div>}
                <div className="text-xs opacity-80">{item.funding_accounts.map(displayText).join(', ') || 'Funding not identified'} · {item.merchant_overlap ? 'Merchant text overlaps' : 'Merchant text differs'}{funding.has(m.match_id) ? item.funding_accounts.includes(funding.get(m.match_id)) ? ' · Selected funding matches' : ' · Selected funding differs' : ''}</div>
              </li>)}</ul>
              <label className="block mt-3 text-xs">Duplicate decision
                <select aria-label={`Duplicate decision for ${m.filename}`} value={duplicateDecisions.get(m.match_id) || ''} onChange={event=>controllerRef.current?.decideDuplicate(m.match_id,event.target.value)} disabled={status!=='review' || !canToggleMatch(run,view.ready,view.pending,m.match_id)} className="block w-full mt-1 p-2 rounded border border-amber-500/30 bg-zinc-950 text-white disabled:opacity-40">
                  <option value="">Choose a decision — approval blocked</option>
                  <option value="already_recorded">Already recorded — skip this receipt</option>
                  <option value="import_separately">Separate payment — import anyway</option>
                </select>
              </label>
              {duplicateDecisions.get(m.match_id)==='already_recorded' && <p className="text-xs mt-2">Skipped for this review. No posting or archive will be created.</p>}
            </section> : <p className="w-full text-xs text-white/40">{Array.isArray(m.possible_duplicates) ? 'No possible ledger matches within seven days. This does not rule out duplicates.' : 'Duplicate check unavailable; resolve analysis errors before approval.'}</p>}
          </div>)}</div>}</div>}
        {(unmatched.length || errors.length) > 0 && <div><h3 className="text-lg font-black uppercase text-red-400 mb-4 border-b border-white/10 pb-2">Requires Manual Attention</h3>
          {[...unmatched, ...errors].map((item, i) => <div key={i} className="p-4 mb-2 rounded border bg-red-950/20 border-red-900/30"><div className="font-bold text-red-400">{displayText(item.filename) || 'System Error'}</div><div className="text-sm text-red-400/70 mt-1">{displayText(item.message)}</div>{item.source_ref && <button type="button" disabled={!view.ready || view.pending || run.cancelling} onClick={()=>controllerRef.current?.revealSource({sourceRef:item.source_ref,runId:run.run_id})} className="mt-2 text-xs underline disabled:opacity-40">Show file</button>}</div>)}</div>}
      </div>}
    </div>
  </div>
}
export default SyncWidget
