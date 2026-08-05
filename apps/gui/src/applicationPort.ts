export const PROTOCOL_VERSION = 2 as const

export type DecimalString = string
export type NonZeroDecimalString = string
export type RequestId = number

export type NonConsecutiveDto = 'off' | 'orthogonal' | 'orthogonal_cyclic' | 'diagonal' | 'diagonal_cyclic'
export type RatingModeDto = 'original' | 'revised'
export type SearchPolicyDto = 'compatibility' | 'forge'

export interface VariantDto {
  blocks: boolean
  disjoint_groups: boolean
  windows: boolean
  sudoku_x: boolean
  girandola: boolean
  asterisk: boolean
  center_dot: boolean
  anti_ferz: boolean
  anti_knight: boolean
  toroidal: boolean
  non_consecutive: NonConsecutiveDto
  forbidden_pairs: boolean
}

export interface EngineDto {
  variant_latin: boolean
  rating_mode: RatingModeDto
  search_policy: SearchPolicyDto
  forcing_chain_plus: 0 | 1 | 2
  unique_loop_fix: boolean
  bug_fix: boolean
  java_default_technique_profile: boolean
}

export type VariantInputDto = Partial<VariantDto>
export type EngineInputDto = Partial<EngineDto>

interface RequestBase {
  protocol_version: typeof PROTOCOL_VERSION
  request_id: RequestId
}

export type ApplicationRequestDto = RequestBase & (
  | { command: 'create_session'; puzzle: string; variant?: VariantInputDto; engine?: EngineInputDto }
  | { command: 'next_hint'; expected_revision: DecimalString }
  | { command: 'apply_hint'; expected_revision: DecimalString; hint_id: NonZeroDecimalString }
  | { command: 'place_value'; expected_revision: DecimalString; cell: number; digit: number }
  | { command: 'toggle_candidate'; expected_revision: DecimalString; cell: number; digit: number }
  | { command: 'undo'; expected_revision: DecimalString }
  | { command: 'redo'; expected_revision: DecimalString }
)

export interface SessionSnapshotDto {
  revision: DecimalString
  values: number[]
  candidate_masks: number[]
  givens: boolean[]
  can_undo: boolean
  can_redo: boolean
}

export type RegionTypeDto = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9
export type TopologyFamilyDto =
  | 'block'
  | 'row'
  | 'column'
  | 'disjoint_group'
  | 'window'
  | 'main_diagonal'
  | 'anti_diagonal'
  | 'girandola'
  | 'asterisk'
  | 'center_dot'

export interface TopologyRegionDto {
  region_type: RegionTypeDto
  region_index: number
  family_key: TopologyFamilyDto
  label: string
  cells: number[]
}

export interface TopologyDto {
  variant: VariantDto
  regions: TopologyRegionDto[]
}

export interface CandidateRefDto {
  cell: number
  digit: number
}

export interface CellMarkDto {
  cell: number
  roles: number
}

export interface RegionMarkDto {
  region_type: RegionTypeDto
  region_index: number
  roles: number
}

export interface CandidateMarkDto {
  candidate: CandidateRefDto
  roles: number
}

export type LinkEndpointDto =
  | { type: 'candidate'; cell: number; digit: number }
  | { type: 'candidate_group'; representative: CandidateRefDto; members: CandidateRefDto[] }
  | { type: 'cell_center'; cell: number }

export type LinkKindDto = 'strong' | 'grouped_strong' | 'weak' | 'implication'

export type LinkCauseDto =
  | { type: 'cell' }
  | { type: 'region'; region_type: RegionTypeDto; region_index: number }
  | { type: 'visibility' }
  | { type: 'derived' }

export interface CandidateLinkDto {
  from: LinkEndpointDto
  to: LinkEndpointDto
  kind: LinkKindDto
  cause: LinkCauseDto
  directed: boolean
}

export type ExplanationInlineDto =
  | { type: 'text'; text: string }
  | { type: 'technique'; technique_key: string }
  | { type: 'cell'; cell: number }
  | { type: 'digit'; digit: number }
  | { type: 'region'; region_type: RegionTypeDto; region_index: number }
  | { type: 'candidate'; cell: number; digit: number }

export type ExplanationBlockDto =
  | { type: 'paragraph'; inlines: ExplanationInlineDto[] }
  | { type: 'unordered_list'; items: ExplanationInlineDto[][] }

export interface ExplanationDocDto {
  blocks: ExplanationBlockDto[]
}

