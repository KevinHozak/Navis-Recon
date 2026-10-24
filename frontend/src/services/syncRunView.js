const positive = run => ['success', 'partial'].includes(run?.analysis_outcome)
export const showsReview = status => ['analyzing', 'review', 'committing', 'complete', 'failed'].includes(status)
export const canToggleMatch = (run, ready, pending, matchId) => ready && !pending && ['analyzing', 'review'].includes(run?.status) && (run.status !== 'review' || !Array.isArray(run.eligible_match_ids) || run.eligible_match_ids.includes(matchId))
export const canStartRun = (run, ready, pending, desktop) => ready && !pending && desktop && (!run?.run_id || ['complete', 'failed', 'cancelled'].includes(run.status))
export const canAbandonReview = (run, ready, pending) => ready && !pending && run?.status === 'review'
export const canCommitReview = (run, ready, pending, selectedCount) => ready && !pending && run?.status === 'review' && positive(run) && selectedCount > 0
