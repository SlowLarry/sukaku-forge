import {
  Check,
  Eye,
  EyeOff,
  ListTree,
  PencilLine,
  Redo2,
  SkipForward,
  Sparkles,
  Undo2,
} from 'lucide-react'
import { BrandMark } from './BrandMark'

interface ToolbarProps {
  menubar?: React.ReactNode
  busy: boolean
  sessionReady: boolean
  canUndo: boolean
  canRedo: boolean
  canRequestHint: boolean
  canRequestAllHints: boolean
  canApply: boolean
  candidatesVisible: boolean
  candidateEntry: boolean
  variantLabel: string
  onUndo: () => void
  onRedo: () => void
  onRequestHint: () => void
  onRequestAllHints: () => void
  onToggleCandidates: () => void
  onToggleCandidateEntry: () => void
  onApply: () => void
  onApplyAndContinue: () => void
}

const ToolButton = ({
  children,
  icon,
  disabled = false,
  active,
  onClick,
  title,
}: {
  children: React.ReactNode
  icon: React.ReactNode
  disabled?: boolean
  active?: boolean
  onClick: () => void
  title?: string
}) => (
  <button
    className={`tool-button${active ? ' is-active' : ''}`}
    disabled={disabled}
    onClick={onClick}
    aria-label={title ?? (typeof children === 'string' ? children : undefined)}
    aria-pressed={active == null ? undefined : active}
    title={title ?? (typeof children === 'string' ? children : undefined)}
    type="button"
  >
    {icon}
    <span>{children}</span>
  </button>
)

export function Toolbar({
  menubar,
  busy,
  sessionReady,
  canUndo,
  canRedo,
  canRequestHint,
  canRequestAllHints,
  canApply,
  candidatesVisible,
  candidateEntry,
  variantLabel,
  onUndo,
  onRedo,
  onRequestHint,
  onRequestAllHints,
  onToggleCandidates,
  onToggleCandidateEntry,
  onApply,
  onApplyAndContinue,
}: ToolbarProps) {
  const boardControlsDisabled = busy || !sessionReady

  return (
    <header className="app-header">
      <div className="titlebar">
        <div className="brand">
          <BrandMark />
          <span>Sukaku Forge</span>
        </div>
        <div className="window-dots" aria-hidden="true"><i /><i /><i /></div>
      </div>
      {menubar}
      <div className="toolbar" role="toolbar" aria-label="Puzzle actions" aria-busy={busy}>
        <div className="tool-group history-controls">
          <ToolButton icon={<Undo2 />} disabled={busy || !canUndo} onClick={onUndo}>Undo</ToolButton>
          <ToolButton icon={<Redo2 />} disabled={busy || !canRedo} onClick={onRedo}>Redo</ToolButton>
        </div>
        <div className="variant-label" aria-label={`Variant: ${variantLabel}`}>
          <span>Variant</span>
          <strong>{variantLabel}</strong>
        </div>
        <div className="tool-group board-controls">
          <ToolButton
            icon={<PencilLine />}
            active={candidateEntry}
            disabled={boardControlsDisabled}
            onClick={onToggleCandidateEntry}
            title="Toggle candidate entry mode (M)"
          >Candidates</ToolButton>
          <button
            className="icon-button"
            onClick={onToggleCandidates}
            disabled={boardControlsDisabled}
            title={candidatesVisible ? 'Hide candidates' : 'Show candidates'}
            aria-label="Show candidates"
            aria-pressed={candidatesVisible}
            type="button"
          >
            {candidatesVisible ? <Eye /> : <EyeOff />}
          </button>
        </div>
        <div className="tool-group push-right action-group">
          <ToolButton
            icon={<Sparkles />}
            disabled={busy || !canRequestHint}
            onClick={onRequestHint}
            title="Get next hint"
          >Next hint</ToolButton>
          <ToolButton
            icon={<ListTree />}
            disabled={busy || !canRequestAllHints}
            onClick={onRequestAllHints}
            title="Get all hints"
          >All hints</ToolButton>
          <button
            className="primary-button"
            onClick={onApply}
            disabled={busy || !canApply}
            aria-label="Apply active hint"
            title={canApply ? 'Apply the active server-owned hint' : 'Request an applicable hint first'}
            type="button"
          >
            <Check /> <span>Apply hint</span>
          </button>
          <ToolButton
            icon={<SkipForward />}
            disabled={busy || !canApply}
            onClick={onApplyAndContinue}
            title="Apply the active hint and request the next"
          >Solve step</ToolButton>
        </div>
      </div>
    </header>
  )
}
