import { useEffect, useRef, useState } from 'react'
import { getCashForecast, saveForecastSettings } from '../services/apiClient'
import { createAnalysisController } from '../services/analysisController'
import { compareMoney, formatMoney } from '../utils/money'
const today = () => { const d = new Date(); return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}` }
const newBill = currency => ({ payee: '', account: '', currency, confirmed: false, cancelled: false, cash_account: '', amount: null, due_date: null, cadence: null })
export default function CashForecastWidget() {
  const [request, setRequest] = useState(() => ({ as_of: today(), horizon_days: 30, currency: 'USD', liquid_accounts: [], include_estimates: true }))
  const [view, setView] = useState({ data: null, loading: false, error: null })
  const [controller] = useState(() => createAnalysisController({ load: getCashForecast, onChange: setView }))
  const [bill, setBill] = useState(() => newBill('USD'))
  const [saving, setSaving] = useState(false)
  const [message, setMessage] = useState('')
  const guard = useRef(false)
  const lifetime = useRef({ active: false })
  useEffect(() => { const state = lifetime.current; let mounted = true; state.active = true; controller.activate(); Promise.resolve().then(() => { if (mounted) void controller.run(request) }); return () => { mounted = false; state.active = false; controller.dispose() };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [controller])
  const data = view.data
  const cash = value => value == null || data?.opening_cash == null ? 'Unavailable' : formatMoney(value, data.currency)
  const change = (key, value) => { controller.invalidate(); setMessage(''); setRequest(old => ({ ...old, [key]: value, ...(key === 'currency' ? { liquid_accounts: [] } : {}) })) }
  function edit(ob) {
    const m = ob.metadata || ob
    setBill({ payee: ob.payee, account: ob.account, currency: ob.currency, confirmed: Boolean(m.forecast_confirmed), cancelled: Boolean(m.forecast_cancelled), cash_account: m.forecast_cash_account || '', amount: m.forecast_amount || ob.latest_amount || null, due_date: m.forecast_due_date || ob.next_projected_date || null, cadence: m.forecast_cadence || null })
    setMessage('Review the amount, due date and funding account before confirming.')
  }
  async function save(e) {
    e.preventDefault(); if (guard.current || !data) return
    guard.current = true; setSaving(true); setMessage('')
    try { await saveForecastSettings(bill, data.registry_revision); if (!lifetime.current.active) return; setMessage('Bill settings saved; recalculating.'); await controller.run(request) }
    catch (e) { if (lifetime.current.active) setMessage(`Settings not saved: ${e.message || e}. Refresh forecast before retrying a conflict.`) }
    finally { guard.current = false; if (lifetime.current.active) setSaving(false) }
  }
  return <section className="rounded-xl border border-cyan-400/30 bg-zinc-950 p-5 space-y-4">
    <h3 className="text-xl font-bold text-cyan-300">Cash availability and upcoming bills</h3>
    <fieldset disabled={saving} className="flex flex-wrap gap-3 items-end">
      <label className="text-xs">As of (end of day)<input aria-label="Forecast as of" type="date" className="block bg-black border border-white/20 rounded p-2" value={request.as_of} onChange={e => change('as_of',e.target.value)} /></label>
      <label className="text-xs">Horizon days<input aria-label="Forecast horizon" type="number" min="1" max="366" className="block bg-black border border-white/20 rounded p-2 w-24" value={request.horizon_days} onChange={e => change('horizon_days',Number(e.target.value))} /></label>
      <label className="text-xs">Currency / unit<input aria-label="Forecast currency" className="block bg-black border border-white/20 rounded p-2 w-24" value={request.currency} onChange={e => change('currency',e.target.value)} /></label>
      <label className="text-xs"><input type="checkbox" checked={request.include_estimates} onChange={e => change('include_estimates',e.target.checked)} /> Include detected estimates in scenario</label>
      <button type="button" disabled={view.loading} onClick={() => controller.run(request)} className="px-4 py-2 rounded bg-cyan-500/10 border border-cyan-400/30 text-cyan-300 disabled:opacity-40">{view.loading ? 'Calculating…' : 'Refresh forecast'}</button>
    </fieldset>
    {view.loading && <p role="status">Reading validated cash and obligations…</p>}{view.error && <p role="alert" className="text-red-300">Forecast unavailable: {view.error}</p>}
    {message && <p role="status" className="text-cyan-200 text-sm">{message}</p>}
    {data && <>
      <fieldset disabled={saving} className="space-y-2"><legend className="text-sm font-semibold">Choose liquid accounts explicitly</legend><div className="flex flex-wrap gap-3">{data.liquid_candidates.map(a => <label key={a} className="text-xs"><input type="checkbox" checked={request.liquid_accounts.includes(a)} onChange={e => { const accounts = e.target.checked ? [...request.liquid_accounts,a] : request.liquid_accounts.filter(v => v !== a); setRequest(old => ({ ...old, liquid_accounts: accounts })); void controller.run({ ...request, liquid_accounts: accounts }) }} /> {a}</label>)}</div>{!data.liquid_candidates.length && <p className="text-sm text-amber-300">No bank/cash accounts found in this unit.</p>}</fieldset>
      <p className="font-mono">Cash at {data.as_of}: {cash(data.opening_cash)} · through {data.end}</p>
      <p className="text-xs text-white/50">Only selected bank/cash accounts are spendable. Opening includes pending postings. Confirmed balance includes user-confirmed bills and all dated ledger movements; the estimate scenario optionally adds detector projections. No future income or investment sales are invented. Card-funded bills are excluded until explicitly mapped; recorded card repayments remain ledger cash movements. Projections within 3 days of a matching recorded payment are suppressed. Cancelled subscriptions do not erase recorded charges. Balances are end-of-day totals; intraday posting order is unknown. This is a scenario, not a bank-availability guarantee.</p>
      {data.warnings.map((w,i) => <p key={i} className="text-amber-300 text-xs">{w}</p>)}
      <div className="overflow-auto"><table className="w-full text-xs text-left"><thead><tr>{['Date / bill','Basis and evidence','Movement','Confirmed cash','With estimates'].map(h => <th key={h} className="p-2">{h}</th>)}</tr></thead><tbody>{data.events.map((event,i) => <tr key={`${event.key}:${event.date}:${i}`} className="border-t border-white/10"><td className="p-2">{event.date}<br />{event.payee}</td><td className="p-2"><strong>{event.kind.replaceAll('_',' ')}</strong> · {event.confidence.replaceAll('_',' ')}{event.stale ? ' · STALE / overdue' : ''}<br />Last observation: {event.last_observation || 'none'}{event.trial_end && <p className="text-amber-300">Trial ends {event.trial_end}; verify renewal terms</p>}{event.price_change && <p className="text-amber-300">Price change: {formatMoney(event.price_change.dollar_delta, data.currency)} · check latest charge</p>}<details><summary>Assumptions</summary>{event.note}</details></td><td className="p-2 font-mono">{formatMoney(event.amount,data.currency)}</td>{[event.confirmed_balance,event.estimated_balance].map((v,j) => <td key={j} className={`p-2 font-mono ${data.opening_cash != null && compareMoney(v,'0') < 0 ? 'text-red-300' : ''}`}>{cash(v)}</td>)}</tr>)}</tbody></table></div>
      <details className="border border-white/10 rounded p-3"><summary>Detected patterns and bill settings</summary><p className="text-xs text-white/50 mt-2">No pattern is a promise of a future charge. Choose an entry to confirm, map funding or cancel it. One-off or irregular bills can be entered explicitly.</p><div className="space-y-2 mt-3">{data.detected_obligations.map(ob => <div className="text-xs flex justify-between gap-3" key={ob.key}><span>{ob.payee} · {ob.account} · {ob.total_count} observations · last {ob.last_date} · {ob.cadence} · latest {formatMoney(ob.latest_amount,ob.currency)}{ob.metadata?.is_trial && ob.metadata?.trial_end_date ? ` · trial ends ${ob.metadata.trial_end_date}` : ''}</span><button disabled={saving} type="button" className="underline" onClick={() => edit(ob)}>Edit forecast settings</button></div>)}{data.settings.map(m => <button disabled={saving} type="button" key={m.key} className="block underline text-xs" onClick={() => edit(m)}>{m.payee} · {m.account} · {m.forecast_cancelled ? 'cancelled' : m.forecast_confirmed ? 'confirmed' : 'unconfirmed'}</button>)}</div></details>
      <form onSubmit={save} className="space-y-3 border-t border-white/10 pt-4"><div className="flex gap-3"><h4 className="font-semibold">Confirm / manage a bill</h4><button disabled={saving} type="button" onClick={() => setBill(newBill(request.currency))} className="text-xs underline">New bill</button></div><fieldset disabled={saving} className="grid sm:grid-cols-3 gap-3">{[['payee','Payee'],['account','Expense account'],['cash_account','Funding bank/cash account'],['currency','Bill currency'],['amount','Positive amount'],['due_date','Due date']].map(([key,label]) => <label className="text-xs" key={key}>{label}<input aria-label={label} required={bill.confirmed || key === 'payee' || key === 'account'} type={key === 'due_date' ? 'date' : 'text'} className="block w-full bg-black border border-white/20 rounded p-2" value={bill[key] ?? ''} onChange={e => setBill(old => ({ ...old, [key]: ['amount','due_date'].includes(key) ? e.target.value || null : e.target.value }))} /></label>)}<label className="text-xs">Cadence<select className="block bg-black border border-white/20 rounded p-2" value={bill.cadence || ''} onChange={e => setBill(old => ({ ...old, cadence: e.target.value || null }))}><option value="">Use detected pattern / one-off without pattern</option>{['weekly','biweekly','monthly','quarterly','semi_annual','annual'].map(c => <option key={c}>{c}</option>)}</select></label><label className="text-xs"><input type="checkbox" checked={bill.confirmed} onChange={e => setBill(old => ({ ...old, confirmed: e.target.checked }))} /> Amount and due date confirmed</label><label className="text-xs"><input type="checkbox" checked={bill.cancelled} onChange={e => setBill(old => ({ ...old, cancelled: e.target.checked }))} /> Cancelled; no future projection</label></fieldset><button disabled={saving || view.loading} type="submit" className="px-4 py-2 rounded border border-cyan-400/30 text-cyan-300 disabled:opacity-40">{saving ? 'Saving…' : 'Save bill settings'}</button></form>
      <p className="text-xs text-white/35 break-all">Ledger revision: {data.ledger_revision}</p>
    </>}
  </section>
}
