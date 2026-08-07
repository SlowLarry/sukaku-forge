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
})
