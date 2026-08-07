import { useMemo, useSyncExternalStore } from 'react'
import type {
  ApplicationPort,
  ApplicationRequestDto,
  ApplicationResponseDto,
  EngineInputDto,
  HintEffectsDto,
  HintSummaryDto,
  NonZeroDecimalString,
  PortErrorDto,
  PortGapDto,
  RequestId,
  TopologyDto,
  UnsupportedPresentationDto,
  VariantInputDto,
} from './applicationPort'
import { PROTOCOL_VERSION } from './applicationPort'
import type { BoardSnapshot, BoardTopology, CandidateRef, CellRef, HintPresentation } from './model'
import { mapHintPresentation, mapSessionSnapshot, mapTopology } from './wireMapper'

type WithoutEnvelope<T> = T extends { protocol_version: number; request_id: number }
  ? Omit<T, 'protocol_version' | 'request_id'>
  : never

type ApplicationCommandDto = WithoutEnvelope<ApplicationRequestDto>
type CommandName = ApplicationCommandDto['command']
type ExpectedResponse = Exclude<ApplicationResponseDto['response'], 'error'>

export type HintResult =
  | { kind: 'presented'; hintId: NonZeroDecimalString }
  | {
    kind: 'unsupported'
    hintId: NonZeroDecimalString
    unsupported: UnsupportedPresentationDto
    effects: HintEffectsDto
  }
  | { kind: 'none' }
  | { kind: 'incomplete'; gap: PortGapDto }

export type HintCatalogResult =
  | { kind: 'complete' }
  | { kind: 'confirmation-required' }
  | { kind: 'incomplete'; gap: PortGapDto }

export interface SessionControllerState {
  snapshot: BoardSnapshot | null
  topology: BoardTopology | null
  hint: HintPresentation | null
  hintResult: HintResult | null
  hintCatalog: HintSummaryDto[]
  hintCatalogResult: HintCatalogResult | null
  selectedHintId: NonZeroDecimalString | null
  busy: boolean
  pendingRequestId: RequestId | null
  pendingCommand: CommandName | null
  error: PortErrorDto | null
}

export const initialSessionControllerState: SessionControllerState = {
  snapshot: null,
  topology: null,
  hint: null,
  hintResult: null,
  hintCatalog: [],
  hintCatalogResult: null,
  selectedHintId: null,
  busy: false,
  pendingRequestId: null,
  pendingCommand: null,
  error: null,
}

export type SessionControllerEvent =
  | { type: 'request-started'; requestId: RequestId; command: CommandName }
  | {
    type: 'session-created'
    requestId: RequestId
    snapshot: BoardSnapshot
    topology: BoardTopology
  }
  | { type: 'snapshot-received'; requestId: RequestId; snapshot: BoardSnapshot }
  | {
    type: 'hint-received'
    requestId: RequestId
    hint: HintPresentation | null
    result: HintResult
    selectedHintId: NonZeroDecimalString | null
    retainCatalog: boolean
  }
  | {
    type: 'hint-catalog-received'
    requestId: RequestId
    hints: HintSummaryDto[]
    result: HintCatalogResult
  }
  | { type: 'request-failed'; requestId: RequestId; error: PortErrorDto }
  | { type: 'local-error'; error: PortErrorDto }

const completesCurrentRequest = (state: SessionControllerState, requestId: RequestId) => (
  state.pendingRequestId === requestId
)

export function sessionControllerReducer(
  state: SessionControllerState,
  event: SessionControllerEvent,
): SessionControllerState {
  if (event.type === 'request-started') {
    return {
      ...state,
      busy: true,
      pendingRequestId: event.requestId,
      pendingCommand: event.command,
      error: null,
    }
  }
  if (event.type === 'local-error') return { ...state, error: event.error }
  if (!completesCurrentRequest(state, event.requestId)) return state

  const settled = {
    busy: false,
    pendingRequestId: null,
    pendingCommand: null,
    error: null,
  } as const

  switch (event.type) {
    case 'session-created':
      return {
        ...state,
        ...settled,
        snapshot: event.snapshot,
        topology: event.topology,
        hint: null,
        hintResult: null,
        hintCatalog: [],
        hintCatalogResult: null,
        selectedHintId: null,
      }
    case 'snapshot-received':
      return {
        ...state,
        ...settled,
        snapshot: event.snapshot,
        hint: null,
        hintResult: null,
        hintCatalog: [],
        hintCatalogResult: null,
        selectedHintId: null,
      }
    case 'hint-received':
      return {
        ...state,
        ...settled,
        hint: event.hint,
        hintResult: event.result,
        hintCatalog: event.retainCatalog ? state.hintCatalog : [],
        hintCatalogResult: event.retainCatalog ? state.hintCatalogResult : null,
        selectedHintId: event.selectedHintId,
      }
    case 'hint-catalog-received':
      return {
        ...state,
        ...settled,
        hint: null,
        hintResult: null,
        hintCatalog: event.hints,
        hintCatalogResult: event.result,
        selectedHintId: null,
      }
    case 'request-failed':
      return { ...state, ...settled, error: event.error }
  }
}