export interface HintIdentityDto {
  technique_key: string
  name: string
  short_name: string
  rating_tenths: number
}

export interface HintViewDto {
  key: string
  label: string
  cell_marks: CellMarkDto[]
  region_marks: RegionMarkDto[]
  candidate_marks: CandidateMarkDto[]
  links: CandidateLinkDto[]
}

export interface HintPresentationDto {
  identity: HintIdentityDto
  views: HintViewDto[]
  explanation: ExplanationDocDto
}

export interface HintPresentationEnvelopeDto {
  protocol_version: typeof PROTOCOL_VERSION
  revision: DecimalString
  presentation: HintPresentationDto
}

export interface CandidateRemovalDto {
  cell: number
  digits: number
}

export interface HintEffectsDto {
  placement: CandidateRefDto | null
  removals: CandidateRemovalDto[]
  elimination_count: number
}

export interface UnsupportedPresentationDto {
  technique_key: string
  kind: 'missing_chain_proof' | 'evidence_not_implemented'
}

export interface PortGapDto {
  code: 'producer_not_ported' | 'indirect_techniques' | 'legacy_fc_plus_2'
  message: string
}

export type NextHintOutcomeDto =
  | {
    outcome: 'presented'
    hint_id: NonZeroDecimalString
    presentation: HintPresentationDto
    effects: HintEffectsDto
  }
  | {
    outcome: 'unsupported'
    hint_id: NonZeroDecimalString
    unsupported: UnsupportedPresentationDto
    effects: HintEffectsDto
  }
  | { outcome: 'none' }
  | { outcome: 'incomplete'; gap: PortGapDto }

export interface PortErrorDto {
  code: string
  message: string
  expected_revision?: DecimalString
  actual_revision?: DecimalString
}

interface ResponseBase {
  protocol_version: typeof PROTOCOL_VERSION
  request_id: RequestId
}

export type ApplicationResponseDto = ResponseBase & (
  | { response: 'session_created'; snapshot: SessionSnapshotDto; topology: TopologyDto }
  | { response: 'snapshot'; snapshot: SessionSnapshotDto }
  | { response: 'next_hint'; revision: DecimalString; outcome: NextHintOutcomeDto }
  | { response: 'error'; error: PortErrorDto }
)

/** A transport adapter whose async boundary already returns validated protocol DTOs. */
export interface ApplicationPort {
  dispatch(request: ApplicationRequestDto): Promise<ApplicationResponseDto>
}

/** A lower-level adapter may use this helper before exposing `ApplicationPort`. */
export async function dispatchValidated(
  dispatch: (request: ApplicationRequestDto) => Promise<unknown>,
  request: ApplicationRequestDto,
): Promise<ApplicationResponseDto> {
  return parseApplicationResponse(await dispatch(request), request.request_id)
}

export class ApplicationProtocolError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'ApplicationProtocolError'
  }
}

const U64_MAX = 18_446_744_073_709_551_615n
const DECIMAL_PATTERN = /^(0|[1-9][0-9]*)$/
const ROLE_MASK = 0x00ff
const CANDIDATE_MASK = 0x03fe
const TOPOLOGY_FAMILIES = [
  'block',
  'row',
  'column',
  'disjoint_group',
  'window',
  'main_diagonal',
  'anti_diagonal',
  'girandola',
  'asterisk',
  'center_dot',
] as const
const NON_CONSECUTIVE_MODES = ['off', 'orthogonal', 'orthogonal_cyclic', 'diagonal', 'diagonal_cyclic'] as const
const LINK_KINDS = ['strong', 'grouped_strong', 'weak', 'implication'] as const

const fail = (path: string, expectation: string): never => {
  throw new ApplicationProtocolError(`${path} must be ${expectation}`)
}

const objectValue = (value: unknown, path: string): Record<string, unknown> => {
  if (value == null || typeof value !== 'object' || Array.isArray(value)) return fail(path, 'an object')
  return value as Record<string, unknown>
}

const stringValue = (value: unknown, path: string): string => {
  if (typeof value !== 'string') return fail(path, 'a string')
  return value
}

const nonEmptyStringValue = (value: unknown, path: string): string => {
  const result = stringValue(value, path)
  if (result.length === 0) return fail(path, 'a non-empty string')
  return result
}

const booleanValue = (value: unknown, path: string): boolean => {
  if (typeof value !== 'boolean') return fail(path, 'a boolean')
  return value
}

