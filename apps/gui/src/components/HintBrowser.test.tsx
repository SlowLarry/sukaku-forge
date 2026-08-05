import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { HintBrowser } from './HintBrowser'

describe('HintBrowser', () => {
  it('labels presented and unsupported hints without fabricating counts', () => {
    const markup = renderToStaticMarkup(
      <HintBrowser
        items={[
          { id: '9', label: 'Hidden Single', detail: 'Placement · 1 proof view', kind: 'presented', rating: 1.2 },
          { id: '10', label: 'Forcing Chain', detail: 'Missing Chain Proof · 2 eliminations', kind: 'unsupported' },
        ]}
        selectedId="9"
        emptyMessage="Request a hint."
      />,
    )

    expect(markup).toContain('2 active deductions')
    expect(markup).toContain('data-hint-kind="presented"')
    expect(markup).toContain('data-hint-kind="unsupported"')
    expect(markup).toContain('Proof unavailable')
    expect(markup).toContain('>1.2</strong>')
    expect(markup).not.toContain('Count')
  })

  it('shows the supplied authoritative empty outcome', () => {
    const markup = renderToStaticMarkup(
      <HintBrowser items={[]} selectedId={null} emptyMessage="No applicable hint exists." />,
    )

    expect(markup).toContain('No applicable hint exists.')
    expect(markup).not.toContain('Search active hints')
  })
})