export interface CreateSessionOptions {
  variant?: VariantInputDto
  engine?: EngineInputDto
}

export interface SessionControllerActions {
  createSession(puzzle: string, options?: CreateSessionOptions): Promise<boolean>
  nextHint(): Promise<void>
  getAllHints(includeExpensive?: boolean): Promise<void>
  selectHint(hintId: NonZeroDecimalString): Promise<void>
  applyHint(): Promise<void>
  applyAndNext(): Promise<void>
  placeValue(cell: CellRef, digit: number): Promise<void>
  toggleCandidate(candidate: CandidateRef): Promise<void>
  undo(): Promise<void>
  redo(): Promise<void>
}

export interface SessionController extends SessionControllerActions {
  getState(): SessionControllerState
  subscribe(listener: () => void): () => void
}

const controllerError = (code: string, message: string): PortErrorDto => ({ code, message })

const errorMessage = (error: unknown) => error instanceof Error ? error.message : String(error)

const rawCell = ({ row, col }: CellRef) => row * 9 + col

class InjectedSessionController implements SessionController {
  private state = initialSessionControllerState
  private readonly listeners = new Set<() => void>()
  private nextRequestId = 1
  private wireTopology: TopologyDto | null = null

  constructor(private readonly port: ApplicationPort) {}

  getState = () => this.state

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  createSession = async (puzzle: string, options: CreateSessionOptions = {}) => {
    const command: ApplicationCommandDto = {
      command: 'create_session',
      puzzle,
      ...(options.variant == null ? {} : { variant: options.variant }),
      ...(options.engine == null ? {} : { engine: options.engine }),
    }
    return this.dispatch(command, 'session_created')
  }

  nextHint = async () => {
    const revision = this.currentRevision()
    if (revision == null) return
    await this.dispatch({ command: 'next_hint', expected_revision: revision }, 'next_hint')
  }

  getAllHints = async (includeExpensive = false) => {
    const revision = this.currentRevision()
    if (revision == null) return
    const accepted = await this.dispatch({
      command: 'get_all_hints',
      expected_revision: revision,
      ...(includeExpensive ? { include_expensive: true } : {}),
    }, 'all_hints')
    if (!accepted) return
    const first = this.state.hintCatalog[0]
    if (first != null) await this.selectHint(first.hint_id)
  }

  selectHint = async (hintId: NonZeroDecimalString) => {
    const revision = this.currentRevision()
    if (revision == null) return
    if (!this.state.hintCatalog.some((hint) => hint.hint_id === hintId)) {
      this.publish({
        type: 'local-error',
        error: controllerError('unknown_hint', 'the selected hint is not in the current catalog'),
      })
      return
    }
    if (this.state.selectedHintId === hintId && this.state.hintResult != null) return
    await this.dispatch({
      command: 'get_hint',
      expected_revision: revision,
      hint_id: hintId,
    }, 'hint')
  }

  applyHint = async () => {
    const revision = this.currentRevision()
    if (revision == null) return
    const hintResult = this.state.hintResult
    const hintId = hintResult?.kind === 'presented' || hintResult?.kind === 'unsupported'
      ? hintResult.hintId
      : null
    if (hintId == null) {
      this.publish({
        type: 'local-error',
        error: controllerError('no_active_hint', 'request a presented hint before applying it'),
      })
      return
    }
    await this.dispatch({ command: 'apply_hint', expected_revision: revision, hint_id: hintId }, 'snapshot')
  }

  applyAndNext = async () => {
    const revision = this.currentRevision()
    if (revision == null) return
    const hintResult = this.state.hintResult
    const hintId = hintResult?.kind === 'presented' || hintResult?.kind === 'unsupported'
      ? hintResult.hintId
      : null
    if (hintId == null) {
      this.publish({
        type: 'local-error',
        error: controllerError('no_active_hint', 'request a presented hint before applying it'),
      })
      return
    }

    const applied = await this.dispatch(
      { command: 'apply_hint', expected_revision: revision, hint_id: hintId },
      'snapshot',
    )
    if (!applied) return
    await this.nextHint()
  }

  placeValue = async (cell: CellRef, digit: number) => {
    const revision = this.currentRevision()
    if (revision == null) return
    await this.dispatch({
      command: 'place_value',
      expected_revision: revision,
      cell: rawCell(cell),
      digit,
    }, 'snapshot')
  }

