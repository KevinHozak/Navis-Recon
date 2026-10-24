// Runs in the WebView at document start, before even a stale cached entrypoint.
// Tauri gives this application its own origin and WebView profile. Keep cleanup
// on that origin, and only delete the asset caches created by Workbox/Vite PWA.
(() => {
  if (!('serviceWorker' in navigator)) return

  const migrationParameter = '__navis_recon_worker_migration'
  const maxReloads = 2
  const url = new URL(window.location.href)
  const reloadCount = Number.parseInt(url.searchParams.get(migrationParameter) || '0', 10) || 0

  // A stale entrypoint may itself try to install another worker while this
  // migration is running. Reject it before any page script gets a turn.
  const rejectRegistration = () => Promise.reject(new DOMException('Service workers are disabled in Navis-Recon desktop.', 'NotAllowedError'))
  try {
    Object.defineProperty(navigator.serviceWorker, 'register', {
      configurable: false,
      value: rejectRegistration,
    })
  } catch {
    try {
      Object.defineProperty(Object.getPrototypeOf(navigator.serviceWorker), 'register', {
        configurable: false,
        value: rejectRegistration,
      })
    } catch {
      // Cleanup still runs when the WebView does not allow replacing this method.
    }
  }

  let migrationFinished = false
  const hideDocument = () => {
    if (!migrationFinished && document.documentElement) document.documentElement.style.visibility = 'hidden'
  }
  const showDocument = () => {
    migrationFinished = true
    document.removeEventListener('DOMContentLoaded', hideDocument)
    if (document.documentElement) document.documentElement.style.visibility = ''
  }
  if (document.documentElement) hideDocument()
  else document.addEventListener('DOMContentLoaded', hideDocument, { once: true })

  const withTimeout = (promise, milliseconds) => Promise.race([
    promise,
    new Promise((resolve) => window.setTimeout(() => resolve(null), milliseconds)),
  ])

  const migrate = async () => {
    let changed = Boolean(navigator.serviceWorker.controller)

    try {
      const registrations = await withTimeout(navigator.serviceWorker.getRegistrations(), 4000) || []
      for (const registration of registrations) {
        try {
          const scope = new URL(registration.scope)
          const script = registration.active?.scriptURL || registration.waiting?.scriptURL || registration.installing?.scriptURL
          if (scope.origin !== window.location.origin || !script || new URL(script).origin !== window.location.origin) continue
          const removed = await withTimeout(registration.unregister(), 4000)
          changed = changed || removed === true
        } catch {
          // Keep the startup usable if an individual old registration is broken.
        }
      }
    } catch {
      // Service worker storage can be unavailable or reject during WebView startup.
    }

    try {
      const cacheNames = await withTimeout(caches.keys(), 4000) || []
      for (const name of cacheNames) {
        if (!name.startsWith('workbox-precache-') && !name.startsWith('workbox-runtime-')) continue
        try {
          const removed = await withTimeout(caches.delete(name), 4000)
          changed = changed || removed === true
        } catch {
          // Preserve app startup if one generated cache cannot be deleted.
        }
      }
    } catch {
      // CacheStorage may be unavailable; no app data is cleared as a fallback.
    }

    if (changed && reloadCount < maxReloads) {
      url.searchParams.set(migrationParameter, String(reloadCount + 1))
      window.location.replace(url.href)
      return
    }

    // Strip our one-time loop guard without touching app storage or route state.
    if (url.searchParams.has(migrationParameter)) {
      url.searchParams.delete(migrationParameter)
      window.history.replaceState(window.history.state, '', `${url.pathname}${url.search}${url.hash}`)
    }
    showDocument()
  }

  // A broken WebView API must not leave the window permanently blank.
  withTimeout(migrate(), 12000).then(() => showDocument(), () => showDocument())
})()