const integerValue = (value: unknown, path: string, minimum: number, maximum: number): number => {
  if (!Number.isInteger(value) || (value as number) < minimum || (value as number) > maximum) {
    return fail(path, `an integer from ${minimum} through ${maximum}`)
  }
  return value as number
}

const enumValue = <T extends string>(value: unknown, path: string, values: readonly T[]): T => {
  if (typeof value !== 'string' || !values.includes(value as T)) return fail(path, `one of ${values.join(', ')}`)
  return value as T
}

const arrayValue = <T>(value: unknown, path: string, parse: (item: unknown, path: string) => T): T[] => {
  if (!Array.isArray(value)) return fail(path, 'an array')
  return value.map((item, index) => parse(item, `${path}[${index}]`))
}

const fixedArrayValue = <T>(
  value: unknown,
  path: string,
  length: number,
  parse: (item: unknown, path: string) => T,
): T[] => {
  const result = arrayValue(value, path, parse)
  if (result.length !== length) return fail(path, `an array of length ${length}`)
  return result
}

const decimalValue = (value: unknown, path: string, nonZero = false): DecimalString | NonZeroDecimalString => {
  const decimal = stringValue(value, path)
  if (!DECIMAL_PATTERN.test(decimal) || decimal.length > 20 || BigInt(decimal) > U64_MAX || (nonZero && decimal === '0')) {
    return fail(path, nonZero ? 'a canonical non-zero u64 decimal string' : 'a canonical u64 decimal string')
  }
  return decimal
}

const requestIdValue = (value: unknown, path: string): RequestId => integerValue(value, path, 0, 4_294_967_295)
const cellValue = (value: unknown, path: string) => integerValue(value, path, 0, 80)
const digitValue = (value: unknown, path: string) => integerValue(value, path, 1, 9)
const regionTypeValue = (value: unknown, path: string) => integerValue(value, path, 0, 9) as RegionTypeDto
const regionIndexValue = (value: unknown, path: string) => integerValue(value, path, 0, 8)

const roleMaskValue = (value: unknown, path: string) => {
  const mask = integerValue(value, path, 0, 65_535)
  if ((mask & ~ROLE_MASK) !== 0) return fail(path, 'a mask containing only known highlight-role bits')
  return mask
}

const candidateMaskValue = (value: unknown, path: string) => {
  const mask = integerValue(value, path, 0, 65_535)
  if ((mask & ~CANDIDATE_MASK) !== 0) return fail(path, 'a candidate mask using only bits 1 through 9')
  return mask
}

const parseVariant = (value: unknown, path: string): VariantDto => {
  const source = objectValue(value, path)
  return {
    blocks: booleanValue(source.blocks, `${path}.blocks`),
    disjoint_groups: booleanValue(source.disjoint_groups, `${path}.disjoint_groups`),
    windows: booleanValue(source.windows, `${path}.windows`),
    sudoku_x: booleanValue(source.sudoku_x, `${path}.sudoku_x`),
    girandola: booleanValue(source.girandola, `${path}.girandola`),
    asterisk: booleanValue(source.asterisk, `${path}.asterisk`),
    center_dot: booleanValue(source.center_dot, `${path}.center_dot`),
    anti_ferz: booleanValue(source.anti_ferz, `${path}.anti_ferz`),
    anti_knight: booleanValue(source.anti_knight, `${path}.anti_knight`),
    toroidal: booleanValue(source.toroidal, `${path}.toroidal`),
    non_consecutive: enumValue(source.non_consecutive, `${path}.non_consecutive`, NON_CONSECUTIVE_MODES),
    forbidden_pairs: booleanValue(source.forbidden_pairs, `${path}.forbidden_pairs`),
  }
}

const parseCandidateRef = (value: unknown, path: string): CandidateRefDto => {
  const source = objectValue(value, path)
  return { cell: cellValue(source.cell, `${path}.cell`), digit: digitValue(source.digit, `${path}.digit`) }
}

export const parseSessionSnapshot = (value: unknown, path = 'snapshot'): SessionSnapshotDto => {
  const source = objectValue(value, path)
  const values = fixedArrayValue(source.values, `${path}.values`, 81, (item, itemPath) => integerValue(item, itemPath, 0, 9))
  const givens = fixedArrayValue(source.givens, `${path}.givens`, 81, booleanValue)
  givens.forEach((given, index) => {
    if (given && values[index] === 0) fail(`${path}.givens[${index}]`, 'false when the corresponding value is unresolved')
  })
  return {
    revision: decimalValue(source.revision, `${path}.revision`),
    values,
    candidate_masks: fixedArrayValue(source.candidate_masks, `${path}.candidate_masks`, 81, candidateMaskValue),
    givens,
    can_undo: booleanValue(source.can_undo, `${path}.can_undo`),
    can_redo: booleanValue(source.can_redo, `${path}.can_redo`),
  }
}

