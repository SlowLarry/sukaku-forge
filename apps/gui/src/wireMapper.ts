import type {
  CandidateRefDto,
  ExplanationBlockDto,
  ExplanationInlineDto,
  HintEffectsDto,
  HintPresentationDto,
  LinkCauseDto,
  LinkEndpointDto,
  LinkKindDto,
  NonConsecutiveDto,
  NonZeroDecimalString,
  SessionSnapshotDto,
  TopologyDto,
  TopologyRegionDto,
} from './applicationPort'
import type {
  BoardSnapshot,
  BoardTopology,
  BoardVariant,
  CandidateRef,
  ExplanationBlock,
  ExplanationInline,
  HighlightRole,
  HintPresentation,
  LinkCause,
  LinkEndpoint,
  LinkRole,
  RegionMark,
  TopologyPath,
  TopologyRegion,
} from './model'

const ROLE_BITS: ReadonlyArray<readonly [number, HighlightRole]> = [
  [0x0001, 'selected'],
  [0x0002, 'pattern'],
  [0x0004, 'positive'],
  [0x0008, 'negative'],
  [0x0010, 'auxiliary'],
  [0x0020, 'conclusion'],
  [0x0040, 'primary'],
  [0x0080, 'secondary'],
]

const REGION_FAMILIES: Record<Exclude<TopologyRegionDto['family_key'], 'main_diagonal' | 'anti_diagonal'>, TopologyRegion['family']> = {
  block: 'block',
  row: 'row',
  column: 'column',
  disjoint_group: 'disjoint-group',
  window: 'window',
  girandola: 'girandola',
  asterisk: 'asterisk',
  center_dot: 'center-dot',
}

const PATH_FAMILIES: Record<'main_diagonal' | 'anti_diagonal', TopologyPath['family']> = {
  main_diagonal: 'main-diagonal',
  anti_diagonal: 'anti-diagonal',
}

const NON_CONSECUTIVE_MODES: Record<NonConsecutiveDto, BoardVariant['nonConsecutive']> = {
  off: 'off',
  orthogonal: 'orthogonal',
  orthogonal_cyclic: 'orthogonal-cyclic',
  diagonal: 'diagonal',
  diagonal_cyclic: 'diagonal-cyclic',
}

const LINK_ROLES: Record<LinkKindDto, LinkRole> = {
  strong: 'strong',
  grouped_strong: 'grouped-strong',
  weak: 'weak',
  implication: 'implication',
}

export const cellRefFromIndex = (cell: number) => ({
  row: Math.floor(cell / 9),
  col: cell % 9,
})

const mapCandidate = ({ cell, digit }: CandidateRefDto): CandidateRef => ({
  ...cellRefFromIndex(cell),
  digit,
})

const mapRoles = (mask: number): HighlightRole[] => ROLE_BITS.flatMap(([bit, role]) => (mask & bit) !== 0 ? [role] : [])

const regionKey = (regionType: number, regionIndex: number) => `${regionType}:${regionIndex}`

const topologyRegionId = (region: TopologyRegionDto) => `${region.family_key}-${region.region_index}`

export function mapSessionSnapshot(snapshot: SessionSnapshotDto): BoardSnapshot {
  return {
    revision: snapshot.revision,
    values: snapshot.values.map((value) => value === 0 ? null : value),
    candidateMasks: [...snapshot.candidate_masks],
    givens: [...snapshot.givens],
    canUndo: snapshot.can_undo,
    canRedo: snapshot.can_redo,
  }
}

export function mapTopology(topology: TopologyDto): BoardTopology {
  const regions: TopologyRegion[] = []
  const paths: TopologyPath[] = []
  for (const region of topology.regions) {
    const cells = region.cells.map(cellRefFromIndex)
    if (region.family_key === 'main_diagonal' || region.family_key === 'anti_diagonal') {
      paths.push({
        id: topologyRegionId(region),
        label: region.label,
        cells,
        family: PATH_FAMILIES[region.family_key],
      })
    } else {
      regions.push({
        id: topologyRegionId(region),
        label: region.label,
        cells,
        family: REGION_FAMILIES[region.family_key],
      })
    }
  }

  return {
    regions,
    paths,
    variant: {
      blocks: topology.variant.blocks,
      disjointGroups: topology.variant.disjoint_groups,
      windows: topology.variant.windows,
      sudokuX: topology.variant.sudoku_x,
      girandola: topology.variant.girandola,
      asterisk: topology.variant.asterisk,
      centerDot: topology.variant.center_dot,
      antiFerz: topology.variant.anti_ferz,
      antiKnight: topology.variant.anti_knight,
      toroidal: topology.variant.toroidal,
      nonConsecutive: NON_CONSECUTIVE_MODES[topology.variant.non_consecutive],
      forbiddenPairs: topology.variant.forbidden_pairs,
    },
  }
}

