interface StatusBarProps {
  message: string
  progress: number
}

export function StatusBar({ message, progress }: StatusBarProps) {
  return (
    <footer className="status-bar">
      <span className="status-message" role="status" aria-live="polite" aria-atomic="true"><i aria-hidden="true" /> {message}</span>
      <span className="puzzle-meta">Puzzle: <strong>Classic Example 02</strong><b />37 clues</span>
      <div className="analysis-progress">
        <span>Analyzing…</span>
        <div className="progress-track" aria-label="Analysis progress" role="progressbar" aria-valuenow={progress} aria-valuemin={0} aria-valuemax={100}>
          <i style={{ width: `${progress}%` }} />
        </div>
        <strong>{progress}%</strong>
        <button type="button" disabled title="Analysis cancellation is not connected yet">Cancel</button>
      </div>
    </footer>
  )
}
