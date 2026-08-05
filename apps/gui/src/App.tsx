import { useEffect, useMemo, useRef, useState } from 'react'
import type { ApplicationPort, HintEffectsDto } from './applicationPort'
import { Board } from './components/Board'
import { Explanation } from './components/Explanation'
import { HintBrowser, type HintBrowserItem } from './components/HintBrowser'
import { StatusBar, type StatusState } from './components/StatusBar'
import { Toolbar } from './components/Toolbar'
import { ViewTabs } from './components/ViewTabs'
import type { BoardTopology, CellRef, HintPresentation, HintView } from './model'
import { cellName } from './model'
import { type HintResult, type SessionControllerView, useSessionController } from './sessionController'

export const DEFAULT_PUZZLE = '53..7....6..195....98....6.8...6...34..8.3..17...2...6.6....28....419..5....8..79'

const emptyHintView: HintView = {
  id: 'no-hint-view',
  label: 'No hint selected',
  candidateMarks: [],
  cellMarks: [],
  regions: [],
  links: [],
  chainCells: [],
}

interface AppProps {
  port: ApplicationPort
  initialPuzzle?: string
}

interface SessionWorkspaceProps {
  session: SessionControllerView
}

interface StatusDescriptor {
  state: StatusState
  message: string
}

const titleFromKey = (value: string) => value
  .split('_')
  .filter(Boolean)
  .map((word) => `${word[0]?.toLocaleUpperCase() ?? ''}${word.slice(1)}`)
  .join(' ')

const pendingMessage = (command: SessionControllerView['pendingCommand']) => {
  switch (command) {
    case 'create_session': return 'Creating puzzle session…'
    case 'next_hint': return 'Finding the next hint…'
    case 'apply_hint': return 'Applying the active hint…'
    case 'place_value': return 'Placing value…'
    case 'toggle_candidate': return 'Updating candidate…'
    case 'undo': return 'Undoing the last change…'
    case 'redo': return 'Redoing the last change…'
    default: return 'Working…'
  }
}

const statusDescriptor = (session: SessionControllerView, localNotice: string | null): StatusDescriptor => {
  if (session.error) return { state: 'error', message: `${session.error.code}: ${session.error.message}` }
  if (session.busy) return { state: 'running', message: pendingMessage(session.pendingCommand) }
  if (localNotice) return { state: 'idle', message: localNotice }
  if (!session.snapshot) return { state: 'idle', message: 'Waiting to create puzzle session…' }

  switch (session.hintResult?.kind) {
    case 'presented':
      return { state: 'idle', message: `${session.hint?.technique ?? 'Hint'} is ready to inspect.` }
    case 'unsupported':
      return {
        state: 'idle',
        message: `${titleFromKey(session.hintResult.unsupported.technique_key)} is applicable, but its visual proof is unavailable.`,
      }
    case 'none':
      return { state: 'idle', message: 'No applicable next hint was found.' }
    case 'incomplete':
      return { state: 'idle', message: `Hint search incomplete: ${session.hintResult.gap.message}` }
    default:
      return { state: 'idle', message: `Ready at revision ${session.snapshot.revision ?? 'unknown'}.` }
  }
}

const variantLabel = (topology: BoardTopology | null) => {
  const variant = topology?.variant
  if (!variant) return '9×9 Sudoku'

  const additions = [
    variant.disjointGroups && 'Disjoint groups',
    variant.windows && 'Windows',
    variant.sudokuX && 'Sudoku X',
    variant.girandola && 'Girandola',
    variant.asterisk && 'Asterisk',
    variant.centerDot && 'Center dot',
    variant.antiFerz && 'Anti-ferz',
    variant.antiKnight && 'Anti-knight',
    variant.toroidal && 'Toroidal',
    variant.nonConsecutive !== 'off' && 'Non-consecutive',
    variant.forbiddenPairs && 'Forbidden pairs',
  ].filter((label): label is string => Boolean(label))

  if (variant.blocks && additions.length === 0) return 'Classic Sudoku'
  return additions.length === 0 ? 'Latin 9×9' : additions.join(' + ')
}

const effectSummary = (effects: HintEffectsDto) => {
  const placement = effects.placement == null
    ? null
    : `${cellName({ row: Math.floor(effects.placement.cell / 9), col: effects.placement.cell % 9 })} = ${effects.placement.digit}`
  const eliminations = `${effects.elimination_count} elimination${effects.elimination_count === 1 ? '' : 's'}`
  return placement == null ? eliminations : effects.elimination_count === 0 ? placement : `${placement} · ${eliminations}`
}

