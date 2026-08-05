import { describe, expect, it } from 'vitest'
import type { HintEffectsDto, HintPresentationDto, SessionSnapshotDto, TopologyDto } from './applicationPort'
import { cellRefFromIndex, mapHintPresentation, mapSessionSnapshot, mapTopology } from './wireMapper'

const variant: TopologyDto['variant'] = {
  blocks: true,
  disjoint_groups: false,
  windows: false,
  sudoku_x: true,
  girandola: false,
  asterisk: false,
  center_dot: false,
  anti_ferz: false,
  anti_knight: false,
  toroidal: false,
  non_consecutive: 'orthogonal_cyclic',
  forbidden_pairs: true,
}

const topology: TopologyDto = {
  variant,
  regions: [
    {
      region_type: 1,
      region_index: 0,
      family_key: 'row',
      label: 'Row 1',
      cells: [0, 1, 2, 3, 4, 5, 6, 7, 8],
    },
    {
      region_type: 5,
      region_index: 0,
      family_key: 'main_diagonal',
      label: 'Main diagonal',
      cells: [0, 10, 20, 30, 40, 50, 60, 70, 80],
    },
  ],
}

const presentation: HintPresentationDto = {
  identity: {
    technique_key: 'four_strong_links',
    name: 'Grouped 4 Strong links',
    short_name: '4SL',
    rating_tenths: 61,
  },
  views: [{
    key: 'chain-1',
    label: 'Chain 1',
    cell_marks: [{ cell: 80, roles: 0x0041 }],
    region_marks: [{ region_type: 1, region_index: 0, roles: 0x0042 }],
    candidate_marks: [{ candidate: { cell: 1, digit: 2 }, roles: 0x000c }],
    links: [
      {
        from: {
          type: 'candidate_group',
          representative: { cell: 1, digit: 2 },
          members: [{ cell: 1, digit: 2 }, { cell: 2, digit: 2 }],
        },
        to: { type: 'candidate', cell: 58, digit: 2 },
        kind: 'grouped_strong',
        cause: { type: 'region', region_type: 1, region_index: 0 },
        directed: false,
      },
      {
        from: { type: 'candidate', cell: 58, digit: 2 },
        to: { type: 'cell_center', cell: 80 },
        kind: 'implication',
        cause: { type: 'derived' },
        directed: true,
      },
    ],
  }],
  explanation: {
    blocks: [{
      type: 'paragraph',
      inlines: [
        { type: 'technique', technique_key: 'four_strong_links' },
        { type: 'text', text: ' removes ' },
        { type: 'candidate', cell: 39, digit: 2 },
      ],
    }],
  },
}

describe('wire mappers', () => {
  it('converts zero-based Rust cell indexes into row/column references', () => {
    expect(cellRefFromIndex(0)).toEqual({ row: 0, col: 0 })
    expect(cellRefFromIndex(10)).toEqual({ row: 1, col: 1 })
    expect(cellRefFromIndex(80)).toEqual({ row: 8, col: 8 })

    const snapshot: SessionSnapshotDto = {
      revision: '9007199254740993',
      values: Array<number>(81).fill(0),
      candidate_masks: Array<number>(81).fill(0x03fe),
      givens: Array<boolean>(81).fill(false),
      can_undo: true,
      can_redo: false,
    }
    snapshot.values[10] = 7
    snapshot.givens[10] = true
    snapshot.candidate_masks[10] = 0

    expect(mapSessionSnapshot(snapshot)).toMatchObject({
      revision: '9007199254740993',
      values: expect.arrayContaining([7]),
      canUndo: true,
      canRedo: false,
    })
    expect(mapSessionSnapshot(snapshot).values[10]).toBe(7)
  })

  it('maps topology families and preserves the complete variant configuration', () => {
    const mapped = mapTopology(topology)

    expect(mapped.regions[0]).toMatchObject({ id: 'row-0', family: 'row', label: 'Row 1' })
    expect(mapped.paths[0]).toMatchObject({ id: 'main_diagonal-0', family: 'main-diagonal', label: 'Main diagonal' })
    expect(mapped.paths[0]?.cells[8]).toEqual({ row: 8, col: 8 })
    expect(mapped.variant).toMatchObject({
      blocks: true,
      sudokuX: true,
      nonConsecutive: 'orthogonal-cyclic',
      forbiddenPairs: true,
    })
  })

  it('preserves role masks, grouped endpoints, causes, explanation tags, and exact effects', () => {
    const effects: HintEffectsDto = {
      placement: null,
      removals: [{ cell: 39, digits: (1 << 2) | (1 << 9) }],
      elimination_count: 2,
    }

    const mapped = mapHintPresentation({
      hintId: '18446744073709551615',
      revision: '9007199254740993',
      presentation,
      effects,
      topology,
    })
    const view = mapped.views[0]!

    expect(mapped).toMatchObject({
      id: '18446744073709551615',
      revision: '9007199254740993',
      techniqueKey: 'four_strong_links',
      rating: 6.1,
      type: 'Elimination (2)',
      eliminationCount: 2,
    })
    expect(mapped).not.toHaveProperty('chainCount')
    expect(mapped).not.toHaveProperty('count')
    expect(mapped.affects).toEqual([
      { row: 4, col: 3, digit: 2 },
      { row: 4, col: 3, digit: 9 },
    ])
    expect(view.cellMarks[0]?.roles).toEqual(['selected', 'primary'])
    expect(view.candidateMarks[0]?.roles).toEqual(['positive', 'negative'])
    expect(view.regions[0]).toMatchObject({ id: 'row-0', label: 'Row 1', roles: ['pattern', 'primary'] })
    expect(view.links[0]).toMatchObject({
      role: 'grouped-strong',
      direction: 'both',
      cause: { kind: 'region', regionType: 1, regionIndex: 0 },
      from: {
        kind: 'candidate-group',
        representative: { row: 0, col: 1, digit: 2 },
        members: [{ row: 0, col: 1, digit: 2 }, { row: 0, col: 2, digit: 2 }],
      },
    })
    expect(view.links[1]).toMatchObject({
      role: 'implication',
      direction: 'forward',
      to: { kind: 'cell-center', cell: { row: 8, col: 8 } },
    })
    expect(view.chainCells).toEqual([])
    expect(mapped.explanation?.blocks[0]).toEqual({
      kind: 'paragraph',
      inlines: [
        { kind: 'technique', techniqueKey: 'four_strong_links' },
        { kind: 'text', text: ' removes ' },
        { kind: 'candidate', candidate: { row: 4, col: 3, digit: 2 } },
      ],
    })
  })
})
