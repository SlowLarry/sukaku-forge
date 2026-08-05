import type { BoardTopology, CellRef, HintPresentation, HintRow } from './model'

const c = (row: number, col: number, digit: number) => ({ row, col, digit })
const cell = (row: number, col: number): CellRef => ({ row, col })
const p = (row: number, col: number, digit: number) => ({ kind: 'candidate' as const, candidate: c(row, col, digit) })

export const initialValues: Array<number | null> = [
  '5..9.4.7.',
  '.74.3.9..',
  '.98.7.4.3',
  '6..5.9.3.',
  '..9.6....',
  '4.5..1..6',
  '7..6.3..4',
  '..1.9.65.',
  '.46.17.95',
].flatMap((row) => [...row].map((value) => (value === '.' ? null : Number(value))))

function exactCandidateMask(values: Array<number | null>, row: number, col: number) {
  if (values[row * 9 + col] != null) return 0
  let mask = 0x03fe
  for (let index = 0; index < 9; index += 1) {
    const rowValue = values[row * 9 + index]
    const colValue = values[index * 9 + col]
    if (rowValue) mask &= ~(1 << rowValue)
    if (colValue) mask &= ~(1 << colValue)
  }
  const boxRow = Math.floor(row / 3) * 3
  const boxCol = Math.floor(col / 3) * 3
  for (let r = boxRow; r < boxRow + 3; r += 1) {
    for (let c = boxCol; c < boxCol + 3; c += 1) {
      const value = values[r * 9 + c]
      if (value) mask &= ~(1 << value)
    }
  }
  return mask
}

export const initialCandidateMasks = Array.from(
  { length: 81 },
  (_, index) => exactCandidateMask(initialValues, Math.floor(index / 9), index % 9),
)

const rectangularRegion = (id: string, label: string, family: 'block' | 'window', startRow: number, startCol: number) => ({
  id,
  label,
  family,
  cells: Array.from({ length: 9 }, (_, index) => cell(startRow + Math.floor(index / 3), startCol + index % 3)),
})

export const boardTopology: BoardTopology = {
  regions: [
    ...Array.from({ length: 9 }, (_, index) => rectangularRegion(`box-${index + 1}`, `Classic box ${index + 1}`, 'block', Math.floor(index / 3) * 3, (index % 3) * 3)),
  ],
  paths: [],
}

