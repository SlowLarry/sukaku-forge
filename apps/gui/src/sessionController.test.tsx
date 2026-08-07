import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import type {
  ApplicationPort,
  ApplicationRequestDto,
  ApplicationResponseDto,
  HintPresentationDto,
  SessionSnapshotDto,
  TopologyDto,
} from './applicationPort'
import { PROTOCOL_VERSION } from './applicationPort'
import {
  createSessionController,
  initialSessionControllerState,
  sessionControllerReducer,
  type SessionController,
  type SessionControllerView,
  useSessionController,
} from './sessionController'

type WithoutEnvelope<T> = T extends { protocol_version: number; request_id: number }
  ? Omit<T, 'protocol_version' | 'request_id'>
  : never

type ResponsePayload = WithoutEnvelope<ApplicationResponseDto>

interface PendingDispatch {
  request: ApplicationRequestDto
  resolve: (response: ApplicationResponseDto) => void
  reject: (error: unknown) => void
}

class FakePort implements ApplicationPort {
  readonly requests: ApplicationRequestDto[] = []
  private readonly pending: PendingDispatch[] = []

  dispatch = (request: ApplicationRequestDto) => {
    this.requests.push(request)
    return new Promise<ApplicationResponseDto>((resolve, reject) => {
      this.pending.push({ request, resolve, reject })
    })
  }

  respond(index: number, payload: ResponsePayload) {
    const pending = this.pending[index]
    if (!pending) throw new Error(`no pending request ${index}`)
    pending.resolve({
      protocol_version: PROTOCOL_VERSION,
      request_id: pending.request.request_id,
      ...payload,
    } as ApplicationResponseDto)
  }

  fail(index: number, error: unknown) {
    const pending = this.pending[index]
    if (!pending) throw new Error(`no pending request ${index}`)
    pending.reject(error)
  }
}