  toggleCandidate = async (candidate: CandidateRef) => {
    const revision = this.currentRevision()
    if (revision == null) return
    await this.dispatch({
      command: 'toggle_candidate',
      expected_revision: revision,
      cell: rawCell(candidate),
      digit: candidate.digit,
    }, 'snapshot')
  }

  undo = async () => {
    const revision = this.currentRevision()
    if (revision == null) return
    await this.dispatch({ command: 'undo', expected_revision: revision }, 'snapshot')
  }

  redo = async () => {
    const revision = this.currentRevision()
    if (revision == null) return
    await this.dispatch({ command: 'redo', expected_revision: revision }, 'snapshot')
  }

  private currentRevision() {
    const revision = this.state.snapshot?.revision
    if (revision != null) return revision
    this.publish({
      type: 'local-error',
      error: controllerError('session_not_initialized', 'create a session before issuing this command'),
    })
    return null
  }

  private allocateRequestId() {
    const requestId = this.nextRequestId
    this.nextRequestId = requestId === 4_294_967_295 ? 1 : requestId + 1
    return requestId
  }

  private async dispatch(command: ApplicationCommandDto, expectedResponse: ExpectedResponse): Promise<boolean> {
    if (this.state.busy) {
      this.publish({
        type: 'local-error',
        error: controllerError(
          'request_in_progress',
          `${this.state.pendingCommand ?? 'another command'} is already in progress`,
        ),
      })
      return false
    }

    const requestId = this.allocateRequestId()
    const request = {
      protocol_version: PROTOCOL_VERSION,
      request_id: requestId,
      ...command,
    } as ApplicationRequestDto
    this.publish({ type: 'request-started', requestId, command: command.command })

    let response: ApplicationResponseDto
    try {
      response = await this.port.dispatch(request)
    } catch (error) {
      this.publish({
        type: 'request-failed',
        requestId,
        error: controllerError('transport_error', errorMessage(error)),
      })
      return false
    }

    if (!this.isCurrent(requestId)) return false
    if (response.request_id !== requestId) {
      this.publish({
        type: 'request-failed',
        requestId,
        error: controllerError(
          'request_id_mismatch',
          `response ${response.request_id} does not match request ${requestId}`,
        ),
      })
      return false
    }
    if (response.response === 'error') {
      this.publish({ type: 'request-failed', requestId, error: response.error })
      return false
    }
    if (response.response !== expectedResponse) {
      this.publish({
        type: 'request-failed',
        requestId,
        error: controllerError(
          'unexpected_response',
          `${command.command} received ${response.response}; expected ${expectedResponse}`,
        ),
      })
      return false
    }
    if (
      command.command === 'get_hint'
      && response.response === 'hint'
      && response.hint_id !== command.hint_id
    ) {
      this.publish({
        type: 'request-failed',
        requestId,
        error: controllerError(
          'hint_id_mismatch',
          `response hint ${response.hint_id} does not match requested hint ${command.hint_id}`,
        ),
      })
      return false
    }
    if (
      'expected_revision' in command
      && 'revision' in response
      && response.revision !== command.expected_revision
    ) {
      this.publish({
        type: 'request-failed',
        requestId,
        error: controllerError(
          'revision_mismatch',
          `response revision ${response.revision} does not match requested revision ${command.expected_revision}`,
        ),
      })
      return false
    }

    this.acceptResponse(requestId, response)
    return !this.state.busy && this.state.error == null
  }

  private acceptResponse(
    requestId: RequestId,
    response: Exclude<ApplicationResponseDto, { response: 'error' }>,
  ) {
    switch (response.response) {
      case 'session_created':
        this.wireTopology = response.topology
        this.publish({
          type: 'session-created',
          requestId,
          snapshot: mapSessionSnapshot(response.snapshot),
          topology: mapTopology(response.topology),
        })
        return
      case 'snapshot':
        this.publish({
          type: 'snapshot-received',
          requestId,
          snapshot: mapSessionSnapshot(response.snapshot),
        })
        return
      case 'next_hint':
        this.acceptNextHint(requestId, response)
        return
      case 'all_hints':
        this.acceptHintCatalog(requestId, response)
        return
      case 'hint':
        this.acceptMaterializedHint(requestId, response)
    }
  }

