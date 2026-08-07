// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { HintEffectsDto } from '../applicationPort'
import { HintBrowser, type HintBrowserItem } from './HintBrowser'

afterEach(cleanup)

const effects = (
  placement: HintEffectsDto['placement'],
  removals: HintEffectsDto['removals'] = [],
): HintEffectsDto => ({
  placement,
  removals,
  elimination_count: removals.reduce((count, removal) => (
    count + removal.digits.toString(2).replaceAll('0', '').length
  ), 0),
})

const catalog: HintBrowserItem[] = [
  {
    id: '1',
    category: 'direct',
    techniqueKey: 'hidden_single',
    label: 'Hidden Single',
    detail: 'r1c1 = 2',
    kind: 'presented',
    rating: 1,
    effects: effects({ cell: 0, digit: 2 }),
  },
  {
    id: '2',
    category: 'direct',
    techniqueKey: 'naked_single',
    label: 'Naked Single',
    detail: 'r1c1 = 2',
    kind: 'available',
    rating: 2.3,
    effects: effects({ cell: 0, digit: 2 }),
  },
  {
    id: '3',
    category: 'direct',
    techniqueKey: 'naked_single',
    label: 'Naked Single',
    detail: 'r1c2 = 3',
    kind: 'available',
    rating: 2.3,
    effects: effects({ cell: 1, digit: 3 }),
  },
  {
    id: '4',
    category: 'indirect',
    techniqueKey: 'locking',
    label: 'Locked Candidates',
    detail: 'r1c1 = 2',
    kind: 'available',
    rating: 2.6,
    effects: effects({ cell: 0, digit: 2 }),
  },
  {
    id: '5',
    category: 'indirect',
    techniqueKey: 'fish',
    label: 'X-Wing',
    detail: 'r1c1 = 2',
    kind: 'available',
    rating: 3.2,
    effects: effects({ cell: 0, digit: 2 }),
  },
  {
    id: '6',
    category: 'indirect',
    techniqueKey: 'fish',
    label: 'X-Wing',
    detail: '2 eliminations',
    kind: 'available',
    rating: 3.2,
    effects: effects({ cell: 0, digit: 2 }, [
      { cell: 10, digits: 1 << 4 },
      { cell: 11, digits: 1 << 4 },
    ]),
  },
]

