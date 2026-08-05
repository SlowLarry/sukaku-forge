import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import type { HintView } from '../model'
import { Board } from './Board'

const candidate = (row: number, col: number, digit: number) => ({ row, col, digit })

describe('Board', () => {
  it('preserves grouped endpoint membership and distinguishes semantic link kinds', () => {
    const view: HintView = {
      id: 'semantic-links',
      label: 'Semantic links',
      candidateMarks: [],
      cellMarks: [],
      regions: [],
      chainCells: [],
      links: [
        {
          id: 'grouped',
          from: {
            kind: 'candidate-group',
            representative: candidate(0, 0, 4),
            members: [candidate(0, 0, 4), candidate(0, 1, 4)],
          },
          to: { kind: 'candidate', candidate: candidate(3, 0, 4) },
          role: 'grouped-strong',
          direction: 'both',
        },
        {
          id: 'strong',
          from: { kind: 'candidate', candidate: candidate(3, 0, 4) },
          to: { kind: 'candidate', candidate: candidate(3, 3, 4) },
          role: 'strong',
          direction: 'forward',
        },
        {
          id: 'weak',
          from: { kind: 'candidate', candidate: candidate(3, 3, 4) },
          to: { kind: 'candidate', candidate: candidate(6, 3, 4) },
          role: 'weak',
          direction: 'forward',
        },
        {
          id: 'implication',
          from: { kind: 'candidate', candidate: candidate(6, 3, 4) },
          to: { kind: 'cell-center', cell: { row: 8, col: 8 } },
          role: 'implication',
          direction: 'forward',
        },
      ],
    }

    const markup = renderToStaticMarkup(
      <Board
        board={{ values: Array<number | null>(81).fill(null), candidateMasks: Array<number>(81).fill(0x03fe) }}
        topology={{ regions: [], paths: [] }}
        view={view}
        selected={null}
        candidatesVisible
        onSelect={() => undefined}
        onKeyDown={() => undefined}
      />,
    )

    expect(markup.match(/candidate-group-halo/g)).toHaveLength(2)
    expect(markup).toContain('data-group-link="grouped" data-group-side="from"')
    expect(markup).toContain('chain-link chain-link--grouped-strong')
    expect(markup).toContain('chain-link chain-link--strong')
    expect(markup).toContain('chain-link chain-link--weak')
    expect(markup).toContain('chain-link chain-link--implication')
    expect(markup).toContain('marker-end="url(#arrow-implication)"')
  })
})
