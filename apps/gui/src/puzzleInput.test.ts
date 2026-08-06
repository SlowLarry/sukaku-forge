import { describe, expect, it } from 'vitest'
import { BLANK_PUZZLE, canonicalValueGrid, normalizeValueGrid } from './puzzleInput'

describe('value-grid input', () => {
  it('accepts an 81-character grid and canonicalizes zeroes to dots', () => {
    const input = `  ${'123456789'.repeat(8)}123456780  `

    expect(normalizeValueGrid(input)).toEqual({
      ok: true,
      puzzle: `${'123456789'.repeat(8)}12345678.`,
    })
    expect(canonicalValueGrid('0'.repeat(81))).toBe(BLANK_PUZZLE)
  })

  it('reports length and invalid-character errors separately', () => {
    expect(normalizeValueGrid('.'.repeat(80))).toEqual({
      ok: false,
      message: 'Enter exactly 81 characters; the current input has 80.',
    })
    expect(normalizeValueGrid(`${'.'.repeat(80)}x`)).toEqual({
      ok: false,
      message: 'Use only digits 1–9, with . or 0 for empty cells.',
    })
  })
})
