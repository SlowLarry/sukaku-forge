import { describe, expect, it } from 'vitest'
import { ApplicationProtocolError, PROTOCOL_VERSION, parseApplicationResponse, parseTopology } from './applicationPort'

const canonicalPresentation = {
  identity: {
    technique_key: 'four_strong_links',
    name: 'Grouped 4 Strong links',
    short_name: '4SL',
    rating_tenths: 61,
  },
  views: [{
    key: 'chain-1',
    label: 'Chain 1',
    cell_marks: [{ cell: 1, roles: 0x0041 }],
    region_marks: [{ region_type: 1, region_index: 0, roles: 0x0042 }],
    candidate_marks: [{ candidate: { cell: 1, digit: 2 }, roles: 0x000c }],
    links: [{
      from: {
        type: 'candidate_group',
        representative: { cell: 1, digit: 2 },
        members: [{ cell: 1, digit: 2 }, { cell: 2, digit: 2 }],
      },
      to: { type: 'candidate', cell: 58, digit: 2 },
      kind: 'grouped_strong',
      cause: { type: 'region', region_type: 1, region_index: 0 },
      directed: false,
    }],
  }],
  explanation: {
    blocks: [
      {
        type: 'paragraph',
        inlines: [
          { type: 'technique', technique_key: 'four_strong_links' },
          { type: 'text', text: ' removes ' },
          { type: 'candidate', cell: 39, digit: 2 },
        ],
      },
      {
        type: 'unordered_list',
        items: [[
          { type: 'region', region_type: 1, region_index: 0 },
          { type: 'text', text: ' contains digit ' },
          { type: 'digit', digit: 2 },
        ]],
      },
    ],
  },
}

const canonicalVariant = {
  blocks: true,
  disjoint_groups: false,
  windows: false,
  sudoku_x: false,
  girandola: false,
  asterisk: false,
  center_dot: false,
  anti_ferz: false,
  anti_knight: false,
  toroidal: false,
  non_consecutive: 'off',
  forbidden_pairs: false,
}

