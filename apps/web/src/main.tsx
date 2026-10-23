import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import './index.css'
import App from './App.tsx'
import { applyTheme, loadPreference } from './lib/theme'

// Thème : préférence mémorisée (clair / sombre / système) ; « système » suit l'OS en direct.
applyTheme(loadPreference())
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => applyTheme(loadPreference()))

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </QueryClientProvider>
  </StrictMode>,
)
