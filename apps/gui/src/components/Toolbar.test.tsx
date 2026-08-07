import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { APP_VERSION } from '../appVersion'
import { Toolbar } from './Toolbar'

const props = {
  busy: false,
  sessionReady: true,
  canUndo: true,
  canRedo: true,
  canRequestHint: true,
  canRequestAllHints: true,
  canApply: true,
  candidatesVisible: true,
  candidateEntry: false,
  variantLabel: 'Classic Sudoku',
  onUndo: vi.fn(),
  onRedo: vi.fn(),
  onRequestHint: vi.fn(),
  onRequestAllHints: vi.fn(),
  onToggleCandidates: vi.fn(),
  onToggleCandidateEntry: vi.fn(),
  onApply: vi.fn(),
  onApplyAndContinue: vi.fn(),
}

describe('Toolbar', () => {
  it('exposes next-hint and all-hints protocol actions', () => {
    const markup = renderToStaticMarkup(<Toolbar {...props} />)

    expect(markup).toContain('>Undo</span>')
    expect(markup).toContain('>Redo</span>')
    expect(markup).toContain('>Next hint</span>')
    expect(markup).toContain('aria-label="Get all hints"')
    expect(markup).toContain('>All hints</span>')
    expect(markup).toContain('>Apply hint</span>')
    expect(markup).toContain('>Solve step</span>')
    expect(markup).toContain('Variant: Classic Sudoku')
    expect(markup).toContain(`v${APP_VERSION}`)
    expect(markup).toContain('aria-label="Show candidates" aria-pressed="true"')
    expect(markup).not.toContain('>New</span>')
  })

  it('disables every engine-mutating action while busy', () => {
    const markup = renderToStaticMarkup(<Toolbar {...props} busy />)

    expect(markup).toContain('aria-busy="true"')
    expect(markup.match(/disabled=""/g)?.length).toBeGreaterThanOrEqual(6)
  })
})
