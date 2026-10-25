import { useEffect, useState } from 'react'
import { getHouseholdAnalysis } from '../services/apiClient'
import { createAnalysisController } from '../services/analysisController'
import { formatMoney } from '../utils/money'
const localDate = d => `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`
function defaults() { const end = new Date(); return { start: localDate(new Date(end.getFullYear(), end.getMonth()-23, 1)), end: localDate(end), currency: 'USD', comparison_start: null, comparison_end: null } }
export default function HouseholdAnalysisWidget({ onHistory }) {
  const [request, setRequest] = useState(defaults)
  const [view, setView] = useState({ data: null, loading: false, error: null })
  const [controller] = useState(() => createAnalysisController({ load: getHouseholdAnalysis, onChange: setView }))
  useEffect(() => { let mounted = true; controller.activate(); Promise.resolve().then(() => { if (mounted) void controller.run(request) }); return () => { mounted = false; controller.dispose() }; /* initial external snapshot */
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [controller])
  const change = (key, value) => { controller.invalidate(); onHistory?.([], 'range unavailable'); setRequest(old => ({ ...old, [key]: value || null })) }
  async function apply() { const result = await controller.run(request); if (!result.stale) onHistory?.(result.data?.chart_history || [], result.data ? 'selected range' : 'range unavailable') }
  const data = view.data
  const money = value => value == null ? 'Unavailable' : formatMoney(value, data?.currency || request.currency)
  const sources = ids => <div className="space-y-2 mt-2">{ids.map(id => { const t = data.sources.find(t => t.id === id); return t ? <div key={id} className="text-xs border border-white/10 rounded p-2"><p>{t.date} · {t.description || 'Ledger transaction'} · {t.pending ? 'pending' : 'cleared'}{t.internal_transfer ? ' · internal transfer / card repayment' : ''}</p>{Object.entries(t.postings).map(([a,v]) => <p key={a} className="font-mono">{a}: {money(v)}</p>)}</div> : <p key={id}>Source unavailable; refresh analysis.</p> })}</div>
  return <section className="space-y-4 rounded-xl border border-cyan-400/30 bg-zinc-950 p-5">
    <h3 className="text-xl font-bold text-cyan-300">History and cash-flow explanation</h3>
    <form onSubmit={e => { e.preventDefault(); void apply() }} className="flex flex-wrap items-end gap-3">
      {[['start','Start'],['end','End'],['comparison_start','Comparison start (optional)'],['comparison_end','Comparison end (optional)']].map(([key,label]) => <label className="text-xs" key={key}>{label}<input aria-label={label} type="date" required={key === 'start' || key === 'end'} value={request[key] || ''} onChange={e => change(key,e.target.value)} className="block rounded border border-white/20 bg-black p-2" /></label>)}
      <label className="text-xs">Currency / unit<input aria-label="Analysis currency" value={request.currency || ''} onChange={e => change('currency',e.target.value)} className="block w-24 rounded border border-white/20 bg-black p-2" /></label>
      <button type="button" onClick={() => { controller.invalidate(); onHistory?.(null, 'latest 24 ledger months') }} className="text-xs underline text-cyan-300">Latest 24 ledger months</button>
      <button disabled={view.loading} type="submit" className="rounded border border-cyan-400/30 bg-cyan-500/10 px-4 py-2 text-cyan-300 disabled:opacity-40">{view.loading ? 'Calculating…' : 'Apply range'}</button>
    </form>
    {view.error && <p role="alert" className="text-red-300">Analysis unavailable: {view.error}</p>}
    {view.loading && <p role="status">Reading validated ledger…</p>}
    {data && <><p className="text-xs text-white/50">Latest unit evidence: {data.latest_transaction_date || 'none'}. Exact native units; no price valuation or conversion. Carried months mean no new postings, not proof of complete statements. Partial boundary months use the selected dates.</p>
      <div className="overflow-auto"><table className="w-full text-xs text-left"><thead><tr>{['Period / coverage','Opening NW','Closing NW','Movement','Income','Net spending','Equity / adjustments','Unexplained'].map(h => <th key={h} className="p-2">{h}</th>)}</tr></thead><tbody>{data.months.map(m => <tr key={m.month} className="border-t border-white/10"><td className="p-2"><details><summary>{m.month} · {m.evidence_status.replaceAll('_',' ')}</summary><p>{m.start} – {m.end}</p>{sources(m.source_ids)}</details></td>{[m.opening_net_worth,m.closing_net_worth,m.net_worth_change,m.income,m.spending,m.equity_and_adjustments,m.unexplained].map((v,i) => <td key={i} className="p-2 font-mono">{m.evidence_status === 'no_currency_evidence' ? 'Unavailable' : money(v)}</td>)}</tr>)}</tbody></table></div>
      <p className="text-xs text-white/50">Movement = income − net expenses + equity/adjustment postings + unexplained remainder. Refunds and corrections retain their signed category amounts. Asset transfers and card repayments are excluded from spending. Valuation changes are not inferred; unsupported price/cost syntax withholds analysis.</p>
      <h4 className="font-semibold">Category comparison and source transactions</h4>
      <div className="overflow-auto"><table className="w-full text-xs text-left"><thead><tr><th>Category</th><th>Selected period</th><th>Comparison period</th><th>Difference</th></tr></thead><tbody>{data.categories.map(c => <tr key={c.account} className="border-t border-white/10"><td className="py-2"><details><summary>{c.account}</summary><p>Selected period</p>{sources(c.source_ids)}<p>Comparison period</p>{sources(c.comparison_source_ids)}</details></td><td>{money(c.current)}</td><td>{data.comparison_start ? money(c.previous) : 'Not selected'}</td><td>{data.comparison_start ? money(c.difference) : 'Not selected'}</td></tr>)}</tbody></table></div><p className="text-xs text-white/35 break-all">Ledger revision: {data.ledger_revision}</p>
    </>}
  </section>
}