const parseTopologyRegion = (value: unknown, path: string): TopologyRegionDto => {
  const source = objectValue(value, path)
  const regionType = regionTypeValue(source.region_type, `${path}.region_type`)
  const familyKey = enumValue(source.family_key, `${path}.family_key`, TOPOLOGY_FAMILIES)
  if (familyKey !== TOPOLOGY_FAMILIES[regionType]) return fail(`${path}.family_key`, `the family for region type ${regionType}`)
  const cells = fixedArrayValue(source.cells, `${path}.cells`, 9, cellValue)
  if (new Set(cells).size !== cells.length) return fail(`${path}.cells`, 'nine distinct cell indexes')
  return {
    region_type: regionType,
    region_index: regionIndexValue(source.region_index, `${path}.region_index`),
    family_key: familyKey,
    label: nonEmptyStringValue(source.label, `${path}.label`),
    cells,
  }
}

export const parseTopology = (value: unknown, path = 'topology'): TopologyDto => {
  const source = objectValue(value, path)
  return {
    variant: parseVariant(source.variant, `${path}.variant`),
    regions: arrayValue(source.regions, `${path}.regions`, parseTopologyRegion),
  }
}

const parseLinkEndpoint = (value: unknown, path: string): LinkEndpointDto => {
  const source = objectValue(value, path)
  const type = enumValue(source.type, `${path}.type`, ['candidate', 'candidate_group', 'cell_center'] as const)
  if (type === 'candidate') {
    return { type, cell: cellValue(source.cell, `${path}.cell`), digit: digitValue(source.digit, `${path}.digit`) }
  }
  if (type === 'cell_center') return { type, cell: cellValue(source.cell, `${path}.cell`) }

  const representative = parseCandidateRef(source.representative, `${path}.representative`)
  const members = arrayValue(source.members, `${path}.members`, parseCandidateRef)
  if (members.length < 2 || members.length > 9) return fail(`${path}.members`, 'an array of 2 through 9 candidates')
  if (members.some((member) => member.digit !== representative.digit)) {
    return fail(`${path}.members`, 'candidates with the representative digit')
  }
  if (!members.some((member) => member.cell === representative.cell && member.digit === representative.digit)) {
    return fail(`${path}.members`, 'an array containing the exact representative candidate')
  }
  if (new Set(members.map((member) => `${member.cell}:${member.digit}`)).size !== members.length) {
    return fail(`${path}.members`, 'distinct candidates')
  }
  return { type, representative, members }
}

const parseLinkCause = (value: unknown, path: string): LinkCauseDto => {
  const source = objectValue(value, path)
  const type = enumValue(source.type, `${path}.type`, ['cell', 'region', 'visibility', 'derived'] as const)
  if (type === 'region') {
    return {
      type,
      region_type: regionTypeValue(source.region_type, `${path}.region_type`),
      region_index: regionIndexValue(source.region_index, `${path}.region_index`),
    }
  }
  return { type }
}

const parseCandidateLink = (value: unknown, path: string): CandidateLinkDto => {
  const source = objectValue(value, path)
  return {
    from: parseLinkEndpoint(source.from, `${path}.from`),
    to: parseLinkEndpoint(source.to, `${path}.to`),
    kind: enumValue(source.kind, `${path}.kind`, LINK_KINDS),
    cause: parseLinkCause(source.cause, `${path}.cause`),
    directed: booleanValue(source.directed, `${path}.directed`),
  }
}

const parseExplanationInline = (value: unknown, path: string): ExplanationInlineDto => {
  const source = objectValue(value, path)
  const type = enumValue(source.type, `${path}.type`, ['text', 'technique', 'cell', 'digit', 'region', 'candidate'] as const)
  switch (type) {
    case 'text':
      return { type, text: stringValue(source.text, `${path}.text`) }
    case 'technique':
      return { type, technique_key: nonEmptyStringValue(source.technique_key, `${path}.technique_key`) }
    case 'cell':
      return { type, cell: cellValue(source.cell, `${path}.cell`) }
    case 'digit':
      return { type, digit: digitValue(source.digit, `${path}.digit`) }
    case 'region':
      return {
        type,
        region_type: regionTypeValue(source.region_type, `${path}.region_type`),
        region_index: regionIndexValue(source.region_index, `${path}.region_index`),
      }
    case 'candidate':
      return { type, cell: cellValue(source.cell, `${path}.cell`), digit: digitValue(source.digit, `${path}.digit`) }
  }
}

