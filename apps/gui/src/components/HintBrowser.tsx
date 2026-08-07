import { useMemo, useState } from 'react'
import { Search } from 'lucide-react'
import type {
  HintCategoryDto,
  HintEffectsDto,
  NonZeroDecimalString,
  PortGapDto,
} from '../applicationPort'

export type HintBrowserItemKind = 'available' | 'presented' | 'unsupported' | 'incomplete'

export interface HintBrowserItem {
  id: NonZeroDecimalString
  label: string
  detail: string
  kind: HintBrowserItemKind
  rating?: number
  category?: HintCategoryDto
  groupKey?: string
  groupName?: string
  techniqueKey?: string
  shortName?: string
  effects?: HintEffectsDto
  filterEffects?: HintEffectsDto
}

export type HintCatalogResult =
  | { kind: 'complete' }
  | { kind: 'confirmation-required' }
  | { kind: 'incomplete'; gap: PortGapDto }

interface HintBrowserProps {
  items: HintBrowserItem[]
  selectedId: NonZeroDecimalString | null
  emptyMessage: string
  catalogResult?: HintCatalogResult | null
  busy?: boolean
  onSelect?: (id: NonZeroDecimalString) => void
  onSearchAdvanced?: () => void
}

interface HintTechniqueGroup {
  key: string
  label: string
  items: HintBrowserItem[]
}

interface HintCategoryGroup {
  category: HintCategoryDto
  techniques: HintTechniqueGroup[]
  count: number
}

const removalNovelty = (
  effects: HintEffectsDto,
  seenPlacementCells: Set<number>,
  seenRemovals: Map<number, number>,
) => {
  if (effects.placement != null && !seenPlacementCells.has(effects.placement.cell)) return true

  return effects.removals.some((removal) => (
    (removal.digits & ~(seenRemovals.get(removal.cell) ?? 0)) !== 0
  ))
}

const rememberEffects = (
  effects: HintEffectsDto,
  seenPlacementCells: Set<number>,
  seenRemovals: Map<number, number>,
) => {
  if (effects.placement != null) {
    seenPlacementCells.add(effects.placement.cell)
  }
  for (const removal of effects.removals) {
    seenRemovals.set(removal.cell, (seenRemovals.get(removal.cell) ?? 0) | removal.digits)
  }
}

/** Matches Sukaku Explainer's stable, greedy "filter similar outcomes" projection. */
function filterSimilarHintOutcomes(items: HintBrowserItem[]): HintBrowserItem[] {
  const retained: HintBrowserItem[] = []
  const seenPlacementCells = new Set<number>()
  const seenRemovals = new Map<number, number>()

  for (const item of items) {
    const effects = item.filterEffects ?? item.effects
    if (effects == null || item.category == null) {
      retained.push(item)
      continue
    }

    if (item.category === 'direct') {
      const cell = effects.placement?.cell
      if (cell != null && seenPlacementCells.has(cell)) continue
      retained.push(item)
      rememberEffects(effects, seenPlacementCells, seenRemovals)
      continue
    }

    if (!removalNovelty(effects, seenPlacementCells, seenRemovals)) continue
    retained.push(item)
    rememberEffects(effects, seenPlacementCells, seenRemovals)
  }

  return retained
}

const groupItems = (items: HintBrowserItem[]): HintCategoryGroup[] => {
  const categories = new Map<HintCategoryDto, HintCategoryGroup>()
  const techniques = new Map<HintCategoryDto, Map<string, HintTechniqueGroup>>()

  for (const item of items) {
    if (item.category == null) continue
    let category = categories.get(item.category)
    if (category == null) {
      category = { category: item.category, techniques: [], count: 0 }
      categories.set(item.category, category)
      techniques.set(item.category, new Map())
    }

    const techniqueKey = item.groupKey ?? item.techniqueKey ?? item.label
    const categoryTechniques = techniques.get(item.category)!
    let technique = categoryTechniques.get(techniqueKey)
    if (technique == null) {
      technique = { key: techniqueKey, label: item.groupName ?? item.label, items: [] }
      categoryTechniques.set(techniqueKey, technique)
      category.techniques.push(technique)
    }
    technique.items.push(item)
    category.count += 1
  }

  // Swing inserts the direct root before the indirect root even when an
  // IndirectHint is encountered in Java's nominally "direct" producer band.
  return (['direct', 'indirect'] as const)
    .map((category) => categories.get(category))
    .filter((category): category is HintCategoryGroup => category != null)
}

const categoryLabel = (category: HintCategoryDto) => (
  category === 'direct' ? 'Sudoku Rules' : 'Solving Techniques'
)

const stateLabel = (kind: HintBrowserItemKind) => {
  switch (kind) {
    case 'presented': return 'Presented'
    case 'unsupported': return 'Proof unavailable'
    case 'incomplete': return 'Search incomplete'
    case 'available': return 'Available'
  }
}

const searchItems = (items: HintBrowserItem[], search: string) => {
  const query = search.trim().toLocaleLowerCase()
  if (query.length === 0) return items
  return items.filter((item) => [
    item.label,
    item.techniqueKey,
    item.shortName,
    item.detail,
    item.rating?.toFixed(1),
  ].some((value) => value?.toLocaleLowerCase().includes(query)))
}

