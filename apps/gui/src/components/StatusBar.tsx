export type StatusState = 'idle' | 'running' | 'error'

interface StatusBarProps {
  state: StatusState
  message: string
  revision?: string
  clueCount?: number
}

export function StatusBar({ state, message, revision, clueCount }: StatusBarProps) {
  return (
    <footer className="status-bar" data-status-state={state}>
      <span
        className="status-message"
        role={state === 'error' ? 'alert' : 'status'}
        aria-live={state === 'error' ? 'assertive' : 'polite'}
        aria-atomic="true"
      >
        <i aria-hidden="true" />
        {message}
      </span>
      <span className="puzzle-meta">
        Revision <strong>{revision ?? '—'}</strong>
        <b aria-hidden="true" />
        {clueCount == null ? 'Session not ready' : `${clueCount} given${clueCount === 1 ? '' : 's'}`}
      </span>
      <span className="status-mode" aria-hidden="true">
        {state === 'running' ? 'Working' : state === 'error' ? 'Error' : 'Ready'}
      </span>
    </footer>
  )
}
