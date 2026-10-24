const terminal = new Set(['complete', 'failed', 'cancelled'])
const statuses = new Set(['idle', 'analyzing', 'review', 'committing', 'complete', 'failed', 'cancelled'])

export function reviewSelection(run, current) {
  if (current.runId === run.run_id) return current.values
  return new Set((run.matches || []).map((_, index) => index))
}

function validRun(run) {
  return Boolean(run && Number.isSafeInteger(run.revision) && run.revision >= 0 &&
    Number.isSafeInteger(run.sequence) && run.sequence >= 0 && statuses.has(run.status) &&
    (run.run_id === null || (typeof run.run_id === 'string' && run.run_id.length > 0)) &&
    (run.status === 'idle' ? run.run_id === null : run.run_id !== null) &&
    ['logs', 'matches', 'unmatched', 'errors'].every(key => Array.isArray(run[key])) &&
    Object.hasOwn(run, 'result') && (run.result === null || (typeof run.result === 'object' && !Array.isArray(run.result))))
}

export function createSyncRunController({ api, onChange = () => {} }) {
  let state = null
  let disposed = false
  let pending = false
  let pendingToken = 0
  let unlisten = null
  let registration = null
  let generation = 0
  let ready = false
  let error = null
  const publish = () => { if (!disposed) onChange({ run: state, pending, ready, ...(error ? { error } : {}) }) }

  function accept(next, current) {
    if (disposed || current !== generation || !validRun(next)) return false
    if (state && next.revision <= state.revision) return false
    state = next
    error = null
    publish()
    return true
  }

  async function connect() {
    if (disposed) return false
    const current = ++generation
    pendingToken += 1
    pending = false
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
      if (!state || initial.revision > state.revision) state = initial
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
      if (disposed || current !== generation) return false
      accept(result, current)
      return true
    } catch (failure) {
      if (disposed || current !== generation) return false
      try {
        const latest = await api.getSyncRun()
        if (!disposed && current === generation && validRun(latest) && (!state || latest.revision > state.revision)) accept(latest, current)
      } catch { /* retain the last accepted snapshot */ }
      if (!disposed && current === generation) { error = failure; publish() }
      return false
    } finally {
      if (token === pendingToken) { pending = false; publish() }
    }
  }
  return {
    connect,
    start: () => !ready || (state && !terminal.has(state.status) && state.status !== 'idle') ? Promise.resolve(false) : invoke(() => api.startLedgerSync()),
    commit: (matches) => ready && state?.status === 'review' && matches.length ? invoke(() => api.commitLedgerSync(state.run_id, matches)) : Promise.resolve(false),
    cancel: () => ready && state?.status === 'review' ? invoke(() => api.cancelSyncRun(state.run_id)) : Promise.resolve(false),
    dispose() { disposed = true; generation += 1; registration?.abort(); registration = null; unlisten?.(); unlisten = null },
    getState: () => ({ run: state, pending, ready }),
  }
}
