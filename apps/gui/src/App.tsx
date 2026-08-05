import { useReducer, useState } from 'react'
import { Board } from './components/Board'
import { Explanation } from './components/Explanation'
import { HintBrowser } from './components/HintBrowser'
import { HintDetails } from './components/HintDetails'
import { StatusBar } from './components/StatusBar'
import { Toolbar } from './components/Toolbar'
import { ViewTabs } from './components/ViewTabs'
import { boardReducer } from './editorState'
import { boardTopology, hintRows, initialCandidateMasks, initialValues } from './fixture'
import type { CellRef, HintPresentation, HintRow, HintView } from './model'

const emptyHintView: HintView = {
  id: 'no-hint-view',
  label: 'No hint selected',
  candidateMarks: [],
  cellMarks: [],
  regions: [],
  links: [],
  chainCells: [],
}

const emptyHintPresentation: HintPresentation = {
  id: 'no-hint',
  technique: 'No hint selected',
  shortName: 'No hint selected',
  type: 'Select an individual hint',
  rating: 0,
  count: 0,
  affects: [],
  chainCount: 0,
  views: [],
}

interface AppProps {
  initialHintId?: string
}

export default function App({ initialHintId }: AppProps = {}) {
  const initialHint = hintRows.find((row) => row.id === initialHintId)
    ?? hintRows.find((row) => row.presentation)
    ?? hintRows[0]!
  const [history, dispatch] = useReducer(boardReducer, {
    present: { values: initialValues, candidateMasks: initialCandidateMasks },
    past: [],
  })
  const [selectedCell, setSelectedCell] = useState<CellRef | null>({ row: 6, col: 4 })
  const [selectedHint, setSelectedHint] = useState<HintRow>(initialHint)
  const [selectedViewId, setSelectedViewId] = useState(initialHint.presentation?.views[0]?.id ?? '')
  const [candidatesVisible, setCandidatesVisible] = useState(true)
  const [candidateEntry, setCandidateEntry] = useState(false)
  const [status, setStatus] = useState('Ready')
  const selectedPresentation = selectedHint.presentation ?? null
  const selectedView = selectedPresentation?.views.find((view) => view.id === selectedViewId)
    ?? selectedPresentation?.views[0]
    ?? null
  const applied = selectedPresentation != null
    && selectedPresentation.affects.length > 0
    && selectedPresentation.affects.every((candidate) => ((history.present.candidateMasks[candidate.row * 9 + candidate.col] ?? 0) & (1 << candidate.digit)) === 0)

  const selectHint = (row: HintRow) => {
    setSelectedHint(row)
    if (row.presentation) {
      setSelectedViewId(row.presentation.views[0]?.id ?? '')
      setStatus(`${row.count} matching hints loaded`)
    } else {
      setSelectedViewId('')
      setStatus(row.group ? `${row.label} technique family selected` : `${row.count} ${row.label} hints available`)
    }
  }

  const applyHint = () => {
    if (!selectedPresentation) {
      setStatus('Select an individual hint before applying it')
      return
    }
    dispatch({ type: 'apply-hint', eliminations: selectedPresentation.affects })
    const count = selectedPresentation.affects.length
    setStatus(applied ? 'Hint is already applied' : `Applied ${count} candidate elimination${count === 1 ? '' : 's'}`)
  }

  const undo = () => {
    dispatch({ type: 'undo' })
    setStatus('Last board change undone')
  }

  const handleBoardKey = (event: React.KeyboardEvent) => {
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
      setStatus('Cell selection cleared')
      return
    }
    if (event.key.toLowerCase() === 'm') {
      event.preventDefault()
      setCandidateEntry((current) => !current)
      return
    }
    if (!selectedCell) return
    if (/^[1-9]$/.test(event.key)) {
      event.preventDefault()
      const digit = Number(event.key)
      if (candidateEntry) {
        dispatch({ type: 'toggle-candidate', candidate: { ...selectedCell, digit } })
        setStatus(`Candidate ${digit} toggled in r${selectedCell.row + 1}c${selectedCell.col + 1}`)
      } else {
        dispatch({ type: 'set-value', cell: selectedCell, value: digit })
        setStatus(`Value ${digit} entered in r${selectedCell.row + 1}c${selectedCell.col + 1}`)
      }
      return
    }
    if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault()
      dispatch({ type: 'set-value', cell: selectedCell, value: null })
      setStatus(`Cleared r${selectedCell.row + 1}c${selectedCell.col + 1}`)
    }
  }

  return (
    <div className="app-shell">
      <Toolbar
        canUndo={history.past.length > 0}
        canApply={selectedPresentation != null && selectedPresentation.affects.length > 0}
        candidatesVisible={candidatesVisible}
        candidateEntry={candidateEntry}
        onUndo={undo}
        onToggleCandidates={() => setCandidatesVisible((current) => !current)}
        onToggleCandidateEntry={() => setCandidateEntry((current) => !current)}
        onApply={applyHint}
      />
      <main className="workspace">
        <div className="board-pane">
          <Board
            board={history.present}
            topology={boardTopology}
            view={selectedView ?? emptyHintView}
            selected={selectedCell}
            candidatesVisible={candidatesVisible}
            onSelect={setSelectedCell}
            onKeyDown={handleBoardKey}
          />
        </div>
        <aside className="inspector-pane">
          <ViewTabs views={selectedPresentation?.views ?? []} selectedId={selectedView?.id ?? ''} onSelect={setSelectedViewId} />
          <HintBrowser rows={hintRows} selectedId={selectedHint.id} onSelect={selectHint} />
          <HintDetails hint={selectedPresentation ?? emptyHintPresentation} selectedRow={selectedHint} />
        </aside>
        <Explanation hint={selectedPresentation} view={selectedView} applied={applied} />
      </main>
      <StatusBar message={status} progress={68} />
    </div>
  )
}
