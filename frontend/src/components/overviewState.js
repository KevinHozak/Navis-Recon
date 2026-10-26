import { addMoney, compareMoney, sumMoney } from '../utils/money.js'

const validDate = value => {
  if (value === '') return true
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false
  const date = new Date(`${value}T00:00:00.000Z`)
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value
}
const nonnegativeInteger = value => Number.isSafeInteger(value) && value >= 0
const validTimestamp = value => nonnegativeInteger(value) && value <= 8_640_000_000_000_000

export function validateLedgerSummary(summary) {
  try {
    if (!summary || summary.status !== 'validated' || summary.validation_status !== 'validated') throw new Error('The ledger did not return a validated snapshot.')
    if (!/^[a-f\d]{64}$/i.test(summary.ledger_revision) || !validTimestamp(summary.read_at_unix_ms) || !validDate(summary.latest_transaction_date)) throw new Error('The snapshot provenance is incomplete or malformed.')
    if (summary.valuation_basis !== 'native_units_no_conversion' || !summary.selected_currency) throw new Error('The selected-unit valuation basis is unsupported.')
    for (const field of ['transaction_count', 'posting_count', 'account_count', 'selected_included_account_count', 'selected_excluded_account_count']) {
      if (!nonnegativeInteger(summary[field])) throw new Error(`The snapshot count ${field} is missing or invalid.`)
    }
    for (const field of ['total_assets', 'total_liabilities', 'net_worth']) compareMoney(summary[field], '0')
    if (!Array.isArray(summary.coverage)) throw new Error('Account coverage is missing.')

    const selected = summary.selected_currency
    const accounts = new Set()
    const accountUnits = new Set()
    const included = []
    const assets = []
    const liabilities = []
    let selectedIncluded = 0
    let selectedExcluded = 0
    const allowedReasons = new Set(['included_selected_unit_asset', 'included_selected_unit_liability', 'unvalued_unit', 'outside_net_worth'])
    for (const row of summary.coverage) {
      if (!row || typeof row.account !== 'string' || typeof row.currency !== 'string' || typeof row.included_in_net_worth !== 'boolean' || !allowedReasons.has(row.reason)) throw new Error('An account coverage row is malformed.')
      compareMoney(row.balance, '0')
      accounts.add(row.account)
      const accountUnit = JSON.stringify([row.account, row.currency])
      if (accountUnits.has(accountUnit)) throw new Error(`Coverage contains a duplicate account/unit row for ${row.account} in ${row.currency}.`)
      accountUnits.add(accountUnit)
      const selectedUnit = row.currency === selected
      const includedReason = row.reason === 'included_selected_unit_asset' || row.reason === 'included_selected_unit_liability'
      if (row.reason === 'included_selected_unit_asset' && !row.account.startsWith('Assets:')) throw new Error(`An included asset reason does not match ${row.account}.`)
      if (row.reason === 'included_selected_unit_liability' && !row.account.startsWith('Liabilities:')) throw new Error(`An included liability reason does not match ${row.account}.`)
      if (selectedUnit && row.reason === 'outside_net_worth' && /^(Assets|Liabilities):/.test(row.account)) throw new Error(`A selected-unit asset or liability cannot be outside net worth: ${row.account}.`)
      if (row.included_in_net_worth !== (selectedUnit && includedReason)) throw new Error(`Coverage inclusion does not match its reason for ${row.account}.`)
      if (!selectedUnit && row.reason !== 'unvalued_unit') throw new Error(`An unvalued unit has an invalid reason for ${row.account}.`)
      if (selectedUnit && !includedReason && row.reason !== 'outside_net_worth') throw new Error(`An excluded selected-unit row has an invalid reason for ${row.account}.`)
      if (selectedUnit) {
        if (row.included_in_net_worth) {
          selectedIncluded += 1
          included.push(row.balance)
          if (row.reason === 'included_selected_unit_asset') assets.push(row.balance)
          else liabilities.push(row.balance)
        } else selectedExcluded += 1
      }
    }
    if (accounts.size !== summary.account_count || selectedIncluded !== summary.selected_included_account_count || selectedExcluded !== summary.selected_excluded_account_count) throw new Error('Coverage row counts do not match the snapshot counts.')
    const coveredAssets = sumMoney(assets)
    const coveredLiabilities = sumMoney(liabilities)
    const coveredNetWorth = addMoney(coveredAssets, coveredLiabilities)
    if (compareMoney(coveredAssets, summary.total_assets) !== 0 || compareMoney(coveredLiabilities, summary.total_liabilities) !== 0 || compareMoney(coveredNetWorth, summary.net_worth) !== 0 || compareMoney(sumMoney(included), summary.net_worth) !== 0) {
      throw new Error('Account coverage does not reconcile exactly to the headline totals.')
    }
    return { valid: true, coveredAssets, coveredLiabilities, coveredNetWorth }
  } catch (error) {
    return { valid: false, error: error?.message || 'The ledger summary is malformed.' }
  }
}