const parseExplanationBlock = (value: unknown, path: string): ExplanationBlockDto => {
  const source = objectValue(value, path)
  const type = enumValue(source.type, `${path}.type`, ['paragraph', 'unordered_list'] as const)
  if (type === 'paragraph') {
    return { type, inlines: arrayValue(source.inlines, `${path}.inlines`, parseExplanationInline) }
  }
  return {
    type,
    items: arrayValue(source.items, `${path}.items`, (item, itemPath) => arrayValue(item, itemPath, parseExplanationInline)),
  }
}

export const parseHintPresentation = (value: unknown, path = 'presentation'): HintPresentationDto => {
  const source = objectValue(value, path)
  const identitySource = objectValue(source.identity, `${path}.identity`)
  const explanationSource = objectValue(source.explanation, `${path}.explanation`)
  return {
    identity: {
      technique_key: nonEmptyStringValue(identitySource.technique_key, `${path}.identity.technique_key`),
      name: nonEmptyStringValue(identitySource.name, `${path}.identity.name`),
      short_name: nonEmptyStringValue(identitySource.short_name, `${path}.identity.short_name`),
      rating_tenths: integerValue(identitySource.rating_tenths, `${path}.identity.rating_tenths`, 0, 65_535),
    },
    views: arrayValue(source.views, `${path}.views`, (view, viewPath): HintViewDto => {
      const viewSource = objectValue(view, viewPath)
      return {
        key: nonEmptyStringValue(viewSource.key, `${viewPath}.key`),
        label: nonEmptyStringValue(viewSource.label, `${viewPath}.label`),
        cell_marks: arrayValue(viewSource.cell_marks, `${viewPath}.cell_marks`, (mark, markPath) => {
          const markSource = objectValue(mark, markPath)
          return { cell: cellValue(markSource.cell, `${markPath}.cell`), roles: roleMaskValue(markSource.roles, `${markPath}.roles`) }
        }),
        region_marks: arrayValue(viewSource.region_marks, `${viewPath}.region_marks`, (mark, markPath) => {
          const markSource = objectValue(mark, markPath)
          return {
            region_type: regionTypeValue(markSource.region_type, `${markPath}.region_type`),
            region_index: regionIndexValue(markSource.region_index, `${markPath}.region_index`),
            roles: roleMaskValue(markSource.roles, `${markPath}.roles`),
          }
        }),
        candidate_marks: arrayValue(viewSource.candidate_marks, `${viewPath}.candidate_marks`, (mark, markPath) => {
          const markSource = objectValue(mark, markPath)
          return {
            candidate: parseCandidateRef(markSource.candidate, `${markPath}.candidate`),
            roles: roleMaskValue(markSource.roles, `${markPath}.roles`),
          }
        }),
        links: arrayValue(viewSource.links, `${viewPath}.links`, parseCandidateLink),
      }
    }),
    explanation: {
      blocks: arrayValue(explanationSource.blocks, `${path}.explanation.blocks`, parseExplanationBlock),
    },
  }
}

const parseHintEffects = (value: unknown, path: string): HintEffectsDto => {
  const source = objectValue(value, path)
  if (!Object.hasOwn(source, 'placement')) return fail(`${path}.placement`, 'a candidate or null')
  const placement = source.placement == null ? null : parseCandidateRef(source.placement, `${path}.placement`)
  const removals = arrayValue(source.removals, `${path}.removals`, (removal, removalPath): CandidateRemovalDto => {
    const removalSource = objectValue(removal, removalPath)
    const digits = candidateMaskValue(removalSource.digits, `${removalPath}.digits`)
    if (digits === 0) return fail(`${removalPath}.digits`, 'a non-empty candidate mask')
    return { cell: cellValue(removalSource.cell, `${removalPath}.cell`), digits }
  })
  const eliminationCount = integerValue(source.elimination_count, `${path}.elimination_count`, 0, 65_535)
  const projectedCount = removals.reduce((total, removal) => {
    let digits = removal.digits
    let count = 0
    while (digits !== 0) {
      count += digits & 1
      digits >>>= 1
    }
    return total + count
  }, 0)
  if (eliminationCount !== projectedCount) return fail(`${path}.elimination_count`, `${projectedCount} for the supplied removal masks`)
  return { placement, removals, elimination_count: eliminationCount }
}

