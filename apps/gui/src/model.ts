export type HighlightRole = 'selected' | 'pattern' | 'positive' | 'negative' | 'auxiliary' | 'conclusion' | 'primary' | 'secondary'
export type LinkRole =
  | 'strong-true'
  | 'strong-false'
  | 'grouped'
  | 'strong'
  | 'weak'
  | 'grouped-strong'
  | 'implication'

export interface CellRef {
  row: number
  col: number
}

export interface CandidateRef extends CellRef {
  digit: number
}

export interface CandidateMark {
  candidate: CandidateRef
  roles: HighlightRole[]
}

export interface CellMark {
  cell: CellRef
  roles: HighlightRole[]
}

export interface RegionMark {
  id: string
  label: string
  cells: CellRef[]
  roles: HighlightRole[]
}

export type LinkEndpoint =
  | { kind: 'candidate'; candidate: CandidateRef }
  | { kind: 'candidate-group'; representative: CandidateRef; members: CandidateRef[] }
  | { kind: 'cell-center'; cell: CellRef }

export type LinkCause =
  | { kind: 'cell' }
  | { kind: 'region'; regionType: number; regionIndex: number }
  | { kind: 'visibility' }
  | { kind: 'derived' }

export interface ChainLink {
  id: string
  from: LinkEndpoint
  to: LinkEndpoint
  role: LinkRole
  direction: 'forward' | 'both'
  cause?: LinkCause
}

export type ExplanationInline =
  | { kind: 'text'; text: string }
  | { kind: 'technique'; techniqueKey: string }
  | { kind: 'cell'; cell: CellRef }
  | { kind: 'digit'; digit: number }
  | { kind: 'region'; regionType: number; regionIndex: number }
  | { kind: 'candidate'; candidate: CandidateRef }

export type ExplanationBlock =
  | { kind: 'paragraph'; inlines: ExplanationInline[] }
  | { kind: 'unordered-list'; items: ExplanationInline[][] }

export interface ExplanationDocument {
  blocks: ExplanationBlock[]
}

export interface HintView {
  id: string
  label: string
  candidateMarks: CandidateMark[]
  cellMarks: CellMark[]
  regions: RegionMark[]
  links: ChainLink[]
  chainCells: CandidateRef[]
}

export interface HintPresentation {
  id: string
  revision?: string
  techniqueKey?: string
  technique: string
  shortName: string
  type: string
  rating: number
  count?: number
  affects: CandidateRef[]
  placement?: CandidateRef
  eliminationCount?: number
  chainCount?: number
  views: HintView[]
  explanation?: ExplanationDocument
}

export interface HintRow {
  id: string
  label: string
  count: number
  rating: number
  indent: number
  group?: boolean
  expanded?: boolean
  presentation?: HintPresentation
}

export interface BoardSnapshot {
  revision?: string
  values: Array<number | null>
  candidateMasks: number[]
  givens?: boolean[]
  canUndo?: boolean
  canRedo?: boolean
}

export interface BoardVariant {
  blocks: boolean
  disjointGroups: boolean
  windows: boolean
  sudokuX: boolean
  girandola: boolean
  asterisk: boolean
  centerDot: boolean
  antiFerz: boolean
  antiKnight: boolean
  toroidal: boolean
  nonConsecutive: 'off' | 'orthogonal' | 'orthogonal-cyclic' | 'diagonal' | 'diagonal-cyclic'
  forbiddenPairs: boolean
}

export interface TopologyRegion {
  id: string
  label: string
  cells: CellRef[]
  family: 'block' | 'row' | 'column' | 'disjoint-group' | 'window' | 'girandola' | 'asterisk' | 'center-dot'
}

export interface TopologyPath {
  id: string
  label: string
  cells: CellRef[]
  family: 'main-diagonal' | 'anti-diagonal'
}

export interface BoardTopology {
  regions: TopologyRegion[]
  paths: TopologyPath[]
  variant?: BoardVariant
}

export const cellKey = ({ row, col }: CellRef) => `${row}:${col}`
export const candidateKey = ({ row, col, digit }: CandidateRef) => `${row}:${col}:${digit}`
export const cellName = ({ row, col }: CellRef) => `r${row + 1}c${col + 1}`

export function sameCell(a: CellRef, b: CellRef): boolean {
  return a.row === b.row && a.col === b.col
}
