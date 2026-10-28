const positive = run => ['success', 'partial'].includes(run?.analysis_outcome)
export const showsReview = status => ['analyzing', 'review', 'committing', 'complete', 'failed'].includes(status)
export const canToggleMatch = (run, ready, pending, matchId) => ready && !pending && ['analyzing', 'review'].includes(run?.status) && (run.status !== 'review' || !Array.isArray(run.eligible_match_ids) || run.eligible_match_ids.includes(matchId))
export const canStartRun = (run, ready, pending, desktop) => ready && !pending && desktop && (!run?.run_id || ['complete', 'failed', 'cancelled'].includes(run.status))
export const canAbandonReview = (run, ready, pending) => ready && !pending && run?.status === 'review'
export const canCommitReview = (run, ready, pending, selectedCount, fundingComplete = true) => ready && !pending && run?.status === 'review' && positive(run) && selectedCount > 0 && fundingComplete
const outcomeStates = new Set(['succeeded', 'failed', 'uncertain'])
export function validManagedOutcomes(items) {
  if (!Array.isArray(items)) return false
  const operations = new Set()
  return items.every(item => {
    if (!item || typeof item.item_id !== 'string' || typeof item.filename !== 'string' || !outcomeStates.has(item.state) || typeof item.message !== 'string' || typeof item.retry_safe !== 'boolean' || !(item.operation_id === null || item.operation_id === undefined || (typeof item.operation_id === 'string' && /^[a-f0-9]{64}$/i.test(item.operation_id)))) return false
    if (item.operation_id) { if (operations.has(item.operation_id)) return false; operations.add(item.operation_id) }
    return true
  })
}
export const managedOutcomes = run => Array.isArray(run?.managed_outcomes) && validManagedOutcomes(run.managed_outcomes) ? run.managed_outcomes : []
export const retryableManagedItems = source => managedOutcomes(Array.isArray(source) ? { managed_outcomes: source } : source).filter(item => item.state === 'failed' && item.retry_safe && typeof item.operation_id === 'string')

export const filterRecoveryOutcomes = (items, filter) => items.filter(item => filter === 'all' || (filter === 'attention' ? item.state !== 'succeeded' : (item.operation_id || item.item_id) === filter))

export const sanitizeManagedOutcomes = items => items.map(({ item_id, filename, state, retry_safe, message, operation_id = null }) => ({ item_id, filename, state, retry_safe, message, operation_id }))