const hintItems = (hint: HintPresentation | null, result: HintResult | null): HintBrowserItem[] => {
  if (hint && result?.kind === 'presented') {
    return [{
      id: hint.id,
      label: hint.technique,
      detail: `${hint.type} · ${hint.views.length} proof view${hint.views.length === 1 ? '' : 's'}`,
      kind: 'presented',
      rating: hint.rating,
    }]
  }
  if (result?.kind === 'unsupported') {
    return [{
      id: result.hintId,
      label: titleFromKey(result.unsupported.technique_key),
      detail: `${titleFromKey(result.unsupported.kind)} · ${effectSummary(result.effects)}`,
      kind: 'unsupported',
    }]
  }
  return []
}

const emptyHintMessage = (result: HintResult | null) => {
  if (result?.kind === 'none') return 'The engine reports no applicable next hint.'
  if (result?.kind === 'incomplete') return result.gap.message
  return 'Request the next hint to inspect its presentation and effects.'
}

function HintOutcomeDetails({ hint, result }: { hint: HintPresentation | null; result: HintResult | null }) {
  if (hint && result?.kind === 'presented') {
    const effects = [
      hint.placement ? `${cellName(hint.placement)} = ${hint.placement.digit}` : null,
      hint.affects.length > 0
        ? hint.affects.map((candidate) => `${cellName(candidate)}(${candidate.digit})`).join(', ')
        : null,
    ].filter((effect): effect is string => effect != null)
    const affected = effects.join(' · ') || 'None'
    return (
      <section className="hint-details" aria-labelledby="hint-details-title">
        <div className="details-title-row">
          <h2 id="hint-details-title">Hint details</h2>
          <span>Hint rating <strong>{hint.rating.toFixed(1)}</strong></span>
        </div>
        <dl>
          <dt>Technique</dt><dd>{hint.technique}</dd>
          <dt>Type</dt><dd>{hint.type}</dd>
          <dt>Affects</dt><dd>{affected}</dd>
          <dt>Proof</dt><dd>{hint.views.length} proof view{hint.views.length === 1 ? '' : 's'}</dd>
        </dl>
      </section>
    )
  }

  if (result?.kind === 'unsupported') {
    return (
      <section className="hint-details" aria-labelledby="hint-details-title">
        <div className="details-title-row">
          <h2 id="hint-details-title">Hint details</h2>
          <span className="unsupported-label">Presentation unavailable</span>
        </div>
        <dl>
          <dt>Technique</dt><dd>{titleFromKey(result.unsupported.technique_key)}</dd>
          <dt>Effect</dt><dd>{effectSummary(result.effects)}</dd>
          <dt>Proof</dt><dd>{titleFromKey(result.unsupported.kind)}</dd>
          <dt>Action</dt><dd>The server-owned hint can still be applied safely.</dd>
        </dl>
      </section>
    )
  }

  const heading = result?.kind === 'none'
    ? 'No applicable hint'
    : result?.kind === 'incomplete' ? 'Hint search incomplete' : 'No hint requested'
  return (
    <section className="hint-details hint-details--empty" aria-labelledby="hint-details-title">
      <div className="details-title-row"><h2 id="hint-details-title">{heading}</h2></div>
      <p>{emptyHintMessage(result)}</p>
    </section>
  )
}

