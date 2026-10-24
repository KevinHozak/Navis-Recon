const MAX_EFFECTS = 2048
const seenEffects = new Set()
const validEffect = value => typeof value === 'string' && /^[a-f0-9]{64}$/i.test(value)

export function createSyncResourceObserver({ subscribe, snapshot, onEffects, onError = () => {} }) {
  let disposed = false
  let stop = null
  let generation = 0
  const apply = event => {
    if (disposed || !event || !Array.isArray(event.confirmed_effect_ids) || !Number.isSafeInteger(event.run_revision)) return
    const fresh = []
    for (const id of event.confirmed_effect_ids) {
      if (!validEffect(id) || seenEffects.has(id)) continue
      seenEffects.add(id); fresh.push(id)
      if (seenEffects.size > MAX_EFFECTS) seenEffects.delete(seenEffects.values().next().value)
    }
    if (fresh.length) { try { onEffects(fresh) } catch (error) { onError(error) } }
  }
  async function connect() {
    const current = ++generation
    try {
      const cleanup = await subscribe(apply)
      if (disposed || current !== generation) { cleanup?.(); return false }
      stop?.(); stop = cleanup
      const state = await snapshot()
      if (disposed || current !== generation) return false
      const outcomes = state?.managed_outcomes
      if (Array.isArray(outcomes)) apply({ run_revision: state.revision, confirmed_effect_ids: outcomes.filter(x => x?.state === 'succeeded').map(x => x.operation_id).filter(validEffect) })
      return true
    } catch (error) { if (!disposed && current === generation) onError(error); return false }
  }
  return { connect, dispose() { disposed = true; generation++; stop?.(); stop = null }, apply }
}
