// One authoritative persisted snapshot; requests never update a checkmark optimistically.
export function createDocumentSaver({ persist, onChange }) {
  let snapshot = null
  let busy = false
  let active = true
  function publish(status, error = null) { if (active) onChange({ snapshot, status, error }) }
  return {
    accept(value) { if (busy) return false; snapshot = value; publish('idle'); return true },
    isBusy: () => busy,
    activate() { active = true },
    dispose() { active = false },
    async save(candidate) {
      if (!active || busy || !snapshot) return { skipped: true }
      busy = true; publish('saving')
      try {
        const saved = await persist({ ...candidate, revision: snapshot.revision })
        if (!saved || typeof saved.revision !== 'string') throw new Error('Save readback did not include a revision; refresh before retrying.')
        snapshot = saved; publish('saved'); return { saved: true }
      } catch (error) { publish('failed', error?.message || String(error)); return { saved: false } }
      finally { busy = false }
    },
  }
}
