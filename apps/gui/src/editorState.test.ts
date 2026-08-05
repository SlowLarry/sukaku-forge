import { describe, expect, it } from 'vitest'
import { boardReducer, type HistoryState } from './editorState'

const initial: HistoryState = {
  present: { values: Array(81).fill(null), candidateMasks: Array(81).fill(0x03fe) },
  past: [],
}

describe('boardReducer', () => {
  it('applies an elimination and restores the exact snapshot on undo', () => {
    const applied = boardReducer(initial, {
      type: 'apply-hint',
      eliminations: [{ row: 4, col: 3, digit: 2 }, { row: 7, col: 5, digit: 2 }],
    })
    expect(applied.present.candidateMasks[39]! & (1 << 2)).toBe(0)
    expect(applied.present.candidateMasks[68]! & (1 << 2)).toBe(0)
    expect(applied.past).toHaveLength(1)
    expect(boardReducer(applied, { type: 'undo' })).toEqual(initial)
  })

  it('commits keyboard-style value edits', () => {
    const edited = boardReducer(initial, { type: 'set-value', cell: { row: 2, col: 4 }, value: 7 })
    expect(edited.present.values[22]).toBe(7)
    expect(edited.past).toHaveLength(1)
  })

  it('uses the legacy wire bits at both digit boundaries', () => {
    const withoutOne = boardReducer(initial, { type: 'toggle-candidate', candidate: { row: 0, col: 0, digit: 1 } })
    expect(withoutOne.present.candidateMasks[0]! & (1 << 1)).toBe(0)
    expect(withoutOne.present.candidateMasks[0]! & (1 << 9)).not.toBe(0)
    const withoutNine = boardReducer(withoutOne, { type: 'toggle-candidate', candidate: { row: 0, col: 0, digit: 9 } })
    expect(withoutNine.present.candidateMasks[0]! & (1 << 9)).toBe(0)
  })
})
