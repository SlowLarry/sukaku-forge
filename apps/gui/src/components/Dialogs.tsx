import { useEffect, useId, useRef, useState } from 'react'
import { Info, Upload, X } from 'lucide-react'
import { normalizeValueGrid } from '../puzzleInput'

interface DialogFrameProps {
  title: string
  description: string
  onClose: () => void
  children: React.ReactNode
  initialFocus: React.RefObject<HTMLElement | null>
  returnFocus?: HTMLElement | null
}

function DialogFrame({ title, description, onClose, children, initialFocus, returnFocus }: DialogFrameProps) {
  const titleId = useId()
  const descriptionId = useId()

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const restoreTarget = returnFocus ?? previousFocus
    initialFocus.current?.focus()
    return () => restoreTarget?.focus()
  }, [initialFocus, returnFocus])

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      onClose()
      return
    }
    if (event.key !== 'Tab') return

    const focusable = Array.from(event.currentTarget.querySelectorAll<HTMLElement>(
      'button:not(:disabled), textarea:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])',
    ))
    if (focusable.length === 0) return
    const first = focusable[0]!
    const last = focusable.at(-1)!
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault()
      last.focus()
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault()
      first.focus()
    }
  }

  return (
    <div
      className="dialog-backdrop"
      onPointerDown={(event) => { if (event.target === event.currentTarget) onClose() }}
    >
      <div
        className="app-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        onKeyDown={handleKeyDown}
      >
        <header className="dialog-heading">
          <div>
            <h2 id={titleId}>{title}</h2>
            <p id={descriptionId}>{description}</p>
          </div>
          <button className="dialog-close" type="button" onClick={onClose} aria-label={`Close ${title}`}>
            <X aria-hidden="true" />
          </button>
        </header>
        {children}
      </div>
    </div>
  )
}

interface ImportPuzzleDialogProps {
  onClose: () => void
  onImport: (puzzle: string) => void
  returnFocus?: HTMLElement | null
}

export function ImportPuzzleDialog({ onClose, onImport, returnFocus }: ImportPuzzleDialogProps) {
  const [value, setValue] = useState('')
  const [error, setError] = useState<string | null>(null)
  const input = useRef<HTMLTextAreaElement>(null)
  const inputId = useId()
  const trimmedLength = value.trim().length

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault()
    const normalized = normalizeValueGrid(value)
    if (!normalized.ok) {
      setError(normalized.message)
      return
    }
    onImport(normalized.puzzle)
  }

  return (
    <DialogFrame
      title="Import value grid"
      description="Start a new session from a single 81-character Sudoku string."
      onClose={onClose}
      initialFocus={input}
      returnFocus={returnFocus}
    >
      <form className="dialog-form" onSubmit={handleSubmit} noValidate>
        <label htmlFor={inputId}>81-character puzzle</label>
        <textarea
          ref={input}
          id={inputId}
          value={value}
          onChange={(event) => {
            setValue(event.currentTarget.value)
            setError(null)
          }}
          rows={4}
          spellCheck={false}
          autoCapitalize="off"
          autoComplete="off"
          aria-invalid={error != null}
          aria-describedby={`${inputId}-help${error ? ` ${inputId}-error` : ''}`}
          placeholder="53..7....6..195..."
        />
        <div className="dialog-field-meta" id={`${inputId}-help`}>
          <span>Digits 1–9; use <kbd>.</kbd> or <kbd>0</kbd> for empty cells.</span>
          <span className={trimmedLength === 81 ? 'is-complete' : undefined}>{trimmedLength} / 81</span>
        </div>
        {error && <p className="dialog-error" id={`${inputId}-error`} role="alert">{error}</p>}
        <footer className="dialog-actions">
          <button type="button" className="secondary-button" onClick={onClose}>Cancel</button>
          <button type="submit" className="primary-button"><Upload aria-hidden="true" /> <span>Import puzzle</span></button>
        </footer>
      </form>
    </DialogFrame>
  )
}

export function AboutDialog({ onClose, returnFocus }: { onClose: () => void; returnFocus?: HTMLElement | null }) {
  const closeButton = useRef<HTMLButtonElement>(null)
  return (
    <DialogFrame
      title="About Sukaku Forge"
      description="A modern, shared desktop and browser interface for the Sukaku Explainer engine."
      onClose={onClose}
      initialFocus={closeButton}
      returnFocus={returnFocus}
    >
      <div className="about-copy">
        <Info aria-hidden="true" />
        <p>
          Sukaku Forge keeps solver state authoritative in Rust while React renders the board,
          explanations, and semantic hint visualizations.
        </p>
      </div>
      <footer className="dialog-actions">
        <button ref={closeButton} type="button" className="primary-button" onClick={onClose}>Close</button>
      </footer>
    </DialogFrame>
  )
}
