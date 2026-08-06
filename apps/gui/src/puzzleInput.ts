export const BLANK_PUZZLE = '.'.repeat(81)

const VALUE_GRID_PATTERN = /^[1-9.0]{81}$/

export type NormalizedValueGrid =
  | { ok: true; puzzle: string }
  | { ok: false; message: string }

export function normalizeValueGrid(input: string): NormalizedValueGrid {
  const value = input.trim()
  if (value.length !== 81) {
    return {
      ok: false,
      message: `Enter exactly 81 characters; the current input has ${value.length}.`,
    }
  }
  if (!VALUE_GRID_PATTERN.test(value)) {
    return {
      ok: false,
      message: 'Use only digits 1–9, with . or 0 for empty cells.',
    }
  }
  return { ok: true, puzzle: value.replaceAll('0', '.') }
}

export function canonicalValueGrid(input: string): string {
  const normalized = normalizeValueGrid(input)
  return normalized.ok ? normalized.puzzle : input.trim()
}