export function createOverviewController({ loadSummary, checkSource, onChange }) {
  let generation = 0
  let disposed = false
  let checkingGeneration = null
  let state = { summary: null, loading: false, error: null, source: 'unknown', sourceError: null, sourceChecking: false, sourceStale: false }
  const publish = patch => {
    if (disposed) return
    state = { ...state, ...patch }
    onChange(state)
  }
  const snapshot = () => state

  async function refresh() {
    if (disposed) return
    const thisGeneration = ++generation
    checkingGeneration = null
    publish({ loading: true, error: null, source: 'unknown', sourceError: null, sourceChecking: false, sourceStale: Boolean(state.summary) })
    try {
      const summary = await loadSummary()
      if (disposed || thisGeneration !== generation) return
      const validation = validateLedgerSummary(summary)
      if (!validation.valid) throw new Error(`Ledger summary could not be validated: ${validation.error}`)
      publish({ summary, loading: false, source: 'unknown', sourceError: null, sourceChecking: true, sourceStale: false })
      void check(thisGeneration, summary.ledger_revision)
    } catch (error) {
      if (disposed || thisGeneration !== generation) return
      publish({ summary: null, loading: false, error: error?.message || String(error), source: 'unknown', sourceError: null, sourceChecking: false, sourceStale: false })
    }
  }

  async function check(expectedGeneration = generation, revision = state.summary?.ledger_revision) {
    if (disposed || !state.summary || !revision || expectedGeneration !== generation || checkingGeneration === expectedGeneration) return
    checkingGeneration = expectedGeneration
    publish({ sourceChecking: true })
    try {
      const result = await checkSource(revision)
      if (disposed || expectedGeneration !== generation || state.summary?.ledger_revision !== revision) return
      const malformed = result?.expected_revision !== revision
        || !['current', 'changed', 'unavailable'].includes(result?.state)
        || !validTimestamp(result?.checked_at_unix_ms)
        || !(result?.error === null || typeof result?.error === 'string')
        || (result?.state === 'current' && (result?.observed_revision !== revision || result?.error !== null))
        || (result?.state === 'changed' && (!/^[a-f\d]{64}$/i.test(result?.observed_revision || '') || result.observed_revision === revision || result?.error !== null))
        || (result?.state === 'unavailable' && (result?.observed_revision !== null || !(typeof result?.error === 'string' && result.error.trim())))
      if (malformed) {
        publish({ source: 'unavailable', sourceError: result?.error || 'The source freshness check returned an unknown result.', sourceChecking: false })
      } else {
        publish({ source: result.state, sourceError: result.error || null, sourceChecking: false, sourceStale: result.state === 'current' ? false : result.state === 'changed' ? true : state.sourceStale })
      }
    } catch (error) {
      if (!disposed && expectedGeneration === generation && state.summary?.ledger_revision === revision) {
        publish({ source: 'unavailable', sourceError: error?.message || String(error), sourceChecking: false })
      }
    } finally {
      if (checkingGeneration === expectedGeneration) checkingGeneration = null
      if (!disposed && expectedGeneration === generation && checkingGeneration === null && state.sourceChecking) publish({ sourceChecking: false })
    }
  }

  function dispose() { disposed = true; generation += 1; checkingGeneration = null }
  return { refresh, check, snapshot, dispose }
}
