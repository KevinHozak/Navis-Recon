const terminal = new Set(['complete', 'failed', 'cancelled'])
const statuses = new Set(['idle', 'analyzing', 'review', 'committing', 'complete', 'failed', 'cancelled'])

export function createSelectionStore() {
  let current = null
  return {
    values(run) {
      if (!run?.run_id) return new Set()
      return current?.run_id === run.run_id ? new Set(current.selected) : new Set()
    },
    reconcile(run) {
      if (!run?.run_id || !Number.isSafeInteger(run.revision)) return false
      if (current && run.revision <= current.revision && run.run_id !== current.run_id) return false
      if (current?.run_id !== run.run_id) current = { run_id: run.run_id, revision: run.revision, seen: new Set(), selected: new Set() }
      if (run.revision < current.revision) return false
      current.revision = run.revision
      for (const match of run.matches || []) if (typeof match.match_id === 'string' && !current.seen.has(match.match_id)) { current.seen.add(match.match_id); current.selected.add(match.match_id) }
      if (run.status === 'review' && Array.isArray(run.eligible_match_ids)) {
        const eligible = new Set(run.eligible_match_ids)
        current.selected = new Set([...current.selected].filter(id => eligible.has(id)))
      }
      return true
    },
    toggle(run, matchId) {
      if (!run?.run_id || !(run.matches || []).some(match => match.match_id === matchId)) return
      this.reconcile(run)
      if (current?.run_id !== run.run_id) return
      current.selected.has(matchId) ? current.selected.delete(matchId) : current.selected.add(matchId)
    },
  }
}
const sharedSelection = createSelectionStore()

function validRun(run) {
  return Boolean(run && Number.isSafeInteger(run.revision) && run.revision >= 0 &&
    Number.isSafeInteger(run.sequence) && run.sequence >= 0 && statuses.has(run.status) &&
    (run.run_id === null || (typeof run.run_id === 'string' && run.run_id.length > 0)) &&
    (run.status === 'idle' ? run.run_id === null : run.run_id !== null) &&
    ['logs', 'matches', 'unmatched', 'errors'].every(key => Array.isArray(run[key])) &&
    (['analyzing', 'idle'].includes(run.status) ? run.analysis_outcome === null : ['success', 'partial', 'failed'].includes(run.analysis_outcome)) &&
    new Set(run.matches.map(match => match?.match_id)).size === run.matches.length && run.matches.every(match => typeof match?.match_id === 'string' && match.match_id.length > 0 && typeof match.filename === 'string' && match.filename.length > 0 && typeof match.new_name === 'string' && match.new_name.length > 0) &&
    (run.eligible_match_ids === undefined || (Array.isArray(run.eligible_match_ids) && new Set(run.eligible_match_ids).size === run.eligible_match_ids.length && run.eligible_match_ids.every(id => run.matches.some(match => match.match_id === id)))) &&
    (!['review', 'committing', 'complete', 'cancelled'].includes(run.status) || ['success', 'partial'].includes(run.analysis_outcome)) &&
    Object.hasOwn(run, 'result') && (run.result === null || (typeof run.result === 'object' && !Array.isArray(run.result))))
}

