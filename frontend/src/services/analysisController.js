export function createAnalysisController({ load, onChange }) {
  let generation = 0; let active = true
  const publish = value => { if (active) onChange(value) }
  return {
    activate() { active = true },
    invalidate() { generation++; publish({ data: null, loading: false, error: null }) },
    dispose() { active = false; generation++ },
    async run(request) {
      if (!active) return { stale: true }
      const id = ++generation
      publish({ data: null, loading: true, error: null })
      try {
        const data = await load(request)
        if (!active || id !== generation) return { stale: true }
        publish({ data, loading: false, error: null }); return { data }
      } catch (e) {
        if (!active || id !== generation) return { stale: true }
        const error = e?.message || String(e)
        publish({ data: null, loading: false, error }); return { error }
      }
    },
  }
}
