export function createLatestRequestLoader({ load, update, map = (value) => value }) {
  let generation = 0
  let disposed = false

  async function refresh() {
    const requestId = ++generation
    try {
      const value = await load()
      if (disposed || requestId !== generation) return { stale: true }
      const mapped = map(value)
      if (mapped === null) throw new Error('Count resource returned an invalid response.')
      update(mapped)
      return { value: mapped }
    } catch (error) {
      if (disposed || requestId !== generation) return { stale: true }
      update(null, error)
      return { error }
    }
  }

  function invalidate() { generation += 1 }
  function dispose() { disposed = true; generation += 1 }
  return { refresh, invalidate, dispose }
}
