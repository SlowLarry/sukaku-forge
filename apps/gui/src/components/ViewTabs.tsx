import { Grid2X2, Route } from 'lucide-react'
import type { HintView } from '../model'

interface ViewTabsProps {
  views: HintView[]
  selectedId: string
  onSelect: (id: string) => void
}

export function ViewTabs({ views, selectedId, onSelect }: ViewTabsProps) {
  return (
    <nav className="view-tabs" aria-label="Hint views">
      {views.map((view, index) => (
        <button
          key={view.id}
          className={selectedId === view.id ? 'is-active' : ''}
          aria-current={selectedId === view.id ? 'page' : undefined}
          onClick={() => onSelect(view.id)}
          type="button"
        >
          {index < 2 ? <Route /> : <Grid2X2 />}
          <span>{index < 2 ? `View ${index + 1}` : 'Cover'}</span>
        </button>
      ))}
    </nav>
  )
}
