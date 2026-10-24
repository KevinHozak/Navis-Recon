import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import vm from 'node:vm'

const source = await readFile(new URL('./startup_migration.js', import.meta.url), 'utf8')

async function runMigration({
  registrations = [],
  cacheNames = [],
  controller = null,
  href = 'http://tauri.localhost/',
  rejectRegistrations = false,
  rejectCaches = false,
  documentElementInitiallyMissing = false,
} = {}) {
  const deletedCaches = []
  const historyChanges = []
  const reloads = []
  const storageCalls = []
  const localStorage = {
    getItem: (key) => { storageCalls.push(['getItem', key]); return 'keep-me' },
    setItem: (key, value) => { storageCalls.push(['setItem', key, value]) },
    removeItem: (key) => { storageCalls.push(['removeItem', key]); throw Error('user storage must be preserved') },
    clear: () => { storageCalls.push(['clear']); throw Error('user storage must be preserved') },
  }
  const sw = {
    controller,
    register: () => Promise.resolve('original-register'),
    getRegistrations: () => rejectRegistrations ? Promise.reject(Error('unavailable')) : Promise.resolve(registrations),
  }
  const listeners = new Map()
  const document = {
    documentElement: documentElementInitiallyMissing ? null : { style: {} },
    addEventListener(name, callback) { listeners.set(name, callback) },
    removeEventListener(name, callback) { if (listeners.get(name) === callback) listeners.delete(name) },
  }
  const location = {
    get href() { return href },
    get origin() { return new URL(href).origin },
    replace(nextHref) { reloads.push(nextHref); href = nextHref },
  }
  const window = {
    location,
    history: { state: { route: 'overview' }, replaceState(...args) { historyChanges.push(args) } },
    setTimeout(callback, milliseconds) {
      const timer = setTimeout(callback, milliseconds)
      timer.unref()
      return timer
    },
  }
  const caches = {
    keys: () => rejectCaches ? Promise.reject(Error('unavailable')) : Promise.resolve(cacheNames),
    delete: async (name) => { deletedCaches.push(name); return true },
  }
  const context = { window, document, navigator: { serviceWorker: sw }, localStorage, caches, URL, DOMException, Promise, Number, String }
  vm.runInNewContext(source, context)
  await new Promise((resolve) => setImmediate(resolve))
  return { sw, deletedCaches, historyChanges, reloads, localStorage, storageCalls, document, listeners }
}

test('startup unregisters stale app worker, removes generated caches, and reloads once', async () => {
  let unregistered = 0
  const result = await runMigration({
    controller: {},
    registrations: [{ scope: 'http://tauri.localhost/', active: { scriptURL: 'http://tauri.localhost/sw.js' }, async unregister() { unregistered++; return true } }],
    cacheNames: ['workbox-precache-v2-http://tauri.localhost/', 'workbox-runtime-v2', 'navis-recon-user-cache'],
  })

  assert.equal(unregistered, 1)
  assert.deepEqual(result.deletedCaches, ['workbox-precache-v2-http://tauri.localhost/', 'workbox-runtime-v2'])
  assert.equal(result.reloads.length, 1)
  assert.match(result.reloads[0], /__navis_recon_worker_migration=1/)
  assert.deepEqual(result.storageCalls, [])
  assert.equal(result.document.documentElement.style.visibility, '')
  await assert.rejects(result.sw.register('/sw.js'), { name: 'NotAllowedError' })
})

test('unrelated-origin worker is preserved and storage failures do not reload-loop', async () => {
  let unregistered = 0
  const result = await runMigration({
    controller: {},
    registrations: [{ scope: 'https://other.example/', active: { scriptURL: 'https://other.example/sw.js' }, async unregister() { unregistered++; return true } }],
    rejectCaches: true,
  })

  assert.equal(unregistered, 0)
  assert.equal(result.reloads.length, 1) // one bounded retry for the active controller
  assert.match(result.reloads[0], /__navis_recon_worker_migration=1/)

  const retried = await runMigration({ href: result.reloads[0], controller: {}, rejectRegistrations: true, rejectCaches: true })
  assert.equal(retried.reloads.length, 1) // capped at the second and final retry
  const finalAttempt = await runMigration({ href: retried.reloads[0], controller: {}, rejectRegistrations: true, rejectCaches: true })
  assert.equal(finalAttempt.reloads.length, 0)
  assert.equal(finalAttempt.document.documentElement.style.visibility, '')
})

test('native migration does not depend on Tauri globals being initialized first', async () => {
  const result = await runMigration({ cacheNames: ['workbox-precache-v2'] })
  assert.deepEqual(result.deletedCaches, ['workbox-precache-v2'])
  await assert.rejects(result.sw.register('/sw.js'), { name: 'NotAllowedError' })
})

test('migration completion before DOMContentLoaded cannot leave a later-created document hidden', async () => {
  const result = await runMigration({ documentElementInitiallyMissing: true })
  assert.equal(result.document.documentElement, null)
  assert.equal(result.listeners.has('DOMContentLoaded'), false)
  result.document.documentElement = { style: {} }
  result.listeners.get('DOMContentLoaded')?.()
  assert.notEqual(result.document.documentElement.style.visibility, 'hidden')
  assert.deepEqual(result.storageCalls, [])
})