const variant: TopologyDto['variant'] = {
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

const emptyTopology = (label = 'Row 1'): TopologyDto => ({
  variant,
  regions: [{
    region_type: 1,
    region_index: 0,
    family_key: 'row',
    label,
    cells: [0, 1, 2, 3, 4, 5, 6, 7, 8],
  }],
})

const snapshot = (
  revision: string,
  update: (value: SessionSnapshotDto) => void = () => undefined,
): SessionSnapshotDto => {
  const value: SessionSnapshotDto = {
    revision,
    values: Array<number>(81).fill(0),
    candidate_masks: Array<number>(81).fill(0x03fe),
    givens: Array<boolean>(81).fill(false),
    can_undo: false,
    can_redo: false,
  }
  update(value)
  return value
}

const presentation: HintPresentationDto = {
  identity: {
    technique_key: 'hidden_single',
    name: 'Hidden Single',
    short_name: 'HS',
    rating_tenths: 12,
  },
  views: [{
    key: 'main',
    label: 'View 1',
    cell_marks: [],
    region_marks: [{ region_type: 1, region_index: 0, roles: 0x0042 }],
    candidate_marks: [{ candidate: { cell: 8, digit: 9 }, roles: 0x0024 }],
    links: [],
  }],
  explanation: {
    blocks: [{
      type: 'paragraph',
      inlines: [
        { type: 'technique', technique_key: 'hidden_single' },
        { type: 'text', text: ' places ' },
        { type: 'candidate', cell: 8, digit: 9 },
      ],
    }],
  },
}

const summary = (hintId: string, cell: number, digit: number) => ({
  hint_id: hintId,
  category: 'direct' as const,
  group_key: 'hidden_single',
  group_name: 'Hidden Single',
  identity: presentation.identity,
  effects: {
    placement: { cell, digit },
    removals: [],
    elimination_count: 0,
  },
  filter_effects: {
    placement: { cell, digit },
    removals: [],
    elimination_count: 0,
  },
})

const HookProbe = ({ port, capture }: {
  port: ApplicationPort
  capture: (view: SessionControllerView) => void
}) => {
  const view = useSessionController(port)
  capture(view)
  return <span>{view.busy ? 'busy' : 'idle'}</span>
}

const initialize = async (
  controller: SessionController,
  port: FakePort,
  requestIndex = 0,
  initialSnapshot = snapshot('0'),
  topology = emptyTopology(),
) => {
  const pending = controller.createSession('.'.repeat(81))
  port.respond(requestIndex, { response: 'session_created', snapshot: initialSnapshot, topology })
  await pending
}

describe('session controller', () => {
  it('creates a session through the injected port and publishes busy/ready state', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    let notifications = 0
    const unsubscribe = controller.subscribe(() => { notifications += 1 })

    const pending = controller.createSession('.'.repeat(81), {
      variant: { sudoku_x: true },
      engine: { rating_mode: 'revised' },
    })

    expect(port.requests[0]).toEqual({
      protocol_version: PROTOCOL_VERSION,
      request_id: 1,
      command: 'create_session',
      puzzle: '.'.repeat(81),
      variant: { sudoku_x: true },
      engine: { rating_mode: 'revised' },
    })
    expect(controller.getState()).toMatchObject({
      busy: true,
      pendingRequestId: 1,
      pendingCommand: 'create_session',
      error: null,
    })

    port.respond(0, {
      response: 'session_created',
      snapshot: snapshot('0'),
      topology: emptyTopology(),
    })
    expect(await pending).toBe(true)

    expect(controller.getState()).toMatchObject({
      busy: false,
      pendingRequestId: null,
      snapshot: { revision: '0', canUndo: false, canRedo: false },
      topology: { regions: [{ label: 'Row 1' }] },
      hint: null,
      error: null,
    })
    expect(controller.getState().snapshot?.values.every((value) => value == null)).toBe(true)
    expect(notifications).toBe(2)
    unsubscribe()
  })

  it('requests and applies a hint, then invalidates it on the authoritative snapshot', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    await initialize(controller, port)

    const next = controller.nextHint()
    expect(port.requests[1]).toMatchObject({
      request_id: 2,
      command: 'next_hint',
      expected_revision: '0',
    })
    port.respond(1, {
      response: 'next_hint',
      revision: '0',
      outcome: {
        outcome: 'presented',
        hint_id: '41',
        presentation,
        effects: { placement: { cell: 8, digit: 9 }, removals: [], elimination_count: 0 },
      },
    })
    await next

    expect(controller.getState().hint).toMatchObject({
      id: '41',
      revision: '0',
      placement: { row: 0, col: 8, digit: 9 },
    })
    expect(controller.getState().hintResult).toEqual({ kind: 'presented', hintId: '41' })

    const applied = controller.applyHint()
    expect(port.requests[2]).toMatchObject({
      request_id: 3,
      command: 'apply_hint',
      expected_revision: '0',
      hint_id: '41',
    })
    expect(controller.getState().hint?.id).toBe('41')

    port.respond(2, {
      response: 'snapshot',
      snapshot: snapshot('1', (value) => {
        value.values[8] = 9
        value.candidate_masks[8] = 0
        value.can_undo = true
      }),
    })
    await applied

    expect(controller.getState()).toMatchObject({
      snapshot: { revision: '1', canUndo: true },
      hint: null,
      hintResult: null,
    })
    expect(controller.getState().snapshot?.values[8]).toBe(9)
  })

  it('stores an ordered catalog, lazily selects entries, and applies the selected opaque ID', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    await initialize(controller, port)

    const catalogPending = controller.getAllHints()
    expect(port.requests[1]).toMatchObject({
      command: 'get_all_hints',
      expected_revision: '0',
    })
    port.respond(1, {
      response: 'all_hints',
      revision: '0',
      outcome: { outcome: 'complete', hints: [summary('11', 8, 9), summary('12', 17, 3)] },
    })
    await Promise.resolve()
    await Promise.resolve()
    expect(port.requests[2]).toMatchObject({
      command: 'get_hint',
      expected_revision: '0',
      hint_id: '11',
    })
    port.respond(2, {
      response: 'hint',
      revision: '0',
      hint_id: '11',
      outcome: {
        outcome: 'presented',
        presentation,
        effects: summary('11', 8, 9).effects,
      },
    })
    await catalogPending

    expect(controller.getState()).toMatchObject({
      hintCatalogResult: { kind: 'complete' },
      selectedHintId: '11',
      hint: { id: '11' },
    })
    expect(controller.getState().hintCatalog.map((hint) => hint.hint_id)).toEqual(['11', '12'])

    const selected = controller.selectHint('12')
    expect(port.requests[3]).toMatchObject({ command: 'get_hint', hint_id: '12' })
    port.respond(3, {
      response: 'hint',
      revision: '0',
      hint_id: '12',
      outcome: {
        outcome: 'presented',
        presentation,
        effects: summary('12', 17, 3).effects,
      },
    })
    await selected
    expect(controller.getState()).toMatchObject({ selectedHintId: '12', hint: { id: '12' } })
    expect(controller.getState().hintCatalog).toHaveLength(2)

    const applied = controller.applyHint()
    expect(port.requests[4]).toMatchObject({ command: 'apply_hint', hint_id: '12' })
    port.respond(4, { response: 'snapshot', snapshot: snapshot('1') })
    await applied
    expect(controller.getState()).toMatchObject({
      selectedHintId: null,
      hint: null,
      hintCatalog: [],
      hintCatalogResult: null,
    })
  })

  it('surfaces the explicit expensive-search confirmation and continues on demand', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    await initialize(controller, port)

    const ordinary = controller.getAllHints()
    port.respond(1, {
      response: 'all_hints',
      revision: '0',
      outcome: { outcome: 'confirmation_required' },
    })
    await ordinary
    expect(controller.getState()).toMatchObject({
      hintCatalog: [],
      hintCatalogResult: { kind: 'confirmation-required' },
    })

    const advanced = controller.getAllHints(true)
    expect(port.requests[2]).toMatchObject({
      command: 'get_all_hints',
      expected_revision: '0',
      include_expensive: true,
    })
    port.respond(2, {
      response: 'all_hints',
      revision: '0',
      outcome: {
        outcome: 'incomplete',
        hints: [],
        gap: { code: 'indirect_techniques', message: 'no further logical tier is ported' },
      },
    })
    await advanced
    expect(controller.getState().hintCatalogResult).toMatchObject({
      kind: 'incomplete',
      gap: { code: 'indirect_techniques' },
    })
  })

  it('does not request another hint when apply-and-next fails', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    await initialize(controller, port)

    const next = controller.nextHint()
    port.respond(1, {
      response: 'next_hint',
      revision: '0',
      outcome: {
        outcome: 'presented',
        hint_id: '41',
        presentation,
        effects: { placement: { cell: 8, digit: 9 }, removals: [], elimination_count: 0 },
      },
    })
    await next

    const applyAndNext = controller.applyAndNext()
    expect(port.requests[2]).toMatchObject({ command: 'apply_hint', hint_id: '41' })
    port.respond(2, {
      response: 'error',
      error: { code: 'stale_revision', message: 'the session moved' },
    })
    await applyAndNext

    expect(port.requests.map((request) => request.command)).toEqual([
      'create_session',
      'next_hint',
      'apply_hint',
    ])
    expect(controller.getState()).toMatchObject({
      busy: false,
      error: { code: 'stale_revision' },
      hint: { id: '41' },
    })
  })

  it('requests the next hint only after apply-and-next accepts the new snapshot', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    await initialize(controller, port)

    const next = controller.nextHint()
    port.respond(1, {
      response: 'next_hint',
      revision: '0',
      outcome: {
        outcome: 'presented',
        hint_id: '41',
        presentation,
        effects: { placement: { cell: 8, digit: 9 }, removals: [], elimination_count: 0 },
      },
    })
    await next

    const applyAndNext = controller.applyAndNext()
    expect(port.requests).toHaveLength(3)
    port.respond(2, { response: 'snapshot', snapshot: snapshot('1') })
    await Promise.resolve()
    await Promise.resolve()

    expect(port.requests[3]).toMatchObject({
      command: 'next_hint',
      expected_revision: '1',
    })
    port.respond(3, {
      response: 'next_hint',
      revision: '1',
      outcome: { outcome: 'none' },
    })
    await applyAndNext

    expect(controller.getState()).toMatchObject({
      busy: false,
      snapshot: { revision: '1' },
      hint: null,
      hintResult: { kind: 'none' },
      error: null,
    })
  })

  it('sends value/candidate edits and undo/redo at the latest authoritative revision', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    await initialize(controller, port)

    const placed = controller.placeValue({ row: 2, col: 3 }, 7)
    expect(port.requests[1]).toMatchObject({
      command: 'place_value',
      expected_revision: '0',
      cell: 21,
      digit: 7,
    })
    expect(controller.getState().snapshot?.values[21]).toBeNull()
    port.respond(1, {
      response: 'snapshot',
      snapshot: snapshot('1', (value) => {
        value.values[21] = 7
        value.candidate_masks.fill(0)
        value.can_undo = true
      }),
    })
    await placed
    expect(controller.getState().snapshot?.values[21]).toBe(7)
    expect(controller.getState().snapshot?.candidateMasks.every((mask) => mask === 0)).toBe(true)

    const toggled = controller.toggleCandidate({ row: 4, col: 5, digit: 2 })
    expect(port.requests[2]).toMatchObject({
      command: 'toggle_candidate',
      expected_revision: '1',
      cell: 41,
      digit: 2,
    })
    port.respond(2, {
      response: 'snapshot',
      snapshot: snapshot('2', (value) => {
        value.candidate_masks[41] = 0x03fa
        value.can_undo = true
      }),
    })
    await toggled
    expect(controller.getState().snapshot?.candidateMasks[41]).toBe(0x03fa)

    const undone = controller.undo()
    expect(port.requests[3]).toMatchObject({ command: 'undo', expected_revision: '2' })
    port.respond(3, {
      response: 'snapshot',
      snapshot: snapshot('3', (value) => { value.can_redo = true }),
    })
    await undone
    expect(controller.getState().snapshot).toMatchObject({ revision: '3', canRedo: true })

    const redone = controller.redo()
    expect(port.requests[4]).toMatchObject({ command: 'redo', expected_revision: '3' })
    port.respond(4, {
      response: 'snapshot',
      snapshot: snapshot('4', (value) => { value.can_undo = true }),
    })
    await redone
    expect(controller.getState().snapshot).toMatchObject({ revision: '4', canUndo: true, canRedo: false })
  })

  it('surfaces port and transport errors without replacing authoritative state', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    await initialize(controller, port)
    const authoritative = controller.getState().snapshot

    const stale = controller.undo()
    expect(controller.getState().busy).toBe(true)
    port.respond(1, {
      response: 'error',
      error: {
        code: 'stale_revision',
        message: 'stale session revision 0; current revision is 1',
        expected_revision: '0',
        actual_revision: '1',
      },
    })
    await stale
    expect(controller.getState()).toMatchObject({
      busy: false,
      error: { code: 'stale_revision', expected_revision: '0', actual_revision: '1' },
    })
    expect(controller.getState().snapshot).toBe(authoritative)

    const failed = controller.nextHint()
    port.fail(2, new Error('worker disconnected'))
    await failed
    expect(controller.getState()).toMatchObject({
      busy: false,
      error: { code: 'transport_error', message: 'worker disconnected' },
    })
    expect(controller.getState().snapshot).toBe(authoritative)
  })

  it('rejects an overlapping command before it can reorder the stateful backend', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    const first = controller.createSession('1'.repeat(81))
    const overlappingAccepted = await controller.createSession('2'.repeat(81))

    expect(overlappingAccepted).toBe(false)
    expect(port.requests).toHaveLength(1)
    expect(port.requests.map((request) => request.command)).toEqual(['create_session'])
    expect(controller.getState()).toMatchObject({
      busy: true,
      pendingCommand: 'create_session',
      error: { code: 'request_in_progress' },
    })
    port.respond(0, {
      response: 'session_created',
      snapshot: snapshot('10', (value) => { value.values[0] = 1 }),
      topology: emptyTopology('First topology'),
    })
    await first

    expect(port.requests).toHaveLength(1)
    expect(controller.getState()).toMatchObject({
      snapshot: { revision: '10' },
      topology: { regions: [{ label: 'First topology' }] },
      busy: false,
      error: null,
    })
    expect(controller.getState().snapshot?.values[0]).toBe(1)
  })

  it('clears a displayed hint for none, unsupported, and incomplete outcomes', async () => {
    const port = new FakePort()
    const controller = createSessionController(port)
    await initialize(controller, port)

    const presented = controller.nextHint()
    port.respond(1, {
      response: 'next_hint',
      revision: '0',
      outcome: {
        outcome: 'presented',
        hint_id: '5',
        presentation,
        effects: { placement: { cell: 8, digit: 9 }, removals: [], elimination_count: 0 },
      },
    })
    await presented
    expect(controller.getState().hint?.id).toBe('5')

    const none = controller.nextHint()
    port.respond(2, { response: 'next_hint', revision: '0', outcome: { outcome: 'none' } })
    await none
    expect(controller.getState()).toMatchObject({ hint: null, hintResult: { kind: 'none' } })

    const unsupported = controller.nextHint()
    port.respond(3, {
      response: 'next_hint',
      revision: '0',
      outcome: {
        outcome: 'unsupported',
        hint_id: '6',
        unsupported: { technique_key: 'nested_forcing_chain', kind: 'missing_chain_proof' },
        effects: { placement: null, removals: [{ cell: 80, digits: 1 << 9 }], elimination_count: 1 },
      },
    })
    await unsupported
    expect(controller.getState()).toMatchObject({
      hint: null,
      hintResult: { kind: 'unsupported', hintId: '6' },
    })

    const applyUnsupported = controller.applyHint()
    expect(port.requests[4]).toMatchObject({
      command: 'apply_hint',
      expected_revision: '0',
      hint_id: '6',
    })
    port.respond(4, { response: 'snapshot', snapshot: snapshot('1') })
    await applyUnsupported
    expect(controller.getState()).toMatchObject({ hint: null, hintResult: null, snapshot: { revision: '1' } })

    const incomplete = controller.nextHint()
    port.respond(5, {
      response: 'next_hint',
      revision: '1',
      outcome: {
        outcome: 'incomplete',
        gap: { code: 'producer_not_ported', message: 'the selected solver producer is not yet ported' },
      },
    })
    await incomplete
    expect(controller.getState()).toMatchObject({
      hint: null,
      hintResult: { kind: 'incomplete', gap: { code: 'producer_not_ported' } },
    })
  })

  it('ignores stale reducer events and exposes the injected hook surface', async () => {
    const waiting = sessionControllerReducer(initialSessionControllerState, {
      type: 'request-started',
      requestId: 2,
      command: 'undo',
    })
    const stale = sessionControllerReducer(waiting, {
      type: 'snapshot-received',
      requestId: 1,
      snapshot: { revision: '99', values: [], candidateMasks: [] },
    })
    expect(stale).toBe(waiting)

    const port = new FakePort()
    const captured: { current?: SessionControllerView } = {}
    expect(renderToStaticMarkup(<HookProbe port={port} capture={(view) => { captured.current = view }} />)).toContain('idle')
    const view = captured.current
    if (!view) throw new Error('hook did not render')

    const pending = view.createSession('.'.repeat(81))
    expect(port.requests[0]?.command).toBe('create_session')
    port.respond(0, {
      response: 'session_created',
      snapshot: snapshot('0'),
      topology: emptyTopology(),
    })
    await pending
  })
})
