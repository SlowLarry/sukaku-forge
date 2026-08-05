import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { createPlatformPort } from './platformPort'
import './styles.css'

const root = createRoot(document.getElementById('root')!)

void createPlatformPort().then((port) => {
  root.render(
    <StrictMode>
      <App port={port} />
    </StrictMode>,
  )
}).catch((error: unknown) => {
  const message = error instanceof Error ? error.message : 'The application transport could not start.'
  root.render(
    <main className="startup-error" role="alert">
      <h1>Sukaku Forge could not start</h1>
      <p>{message}</p>
    </main>,
  )
})
