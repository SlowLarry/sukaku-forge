import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { primaryHint } from '../fixture'
import { HintDetails } from './HintDetails'

describe('HintDetails', () => {
  it('describes wire-backed placement and view counts without undefined chain metadata', () => {
    const hint = {
      ...primaryHint,
      type: 'Placement',
      affects: [],
      placement: { row: 0, col: 8, digit: 9 },
      chainCount: undefined,
      views: [primaryHint.views[0]!],
    }
    const row = {
      id: hint.id,
      label: hint.technique,
      count: 1,
      rating: hint.rating,
      indent: 0,
      presentation: hint,
    }
    const markup = renderToStaticMarkup(<HintDetails hint={hint} selectedRow={row} />)

    expect(markup).toContain('<dt>Affects</dt><dd>r1c9 = 9</dd>')
    expect(markup).toContain('<dt>Proof</dt><dd>1 proof view</dd>')
    expect(markup).not.toContain('undefined')
    expect(markup).not.toContain('grouped by digit 2')
  })
})
