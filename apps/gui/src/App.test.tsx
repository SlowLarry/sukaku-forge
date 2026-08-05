import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import App from './App'

describe('App', () => {
  it('renders the meaningful workspace and layered SVG surface server-side', () => {
    const markup = renderToStaticMarkup(<App />)

    expect(markup).toContain('Sukaku Forge')
    expect(markup).toContain('Hint browser')
    expect(markup).toContain('Grouped 4 Strong links 20121')
    expect(markup).toContain('aria-label="Sudoku board')
    expect(markup).toContain('topology-boundary--block')
    expect(markup).toContain('chain-link--strong-true')
    expect(markup).toContain('candidate--mixed')
    expect(markup).toContain('role="status"')
    expect(markup).toContain('aria-label="Get next hint"')
    expect(markup).toContain('aria-pressed="false"')
  })

  it('shows an honest empty presentation when a technique row has no individual hint', () => {
    const markup = renderToStaticMarkup(<App initialHintId="x-chain" />)

    expect(markup).toContain('X-Chain')
    expect(markup).toContain('No individual hint selected')
    expect(markup).toContain('Select an individual deduction in the hint browser')
    expect(markup).toMatch(/<button class="primary-button"[^>]*disabled=""[^>]*aria-label="Apply selected hint"/)
    expect(markup).not.toContain('chain-link--strong-true')
    expect(markup).not.toContain('data-link-role=')
  })
})
