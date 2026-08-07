// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AppMenu } from './AppMenu'

afterEach(cleanup)

const props = () => ({
  busy: false,
  sessionReady: true,
  canUndo: true,
  canRedo: false,
  canRequestHint: true,
  canRequestAllHints: true,
  canApply: true,
  candidatesVisible: true,
  candidateEntry: false,
  theme: 'dark' as const,
  ratingMode: 'original' as const,
  variantPreset: 'classic' as const,
  canReconfigure: true,
  onNew: vi.fn(),
  onImport: vi.fn(),
  onUndo: vi.fn(),
  onRedo: vi.fn(),
  onNextHint: vi.fn(),
  onAllHints: vi.fn(),
  onApply: vi.fn(),
  onApplyAndNext: vi.fn(),
  onToggleCandidates: vi.fn(),
  onToggleCandidateEntry: vi.fn(),
  onTheme: vi.fn(),
  onRatingMode: vi.fn(),
  onVariantPreset: vi.fn(),
  onAbout: vi.fn(),
})

describe('AppMenu', () => {
  it('uses the legacy top-level order and opens menus from the keyboard', async () => {
    render(<AppMenu {...props()} />)
    const topLevel = screen.getByRole('menubar').querySelectorAll(':scope > .application-menu > [role="menuitem"]')

    expect(Array.from(topLevel, (item) => item.textContent)).toEqual([
      'File',
      'Edit',
      'Tools',
      'Options',
      'Variants',
      'Help',
    ])

    const file = screen.getByRole('menuitem', { name: 'File' })
    fireEvent.click(file)
    expect(file.getAttribute('aria-expanded')).toBe('true')
    fireEvent.keyDown(file, { key: 'ArrowDown' })

    const newPuzzle = screen.getByRole('menuitem', { name: 'New blank puzzle' })
    expect(document.getElementById('file-application-menu')?.hasAttribute('hidden')).toBe(false)
    await waitFor(() => expect(document.activeElement).toBe(newPuzzle))

    fireEvent.keyDown(newPuzzle, { key: 'Escape' })
    expect(document.getElementById('file-application-menu')?.hasAttribute('hidden')).toBe(true)
    await waitFor(() => expect(document.activeElement).toBe(file))
  })

  it('switches open top-level menus on hover without opening a closed menubar', () => {
    render(<AppMenu {...props()} />)
    const file = screen.getByRole('menuitem', { name: 'File' })
    const edit = screen.getByRole('menuitem', { name: 'Edit' })
    const tools = screen.getByRole('menuitem', { name: 'Tools' })

    fireEvent.mouseEnter(edit)
    expect(file.getAttribute('aria-expanded')).toBe('false')
    expect(edit.getAttribute('aria-expanded')).toBe('false')

    fireEvent.click(file)
    expect(file.getAttribute('aria-expanded')).toBe('true')

    fireEvent.mouseEnter(edit)
    expect(file.getAttribute('aria-expanded')).toBe('false')
    expect(edit.getAttribute('aria-expanded')).toBe('true')
    expect(document.activeElement).toBe(edit)

    fireEvent.mouseEnter(tools)
    expect(edit.getAttribute('aria-expanded')).toBe('false')
    expect(tools.getAttribute('aria-expanded')).toBe('true')
  })

  it('exposes check/radio state and dispatches the selected working action', () => {
    const actions = props()
    render(<AppMenu {...actions} />)

    fireEvent.click(screen.getByRole('menuitem', { name: 'Options' }))
    expect(screen.getByRole('menuitemcheckbox', { name: 'Show candidates' }).getAttribute('aria-checked')).toBe('true')
    expect(screen.getByRole('menuitemradio', { name: 'Dark theme' }).getAttribute('aria-checked')).toBe('true')
    expect(screen.getByRole('menuitemradio', { name: 'Light theme' }).getAttribute('aria-checked')).toBe('false')

    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Light theme' }))
    expect(actions.onTheme).toHaveBeenCalledWith('light')
    expect(document.getElementById('options-application-menu')?.hasAttribute('hidden')).toBe(true)
  })

  it('exposes all-hints in Tools and dispatches it independently of next hint', () => {
    const actions = props()
    render(<AppMenu {...actions} />)

    fireEvent.click(screen.getByRole('menuitem', { name: 'Tools' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'Get all hints' }))

    expect(actions.onAllHints).toHaveBeenCalledOnce()
    expect(actions.onNextHint).not.toHaveBeenCalled()
  })

  it('exposes the supported variant presets and dispatches the selected preset', () => {
    const actions = props()
    render(<AppMenu {...actions} />)

    fireEvent.click(screen.getByRole('menuitem', { name: 'Variants' }))
    const labels = [
      'Classic Sudoku',
      'Sudoku X',
      'Anti-knight',
      'Anti-king',
      'Non-consecutive',
      'Disjoint groups',
    ]
    const variantMenu = document.getElementById('variants-application-menu')
    expect(Array.from(
      variantMenu?.querySelectorAll('[role="menuitemradio"]') ?? [],
      (item) => item.querySelector('span:last-child')?.textContent,
    )).toEqual(labels)

    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Anti-king' }))
    expect(actions.onVariantPreset).toHaveBeenCalledWith('anti-king')
  })
})
