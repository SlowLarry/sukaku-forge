import { useMemo, useRef } from 'react'
import type { BoardSnapshot, BoardTopology, CellRef, HighlightRole, HintView, LinkEndpoint, LinkRole, TopologyRegion } from '../model'
import { candidateKey, cellKey, cellName } from '../model'

const BOARD = 900
const CELL = 100
const CANDIDATE_OFFSET = [20, 50, 80]
type CandidatePaintRole = 'positive' | 'negative' | 'auxiliary' | 'selected' | 'conclusion' | 'mixed'
type CellPaintRole = 'selected' | 'affected' | 'support' | 'grouped'
type RegionPaintRole = 'support' | 'grouped' | 'warning'

interface BoardProps {
  board: BoardSnapshot
  topology: BoardTopology
  view: HintView
  selected: CellRef | null
  candidatesVisible: boolean
  onSelect: (cell: CellRef) => void
  onKeyDown: (event: React.KeyboardEvent) => void
}

const candidatePosition = (row: number, col: number, digit: number) => ({
  x: col * CELL + CANDIDATE_OFFSET[(digit - 1) % 3]!,
  y: row * CELL + CANDIDATE_OFFSET[Math.floor((digit - 1) / 3)]!,
})

const endpointPosition = (endpoint: LinkEndpoint) => {
  if (endpoint.kind === 'cell-center') {
    return { x: endpoint.cell.col * CELL + CELL / 2, y: endpoint.cell.row * CELL + CELL / 2 }
  }
  const candidate = endpoint.kind === 'candidate' ? endpoint.candidate : endpoint.representative
  return candidatePosition(candidate.row, candidate.col, candidate.digit)
}

function resolvedCandidateRole(roles?: Set<HighlightRole>): CandidatePaintRole | undefined {
  if (!roles?.size) return undefined
  if (roles.has('positive') && roles.has('negative')) return 'mixed'
  if (roles.has('conclusion')) return 'conclusion'
  if (roles.has('selected')) return 'selected'
  if (roles.has('positive')) return 'positive'
  if (roles.has('auxiliary')) return 'auxiliary'
  return 'negative'
}

function resolvedCellRole(roles?: Set<HighlightRole>): CellPaintRole | undefined {
  if (!roles?.size) return undefined
  if (roles.has('negative') || roles.has('conclusion')) return 'affected'
  if (roles.has('selected')) return 'selected'
  if (roles.has('pattern') || roles.has('primary')) return 'grouped'
  return 'support'
}

function resolvedRegionRole(roles: HighlightRole[]): RegionPaintRole {
  if (roles.includes('negative') || roles.includes('conclusion')) return 'warning'
  if (roles.includes('pattern') || roles.includes('primary')) return 'grouped'
  return 'support'
}

function candidateClass(role?: CandidatePaintRole) {
  return role ? `candidate candidate--${role}` : 'candidate'
}

function linkClass(role: LinkRole) {
  return `chain-link chain-link--${role}`
}

function markerForRole(role: LinkRole) {
  if (role === 'grouped' || role === 'grouped-strong') return 'url(#arrow-grouped)'
  if (role === 'strong' || role === 'strong-true') return 'url(#arrow-strong)'
  if (role === 'implication') return 'url(#arrow-implication)'
  return 'url(#arrow-weak)'
}

function RegionBoundary({ region }: { region: TopologyRegion }) {
  const members = new Set(region.cells.map(cellKey))
  return (
    <g className={`topology-boundary topology-boundary--${region.family}`} aria-label={region.label}>
      {region.cells.flatMap(({ row, col }) => {
        const edges: React.ReactNode[] = []
        if (!members.has(cellKey({ row: row - 1, col }))) edges.push(<line key="t" x1={col * CELL} y1={row * CELL} x2={(col + 1) * CELL} y2={row * CELL} />)
        if (!members.has(cellKey({ row, col: col - 1 }))) edges.push(<line key="l" x1={col * CELL} y1={row * CELL} x2={col * CELL} y2={(row + 1) * CELL} />)
        if (!members.has(cellKey({ row: row + 1, col }))) edges.push(<line key="b" x1={col * CELL} y1={(row + 1) * CELL} x2={(col + 1) * CELL} y2={(row + 1) * CELL} />)
        if (!members.has(cellKey({ row, col: col + 1 }))) edges.push(<line key="r" x1={(col + 1) * CELL} y1={row * CELL} x2={(col + 1) * CELL} y2={(row + 1) * CELL} />)
        return <g key={`${region.id}-${row}-${col}`}>{edges}</g>
      })}
    </g>
  )
}