const groupedStrongLinks: HintPresentation = {
  id: 'grouped-4-strong-links',
  technique: 'Grouped 4 Strong links 20121',
  shortName: 'Grouped 4 Strong links',
  type: 'Elimination (2)',
  rating: 6.1,
  count: 3,
  affects: [c(4, 3, 2), c(7, 5, 2)],
  chainCount: 2,
  views: [
    {
      id: 'chain-1',
      label: 'Chain 1',
      candidateMarks: [
        { candidate: c(0, 1, 2), roles: ['selected'] },
        { candidate: c(0, 6, 2), roles: ['positive'] },
        { candidate: c(4, 6, 2), roles: ['positive'] },
        { candidate: c(4, 6, 2), roles: ['negative'] },
        { candidate: c(6, 6, 2), roles: ['positive'] },
        { candidate: c(6, 4, 2), roles: ['selected'] },
        { candidate: c(4, 3, 2), roles: ['negative', 'conclusion'] },
        { candidate: c(7, 5, 2), roles: ['negative', 'conclusion'] },
      ],
      cellMarks: [
        { cell: cell(0, 1), roles: ['pattern', 'primary'] },
        { cell: cell(6, 4), roles: ['selected'] },
        { cell: cell(4, 3), roles: ['negative', 'conclusion'] },
        { cell: cell(7, 5), roles: ['negative', 'conclusion'] },
      ],
      regions: [
        {
          id: 'group-r1',
          label: 'Grouped row segment',
          roles: ['pattern', 'primary'],
          cells: [cell(0, 1), cell(0, 2), cell(0, 3), cell(0, 4), cell(0, 5), cell(0, 6)],
        },
        {
          id: 'cover',
          label: 'Common cover',
          roles: ['auxiliary', 'secondary'],
          cells: [cell(4, 3), cell(6, 4), cell(7, 5)],
        },
      ],
      links: [
        { id: 'l1', from: p(0, 1, 2), to: p(0, 6, 2), role: 'grouped', direction: 'forward' },
        { id: 'l2', from: p(0, 6, 2), to: p(4, 6, 2), role: 'strong-true', direction: 'forward' },
        { id: 'l3', from: p(4, 6, 2), to: p(6, 6, 2), role: 'strong-false', direction: 'forward' },
        { id: 'l4', from: p(6, 6, 2), to: p(6, 4, 2), role: 'grouped', direction: 'forward' },
      ],
      chainCells: [c(0, 1, 2), c(0, 6, 2), c(4, 6, 2), c(6, 6, 2), c(6, 4, 2)],
    },
    {
      id: 'chain-2',
      label: 'Chain 2',
      candidateMarks: [
        { candidate: c(0, 1, 2), roles: ['selected'] },
        { candidate: c(3, 1, 2), roles: ['positive'] },
        { candidate: c(5, 1, 2), roles: ['auxiliary'] },
        { candidate: c(6, 2, 2), roles: ['positive'] },
        { candidate: c(6, 4, 2), roles: ['selected'] },
        { candidate: c(4, 3, 2), roles: ['negative', 'conclusion'] },
        { candidate: c(7, 5, 2), roles: ['negative', 'conclusion'] },
      ],
      cellMarks: [
        { cell: cell(0, 1), roles: ['pattern', 'primary'] },
        { cell: cell(6, 4), roles: ['selected'] },
        { cell: cell(4, 3), roles: ['negative', 'conclusion'] },
        { cell: cell(7, 5), roles: ['negative', 'conclusion'] },
      ],
      regions: [
        {
          id: 'column-passage',
          label: 'Column passage',
          roles: ['pattern', 'primary'],
          cells: [cell(0, 1), cell(1, 1), cell(2, 1), cell(3, 1), cell(4, 1), cell(5, 1)],
        },
      ],
      links: [
        { id: 'l5', from: p(0, 1, 2), to: p(3, 1, 2), role: 'grouped', direction: 'forward' },
        { id: 'l6', from: p(3, 1, 2), to: p(5, 1, 2), role: 'strong-true', direction: 'forward' },
        { id: 'l7', from: p(5, 1, 2), to: p(6, 2, 2), role: 'strong-false', direction: 'forward' },
        { id: 'l8', from: p(6, 2, 2), to: p(6, 4, 2), role: 'grouped', direction: 'forward' },
      ],
      chainCells: [c(0, 1, 2), c(3, 1, 2), c(5, 1, 2), c(6, 2, 2), c(6, 4, 2)],
    },
    {
      id: 'common-cover',
      label: 'Common cover',
      candidateMarks: [
        { candidate: c(0, 1, 2), roles: ['positive'] },
        { candidate: c(6, 4, 2), roles: ['positive'] },
        { candidate: c(4, 3, 2), roles: ['negative', 'conclusion'] },
        { candidate: c(7, 5, 2), roles: ['negative', 'conclusion'] },
      ],
      cellMarks: [
        { cell: cell(0, 1), roles: ['auxiliary', 'secondary'] },
        { cell: cell(6, 4), roles: ['auxiliary', 'secondary'] },
        { cell: cell(4, 3), roles: ['negative', 'conclusion'] },
        { cell: cell(7, 5), roles: ['negative', 'conclusion'] },
      ],
      regions: [
        {
          id: 'common-cover-cells',
          label: 'Cells seeing both ends',
          roles: ['negative', 'conclusion'],
          cells: [cell(4, 3), cell(7, 5)],
        },
      ],
      links: [],
      chainCells: [c(0, 1, 2), c(6, 4, 2)],
    },
  ],
}

export const hintRows: HintRow[] = [
  { id: 'forcing', label: 'Forcing chains', count: 0, rating: 0, indent: 0, group: true, expanded: true },
  { id: 'grouped', label: 'Grouped strong links', count: 0, rating: 0, indent: 1, group: true, expanded: true },
  { id: groupedStrongLinks.id, label: groupedStrongLinks.technique, count: 3, rating: 6.1, indent: 2, presentation: groupedStrongLinks },
  { id: 'grouped-3', label: 'Grouped 3 Strong links', count: 8, rating: 5.2, indent: 2 },
  { id: 'x-chain', label: 'X-Chain', count: 7, rating: 5.0, indent: 2 },
  { id: 'xy-chain', label: 'XY-Chain', count: 12, rating: 4.7, indent: 2 },
  { id: 'remote-pairs', label: 'Remote Pairs', count: 9, rating: 4.3, indent: 2 },
  { id: 'als', label: 'ALS', count: 23, rating: 3.9, indent: 1, group: true },
  { id: 'bivalue', label: 'Bivalue chains', count: 18, rating: 3.6, indent: 1, group: true },
  { id: 'fish', label: 'Fish', count: 16, rating: 3.2, indent: 1, group: true },
  { id: 'simple', label: 'Simple logic', count: 54, rating: 2.1, indent: 1, group: true },
  { id: 'brute', label: 'Brute force', count: 2, rating: 1.0, indent: 1, group: true },
]

export const primaryHint = groupedStrongLinks
