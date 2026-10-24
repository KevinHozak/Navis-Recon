import { StrictMode } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { registerSW } from 'virtual:pwa-register'

// Keep browser installs installable, but never let a desktop worker own the
// app origin. Native startup also removes workers left by older releases.
if (!isTauri()) {
  registerSW({ immediate: true })
}
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.jsx'

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