  private acceptNextHint(
    requestId: RequestId,
    response: Extract<ApplicationResponseDto, { response: 'next_hint' }>,
  ) {
    const outcome = response.outcome
    switch (outcome.outcome) {
      case 'presented': {
        if (this.wireTopology == null) {
          this.publish({
            type: 'request-failed',
            requestId,
            error: controllerError('topology_unavailable', 'a presented hint requires session topology'),
          })
          return
        }
        let hint: HintPresentation
        try {
          hint = mapHintPresentation({
            hintId: outcome.hint_id,
            revision: response.revision,
            presentation: outcome.presentation,
            effects: outcome.effects,
            topology: this.wireTopology,
          })
        } catch (error) {
          this.publish({
            type: 'request-failed',
            requestId,
            error: controllerError('wire_mapping_error', errorMessage(error)),
          })
          return
        }
        this.publish({
          type: 'hint-received',
          requestId,
          hint,
          result: { kind: 'presented', hintId: outcome.hint_id },
          selectedHintId: outcome.hint_id,
          retainCatalog: false,
        })
        return
      }
      case 'unsupported':
        this.publish({
          type: 'hint-received',
          requestId,
          hint: null,
          result: {
            kind: 'unsupported',
            hintId: outcome.hint_id,
            unsupported: outcome.unsupported,
            effects: outcome.effects,
          },
          selectedHintId: outcome.hint_id,
          retainCatalog: false,
        })
        return
      case 'none':
        this.publish({
          type: 'hint-received',
          requestId,
          hint: null,
          result: { kind: 'none' },
          selectedHintId: null,
          retainCatalog: false,
        })
        return
      case 'incomplete':
        this.publish({
          type: 'hint-received',
          requestId,
          hint: null,
          result: { kind: 'incomplete', gap: outcome.gap },
          selectedHintId: null,
          retainCatalog: false,
        })
    }
  }

  private acceptHintCatalog(
    requestId: RequestId,
    response: Extract<ApplicationResponseDto, { response: 'all_hints' }>,
  ) {
    const outcome = response.outcome
    switch (outcome.outcome) {
      case 'complete':
        this.publish({
          type: 'hint-catalog-received',
          requestId,
          hints: outcome.hints,
          result: { kind: 'complete' },
        })
        return
      case 'confirmation_required':
        this.publish({
          type: 'hint-catalog-received',
          requestId,
          hints: [],
          result: { kind: 'confirmation-required' },
        })
        return
      case 'incomplete':
        this.publish({
          type: 'hint-catalog-received',
          requestId,
          hints: outcome.hints,
          result: { kind: 'incomplete', gap: outcome.gap },
        })
    }
  }

  private acceptMaterializedHint(
    requestId: RequestId,
    response: Extract<ApplicationResponseDto, { response: 'hint' }>,
  ) {
    const outcome = response.outcome
    if (outcome.outcome === 'presented') {
      if (this.wireTopology == null) {
        this.publish({
          type: 'request-failed',
          requestId,
          error: controllerError('topology_unavailable', 'a presented hint requires session topology'),
        })
        return
      }
      let hint: HintPresentation
      try {
        hint = mapHintPresentation({
          hintId: response.hint_id,
          revision: response.revision,
          presentation: outcome.presentation,
          effects: outcome.effects,
          topology: this.wireTopology,
        })
      } catch (error) {
        this.publish({
          type: 'request-failed',
          requestId,
          error: controllerError('wire_mapping_error', errorMessage(error)),
        })
        return
      }
      this.publish({
        type: 'hint-received',
        requestId,
        hint,
        result: { kind: 'presented', hintId: response.hint_id },
        selectedHintId: response.hint_id,
        retainCatalog: true,
      })
      return
    }
    if (outcome.outcome === 'unsupported') {
      this.publish({
        type: 'hint-received',
        requestId,
        hint: null,
        result: {
          kind: 'unsupported',
          hintId: response.hint_id,
          unsupported: outcome.unsupported,
          effects: outcome.effects,
        },
        selectedHintId: response.hint_id,
        retainCatalog: true,
      })
      return
    }
    this.publish({
      type: 'hint-received',
      requestId,
      hint: null,
      result: { kind: 'incomplete', gap: outcome.gap },
      selectedHintId: response.hint_id,
      retainCatalog: true,
    })
  }

  private isCurrent(requestId: RequestId) {
    return this.state.pendingRequestId === requestId
  }

  private publish(event: SessionControllerEvent) {
    const next = sessionControllerReducer(this.state, event)
    if (next === this.state) return
    this.state = next
    this.listeners.forEach((listener) => listener())
  }
}

export const createSessionController = (port: ApplicationPort): SessionController => (
  new InjectedSessionController(port)
)

export interface SessionControllerView extends SessionControllerState, SessionControllerActions {}

export function useSessionController(port: ApplicationPort): SessionControllerView {
  const controller = useMemo(() => createSessionController(port), [port])
  const state = useSyncExternalStore(controller.subscribe, controller.getState, controller.getState)
  return useMemo(() => ({
    ...state,
    createSession: controller.createSession,
    nextHint: controller.nextHint,
    getAllHints: controller.getAllHints,
    selectHint: controller.selectHint,
    applyHint: controller.applyHint,
    applyAndNext: controller.applyAndNext,
    placeValue: controller.placeValue,
    toggleCandidate: controller.toggleCandidate,
    undo: controller.undo,
    redo: controller.redo,
  }), [controller, state])
}
