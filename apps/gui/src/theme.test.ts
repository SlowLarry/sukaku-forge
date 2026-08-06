// @vitest-environment happy-dom

import { afterEach, describe, expect, it, vi } from 'vitest'
import { applyDocumentTheme, persistTheme, preferredTheme, THEME_STORAGE_KEY } from './theme'

afterEach(() => {
  window.localStorage.clear()
  delete document.documentElement.dataset.theme
  vi.restoreAllMocks()
})

describe('theme preferences', () => {
  it('persists and restores an explicit theme', () => {
    persistTheme('dark')

    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe('dark')
    expect(preferredTheme()).toBe('dark')
  })

  it('uses the system preference when no stored choice exists', () => {
    vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList)

    expect(preferredTheme()).toBe('dark')
  })

  it('applies the selected theme to the document root', () => {
    applyDocumentTheme('light')

    expect(document.documentElement.dataset.theme).toBe('light')
  })
})
