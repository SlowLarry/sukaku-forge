import { Grid2X2, Route } from 'lucide-react'
import { useId, useRef } from 'react'
import type { HintView } from '../model'

interface ViewTabsProps {
  views: HintView[]
  selectedId: string
  onSelect: (id: string) => void
  children: React.ReactNode
}

// Kept beside the tab widget so its tested key map cannot drift from the handler.
// eslint-disable-next-line react-refresh/only-export-components
export const viewTabKeyboardTarget = (key: string, current: number, length: number) => {
  if (length === 0) return null
  if (key === 'Home') return 0
  if (key === 'End') return length - 1
  if (key === 'ArrowLeft') return (current - 1 + length) % length
  if (key === 'ArrowRight') return (current + 1) % length
  return null
}

export function ViewTabs({ views, selectedId, onSelect, children }: ViewTabsProps) {
  const instanceId = useId().replaceAll(':', '')
  const tabs = useRef<Array<HTMLButtonElement | null>>([])
  const selectedIndex = Math.max(0, views.findIndex((view) => view.id === selectedId))

  const selectAndFocus = (index: number) => {
    const view = views[index]
    if (!view) return
    onSelect(view.id)
    tabs.current[index]?.focus()
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>, index: number) => {
    const nextIndex = viewTabKeyboardTarget(event.key, index, views.length)
    if (nextIndex == null) return
    event.preventDefault()
    selectAndFocus(nextIndex)
  }

  return (
    <section className="view-tabs-shell">
      <div className="view-tabs" role="tablist" aria-label="Hint proof views" aria-orientation="horizontal">
        {views.length === 0 && <span className="empty-tab">No proof views</span>}
        {views.map((view, index) => {
          const selected = index === selectedIndex
          const tabId = `${instanceId}-tab-${index}`
          const panelId = `${instanceId}-panel-${index}`
          return (
            <button
              key={view.id}
              ref={(element) => { tabs.current[index] = element }}
              id={tabId}
              className={selected ? 'is-active' : ''}
              role="tab"
              aria-selected={selected}
              aria-controls={panelId}
              tabIndex={selected ? 0 : -1}
              onClick={() => onSelect(view.id)}
              onKeyDown={(event) => handleKeyDown(event, index)}
              type="button"
            >
              {view.links.length > 0 ? <Route aria-hidden="true" /> : <Grid2X2 aria-hidden="true" />}
              <span>{view.label}</span>
            </button>
          )
        })}
      </div>
      {views.length === 0 ? (
        <div className="view-tabpanel view-tabpanel--empty" role="region" aria-label="Hint information">
          {children}
        </div>
      ) : views.map((view, index) => {
        const selected = index === selectedIndex
        return (
          <div
            key={view.id}
            id={`${instanceId}-panel-${index}`}
            className="view-tabpanel"
            role="tabpanel"
            aria-labelledby={`${instanceId}-tab-${index}`}
            tabIndex={0}
            hidden={!selected}
          >
            {selected ? children : null}
          </div>
        )
      })}
    </section>
  )
}
