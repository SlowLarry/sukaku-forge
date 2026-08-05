export interface HintBrowserItem {
  id: string
  label: string
  detail: string
  kind: 'presented' | 'unsupported'
  rating?: number
}

interface HintBrowserProps {
  items: HintBrowserItem[]
  selectedId: string | null
  emptyMessage: string
}

export function HintBrowser({ items, selectedId, emptyMessage }: HintBrowserProps) {
  return (
    <section className="hint-browser" aria-labelledby="hint-browser-title">
      <div className="section-heading">
        <div>
          <h2 id="hint-browser-title">Hint browser</h2>
          <p>{items.length === 0 ? 'No active deduction' : `${items.length} active deduction${items.length === 1 ? '' : 's'}`}</p>
        </div>
      </div>
      <div className="hint-table">
        <div className="hint-table-header" aria-hidden="true">
          <span>Technique</span>
          <span>State</span>
          <span>Rating</span>
        </div>
        <ul className="hint-table-body" aria-label="Active hints">
          {items.map((item) => (
            <li
              key={item.id}
              className={`hint-row${item.id === selectedId ? ' is-selected' : ''}`}
              aria-current={item.id === selectedId ? 'true' : undefined}
              data-hint-kind={item.kind}
            >
              <span className="hint-name">
                <strong>{item.label}</strong>
                <small>{item.detail}</small>
              </span>
              <span className={`hint-state hint-state--${item.kind}`}>
                {item.kind === 'presented' ? 'Presented' : 'Proof unavailable'}
              </span>
              <strong>{item.rating == null ? '—' : item.rating.toFixed(1)}</strong>
            </li>
          ))}
          {items.length === 0 && <li className="empty-row">{emptyMessage}</li>}
        </ul>
      </div>
    </section>
  )
}
