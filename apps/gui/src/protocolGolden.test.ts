import { describe, expect, it } from 'vitest'
import golden from './fixtures/protocol-v2-hidden-single.json'
import { PROTOCOL_VERSION, parseApplicationResponse } from './applicationPort'

describe('Rust protocol-v2 golden fixture', () => {
  it('validates and preserves the hidden-single session sequence', () => {
    expect(golden.protocol_version).toBe(PROTOCOL_VERSION)
    expect(golden.scenario).toBe('hidden_single_round_trip')
    expect(golden.steps.map((step) => step.request.command)).toEqual([
      'create_session',
      'next_hint',
      'apply_hint',
    ])

    const [created, hinted, applied] = golden.steps.map((step) =>
      parseApplicationResponse(step.response, step.request.request_id),
    )

    expect(created?.response).toBe('session_created')
    if (created?.response !== 'session_created') throw new Error('expected session_created')
    expect(created.snapshot.revision).toBe('0')
    expect(created.topology.regions).toHaveLength(27)

    expect(hinted?.response).toBe('next_hint')
    if (hinted?.response !== 'next_hint' || hinted.outcome.outcome !== 'presented') {
      throw new Error('expected a presented next_hint')
    }
    expect(hinted.revision).toBe(created.snapshot.revision)
    expect(hinted.outcome.presentation.identity.technique_key).toBe('hidden_single')
    expect(hinted.outcome.effects).toEqual({
      placement: { cell: 8, digit: 9 },
      removals: [],
      elimination_count: 0,
    })
    expect(golden.steps[2]?.request.hint_id).toBe(hinted.outcome.hint_id)

    expect(applied?.response).toBe('snapshot')
    if (applied?.response !== 'snapshot') throw new Error('expected an applied snapshot')
    expect(applied.snapshot.revision).toBe('1')
    expect(applied.snapshot.values[8]).toBe(9)
    expect(applied.snapshot.can_undo).toBe(true)
  })
})