function HintRow({
  item,
  selected,
  busy,
  onSelect,
}: {
  item: HintBrowserItem
  selected: boolean
  busy: boolean
  onSelect?: (id: NonZeroDecimalString) => void
}) {
  return (
    <li>
      <button
        type="button"
        className={`hint-row${selected ? ' is-selected' : ''}`}
        aria-current={selected ? 'true' : undefined}
        data-hint-kind={item.kind}
        disabled={busy || onSelect == null}
        onClick={() => onSelect?.(item.id)}
      >
        <span className="hint-name">
          <strong>{item.label}</strong>
          <small>{item.detail}</small>
        </span>
        <span className={`hint-state hint-state--${item.kind}`}>
          {stateLabel(item.kind)}
        </span>
        <strong>{item.rating == null ? '—' : item.rating.toFixed(1)}</strong>
      </button>
    </li>
  )
}

export function HintBrowser({
  items,
  selectedId,
  emptyMessage,
  catalogResult = null,
  busy = false,
  onSelect,
  onSearchAdvanced,
}: HintBrowserProps) {
  const [search, setSearch] = useState('')
  const [filterSimilar, setFilterSimilar] = useState(true)
  const catalogActive = catalogResult != null
  const projectedItems = useMemo(
    () => catalogActive && filterSimilar ? filterSimilarHintOutcomes(items) : items,
    [catalogActive, filterSimilar, items],
  )
  const visibleItems = useMemo(() => searchItems(projectedItems, search), [projectedItems, search])
  const categoryGroups = useMemo(() => groupItems(visibleItems), [visibleItems])

  const selectFirstWhenHidden = (nextVisible: HintBrowserItem[]) => {
    if (busy || nextVisible.length === 0) return
    if (selectedId != null && nextVisible.some((item) => item.id === selectedId)) return
    onSelect?.(nextVisible[0]!.id)
  }

  const handleSearch = (nextSearch: string) => {
    setSearch(nextSearch)
    selectFirstWhenHidden(searchItems(projectedItems, nextSearch))
  }

  const handleSimilarFilter = (enabled: boolean) => {
    setFilterSimilar(enabled)
    const nextProjected = catalogActive && enabled ? filterSimilarHintOutcomes(items) : items
    selectFirstWhenHidden(searchItems(nextProjected, search))
  }

  const countLabel = catalogActive
    ? `${visibleItems.length} of ${items.length} deduction${items.length === 1 ? '' : 's'}`
    : items.length === 0
      ? 'No active deduction'
      : `${items.length} active deduction${items.length === 1 ? '' : 's'}`
  const noVisibleMessage = items.length > 0
    ? 'No hints match the current search and outcome filter.'
    : catalogResult?.kind === 'complete'
      ? 'The engine found no applicable logical hints.'
      : catalogResult?.kind === 'incomplete'
        ? catalogResult.gap.message
      : emptyMessage

  return (
    <section className="hint-browser" aria-labelledby="hint-browser-title">
      <div className="section-heading">
        <div>
          <h2 id="hint-browser-title">Hint browser</h2>
          <p>{countLabel}</p>
        </div>
      </div>

      {catalogActive && (
        <div className="hint-browser-controls">
          <label className="hint-search">
            <span className="sr-only">Search all hints</span>
            <Search aria-hidden="true" />
            <input
              type="search"
              value={search}
              disabled={busy}
              onChange={(event) => handleSearch(event.currentTarget.value)}
              placeholder="Search all hints"
            />
          </label>
          <label className="similar-hints-toggle">
            <input
              type="checkbox"
              checked={filterSimilar}
              disabled={busy}
              onChange={(event) => handleSimilarFilter(event.currentTarget.checked)}
            />
            <span>Filter similar outcomes</span>
          </label>
        </div>
      )}

      {catalogResult?.kind === 'confirmation-required' && (
        <div className="hint-catalog-notice">
          <div role="status">
            <strong>No ordinary hints found</strong>
            <span>Advanced nested-chain techniques can take considerably longer.</span>
          </div>
          <button
            type="button"
            className="secondary-button"
            disabled={busy || onSearchAdvanced == null}
            onClick={onSearchAdvanced}
          >Search advanced hints</button>
        </div>
      )}
      {catalogResult?.kind === 'incomplete' && (
        <div className="hint-catalog-notice hint-catalog-notice--warning" role="status">
          <div>
            <strong>Hint search incomplete</strong>
            <span>{catalogResult.gap.message}</span>
          </div>
        </div>
      )}

      <div className="hint-table">
        <div className="hint-table-header" aria-hidden="true">
          <span>Technique</span>
          <span>State</span>
          <span>Rating</span>
        </div>
        {catalogActive ? (
          <div className="hint-table-body hint-tree" aria-label="All hints">
            {categoryGroups.map((category) => (
              <section className="hint-category" key={category.category}>
                <h3>
                  <span>{categoryLabel(category.category)}</span>
                  <small>{category.count}</small>
                </h3>
                {category.techniques.map((technique) => (
                  <section className="hint-technique" key={technique.key}>
                    <h4>
                      <span>{technique.label}</span>
                      <small>{technique.items.length}</small>
                    </h4>
                    <ul aria-label={`${technique.label} hints`}>
                      {technique.items.map((item) => (
                        <HintRow
                          key={item.id}
                          item={item}
                          selected={item.id === selectedId}
                          busy={busy}
                          onSelect={onSelect}
                        />
                      ))}
                    </ul>
                  </section>
                ))}
              </section>
            ))}
            {visibleItems.length === 0 && <p className="empty-row">{noVisibleMessage}</p>}
          </div>
        ) : (
          <ul className="hint-table-body" aria-label="Active hints">
            {items.map((item) => (
              <HintRow
                key={item.id}
                item={item}
                selected={item.id === selectedId}
                busy={busy}
                onSelect={onSelect}
              />
            ))}
            {items.length === 0 && <li className="empty-row">{emptyMessage}</li>}
          </ul>
        )}
      </div>
    </section>
  )
}
