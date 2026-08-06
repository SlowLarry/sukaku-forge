import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import App, { DEFAULT_PUZZLE, SessionWorkspace } from './App'
import type { ApplicationPort } from './applicationPort'
import { boardTopology, initialCandidateMasks, initialValues, primaryHint } from './fixture'
import type { SessionControllerView } from './sessionController'

const idleAction = vi.fn(async () => undefined)
const createAction = vi.fn(async () => true)

const session = (overrides: Partial<SessionControllerView> = {}): SessionControllerView => ({
  snapshot: {
    revision: '7',
    values: initialValues,
    candidateMasks: initialCandidateMasks,
    givens: initialValues.map((value) => value != null),
    canUndo: true,
    canRedo: false,
  },
  topology: boardTopology,
  hint: null,
  hintResult: null,
  busy: false,
  pendingRequestId: null,
  pendingCommand: null,
  error: null,
  createSession: createAction,
  nextHint: idleAction,
  applyHint: idleAction,
  applyAndNext: idleAction,
  placeValue: idleAction,
  toggleCandidate: idleAction,
  undo: idleAction,
  redo: idleAction,
  ...overrides,
})

const inertPort: ApplicationPort = {
  dispatch: async () => { throw new Error('effects do not run during server rendering') },
}

describe('App', () => {
  it('starts from an honest pre-session shell and ships a valid default puzzle', () => {
    const markup = renderToStaticMarkup(<App port={inertPort} />)

    expect(DEFAULT_PUZZLE).toHaveLength(81)
    expect(DEFAULT_PUZZLE).toMatch(/^[1-9.]{81}$/)
    expect([...DEFAULT_PUZZLE].filter((value) => value !== '.')).toHaveLength(30)
    expect(markup).toContain('Starting puzzle session')
    expect(markup).toContain('The board will appear after the engine returns its first authoritative snapshot.')
    expect(markup).not.toContain('aria-label="Sudoku board')
    expect(markup).not.toContain('Grouped 4 Strong links')
  })

  it('renders only the authoritative snapshot and presented hint semantics', () => {
    const mixedEffectHint = {
      ...primaryHint,
      placement: { row: 0, col: 8, digit: 9 },
    }
    const markup = renderToStaticMarkup(<SessionWorkspace session={session({
      hint: mixedEffectHint,
      hintResult: { kind: 'presented', hintId: primaryHint.id },
    })} />)

    expect(markup).toContain('aria-label="Sudoku board')
    expect(markup).toContain('Grouped 4 Strong links 20121')
    expect(markup).toContain('r1c9 = 9 · r5c4(2), r8c6(2)')
    expect(markup).toContain('data-hint-kind="presented"')
    expect(markup).toContain('role="tablist"')
    expect(markup).toContain('role="tab"')
    expect(markup).toContain('role="tabpanel"')
    expect(markup).toContain('Revision <strong>7</strong>')
    expect(markup).toContain('aria-label="Get next hint"')
    expect(markup).toContain('aria-label="Apply active hint"')
    expect(markup).toContain('role="menubar"')
    expect(markup).toContain('>File</button>')
    expect(markup).toContain('>Options</button>')
    expect(markup).not.toContain('Get all hints')
    expect(markup).toContain('Solve step')
    expect(markup).not.toContain('Open a puzzle')
  })

  it('keeps an unsupported server-owned hint applicable without inventing a proof', () => {
    const markup = renderToStaticMarkup(<SessionWorkspace session={session({
      hintResult: {
        kind: 'unsupported',
        hintId: '52',
        unsupported: { technique_key: 'forcing_chain', kind: 'missing_chain_proof' },
        effects: {
          placement: null,
          removals: [{ cell: 40, digits: 1 << 2 }],
          elimination_count: 1,
        },
      },
    })} />)

    expect(markup).toContain('data-hint-kind="unsupported"')
    expect(markup).toContain('Forcing Chain')
    expect(markup).toContain('Missing Chain Proof · 1 elimination')
    expect(markup).toContain('The server-owned hint can still be applied safely.')
    expect(markup).toMatch(/<button class="primary-button"[^>]*aria-label="Apply active hint"/)
    expect(markup).not.toMatch(/<button class="primary-button"[^>]*disabled=""/)
    expect(markup).not.toContain('data-link-role=')
  })

  it('surfaces busy and error states while disabling conflicting commands', () => {
    const busyMarkup = renderToStaticMarkup(<SessionWorkspace session={session({
      busy: true,
      pendingRequestId: 8,
      pendingCommand: 'next_hint',
    })} />)
    const errorMarkup = renderToStaticMarkup(<SessionWorkspace session={session({
      error: { code: 'stale_revision', message: 'expected revision 7', expected_revision: '7', actual_revision: '8' },
    })} />)

    expect(busyMarkup).toContain('data-status-state="running"')
    expect(busyMarkup).toContain('Finding the next hint…')
    expect(busyMarkup).toContain('aria-busy="true"')
    expect(busyMarkup).toMatch(/<button[^>]*disabled=""[^>]*aria-label="Get next hint"/)
    expect(errorMarkup).toContain('data-status-state="error"')
    expect(errorMarkup).toContain('role="alert"')
    expect(errorMarkup).toContain('stale_revision: expected revision 7')
  })

  it('renders none and incomplete outcomes without stale hint content', () => {
    const noneMarkup = renderToStaticMarkup(<SessionWorkspace session={session({ hintResult: { kind: 'none' } })} />)
    const incompleteMarkup = renderToStaticMarkup(<SessionWorkspace session={session({
      hintResult: {
        kind: 'incomplete',
        gap: { code: 'producer_not_ported', message: 'No producer exists for this technique yet.' },
      },
    })} />)

    expect(noneMarkup).toContain('The engine reports no applicable next hint.')
    expect(noneMarkup).toContain('No applicable hint')
    expect(incompleteMarkup).toContain('No producer exists for this technique yet.')
    expect(incompleteMarkup).toContain('Hint search incomplete')
    expect(incompleteMarkup).not.toContain('data-hint-kind=')
  })
})
