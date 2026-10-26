export function createMonthlyCloseController({ load, save, onChange }) {
  let disposed = false, generation = 0, queuedRefresh = false;
  let state = { snapshot: null, request: null, loading: false, saving: false, error: null };
  const publish = patch => { if (!disposed) { state = { ...state, ...patch }; onChange(state); } };
  const valid = value => {
    if (!value || !['open', 'closed', 'reopened', 'stale'].includes(value.state) || !Array.isArray(value.blockers) || !Array.isArray(value.accounts) || !Array.isArray(value.metrics) || !/^[a-f0-9]{64}$/.test(value.ledger_revision) || !/^[a-f0-9]{64}$/.test(value.evidence_revision) || !(value.revision === 'missing' || /^[a-f0-9]{64}$/.test(value.revision))) throw Error('Monthly-close evidence response is malformed');
    return value;
  };
  async function refresh(request = state.request, retainError = false) {
    if (disposed || !request) return;
    if (state.saving) { queuedRefresh = true; return; }
    const current = ++generation;
    request = structuredClone(request);
    publish({ request, snapshot: JSON.stringify(request) === JSON.stringify(state.request) ? state.snapshot : null, loading: true, error: retainError ? state.error : null });
    try {
      const snapshot = valid(await load(request));
      if (!disposed && current === generation) publish({ snapshot, loading: false });
    } catch (error) { if (!disposed && current === generation) publish({ snapshot: null, loading: false, error: error?.message || String(error) }); }
  }
  async function persist(action) {
    if (disposed || state.saving || state.loading || !state.snapshot || (action === 'closed' && state.snapshot.blockers.length)) return false;
    const current = ++generation;
    const request = structuredClone(state.request), before = state.snapshot;
    publish({ saving: true, error: null });
    try {
      const snapshot = valid(await save(request, before.revision, before.evidence_revision, action));
      if (!disposed && current === generation) {
        publish({ saving: false, snapshot });
        if (queuedRefresh) { queuedRefresh = false; await refresh(request); }
      }
      return true;
    } catch (error) {
      if (!disposed && current === generation) { queuedRefresh = false; publish({ saving: false, snapshot: null, error: `Summary not confirmed saved: ${error?.message || String(error)}` }); await refresh(request, true); }
      return false;
    }
  }
  return { refresh, persist, snapshot: () => state, dispose() { disposed = true; generation++; } };
}
