import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { StatusBar } from './StatusBar'

describe('StatusBar', () => {
  it('distinguishes idle, running, and error status semantics', () => {
    const idle = renderToStaticMarkup(<StatusBar state="idle" message="Ready." revision="4" clueCount={28} />)
    const running = renderToStaticMarkup(<StatusBar state="running" message="Finding hint…" revision="4" clueCount={28} />)
    const error = renderToStaticMarkup(<StatusBar state="error" message="Transport failed." />)

    expect(idle).toContain('data-status-state="idle"')
    expect(idle).toContain('role="status"')
    expect(idle).toContain('28 givens')
    expect(running).toContain('data-status-state="running"')
    expect(running).toContain('Working')
    expect(error).toContain('data-status-state="error"')
    expect(error).toContain('role="alert"')
    expect(error).toContain('Session not ready')
  })
})
