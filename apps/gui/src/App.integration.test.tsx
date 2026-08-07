// @vitest-environment happy-dom

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import App, { DEFAULT_PUZZLE } from './App'
import { APP_VERSION } from './appVersion'
import type {
  ApplicationPort,
  ApplicationRequestDto,
  ApplicationResponseDto,
  SessionSnapshotDto,
  VariantInputDto,
} from './applicationPort'
import { parseApplicationResponse } from './applicationPort'
import golden from './fixtures/protocol-v3-hidden-single.json'

type Deferred<T> = {
  promise: Promise<T>
  resolve(value: T): void
}

const deferred = <T,>(): Deferred<T> => {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((settle) => { resolve = settle })
  return { promise, resolve }
}

afterEach(() => {
  cleanup()
  window.localStorage.clear()
  delete document.documentElement.dataset.theme
})

const responseAt = (index: number): ApplicationResponseDto => {
  const raw = structuredClone(golden.steps[index]!.response)
  raw.protocol_version = 3
  return parseApplicationResponse(raw, raw.request_id)
}

const correlated = (response: ApplicationResponseDto, requestId: number): ApplicationResponseDto => ({
  ...structuredClone(response),
  request_id: requestId,
})

const snapshotResponse = (
  snapshot: SessionSnapshotDto,
  requestId: number,
): ApplicationResponseDto => ({
  protocol_version: 3,
  request_id: requestId,
  response: 'snapshot',
  snapshot,
})

const currentRevision = () => document.querySelector('.puzzle-meta strong')?.textContent
const valueCount = () => document.querySelectorAll('.cell-value').length
const candidateCount = () => document.querySelectorAll('.candidate').length

const mirrorRequestedVariant = (
  response: Extract<ApplicationResponseDto, { response: 'session_created' }>,
  variant: VariantInputDto | undefined,
) => {
  response.topology.variant = { ...response.topology.variant, ...variant }
}