const mapEndpoint = (endpoint: LinkEndpointDto): LinkEndpoint => {
  switch (endpoint.type) {
    case 'candidate':
      return { kind: 'candidate', candidate: mapCandidate(endpoint) }
    case 'candidate_group':
      return {
        kind: 'candidate-group',
        representative: mapCandidate(endpoint.representative),
        members: endpoint.members.map(mapCandidate),
      }
    case 'cell_center':
      return { kind: 'cell-center', cell: cellRefFromIndex(endpoint.cell) }
  }
}

const mapCause = (cause: LinkCauseDto): LinkCause => {
  if (cause.type === 'region') {
    return { kind: 'region', regionType: cause.region_type, regionIndex: cause.region_index }
  }
  return { kind: cause.type }
}

const mapExplanationInline = (inline: ExplanationInlineDto): ExplanationInline => {
  switch (inline.type) {
    case 'text':
      return { kind: 'text', text: inline.text }
    case 'technique':
      return { kind: 'technique', techniqueKey: inline.technique_key }
    case 'cell':
      return { kind: 'cell', cell: cellRefFromIndex(inline.cell) }
    case 'digit':
      return { kind: 'digit', digit: inline.digit }
    case 'region':
      return { kind: 'region', regionType: inline.region_type, regionIndex: inline.region_index }
    case 'candidate':
      return { kind: 'candidate', candidate: mapCandidate(inline) }
  }
}

const mapExplanationBlock = (block: ExplanationBlockDto): ExplanationBlock => block.type === 'paragraph'
  ? { kind: 'paragraph', inlines: block.inlines.map(mapExplanationInline) }
  : { kind: 'unordered-list', items: block.items.map((item) => item.map(mapExplanationInline)) }

const expandRemovals = (effects: HintEffectsDto): CandidateRef[] => effects.removals.flatMap((removal) => (
  Array.from({ length: 9 }, (_, index) => index + 1).flatMap((digit) => (
    (removal.digits & (1 << digit)) !== 0 ? [{ ...cellRefFromIndex(removal.cell), digit }] : []
  ))
))

export interface HintPresentationMappingInput {
  hintId: NonZeroDecimalString
  revision: string
  presentation: HintPresentationDto
  effects: HintEffectsDto
  topology: TopologyDto
}

export function mapHintPresentation({
  hintId,
  revision,
  presentation,
  effects,
  topology,
}: HintPresentationMappingInput): HintPresentation {
  const topologyRegions = new Map(topology.regions.map((region) => [regionKey(region.region_type, region.region_index), region]))
  const affects = expandRemovals(effects)
  const placement = effects.placement == null ? undefined : mapCandidate(effects.placement)
  const type = placement == null
    ? `Elimination (${effects.elimination_count})`
    : effects.elimination_count === 0 ? 'Placement' : `Placement + Elimination (${effects.elimination_count})`

  return {
    id: hintId,
    revision,
    techniqueKey: presentation.identity.technique_key,
    technique: presentation.identity.name,
    shortName: presentation.identity.short_name,
    type,
    rating: presentation.identity.rating_tenths / 10,
    affects,
    placement,
    eliminationCount: effects.elimination_count,
    views: presentation.views.map((view) => ({
      id: view.key,
      label: view.label,
      candidateMarks: view.candidate_marks.map((mark) => ({
        candidate: mapCandidate(mark.candidate),
        roles: mapRoles(mark.roles),
      })),
      cellMarks: view.cell_marks.map((mark) => ({
        cell: cellRefFromIndex(mark.cell),
        roles: mapRoles(mark.roles),
      })),
      regions: view.region_marks.map((mark): RegionMark => {
        const source = topologyRegions.get(regionKey(mark.region_type, mark.region_index))
        if (!source) throw new Error(`presentation references missing region ${mark.region_type}:${mark.region_index}`)
        return {
          id: topologyRegionId(source),
          label: source.label,
          cells: source.cells.map(cellRefFromIndex),
          roles: mapRoles(mark.roles),
        }
      }),
      links: view.links.map((link, index) => ({
        id: `${view.key}-link-${index + 1}`,
        from: mapEndpoint(link.from),
        to: mapEndpoint(link.to),
        role: LINK_ROLES[link.kind],
        direction: link.directed ? 'forward' : 'both',
        cause: mapCause(link.cause),
      })),
      // The wire protocol carries ordered semantic edges, not a fabricated
      // singleton chain path. Legacy fixtures can continue to supply one.
      chainCells: [],
    })),
    explanation: { blocks: presentation.explanation.blocks.map(mapExplanationBlock) },
  }
}
