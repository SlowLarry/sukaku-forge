import type { BoardSnapshot, CandidateRef, CellRef } from './model'

export interface HistoryState {
  present: BoardSnapshot
  past: BoardSnapshot[]
}

export type BoardAction =
  | { type: 'set-value'; cell: CellRef; value: number | null }
  | { type: 'toggle-candidate'; candidate: CandidateRef }
  | { type: 'apply-hint'; eliminations: CandidateRef[] }
  | { type: 'undo' }

const commit = (state: HistoryState, next: BoardSnapshot): HistoryState => ({
  present: next,
  past: [...state.past, state.present],
})

export function boardReducer(state: HistoryState, action: BoardAction): HistoryState {
  if (action.type === 'undo') {
    const previous = state.past.at(-1)
    return previous ? { present: previous, past: state.past.slice(0, -1) } : state
  }
  if (action.type === 'set-value') {
    const index = action.cell.row * 9 + action.cell.col
    if (state.present.values[index] === action.value) return state
    const values = [...state.present.values]
    const candidateMasks = [...state.present.candidateMasks]
    values[index] = action.value
    candidateMasks[index] = action.value == null ? 0x03fe : 0
    return commit(state, { values, candidateMasks })
  }
  if (action.type === 'toggle-candidate') {
    const index = action.candidate.row * 9 + action.candidate.col
    const candidateMasks = [...state.present.candidateMasks]
    candidateMasks[index] = (candidateMasks[index] ?? 0) ^ (1 << action.candidate.digit)
    return commit(state, { ...state.present, candidateMasks })
  }
  const candidateMasks = [...state.present.candidateMasks]
  let changed = false
  action.eliminations.forEach((candidate) => {
    const index = candidate.row * 9 + candidate.col
    const next = (candidateMasks[index] ?? 0) & ~(1 << candidate.digit)
    changed ||= next !== candidateMasks[index]
    candidateMasks[index] = next
  })
  return changed ? commit(state, { ...state.present, candidateMasks }) : state
}