describe('live application flow', () => {
  it('waits for authoritative create/apply snapshots and wires next, undo, and redo', async () => {
    const create = deferred<ApplicationResponseDto>()
    const next = deferred<ApplicationResponseDto>()
    const apply = deferred<ApplicationResponseDto>()
    const initial = responseAt(0)
    const presented = responseAt(1)
    const applied = responseAt(2)

    const dispatch = vi.fn(async (request: ApplicationRequestDto): Promise<ApplicationResponseDto> => {
      switch (request.command) {
        case 'create_session': return create.promise
        case 'next_hint': return next.promise
        case 'apply_hint': return apply.promise
        case 'undo': {
          const snapshot = structuredClone(
            (initial as Extract<ApplicationResponseDto, { response: 'session_created' }>).snapshot,
          )
          snapshot.revision = '2'
          snapshot.can_undo = false
          snapshot.can_redo = true
          return snapshotResponse(snapshot, request.request_id)
        }
        case 'redo': {
          const snapshot = structuredClone(
            (applied as Extract<ApplicationResponseDto, { response: 'snapshot' }>).snapshot,
          )
          snapshot.revision = '3'
          snapshot.can_undo = true
          snapshot.can_redo = false
          return snapshotResponse(snapshot, request.request_id)
        }
        default: throw new Error(`unexpected command ${request.command}`)
      }
    })
    const port: ApplicationPort = { dispatch }

    render(<App port={port} initialPuzzle="12345678........................................................................." />)
    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole('grid')).toBeNull()
    expect(screen.getByText('Starting puzzle session')).toBeTruthy()
    fireEvent.click(screen.getByRole('menuitem', { name: 'Variants' }))
    expect(screen.getByRole('menuitemradio', { name: 'Classic Sudoku' }).getAttribute('aria-checked')).toBe('true')
    fireEvent.click(screen.getByRole('menuitem', { name: 'Variants' }))

    await act(async () => {
      create.resolve(correlated(initial, 1))
      await create.promise
    })
    await screen.findByRole('grid')
    expect(currentRevision()).toBe('0')
    expect(valueCount()).toBe(8)

    fireEvent.click(screen.getByRole('button', { name: 'Get next hint' }))
    expect(dispatch).toHaveBeenCalledTimes(2)
    expect(screen.getByRole('button', { name: 'Get next hint' }).hasAttribute('disabled')).toBe(true)
    expect(valueCount()).toBe(8)

    await act(async () => {
      next.resolve(correlated(presented, 2))
      await next.promise
    })
    await waitFor(() => expect(document.querySelector('[data-hint-kind="presented"]')?.textContent).toContain('Hidden Single'))

    fireEvent.click(screen.getByRole('button', { name: 'Apply active hint' }))
    expect(dispatch).toHaveBeenCalledTimes(3)
    expect(valueCount()).toBe(8)

    await act(async () => {
      apply.resolve(correlated(applied, 3))
      await apply.promise
    })
    await waitFor(() => expect(currentRevision()).toBe('1'))
    expect(valueCount()).toBe(9)
    expect(document.querySelector('[data-hint-kind]')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Undo' }))
    await waitFor(() => expect(currentRevision()).toBe('2'))
    expect(valueCount()).toBe(8)

    fireEvent.click(screen.getByRole('button', { name: 'Redo' }))
    await waitFor(() => expect(currentRevision()).toBe('3'))
    expect(valueCount()).toBe(9)
  })

  it('loads the all-hints catalog first and materializes only the selected server-owned hint', async () => {
    const initial = responseAt(0)
    const next = responseAt(1)
    if (next.response !== 'next_hint' || next.outcome.outcome !== 'presented') {
      throw new Error('expected presented fixture')
    }

    const hiddenSinglePresentation = structuredClone(next.outcome.presentation)
    const hiddenSingleEffects = structuredClone(next.outcome.effects)
    const fishPresentation = structuredClone(hiddenSinglePresentation)
    fishPresentation.identity = {
      technique_key: 'fish_2',
      name: 'X-Wing',
      short_name: 'XW',
      rating_tenths: 32,
    }
    fishPresentation.explanation = {
      blocks: [{
        type: 'paragraph',
        inlines: [{ type: 'text', text: 'The X-Wing removes two candidates.' }],
      }],
    }
    const fishEffects = {
      placement: null,
      removals: [
        { cell: 40, digits: 1 << 2 },
        { cell: 41, digits: 1 << 2 },
      ],
      elimination_count: 2,
    }
    const summaries = [
      {
        hint_id: '11',
        category: 'direct' as const,
        group_key: 'hidden_single',
        group_name: 'Hidden Single',
        identity: hiddenSinglePresentation.identity,
        effects: hiddenSingleEffects,
        filter_effects: hiddenSingleEffects,
      },
      {
        hint_id: '12',
        category: 'indirect' as const,
        group_key: 'x_wing',
        group_name: 'X-Wing',
        identity: fishPresentation.identity,
        effects: fishEffects,
        filter_effects: fishEffects,
      },
    ]

    const dispatch = vi.fn(async (request: ApplicationRequestDto): Promise<ApplicationResponseDto> => {
      switch (request.command) {
        case 'create_session': return correlated(initial, request.request_id)
        case 'get_all_hints':
          return {
            protocol_version: 3,
            request_id: request.request_id,
            response: 'all_hints',
            revision: '0',
            outcome: { outcome: 'complete', hints: summaries },
          }
        case 'get_hint': {
          const fish = request.hint_id === '12'
          return {
            protocol_version: 3,
            request_id: request.request_id,
            response: 'hint',
            revision: '0',
            hint_id: request.hint_id,
            outcome: {
              outcome: 'presented',
              presentation: fish ? fishPresentation : hiddenSinglePresentation,
              effects: fish ? fishEffects : hiddenSingleEffects,
            },
          }
        }
        default: throw new Error(`unexpected command ${request.command}`)
      }
    })

    render(<App port={{ dispatch }} />)
    await screen.findByRole('grid')
    fireEvent.click(screen.getByRole('button', { name: 'Get all hints' }))

    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(3))
    expect(dispatch.mock.calls[1]?.[0]).toMatchObject({
      command: 'get_all_hints',
      expected_revision: '0',
    })
    expect(dispatch.mock.calls[2]?.[0]).toMatchObject({
      command: 'get_hint',
      hint_id: '11',
      expected_revision: '0',
    })
    expect(screen.getByRole('searchbox', { name: 'Search all hints' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Sudoku Rules 1' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Solving Techniques 1' })).toBeTruthy()

    fireEvent.click(screen.getByText('2 eliminations', { selector: 'small' }).closest('button')!)
    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(4))
    expect(dispatch.mock.calls[3]?.[0]).toMatchObject({ command: 'get_hint', hint_id: '12' })
    await waitFor(() => {
      expect(document.querySelector('[data-hint-kind="presented"]')?.textContent).toContain('X-Wing')
    })
    expect(screen.getByRole('heading', { name: 'Hint details' }).parentElement?.parentElement?.textContent)
      .toContain('X-Wing')
  })

  it('requires an explicit second request before searching expensive hints', async () => {
    const initial = responseAt(0)
    const dispatch = vi.fn(async (request: ApplicationRequestDto): Promise<ApplicationResponseDto> => {
      if (request.command === 'create_session') return correlated(initial, request.request_id)
      if (request.command === 'get_all_hints') {
        return {
          protocol_version: 3,
          request_id: request.request_id,
          response: 'all_hints',
          revision: '0',
          outcome: request.include_expensive
            ? { outcome: 'complete', hints: [] }
            : { outcome: 'confirmation_required' },
        }
      }
      throw new Error(`unexpected command ${request.command}`)
    })

    render(<App port={{ dispatch }} />)
    await screen.findByRole('grid')
    fireEvent.click(screen.getByRole('button', { name: 'Get all hints' }))

    expect(await screen.findByText('No ordinary hints found')).toBeTruthy()
    expect(dispatch.mock.calls[1]?.[0]).not.toHaveProperty('include_expensive')
    fireEvent.click(screen.getByRole('button', { name: 'Search advanced hints' }))

    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(3))
    expect(dispatch.mock.calls[2]?.[0]).toMatchObject({
      command: 'get_all_hints',
      include_expensive: true,
    })
    expect(await screen.findAllByText('The engine found no applicable logical hints.')).toHaveLength(2)
  })

  it('keeps edits non-optimistic, suppresses conflicts while busy, and wires candidate mode', async () => {
    const initial = responseAt(0) as Extract<ApplicationResponseDto, { response: 'session_created' }>
    const place = deferred<ApplicationResponseDto>()
    const toggle = deferred<ApplicationResponseDto>()
    const placedSnapshot = structuredClone(initial.snapshot)
    placedSnapshot.revision = '1'
    placedSnapshot.values[41] = 4
    placedSnapshot.candidate_masks[41] = 0
    placedSnapshot.can_undo = true

    const dispatch = vi.fn(async (request: ApplicationRequestDto): Promise<ApplicationResponseDto> => {
      if (request.command === 'create_session') return correlated(initial, request.request_id)
      if (request.command === 'place_value') {
        return place.promise
      }
      if (request.command === 'toggle_candidate') {
        return toggle.promise
      }
      throw new Error(`unexpected command ${request.command}`)
    })

    render(<App port={{ dispatch }} initialPuzzle="12345678........................................................................." />)
    const board = await screen.findByRole('grid')
    expect(valueCount()).toBe(8)

    fireEvent.keyDown(board, { key: 'ArrowRight' })
    fireEvent.keyDown(board, { key: '4' })
    expect(dispatch.mock.calls[1]?.[0]).toMatchObject({ command: 'place_value', cell: 41, digit: 4 })
    expect(valueCount()).toBe(8)

    fireEvent.click(screen.getByRole('button', { name: 'Get next hint' }))
    expect(dispatch).toHaveBeenCalledTimes(2)

    await act(async () => {
      place.resolve(snapshotResponse(placedSnapshot, 2))
      await place.promise
    })
    await waitFor(() => expect(currentRevision()).toBe('1'))
    expect(valueCount()).toBe(9)

    fireEvent.click(screen.getByRole('button', { name: 'Toggle candidate entry mode (M)' }))
    expect(screen.getByRole('button', { name: 'Toggle candidate entry mode (M)' }).getAttribute('aria-pressed')).toBe('true')
    fireEvent.keyDown(board, { key: 'ArrowRight' })
    const candidatesBefore = candidateCount()
    fireEvent.keyDown(board, { key: '2' })
    expect(dispatch.mock.calls[2]?.[0]).toMatchObject({ command: 'toggle_candidate', cell: 42, digit: 2 })
    expect(candidateCount()).toBe(candidatesBefore)

    const request = dispatch.mock.calls[2]![0]
    if (request.command !== 'toggle_candidate') throw new Error('toggle was not dispatched')
    const toggledSnapshot = structuredClone(placedSnapshot)
    toggledSnapshot.revision = '2'
    toggledSnapshot.candidate_masks[request.cell] ^= 1 << request.digit
    await act(async () => {
      toggle.resolve(snapshotResponse(toggledSnapshot, request.request_id))
      await toggle.promise
    })
    await waitFor(() => expect(currentRevision()).toBe('2'))
    expect(candidateCount()).toBe(candidatesBefore - 1)
  })

  it('imports canonical value grids, starts blank puzzles, switches theme, and opens About', async () => {
    const initial = responseAt(0)
    const dispatch = vi.fn(async (request: ApplicationRequestDto): Promise<ApplicationResponseDto> => {
      if (request.command !== 'create_session') throw new Error(`unexpected command ${request.command}`)
      return correlated(initial, request.request_id)
    })

    render(<App port={{ dispatch }} />)
    await screen.findByRole('grid')

    const fileMenu = screen.getByRole('menuitem', { name: 'File' })
    fireEvent.click(fileMenu)
    fireEvent.click(screen.getByRole('menuitem', { name: 'Import 81-character string…' }))
    fireEvent.keyDown(screen.getByRole('dialog', { name: 'Import value grid' }), { key: 'Escape' })
    expect(document.activeElement).toBe(fileMenu)

    fireEvent.click(fileMenu)
    fireEvent.click(screen.getByRole('menuitem', { name: 'Import 81-character string…' }))
    fireEvent.change(screen.getByLabelText('81-character puzzle'), { target: { value: '0'.repeat(81) } })
    fireEvent.click(screen.getByRole('button', { name: 'Import puzzle' }))

    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(2))
    expect(dispatch.mock.calls[1]?.[0]).toMatchObject({
      command: 'create_session',
      puzzle: '.'.repeat(81),
    })

    fireEvent.click(screen.getByRole('menuitem', { name: 'File' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'New blank puzzle' }))
    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(3))
    expect(dispatch.mock.calls[2]?.[0]).toMatchObject({
      command: 'create_session',
      puzzle: '.'.repeat(81),
    })

    fireEvent.click(screen.getByRole('menuitem', { name: 'Options' }))
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Dark theme' }))
    expect(document.querySelector('.app-shell')?.getAttribute('data-theme')).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
    expect(window.localStorage.getItem('sukaku-forge:theme:v1')).toBe('dark')

    const helpMenu = screen.getByRole('menuitem', { name: 'Help' })
    fireEvent.click(helpMenu)
    fireEvent.click(screen.getByRole('menuitem', { name: 'About Sukaku Forge' }))
    expect(screen.getByRole('dialog', { name: 'About Sukaku Forge' })).toBeTruthy()
    expect(screen.getByText(`Version ${APP_VERSION}`)).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Close' }))
    expect(screen.queryByRole('dialog', { name: 'About Sukaku Forge' })).toBeNull()
    expect(document.activeElement).toBe(helpMenu)
  })

  it('recreates an unedited session for supported variant and rating choices', async () => {
    const initial = responseAt(0)
    const dispatch = vi.fn(async (request: ApplicationRequestDto): Promise<ApplicationResponseDto> => {
      if (request.command !== 'create_session') throw new Error(`unexpected command ${request.command}`)
      const response = correlated(initial, request.request_id)
      if (response.response !== 'session_created') throw new Error('expected session-created fixture')
      mirrorRequestedVariant(response, request.variant)
      return response
    })

    render(<App port={{ dispatch }} />)
    await screen.findByRole('grid')

    const classicVariant = {
      blocks: true,
      disjoint_groups: false,
      sudoku_x: false,
      anti_ferz: false,
      anti_knight: false,
      non_consecutive: 'off',
      forbidden_pairs: false,
    } as const
    const choices = [
      ['Sudoku X', { ...classicVariant, sudoku_x: true }],
      ['Anti-knight', { ...classicVariant, anti_knight: true }],
      ['Anti-king', { ...classicVariant, anti_ferz: true }],
      ['Non-consecutive', {
        ...classicVariant,
        non_consecutive: 'orthogonal' as const,
        forbidden_pairs: true,
      }],
      ['Disjoint groups', { ...classicVariant, disjoint_groups: true }],
    ] as const

    for (const [index, [label, variant]] of choices.entries()) {
      fireEvent.click(screen.getByRole('menuitem', { name: 'Variants' }))
      fireEvent.click(screen.getByRole('menuitemradio', { name: label }))
      await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(index + 2))
      expect(dispatch.mock.calls[index + 1]?.[0]).toMatchObject({
        command: 'create_session',
        variant,
        engine: { rating_mode: 'original' },
      })
      await waitFor(() => expect(screen.getByLabelText(`Variant: ${label}`).textContent).toContain(label))

      fireEvent.click(screen.getByRole('menuitem', { name: 'Variants' }))
      expect(screen.getByRole('menuitemradio', { name: label }).getAttribute('aria-checked')).toBe('true')
      fireEvent.click(screen.getByRole('menuitem', { name: 'Variants' }))
    }

    fireEvent.click(screen.getByRole('menuitem', { name: 'Options' }))
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Revised rating' }))
    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(7))
    expect(dispatch.mock.calls[6]?.[0]).toMatchObject({
      command: 'create_session',
      variant: { ...classicVariant, disjoint_groups: true },
      engine: { rating_mode: 'revised' },
    })
  })

  it('keeps the accepted puzzle and rating mode when session recreation fails', async () => {
    const initial = responseAt(0)
    let createCount = 0
    const dispatch = vi.fn(async (request: ApplicationRequestDto): Promise<ApplicationResponseDto> => {
      if (request.command !== 'create_session') throw new Error(`unexpected command ${request.command}`)
      createCount += 1
      if (createCount === 2 || createCount === 4) {
        return {
          protocol_version: 3,
          request_id: request.request_id,
          response: 'error',
          error: {
            code: createCount === 2 ? 'invalid_puzzle' : 'engine_configuration_failed',
            message: 'session recreation rejected',
          },
        }
      }

      const response = correlated(initial, request.request_id)
      if (response.response !== 'session_created') throw new Error('expected session-created fixture')
      mirrorRequestedVariant(response, request.variant)
      return response
    })

    render(<App port={{ dispatch }} />)
    await screen.findByRole('grid')

    fireEvent.click(screen.getByRole('menuitem', { name: 'File' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'Import 81-character string…' }))
    fireEvent.change(screen.getByLabelText('81-character puzzle'), { target: { value: '1'.repeat(81) } })
    fireEvent.click(screen.getByRole('button', { name: 'Import puzzle' }))
    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(2))
    expect(screen.getByRole('alert').textContent).toContain('invalid_puzzle')

    fireEvent.click(screen.getByRole('menuitem', { name: 'Variants' }))
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Anti-knight' }))
    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(3))
    expect(dispatch.mock.calls[2]?.[0]).toMatchObject({
      command: 'create_session',
      puzzle: DEFAULT_PUZZLE,
      engine: { rating_mode: 'original' },
    })
    await waitFor(() => expect(screen.getByLabelText('Variant: Anti-knight').textContent).toContain('Anti-knight'))

    fireEvent.click(screen.getByRole('menuitem', { name: 'Options' }))
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Revised rating' }))
    await waitFor(() => expect(dispatch).toHaveBeenCalledTimes(4))
    expect(screen.getByRole('alert').textContent).toContain('engine_configuration_failed')

    fireEvent.click(screen.getByRole('menuitem', { name: 'Options' }))
    expect(screen.getByRole('menuitemradio', { name: 'Original rating' }).getAttribute('aria-checked')).toBe('true')
    expect(screen.getByRole('menuitemradio', { name: 'Revised rating' }).getAttribute('aria-checked')).toBe('false')
  })
})
