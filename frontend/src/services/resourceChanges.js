const listeners = new Set();
export function subscribeResourceChanges(listener) { listeners.add(listener); return () => listeners.delete(listener); }
export function invalidateResources(resources) {
  for (const listener of listeners) {
    try { listener(new Set(resources)); } catch (error) { console.error('Resource subscriber failed:', error); }
  }
}
export async function persistedAction(resources, action) {
  try { return await action(); }
  finally { invalidateResources(resources); } // A lost reply can follow a successful write.
}
