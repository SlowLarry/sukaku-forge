import { ChevronDown, ChevronRight, Search } from 'lucide-react'
import { useMemo, useState } from 'react'
import type { HintRow } from '../model'

interface HintBrowserProps {
  rows: HintRow[]
  selectedId: string
  onSelect: (row: HintRow) => void
}

export function HintBrowser({ rows, selectedId, onSelect }: HintBrowserProps) {
  const [query, setQuery] = useState('')
  const filteredRows = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    return normalized ? rows.filter((row) => row.label.toLocaleLowerCase().includes(normalized)) : rows
  }, [query, rows])

  return (
    <section className="hint-browser" aria-labelledby="hint-browser-title">
      <div className="section-heading">
        <div>
          <h2 id="hint-browser-title">Hint browser</h2>
          <p>{rows.filter((row) => !row.group).reduce((total, row) => total + row.count, 0)} available deductions</p>
        </div>
        <button className="text-button" type="button" disabled title="Hint refresh is not connected yet">Refresh</button>
      </div>
      <div className="browser-controls">
        <label className="search-field">
          <Search aria-hidden="true" />
          <span className="sr-only">Search techniques</span>
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search techniques"
          />
        </label>
        <select aria-label="Hint filter" defaultValue="all" disabled title="Hint filters are not connected yet">
          <option value="all">Show all</option>
          <option value="chains">Chains</option>
          <option value="simple">Simple logic</option>
        </select>
      </div>
      <div className="hint-table" role="treegrid" aria-label="Available hints">
        <div className="hint-table-header" role="row">
          <span role="columnheader">Technique</span>
          <span role="columnheader">Count</span>
          <span role="columnheader">Rating</span>
        </div>
        <div className="hint-table-body">
          {filteredRows.map((row) => {
            const selected = row.id === selectedId
            return (
              <button
                key={row.id}
                className={`hint-row${selected ? ' is-selected' : ''}${row.group ? ' is-group' : ''}`}
                style={{ '--indent': row.indent } as React.CSSProperties}
                onClick={() => onSelect(row)}
                role="row"
                aria-selected={selected}
                aria-level={row.indent + 1}
                aria-expanded={row.group ? Boolean(row.expanded) : undefined}
                type="button"
              >
                <span className="hint-name" role="gridcell">
                  <span className="row-chevron" aria-hidden="true">
                    {row.group ? (row.expanded ? <ChevronDown /> : <ChevronRight />) : null}
                  </span>
                  {row.label}
                </span>
                <span role="gridcell">{row.count || '—'}</span>
                <strong role="gridcell">{row.rating ? row.rating.toFixed(1) : '—'}</strong>
              </button>
            )
          })}
          {filteredRows.length === 0 && <div className="empty-row">No techniques match “{query}”.</div>}
        </div>
      </div>
    </section>
  )
}
