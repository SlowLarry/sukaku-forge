import {
  Bolt,
  Check,
  ChevronDown,
  Eye,
  EyeOff,
  FilePlus2,
  FolderOpen,
  ListTree,
  PencilLine,
  Redo2,
  Save,
  Settings,
  SkipForward,
  Sparkles,
  Undo2,
} from 'lucide-react'
import { BrandMark } from './BrandMark'

interface ToolbarProps {
  canUndo: boolean
  canApply: boolean
  candidatesVisible: boolean
  candidateEntry: boolean
  onUndo: () => void
  onToggleCandidates: () => void
  onToggleCandidateEntry: () => void
  onApply: () => void
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
  onClick?: () => void
  title?: string
}) => (
  <button
    className={`tool-button${active ? ' is-active' : ''}`}
    disabled={disabled || onClick == null}
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
  canUndo,
  canApply,
  candidatesVisible,
  candidateEntry,
  onUndo,
  onToggleCandidates,
  onToggleCandidateEntry,
  onApply,
}: ToolbarProps) {
  return (
    <header className="app-header">
      <div className="titlebar">
        <div className="brand">
          <BrandMark />
          <span>Sukaku Forge</span>
        </div>
        <div className="window-dots" aria-hidden="true"><i /><i /><i /></div>
      </div>
      <div className="toolbar" role="toolbar" aria-label="Puzzle actions">
        <div className="tool-group">
          <ToolButton icon={<FilePlus2 />} title="Create a new puzzle">New</ToolButton>
          <ToolButton icon={<FolderOpen />} title="Open a puzzle">Open</ToolButton>
          <ToolButton icon={<Save />} title="Save puzzle">Save</ToolButton>
        </div>
        <div className="tool-group compact">
          <ToolButton icon={<Undo2 />} disabled={!canUndo} onClick={onUndo}>Undo</ToolButton>
          <ToolButton icon={<Redo2 />} disabled>Redo</ToolButton>
        </div>
        <label className="variant-select">
          <span>Variant</span>
          <button type="button" disabled aria-label="Select Sudoku variant" title="Variant selection is not connected yet">Classic Sudoku <ChevronDown /></button>
        </label>
        <div className="tool-group board-controls">
          <ToolButton
            icon={<PencilLine />}
            active={candidateEntry}
            onClick={onToggleCandidateEntry}
            title="Toggle candidate entry mode (M)"
          >Candidates</ToolButton>
          <button
            className="icon-button"
            onClick={onToggleCandidates}
            title={candidatesVisible ? 'Hide candidates' : 'Show candidates'}
            aria-label={candidatesVisible ? 'Hide candidates' : 'Show candidates'}
            aria-pressed={candidatesVisible}
            type="button"
          >
            {candidatesVisible ? <Eye /> : <EyeOff />}
          </button>
          <button className="icon-button" title="Settings are not connected yet" aria-label="Settings" disabled type="button"><Settings /></button>
        </div>
        <div className="tool-group push-right action-group">
          <ToolButton icon={<Sparkles />}>Get next hint</ToolButton>
          <ToolButton icon={<ListTree />}>Get all hints</ToolButton>
          <button className="primary-button" onClick={onApply} disabled={!canApply} aria-label="Apply selected hint" title={canApply ? 'Apply selected hint' : 'Select an individual hint to apply'} type="button"><Check /> <span>Apply hint</span></button>
          <ToolButton icon={<SkipForward />}>Solve step</ToolButton>
          <button className="icon-button overflow-action" title="More solver actions are not connected yet" aria-label="More solver actions" disabled type="button"><Bolt /></button>
        </div>
      </div>
    </header>
  )
}