describe('HintBrowser', () => {
  it('groups catalog hints in encounter order and applies the legacy similar-outcome filter by default', () => {
    const onSelect = vi.fn()
    render(
      <HintBrowser
        items={catalog}
        selectedId="1"
        emptyMessage="Request a hint."
        catalogResult={{ kind: 'complete' }}
        onSelect={onSelect}
      />,
    )

    expect(screen.getByText('3 of 6 deductions')).toBeTruthy()
    expect((screen.getByRole('checkbox', { name: 'Filter similar outcomes' }) as HTMLInputElement).checked).toBe(true)
    expect(screen.getByRole('heading', { name: 'Sudoku Rules 2' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Solving Techniques 1' })).toBeTruthy()
    expect(screen.getAllByText('r1c1 = 2', { selector: 'small' })).toHaveLength(1)
    expect(screen.getAllByText('Naked Single')).toHaveLength(2)
    expect(screen.getAllByText('X-Wing')).toHaveLength(2)

    const groupLabels = Array.from(
      document.querySelectorAll('.hint-technique > h4 > span'),
      (element) => element.textContent,
    )
    expect(groupLabels).toEqual(['Hidden Single', 'Naked Single', 'X-Wing'])

    fireEvent.click(screen.getAllByText('X-Wing')[1]!.closest('button')!)
    expect(onSelect).toHaveBeenCalledWith('6')

    fireEvent.click(screen.getByRole('checkbox', { name: 'Filter similar outcomes' }))
    expect(screen.getByText('6 of 6 deductions')).toBeTruthy()
    expect(screen.getAllByText('r1c1 = 2')).toHaveLength(4)
  })

  it('searches summaries and asks the controller to materialize the first visible hint', async () => {
    const onSelect = vi.fn()
    const { rerender } = render(
      <HintBrowser
        items={catalog}
        selectedId="1"
        emptyMessage="Request a hint."
        catalogResult={{ kind: 'complete' }}
        onSelect={onSelect}
      />,
    )

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search all hints' }), {
      target: { value: 'X-Wing' },
    })

    expect(screen.getByText('1 of 6 deductions')).toBeTruthy()
    expect(screen.queryByRole('heading', { name: /Sudoku Rules/ })).toBeNull()
    await waitFor(() => expect(onSelect).toHaveBeenCalledWith('6'))

    rerender(
      <HintBrowser
        items={catalog}
        selectedId="1"
        emptyMessage="Request a hint."
        catalogResult={{ kind: 'complete' }}
        busy
        onSelect={onSelect}
      />,
    )
    rerender(
      <HintBrowser
        items={catalog}
        selectedId="1"
        emptyMessage="Request a hint."
        catalogResult={{ kind: 'complete' }}
        onSelect={onSelect}
      />,
    )
    expect(onSelect).toHaveBeenCalledTimes(1)
  })

  it('uses server-projected chain outcomes and keeps the legacy root order', () => {
    const chainPlacement: HintBrowserItem = {
      id: '20',
      category: 'indirect',
      techniqueKey: 'forcing_chain',
      label: 'Forcing Chain',
      detail: 'r1c1 = 2',
      kind: 'available',
      effects: effects({ cell: 0, digit: 2 }),
      filterEffects: effects({ cell: 0, digit: 2 }, [{ cell: 0, digits: 1 << 3 }]),
    }
    const laterElimination: HintBrowserItem = {
      id: '21',
      category: 'indirect',
      techniqueKey: 'locking',
      label: 'Locked Candidates',
      detail: '1 elimination',
      kind: 'available',
      effects: effects(null, [{ cell: 0, digits: 1 << 3 }]),
    }
    const laterDirect: HintBrowserItem = {
      id: '22',
      category: 'direct',
      techniqueKey: 'hidden_single',
      label: 'Hidden Single',
      detail: 'r2c1 = 4',
      kind: 'available',
      effects: effects({ cell: 9, digit: 4 }),
    }

    render(
      <HintBrowser
        items={[chainPlacement, laterElimination, laterDirect]}
        selectedId="20"
        emptyMessage="Request a hint."
        catalogResult={{ kind: 'complete' }}
        onSelect={vi.fn()}
      />,
    )

    expect(screen.getByText('2 of 3 deductions')).toBeTruthy()
    expect(screen.queryByText('Locked Candidates')).toBeNull()
    expect(Array.from(
      document.querySelectorAll('.hint-category > h3 > span'),
      (element) => element.textContent,
    )).toEqual(['Sudoku Rules', 'Solving Techniques'])
  })

  it('surfaces advanced confirmation and incomplete searches without inventing hint rows', () => {
    const onAdvanced = vi.fn()
    const { rerender } = render(
      <HintBrowser
        items={[]}
        selectedId={null}
        emptyMessage="No hints."
        catalogResult={{ kind: 'confirmation-required' }}
        onSearchAdvanced={onAdvanced}
      />,
    )

    expect(screen.getByText('No ordinary hints found')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Search advanced hints' }))
    expect(onAdvanced).toHaveBeenCalledOnce()

    rerender(
      <HintBrowser
        items={[]}
        selectedId={null}
        emptyMessage="No hints."
        catalogResult={{
          kind: 'incomplete',
          gap: { code: 'indirect_techniques', message: 'Some techniques are not ported.' },
        }}
      />,
    )
    expect(screen.getByText('Hint search incomplete')).toBeTruthy()
    expect(screen.getAllByText('Some techniques are not ported.')).toHaveLength(2)
    expect(document.querySelector('[data-hint-kind]')).toBeNull()
  })

  it('preserves the compact next-hint view', () => {
    render(
      <HintBrowser
        items={[{
          id: '9',
          label: 'Hidden Single',
          detail: 'Placement · 1 proof view',
          kind: 'presented',
          rating: 1.2,
        }]}
        selectedId="9"
        emptyMessage="Request a hint."
      />,
    )

    expect(screen.getByText('1 active deduction')).toBeTruthy()
    expect(document.querySelector('[data-hint-kind="presented"]')).toBeTruthy()
    expect(screen.queryByRole('searchbox')).toBeNull()
  })
})
