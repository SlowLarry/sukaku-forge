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

  it('exposes rows, cells, active selection, and given versus entered values', () => {
    const values = Array<number | null>(81).fill(null)
    values[0] = 5
    values[1] = 4
    const givens = Array<boolean>(81).fill(false)
    givens[0] = true
    const markup = renderToStaticMarkup(
      <Board
        board={{ values, candidateMasks: Array<number>(81).fill(0), givens }}
        topology={{ regions: [], paths: [] }}
        view={{ id: 'empty', label: 'Empty', candidateMarks: [], cellMarks: [], regions: [], links: [], chainCells: [] }}
        selected={{ row: 2, col: 3 }}
        candidatesVisible
        onSelect={() => undefined}
        onKeyDown={() => undefined}
      />,
    )

    expect(markup.match(/role="row"/g)).toHaveLength(9)
    expect(markup.match(/role="gridcell"/g)).toHaveLength(81)
    expect(markup).toContain('aria-activedescendant="sudoku-cell-2-3"')
    expect(markup).toContain('aria-label="r1c1, given value 5"')
    expect(markup).toContain('cell-value cell-value--given')
    expect(markup).toContain('cell-value cell-value--entered')
    expect(markup).toContain('<kbd>M</kbd> Candidates')
    expect(markup).not.toContain('<kbd>Del</kbd> Clear')
  })
})
