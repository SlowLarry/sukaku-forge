export type Theme = 'light' | 'dark'

export const THEME_STORAGE_KEY = 'sukaku-forge:theme:v1'

const isTheme = (value: string | null): value is Theme => value === 'light' || value === 'dark'

const browserStorage = (): Storage | null => {
  if (typeof window === 'undefined') return null
  try {
    return window.localStorage
  } catch {
    return null
  }
}

export function preferredTheme(): Theme {
  try {
    const stored = browserStorage()?.getItem(THEME_STORAGE_KEY) ?? null
    if (isTheme(stored)) return stored
  } catch {
    // Storage can be unavailable in private or restricted browser contexts.
  }

  try {
    return typeof window !== 'undefined' && window.matchMedia?.('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light'
  } catch {
    return 'light'
  }
}

export function persistTheme(theme: Theme): void {
  try {
    browserStorage()?.setItem(THEME_STORAGE_KEY, theme)
  } catch {
    // Theme selection remains usable for the current page when persistence fails.
  }
}

export function applyDocumentTheme(theme: Theme): void {
  if (typeof document !== 'undefined') document.documentElement.dataset.theme = theme
}
