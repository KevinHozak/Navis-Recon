import { StrictMode } from 'react'
import { registerSW } from 'virtual:pwa-register'

// Explicitly register the service worker for the PWA
registerSW({ immediate: true })
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.jsx'

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
