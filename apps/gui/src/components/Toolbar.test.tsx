import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { Toolbar } from './Toolbar'

const props = {
  busy: false,
  sessionReady: true,
  canUndo: true,
  canRedo: true,
  canRequestHint: true,
  canApply: true,
  candidatesVisible: true,
  candidateEntry: false,
  variantLabel: 'Classic Sudoku',
  onUndo: vi.fn(),
  onRedo: vi.fn(),
  onRequestHint: vi.fn(),
  onToggleCandidates: vi.fn(),
  onToggleCandidateEntry: vi.fn(),
  onApply: vi.fn(),
  onApplyAndContinue: vi.fn(),
}

describe('Toolbar', () => {
  it('exposes only supported protocol actions', () => {
    const markup = renderToStaticMarkup(<Toolbar {...props} />)

    expect(markup).toContain('>Undo</span>')
    expect(markup).toContain('>Redo</span>')
    expect(markup).toContain('>Next hint</span>')
    expect(markup).toContain('>Apply hint</span>')
    expect(markup).toContain('>Solve step</span>')
    expect(markup).toContain('Variant: Classic Sudoku')
    expect(markup).toContain('aria-label="Show candidates" aria-pressed="true"')
    expect(markup).not.toContain('>New</span>')
    expect(markup).not.toContain('Get all hints')
  })

  it('disables every engine-mutating action while busy', () => {
    const markup = renderToStaticMarkup(<Toolbar {...props} busy />)

    expect(markup).toContain('aria-busy="true"')
    expect(markup.match(/disabled=""/g)?.length).toBeGreaterThanOrEqual(6)
  })
})
