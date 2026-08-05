import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { primaryHint } from '../fixture'
import { Explanation } from './Explanation'

describe('Explanation', () => {
  it('renders connector roles in the presentation order', () => {
    const view = primaryHint.views[0]!
    const markup = renderToStaticMarkup(<Explanation hint={primaryHint} view={view} applied={false} />)

    const firstGrouped = markup.indexOf('data-link-role="grouped"')
    const strongTrue = markup.indexOf('data-link-role="strong-true"')
    const strongFalse = markup.indexOf('data-link-role="strong-false"')
    const secondGrouped = markup.indexOf('data-link-role="grouped"', firstGrouped + 1)

    expect(firstGrouped).toBeGreaterThan(-1)
    expect(strongTrue).toBeGreaterThan(firstGrouped)
    expect(strongFalse).toBeGreaterThan(strongTrue)
    expect(secondGrouped).toBeGreaterThan(strongFalse)
    expect(markup).toContain('flow-link flow-link--2')
    expect(markup).toContain('flow-link flow-link--0')
    expect(markup).toContain('flow-link flow-link--1')
  })

  it('renders bidirectional links as bidirectional and exposes their semantics', () => {
    const sourceView = primaryHint.views[0]!
    const view = {
      ...sourceView,
      id: 'bidirectional-test',
      links: [{ ...sourceView.links[0]!, direction: 'both' as const }],
      chainCells: sourceView.chainCells.slice(0, 2),
    }
    const hint = { ...primaryHint, views: [view] }
    const markup = renderToStaticMarkup(<Explanation hint={hint} view={view} applied={false} />)

    expect(markup).toContain('data-link-direction="both"')
    expect(markup).toContain('in both directions with')
    expect(markup).toContain('↔')
  })

  it('renders a no-presentation state without stale proof content', () => {
    const markup = renderToStaticMarkup(<Explanation hint={null} view={null} applied={false} />)

    expect(markup).toContain('No individual hint selected')
    expect(markup).toContain('No proof loaded')
    expect(markup).not.toContain('data-link-role=')
  })

  it('renders typed wire explanations and ordered semantic edges without a fabricated chain path', () => {
    const sourceView = primaryHint.views[0]!
    const view = {
      ...sourceView,
      id: 'wire-view',
      chainCells: [],
      links: [{
        id: 'wire-edge',
        from: {
          kind: 'candidate-group' as const,
          representative: { row: 0, col: 1, digit: 2 },
          members: [{ row: 0, col: 1, digit: 2 }, { row: 0, col: 2, digit: 2 }],
        },
        to: { kind: 'candidate' as const, candidate: { row: 6, col: 4, digit: 2 } },
        role: 'grouped-strong' as const,
        direction: 'both' as const,
        cause: { kind: 'region' as const, regionType: 1, regionIndex: 0 },
      }],
    }
    const hint = {
      ...primaryHint,
      techniqueKey: 'four_strong_links',
      chainCount: undefined,
      views: [view],
      explanation: {
        blocks: [{
          kind: 'paragraph' as const,
          inlines: [
            { kind: 'technique' as const, techniqueKey: 'four_strong_links' },
            { kind: 'text' as const, text: ' removes ' },
            { kind: 'candidate' as const, candidate: { row: 4, col: 3, digit: 2 } },
          ],
        }],
      },
    }
    const markup = renderToStaticMarkup(<Explanation hint={hint} view={view} applied={false} />)

    expect(markup).toContain('data-explanation-type="technique" data-technique-key="four_strong_links"')
    expect(markup).toContain('data-explanation-type="candidate"')
    expect(markup).toContain('>Grouped 4 Strong links</strong> removes <span data-explanation-type="candidate">r5c4(2)</span>')
    expect(markup).toContain('data-link-id="wire-edge" data-link-role="grouped-strong" data-link-direction="both"')
    expect(markup).toContain('group r1c2(2), r1c3(2)')
    expect(markup).toContain('Grouped strong')
    expect(markup).not.toContain('No chain path in this view')
    expect(markup).not.toContain('undefined')
  })
})