export function createSyncRunController({ api, onChange = () => {}, selectionStore = sharedSelection }) {
  let state = null
  let disposed = false
  let pending = false
  let pendingToken = 0
  let unlisten = null
  let registration = null
  let generation = 0
  let ready = false
  let error = null
  const publish = () => { if (!disposed) onChange({ run: state, pending, ready, selectedIds: selectionStore.values(state), ...(error ? { error } : {}) }) }

  function accept(next, current) {
    if (disposed || current !== generation || !validRun(next)) return false
    if (state && next.revision <= state.revision) return false
    if (next.run_id && !selectionStore.reconcile(next)) return false
    state = next
    error = null
    publish()
    return true
  }

  async function connect() {
    if (disposed) return false
    const current = ++generation
    registration?.abort()
    unlisten?.(); unlisten = null
    ready = false
    error = null
    publish()
    const abort = new AbortController()
    registration = abort
    try {
      const stop = await api.listenForSyncEvents(Object.fromEntries(['state', 'log', 'match', 'unmatched', 'error', 'done'].map(name => [name, event => {
        if (disposed || current !== generation) return
        if (name === 'state') accept(event, current)
        else applyEvent(event, current)
      }])), { signal: abort.signal })
      if (disposed || current !== generation) { stop(); return false }
      unlisten = stop
      const initial = await api.getSyncRun()
      if (disposed || current !== generation) return false
      if (!validRun(initial)) throw new Error('Sync service returned a malformed snapshot.')
      if (!state || initial.revision > state.revision) accept(initial, current)
      ready = true; error = null; publish()
      return true
    } catch (failure) {
      if (!disposed && current === generation) { ready = false; error = failure; publish() }
      return false
    }
  }

  function applyEvent(event, current) {
    const snap = event?.snapshot
    if (disposed || current !== generation || !validRun(snap) || event.run_id !== snap.run_id || event.revision !== snap.revision || event.sequence !== snap.sequence || event.status !== snap.status) return
    accept(snap, current)
  }

  async function invoke(action) {
    if (disposed || pending || !ready) return false
    const current = generation
    const token = ++pendingToken
    pending = true; error = null; publish()
    try {
      const result = await action()
      if (disposed) return false
      if (current !== generation) {
        const latest = await api.getSyncRun()
        if (disposed) return false
        if (!validRun(latest)) throw new Error('Sync service returned a malformed reconciliation snapshot.')
        state = null
        accept(latest, generation)
        if (!state) throw new Error('Unable to reconcile sync command after reconnect.')
        return false
      }
      if (!validRun(result)) throw new Error('Sync service returned a malformed action snapshot.')
      accept(result, current)
      return true
    } catch (failure) {
      if (disposed) return false
      try {
        const latest = await api.getSyncRun()
        if (!disposed && validRun(latest)) {
          if (current !== generation || !state || latest.revision > state.revision) { state = null; accept(latest, generation) }
          if (!state) throw new Error('Unable to reconcile sync command.', { cause: failure })
        } else throw new Error('Sync service returned a malformed reconciliation snapshot.', { cause: failure })
      } catch (reconcileFailure) { if (!disposed) { ready = false; error = reconcileFailure } }
      if (!disposed) { error ||= new Error('Sync command failed.', { cause: failure }); publish() }
      return false
    } finally {
      if (token === pendingToken) { pending = false; publish() }
    }
  }
  return {
    connect,
    start: () => !ready || (state && !terminal.has(state.status) && state.status !== 'idle') ? Promise.resolve(false) : invoke(() => api.startLedgerSync()),
    commit: (matches) => {
      const ids = Array.isArray(matches) ? matches : []
      const eligible = state?.eligible_match_ids || state?.matches.map(match => match.match_id) || []
      const valid = ids.length > 0 && ids.every(id => typeof id === 'string' && eligible.includes(id))
      return ready && state?.status === 'review' && ['success', 'partial'].includes(state.analysis_outcome) && valid ? invoke(() => api.commitLedgerSync(state.run_id, ids)) : Promise.resolve(false)
    },
    cancel: () => ready && state?.status === 'review' ? invoke(() => api.cancelSyncRun(state.run_id)) : Promise.resolve(false),
    toggle(matchId) { if ((state?.status === 'analyzing' || state?.status === 'review') && (state.status !== 'review' || (state.eligible_match_ids || state.matches.map(match => match.match_id)).includes(matchId))) { selectionStore.toggle(state, matchId); publish() } },
    dispose() { disposed = true; generation += 1; registration?.abort(); registration = null; unlisten?.(); unlisten = null },
    getState: () => ({ run: state, pending, ready }),
  }
}