export function Board({ board, topology, view, selected, candidatesVisible, onSelect, onKeyDown }: BoardProps) {
  const boardRef = useRef<SVGSVGElement>(null)
  const candidateRoles = useMemo(() => {
    const roleMap = new Map<string, Set<HighlightRole>>()
    for (const mark of view.candidateMarks) {
      const key = candidateKey(mark.candidate)
      const roles = roleMap.get(key) ?? new Set<HighlightRole>()
      mark.roles.forEach((role) => roles.add(role))
      roleMap.set(key, roles)
    }
    return roleMap
  }, [view])
  const cellRoles = useMemo(() => {
    const roleMap = new Map<string, Set<HighlightRole>>()
    for (const mark of view.cellMarks) {
      const key = cellKey(mark.cell)
      const roles = roleMap.get(key) ?? new Set<HighlightRole>()
      mark.roles.forEach((role) => roles.add(role))
      roleMap.set(key, roles)
    }
    return roleMap
  }, [view])
  const candidateGroups = useMemo(() => view.links.flatMap((link) => (
    (['from', 'to'] as const).flatMap((side) => {
      const endpoint = link[side]
      return endpoint.kind === 'candidate-group' ? [{ linkId: link.id, side, endpoint }] : []
    })
  )), [view])

  const handlePointer = (event: React.PointerEvent<SVGSVGElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect()
    const col = Math.max(0, Math.min(8, Math.floor(((event.clientX - bounds.left) / bounds.width) * 9)))
    const row = Math.max(0, Math.min(8, Math.floor(((event.clientY - bounds.top) / bounds.height) * 9)))
    onSelect({ row, col })
    boardRef.current?.focus()
  }

  return (
    <section className="board-stage" aria-label="Sudoku board">
      <div className="column-labels" aria-hidden="true">
        {Array.from({ length: 9 }, (_, index) => <span key={index}>{index + 1}</span>)}
      </div>
      <div className="board-with-rows">
        <div className="row-labels" aria-hidden="true">
          {'ABCDEFGHI'.split('').map((label) => <span key={label}>{label}</span>)}
        </div>
        <svg
          ref={boardRef}
          className="sudoku-board"
          viewBox={`0 0 ${BOARD} ${BOARD}`}
          role="grid"
          aria-label={selected ? `Sudoku board, selected row ${selected.row + 1}, column ${selected.col + 1}` : 'Sudoku board, no cell selected'}
          tabIndex={0}
          onPointerDown={handlePointer}
          onKeyDown={onKeyDown}
        >
          <defs>
            <marker id="arrow-grouped" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto" markerUnits="strokeWidth"><path d="M0,0 L8,4 L0,8 z" fill="var(--overlay-grouped)" /></marker>
            <marker id="arrow-strong" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto" markerUnits="strokeWidth"><path d="M0,0 L8,4 L0,8 z" fill="var(--overlay-positive)" /></marker>
            <marker id="arrow-weak" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto" markerUnits="strokeWidth"><path d="M0,0 L8,4 L0,8 z" fill="var(--overlay-negative)" /></marker>
            <marker id="arrow-implication" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto" markerUnits="strokeWidth"><path d="M0,0 L8,4 L0,8 z" fill="var(--overlay-auxiliary)" /></marker>
            <filter id="selected-glow" x="-30%" y="-30%" width="160%" height="160%"><feDropShadow dx="0" dy="0" stdDeviation="4" floodColor="#1769ff" floodOpacity=".18" /></filter>
          </defs>

          {/* 1. Permanent supported topology supplied by the fixture. */}
          <rect width={BOARD} height={BOARD} className="board-paper" />
          {topology.paths.flatMap((path) => path.cells.map((pathCell) => (
            <rect key={`${path.id}-${cellKey(pathCell)}`} x={pathCell.col * CELL} y={pathCell.row * CELL} width={CELL} height={CELL} className="topology-wash topology-wash--diagonal"><title>{path.label}</title></rect>
          )))}
          {topology.regions.filter((region) => region.family !== 'block' && region.family !== 'row' && region.family !== 'column').flatMap((region) => region.cells.map((regionCell) => (
            <rect key={`${region.id}-${cellKey(regionCell)}`} x={regionCell.col * CELL} y={regionCell.row * CELL} width={CELL} height={CELL} className="topology-wash topology-wash--window"><title>{region.label}</title></rect>
          )))}

          {/* 2. Semantic hint regions. */}
          {view.regions.flatMap((region) => region.cells.map((markedCell) => (
            <rect key={`${region.id}-${cellKey(markedCell)}`} x={markedCell.col * CELL + 3} y={markedCell.row * CELL + 3} width={CELL - 6} height={CELL - 6} rx="7" className={`region-mark region-mark--${resolvedRegionRole(region.roles)}`}><title>{region.label}</title></rect>
          )))}

          {/* 3. Semantic cell fills. */}
          {Array.from({ length: 81 }, (_, index) => {
            const row = Math.floor(index / 9)
            const col = index % 9
            const role = resolvedCellRole(cellRoles.get(cellKey({ row, col })))
            return role ? <rect key={`mark-${index}`} x={col * CELL + 5} y={row * CELL + 5} width={CELL - 10} height={CELL - 10} rx="5" className={`cell-mark cell-mark--${role}`} /> : null
          })}

          {/* 4. Fine grid and supplied permanent region/path boundaries. */}
          {Array.from({ length: 10 }, (_, index) => (
            <g key={`grid-${index}`}>
              <line x1={index * CELL} y1={0} x2={index * CELL} y2={BOARD} className="grid-minor" />
              <line x1={0} y1={index * CELL} x2={BOARD} y2={index * CELL} className="grid-minor" />
            </g>
          ))}
          {topology.regions.map((region) => <RegionBoundary key={region.id} region={region} />)}
          {topology.paths.map((path) => (
            <polyline key={path.id} points={path.cells.map(({ row, col }) => `${col * CELL + CELL / 2},${row * CELL + CELL / 2}`).join(' ')} className={`topology-path topology-path--${path.family}`}><title>{path.label}</title></polyline>
          ))}

          {/* 5. Values. */}
          {board.values.map((value, index) => value == null ? null : (
            <text key={`value-${index}`} x={(index % 9) * CELL + CELL / 2} y={Math.floor(index / 9) * CELL + 68} className="cell-value" textAnchor="middle">{value}</text>
          ))}

          {/* 6. Group membership and chain links remain presentation-only layers below candidate glyphs. */}
          {candidateGroups.flatMap(({ linkId, side, endpoint }) => endpoint.members.map((member) => {
            const position = candidatePosition(member.row, member.col, member.digit)
            return (
              <circle
                key={`${linkId}-${side}-${candidateKey(member)}`}
                cx={position.x}
                cy={position.y - 5}
                r="19"
                className="candidate-group-halo"
                data-group-link={linkId}
                data-group-side={side}
              >
                <title>{`Grouped endpoint member ${cellName(member)}(${member.digit})`}</title>
              </circle>
            )
          }))}
          {view.links.map((link) => {
            const from = endpointPosition(link.from)
            const to = endpointPosition(link.to)
            const dx = to.x - from.x
            const dy = to.y - from.y
            const length = Math.sqrt(dx * dx + dy * dy) || 1
            const start = { x: from.x + (dx / length) * 15, y: from.y + (dy / length) * 15 }
            const end = { x: to.x - (dx / length) * 18, y: to.y - (dy / length) * 18 }
            const marker = markerForRole(link.role)
            return <line key={link.id} x1={start.x} y1={start.y} x2={end.x} y2={end.y} className={linkClass(link.role)} markerEnd={marker} markerStart={link.direction === 'both' ? marker : undefined} />
          })}

          {/* 7. Exact snapshot candidate masks and composable semantic marks. */}
          {candidatesVisible && board.candidateMasks.flatMap((mask, index) => {
            const row = Math.floor(index / 9)
            const col = index % 9
            return Array.from({ length: 9 }, (_, digitIndex) => digitIndex + 1).map((digit) => {
              if ((mask & (1 << digit)) === 0) return null
              const reference = { row, col, digit }
              const role = resolvedCandidateRole(candidateRoles.get(candidateKey(reference)))
              const position = candidatePosition(row, col, digit)
              return (
                <g key={candidateKey(reference)}>
                  {role && <circle cx={position.x} cy={position.y - 5} r={role === 'conclusion' ? 17 : 15} className={`candidate-halo candidate-halo--${role}`} />}
                  <text x={position.x} y={position.y + 2} textAnchor="middle" className={candidateClass(role)}>{digit}</text>
                </g>
              )
            })
          })}

          {/* 8. Selection and pointer target. */}
          {selected && <rect x={selected.col * CELL + 4} y={selected.row * CELL + 4} width={CELL - 8} height={CELL - 8} rx="5" className="board-selection" filter="url(#selected-glow)" />}
          <rect width={BOARD} height={BOARD} fill="transparent" className="board-hit-target"><title>{selected ? `Selected r${selected.row + 1}c${selected.col + 1}` : 'Select a cell'}</title></rect>
        </svg>
      </div>
      <div className="board-help">
        <span><kbd>↑↓←→</kbd> Move</span><span><kbd>1–9</kbd> Enter</span><span><kbd>Del</kbd> Clear</span><span><kbd>Esc</kbd> Deselect</span>
      </div>
    </section>
  )
}