const parseNextHintOutcome = (value: unknown, path: string): NextHintOutcomeDto => {
  const source = objectValue(value, path)
  const outcome = enumValue(source.outcome, `${path}.outcome`, ['presented', 'unsupported', 'none', 'incomplete'] as const)
  switch (outcome) {
    case 'presented':
      return {
        outcome,
        hint_id: decimalValue(source.hint_id, `${path}.hint_id`, true),
        presentation: parseHintPresentation(source.presentation, `${path}.presentation`),
        effects: parseHintEffects(source.effects, `${path}.effects`),
      }
    case 'unsupported': {
      const unsupportedSource = objectValue(source.unsupported, `${path}.unsupported`)
      return {
        outcome,
        hint_id: decimalValue(source.hint_id, `${path}.hint_id`, true),
        unsupported: {
          technique_key: nonEmptyStringValue(unsupportedSource.technique_key, `${path}.unsupported.technique_key`),
          kind: enumValue(
            unsupportedSource.kind,
            `${path}.unsupported.kind`,
            ['missing_chain_proof', 'evidence_not_implemented'] as const,
          ),
        },
        effects: parseHintEffects(source.effects, `${path}.effects`),
      }
    }
    case 'none':
      return { outcome }
    case 'incomplete': {
      const gapSource = objectValue(source.gap, `${path}.gap`)
      return {
        outcome,
        gap: {
          code: enumValue(
            gapSource.code,
            `${path}.gap.code`,
            ['producer_not_ported', 'indirect_techniques', 'legacy_fc_plus_2'] as const,
          ),
          message: nonEmptyStringValue(gapSource.message, `${path}.gap.message`),
        },
      }
    }
  }
}

export function parseApplicationResponse(value: unknown, expectedRequestId?: RequestId): ApplicationResponseDto {
  const source = objectValue(value, 'response')
  const protocolVersion = integerValue(source.protocol_version, 'response.protocol_version', 0, 65_535)
  if (protocolVersion !== PROTOCOL_VERSION) return fail('response.protocol_version', `${PROTOCOL_VERSION}`)
  const requestId = requestIdValue(source.request_id, 'response.request_id')
  if (expectedRequestId != null && requestId !== expectedRequestId) {
    return fail('response.request_id', `${expectedRequestId} for the pending request`)
  }
  const response = enumValue(source.response, 'response.response', ['session_created', 'snapshot', 'next_hint', 'error'] as const)
  switch (response) {
    case 'session_created':
      return {
        protocol_version: PROTOCOL_VERSION,
        request_id: requestId,
        response,
        snapshot: parseSessionSnapshot(source.snapshot, 'response.snapshot'),
        topology: parseTopology(source.topology, 'response.topology'),
      }
    case 'snapshot':
      return {
        protocol_version: PROTOCOL_VERSION,
        request_id: requestId,
        response,
        snapshot: parseSessionSnapshot(source.snapshot, 'response.snapshot'),
      }
    case 'next_hint':
      return {
        protocol_version: PROTOCOL_VERSION,
        request_id: requestId,
        response,
        revision: decimalValue(source.revision, 'response.revision'),
        outcome: parseNextHintOutcome(source, 'response'),
      }
    case 'error': {
      const errorSource = objectValue(source.error, 'response.error')
      const hasExpectedRevision = Object.hasOwn(errorSource, 'expected_revision')
      const hasActualRevision = Object.hasOwn(errorSource, 'actual_revision')
      if (hasExpectedRevision !== hasActualRevision) {
        return fail('response.error', 'an error with both revision fields or neither revision field')
      }
      const revisions = hasExpectedRevision ? {
        expected_revision: decimalValue(errorSource.expected_revision, 'response.error.expected_revision'),
        actual_revision: decimalValue(errorSource.actual_revision, 'response.error.actual_revision'),
      } : {}
      return {
        protocol_version: PROTOCOL_VERSION,
        request_id: requestId,
        response,
        error: {
          code: nonEmptyStringValue(errorSource.code, 'response.error.code'),
          message: nonEmptyStringValue(errorSource.message, 'response.error.message'),
          ...revisions,
        },
      }
    }
  }
}