describe('application port response validation', () => {
  it('accepts canonical session snapshot and topology arrays from Rust', () => {
    const values = Array<number>(81).fill(0)
    const candidateMasks = Array<number>(81).fill(0x03fe)
    const givens = Array<boolean>(81).fill(false)
    values[0] = 5
    candidateMasks[0] = 0
    givens[0] = true

    const response = parseApplicationResponse({
      protocol_version: PROTOCOL_VERSION,
      request_id: 1,
      response: 'session_created',
      snapshot: {
        revision: '0',
        values,
        candidate_masks: candidateMasks,
        givens,
        can_undo: false,
        can_redo: false,
      },
      topology: {
        variant: canonicalVariant,
        regions: [{
          region_type: 1,
          region_index: 0,
          family_key: 'row',
          label: 'Row 1',
          cells: [0, 1, 2, 3, 4, 5, 6, 7, 8],
        }],
      },
    }, 1)

    expect(response.response).toBe('session_created')
    if (response.response !== 'session_created') throw new Error('expected a created session')
    expect(response.snapshot.values).toHaveLength(81)
    expect(response.topology.regions[0]).toMatchObject({ family_key: 'row', cells: [0, 1, 2, 3, 4, 5, 6, 7, 8] })
  })

  it('accepts the canonical protocol-v3 presented-hint shape without coercing decimal IDs', () => {
    const raw = {
      protocol_version: PROTOCOL_VERSION,
      request_id: 17,
      response: 'next_hint',
      revision: '9007199254740993',
      outcome: 'presented',
      hint_id: '18446744073709551615',
      presentation: canonicalPresentation,
      effects: {
        placement: null,
        removals: [{ cell: 39, digits: (1 << 2) | (1 << 9) }],
        elimination_count: 2,
      },
    }

    const response = parseApplicationResponse(raw, 17)

    expect(response.response).toBe('next_hint')
    if (response.response !== 'next_hint' || response.outcome.outcome !== 'presented') throw new Error('expected a presented hint')
    expect(response.revision).toBe('9007199254740993')
    expect(response.outcome.hint_id).toBe('18446744073709551615')
    expect(response.outcome.presentation.views[0]?.links[0]?.from).toEqual({
      type: 'candidate_group',
      representative: { cell: 1, digit: 2 },
      members: [{ cell: 1, digit: 2 }, { cell: 2, digit: 2 }],
    })
    expect(response.outcome.presentation.explanation.blocks[0]?.type).toBe('paragraph')
  })

  it('accepts a stale-revision error response and correlates its numeric request ID', () => {
    const response = parseApplicationResponse({
      protocol_version: PROTOCOL_VERSION,
      request_id: 12,
      response: 'error',
      error: {
        code: 'stale_revision',
        message: 'stale session revision 7; current revision is 8',
        expected_revision: '7',
        actual_revision: '8',
      },
    }, 12)

    expect(response).toEqual({
      protocol_version: PROTOCOL_VERSION,
      request_id: 12,
      response: 'error',
      error: {
        code: 'stale_revision',
        message: 'stale session revision 7; current revision is 8',
        expected_revision: '7',
        actual_revision: '8',
      },
    })
  })

  it('preserves exact effects for an explicitly unsupported presentation', () => {
    const response = parseApplicationResponse({
      protocol_version: PROTOCOL_VERSION,
      request_id: 19,
      response: 'next_hint',
      revision: '4',
      outcome: 'unsupported',
      hint_id: '7',
      unsupported: {
        technique_key: 'nested_forcing_chain',
        kind: 'missing_chain_proof',
      },
      effects: {
        placement: null,
        removals: [{ cell: 80, digits: 1 << 9 }],
        elimination_count: 1,
      },
    })

    expect(response.response).toBe('next_hint')
    if (response.response !== 'next_hint' || response.outcome.outcome !== 'unsupported') throw new Error('expected unsupported')
    expect(response.outcome.effects).toEqual({
      placement: null,
      removals: [{ cell: 80, digits: 1 << 9 }],
      elimination_count: 1,
    })
  })

  it('accepts an ordered all-hints catalog and one lazily materialized entry', () => {
    const effects = {
      placement: { cell: 8, digit: 9 },
      removals: [],
      elimination_count: 0,
    }
    const summary = {
      hint_id: '1',
      category: 'direct',
      group_key: 'hidden_single',
      group_name: 'Hidden Single',
      identity: canonicalPresentation.identity,
      effects,
      filter_effects: effects,
    }
    const catalog = parseApplicationResponse({
      protocol_version: PROTOCOL_VERSION,
      request_id: 20,
      response: 'all_hints',
      revision: '0',
      outcome: 'complete',
      hints: [summary, { ...summary, hint_id: '2', category: 'indirect' }],
    }, 20)

    expect(catalog.response).toBe('all_hints')
    if (catalog.response !== 'all_hints' || catalog.outcome.outcome !== 'complete') {
      throw new Error('expected an all-hints catalog')
    }
    expect(catalog.outcome.hints.map((hint) => hint.hint_id)).toEqual(['1', '2'])
    expect(catalog.outcome.hints.map((hint) => hint.category)).toEqual(['direct', 'indirect'])

    const detail = parseApplicationResponse({
      protocol_version: PROTOCOL_VERSION,
      request_id: 21,
      response: 'hint',
      revision: '0',
      hint_id: '2',
      outcome: 'presented',
      presentation: canonicalPresentation,
      effects,
    }, 21)
    expect(detail.response).toBe('hint')
    if (detail.response !== 'hint') throw new Error('expected a selected hint detail')
    expect(detail.hint_id).toBe('2')
    expect(detail.outcome.outcome).toBe('presented')
  })

  it('validates all-hints confirmation, partial catalogs, and unique opaque IDs', () => {
    const confirmation = parseApplicationResponse({
      protocol_version: PROTOCOL_VERSION,
      request_id: 22,
      response: 'all_hints',
      revision: '9',
      outcome: 'confirmation_required',
    })
    expect(confirmation.response === 'all_hints' && confirmation.outcome.outcome).toBe('confirmation_required')

    const summary = {
      hint_id: '7',
      category: 'indirect',
      group_key: 'four_strong_links',
      group_name: '4 Strong links',
      identity: canonicalPresentation.identity,
      effects: { placement: null, removals: [], elimination_count: 0 },
      filter_effects: { placement: null, removals: [], elimination_count: 0 },
    }
    const incomplete = parseApplicationResponse({
      protocol_version: PROTOCOL_VERSION,
      request_id: 23,
      response: 'all_hints',
      revision: '9',
      outcome: 'incomplete',
      hints: [summary],
      gap: { code: 'producer_not_ported', message: 'not ported' },
    })
    expect(incomplete.response === 'all_hints' && incomplete.outcome.outcome).toBe('incomplete')

    expect(() => parseApplicationResponse({
      protocol_version: PROTOCOL_VERSION,
      request_id: 24,
      response: 'all_hints',
      revision: '9',
      outcome: 'complete',
      hints: [summary, summary],
    })).toThrowError(/unique within the catalog/)
  })

  it('rejects a candidate group that omits its exact representative', () => {
    const raw = {
      protocol_version: PROTOCOL_VERSION,
      request_id: 3,
      response: 'next_hint',
      revision: '0',
      outcome: 'presented',
      hint_id: '1',
      presentation: structuredClone(canonicalPresentation),
      effects: { placement: null, removals: [], elimination_count: 0 },
    }
    raw.presentation.views[0]!.links[0]!.from = {
      type: 'candidate_group',
      representative: { cell: 1, digit: 2 },
      members: [{ cell: 2, digit: 2 }, { cell: 3, digit: 2 }],
    }

    expect(() => parseApplicationResponse(raw)).toThrowError(/members must be an array containing the exact representative candidate/)
  })

  it('rejects unsafe decimal spellings, unknown role bits, and stale request correlation', () => {
    const makeRaw = () => ({
      protocol_version: PROTOCOL_VERSION,
      request_id: 8,
      response: 'next_hint',
      revision: '0',
      outcome: 'presented',
      hint_id: '1',
      presentation: structuredClone(canonicalPresentation),
      effects: { placement: null, removals: [], elimination_count: 0 },
    })

    const leadingZero = makeRaw()
    leadingZero.revision = '01'
    expect(() => parseApplicationResponse(leadingZero)).toThrow(ApplicationProtocolError)

    const unknownRole = makeRaw()
    unknownRole.presentation.views[0]!.cell_marks[0]!.roles = 0x0100
    expect(() => parseApplicationResponse(unknownRole)).toThrowError(/known highlight-role bits/)

    expect(() => parseApplicationResponse(makeRaw(), 9)).toThrowError(/pending request/)
  })

  it('rejects topology family/type mismatches and out-of-range region indexes', () => {
    const makeTopology = () => ({
      variant: canonicalVariant,
      regions: [{
        region_type: 1,
        region_index: 0,
        family_key: 'row',
        label: 'Row 1',
        cells: [0, 1, 2, 3, 4, 5, 6, 7, 8],
      }],
    })

    const wrongFamily = makeTopology()
    wrongFamily.regions[0]!.family_key = 'column'
    expect(() => parseTopology(wrongFamily)).toThrowError(/family for region type 1/)

    const invalidIndex = makeTopology()
    invalidIndex.regions[0]!.region_index = 9
    expect(() => parseTopology(invalidIndex)).toThrowError(/integer from 0 through 8/)
  })
})