export function SessionWorkspace({ session }: SessionWorkspaceProps) {
  const [selectedCell, setSelectedCell] = useState<CellRef | null>(null)
  const [selectedViewId, setSelectedViewId] = useState('')
  const [candidatesVisible, setCandidatesVisible] = useState(true)
  const [candidateEntry, setCandidateEntry] = useState(false)
  const [localNotice, setLocalNotice] = useState<string | null>(null)

  const selectedView = session.hint?.views.find((view) => view.id === selectedViewId)
    ?? session.hint?.views[0]
    ?? null
  const items = useMemo(
    () => hintItems(session.hint, session.hintResult),
    [session.hint, session.hintResult],
  )
  const status = statusDescriptor(session, localNotice)
  const sessionReady = session.snapshot != null && session.topology != null
  const canApply = session.hintResult?.kind === 'presented' || session.hintResult?.kind === 'unsupported'
  const clueCount = session.snapshot?.givens?.filter(Boolean).length

  const run = (action: () => Promise<void>) => {
    if (session.busy) return
    setLocalNotice(null)
    void action()
  }

  const handleBoardKey = (event: React.KeyboardEvent) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return
    const moveKeys: Record<string, CellRef> = {
      ArrowUp: { row: -1, col: 0 },
      ArrowDown: { row: 1, col: 0 },
      ArrowLeft: { row: 0, col: -1 },
      ArrowRight: { row: 0, col: 1 },
    }
    const movement = moveKeys[event.key]
    if (movement) {
      event.preventDefault()
      const origin = selectedCell ?? { row: 4, col: 4 }
      setSelectedCell({
        row: Math.max(0, Math.min(8, origin.row + movement.row)),
        col: Math.max(0, Math.min(8, origin.col + movement.col)),
      })
      return
    }
    if (event.key === 'Escape') {
      event.preventDefault()
      setSelectedCell(null)
      setLocalNotice('Cell selection cleared.')
      return
    }
    if (event.key.toLocaleLowerCase() === 'm') {
      event.preventDefault()
      if (sessionReady && !session.busy) setCandidateEntry((current) => !current)
      return
    }
    if (!selectedCell || !session.snapshot) return

    if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault()
      setLocalNotice('Clearing a value is not available in protocol v2.')
      return
    }
    if (!/^[1-9]$/.test(event.key)) return

    event.preventDefault()
    if (session.busy) return
    const index = selectedCell.row * 9 + selectedCell.col
    if (session.snapshot.givens?.[index]) {
      setLocalNotice(`${cellName(selectedCell)} is a given and cannot be edited.`)
      return
    }

    const digit = Number(event.key)
    if (candidateEntry) {
      if (session.snapshot.values[index] != null) {
        setLocalNotice(`Candidates can only be edited in an empty cell.`)
        return
      }
      run(() => session.toggleCandidate({ ...selectedCell, digit }))
    } else {
      if (session.snapshot.values[index] != null) {
        setLocalNotice(`${cellName(selectedCell)} already has a value; use Undo to revert an entry.`)
        return
      }
      if (((session.snapshot.candidateMasks[index] ?? 0) & (1 << digit)) === 0) {
        setLocalNotice(`Candidate ${digit} is not available in ${cellName(selectedCell)}.`)
        return
      }
      run(() => session.placeValue(selectedCell, digit))
    }
  }

  return (
    <div className="app-shell">
      <Toolbar
        busy={session.busy}
        sessionReady={sessionReady}
        canUndo={Boolean(session.snapshot?.canUndo)}
        canRedo={Boolean(session.snapshot?.canRedo)}
        canRequestHint={session.snapshot != null}
        canApply={canApply}
        candidatesVisible={candidatesVisible}
        candidateEntry={candidateEntry}
        variantLabel={variantLabel(session.topology)}
        onUndo={() => run(session.undo)}
        onRedo={() => run(session.redo)}
        onRequestHint={() => run(session.nextHint)}
        onToggleCandidates={() => setCandidatesVisible((current) => !current)}
        onToggleCandidateEntry={() => setCandidateEntry((current) => !current)}
        onApply={() => run(session.applyHint)}
      />
      <main className="workspace" aria-busy={session.busy}>
        <div className="board-pane">
          {session.snapshot && session.topology ? (
            <Board
              board={session.snapshot}
              topology={session.topology}
              view={selectedView ?? emptyHintView}
              selected={selectedCell}
              candidatesVisible={candidatesVisible}
              onSelect={setSelectedCell}
              onKeyDown={handleBoardKey}
            />
          ) : (
            <section className="session-placeholder" aria-label="Puzzle session unavailable">
              <span className="session-placeholder-mark" aria-hidden="true">9×9</span>
              <h1>{session.error ? 'Puzzle session could not start' : 'Starting puzzle session'}</h1>
              <p>{session.error?.message ?? 'The board will appear after the engine returns its first authoritative snapshot.'}</p>
            </section>
          )}
        </div>
        <aside className="inspector-pane">
          <ViewTabs
            views={session.hint?.views ?? []}
            selectedId={selectedView?.id ?? ''}
            onSelect={setSelectedViewId}
          >
            <HintBrowser
              items={items}
              selectedId={items[0]?.id ?? null}
              emptyMessage={emptyHintMessage(session.hintResult)}
            />
            <HintOutcomeDetails hint={session.hint} result={session.hintResult} />
          </ViewTabs>
        </aside>
        <Explanation hint={session.hint} view={selectedView} applied={false} />
      </main>
      <StatusBar
        state={status.state}
        message={status.message}
        revision={session.snapshot?.revision}
        clueCount={clueCount}
      />
    </div>
  )
}

export default function App({ port, initialPuzzle = DEFAULT_PUZZLE }: AppProps) {
  const session = useSessionController(port)
  const createSession = session.createSession
  const startedSession = useRef<{ port: ApplicationPort; puzzle: string } | null>(null)

  useEffect(() => {
    if (startedSession.current?.port === port && startedSession.current.puzzle === initialPuzzle) return
    startedSession.current = { port, puzzle: initialPuzzle }
    void createSession(initialPuzzle)
  }, [createSession, initialPuzzle, port])

  return <SessionWorkspace session={session} />
}
