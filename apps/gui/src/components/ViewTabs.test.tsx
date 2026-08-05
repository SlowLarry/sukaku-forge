import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { primaryHint } from '../fixture'
import { ViewTabs, viewTabKeyboardTarget } from './ViewTabs'

describe('ViewTabs', () => {
  it('associates every tab with a panel and exposes a single active tab stop', () => {
    const views = primaryHint.views.slice(0, 2)
    const markup = renderToStaticMarkup(
      <ViewTabs views={views} selectedId={views[1]!.id} onSelect={vi.fn()}>
        <p>Selected proof content</p>
      </ViewTabs>,
    )

    expect(markup).toContain('role="tablist"')
    expect(markup.match(/role="tab"/g)).toHaveLength(2)
    expect(markup.match(/role="tabpanel"/g)).toHaveLength(2)
    expect(markup).toContain('aria-selected="true"')
    expect(markup).toContain('aria-selected="false"')
    expect(markup).toContain('tabindex="-1"')
    expect(markup).toContain('aria-controls=')
    expect(markup).toContain('aria-labelledby=')
    expect(markup).toContain('Selected proof content')
  })

  it('keeps hint information available when no proof view exists', () => {
    const markup = renderToStaticMarkup(
      <ViewTabs views={[]} selectedId="" onSelect={vi.fn()}><p>No proof payload</p></ViewTabs>,
    )

    expect(markup).toContain('No proof views')
    expect(markup).toContain('role="region"')
    expect(markup).toContain('No proof payload')
    expect(markup).not.toContain('role="tab"')
  })

  it('wraps arrow navigation and supports Home and End', () => {
    expect(viewTabKeyboardTarget('ArrowRight', 1, 2)).toBe(0)
    expect(viewTabKeyboardTarget('ArrowLeft', 0, 2)).toBe(1)
    expect(viewTabKeyboardTarget('Home', 1, 2)).toBe(0)
    expect(viewTabKeyboardTarget('End', 0, 2)).toBe(1)
    expect(viewTabKeyboardTarget('ArrowDown', 0, 2)).toBeNull()
    expect(viewTabKeyboardTarget('Enter', 0, 2)).toBeNull()
  })
})
