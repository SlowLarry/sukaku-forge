import { useCallback, useEffect, useRef, useState } from 'react'
import type { RatingModeDto } from '../applicationPort'
import type { Theme } from '../theme'

export type VariantPreset = 'classic' | 'anti-knight' | 'sudoku-x'
type CheckableRole = 'menuitemcheckbox' | 'menuitemradio'

type MenuId = 'file' | 'edit' | 'tools' | 'options' | 'variants' | 'help'

const MENU_ORDER: MenuId[] = ['file', 'edit', 'tools', 'options', 'variants', 'help']

interface MenuActionProps {
  children: React.ReactNode
  onSelect: () => void
  disabled?: boolean
  role?: 'menuitem' | CheckableRole
  checked?: boolean
  title?: string
}

function MenuAction({ children, onSelect, disabled = false, role = 'menuitem', checked, title }: MenuActionProps) {
  return (
    <button
      type="button"
      role={role}
      aria-checked={role === 'menuitem' ? undefined : Boolean(checked)}
      disabled={disabled}
      title={title}
      onClick={onSelect}
    >
      <span className="menu-check" aria-hidden="true">{role !== 'menuitem' && checked ? '✓' : ''}</span>
      <span>{children}</span>
    </button>
  )
}

interface ApplicationMenuProps {
  id: MenuId
  label: string
  open: boolean
  triggerRef: (element: HTMLButtonElement | null) => void
  onToggle: () => void
  onMove: (direction: -1 | 1) => void
  onClose: (restoreFocus?: boolean) => void
  children: React.ReactNode
}

function ApplicationMenu({ id, label, open, triggerRef, onToggle, onMove, onClose, children }: ApplicationMenuProps) {
  const focusEdge = (edge: 'first' | 'last') => {
    const menu = document.querySelector<HTMLElement>(`[data-menu-panel="${id}"]`)
    const actions = menu ? Array.from(menu.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')) : []
    const target = edge === 'first' ? actions[0] : actions.at(-1)
    target?.focus()
  }

  const handleTriggerKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault()
      onMove(event.key === 'ArrowLeft' ? -1 : 1)
    } else if (event.key === 'ArrowDown' || event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      if (!open) onToggle()
      window.requestAnimationFrame(() => focusEdge('first'))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      if (!open) onToggle()
      window.requestAnimationFrame(() => focusEdge('last'))
    } else if (event.key === 'Escape' && open) {
      event.preventDefault()
      onClose(true)
    }
  }

  const handleMenuKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const actions = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'))
    const current = actions.indexOf(document.activeElement as HTMLButtonElement)
    if (event.key === 'Escape') {
      event.preventDefault()
      onClose(true)
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const offset = event.key === 'ArrowDown' ? 1 : -1
      actions[(current + offset + actions.length) % actions.length]?.focus()
    } else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault()
      actions[event.key === 'Home' ? 0 : actions.length - 1]?.focus()
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault()
      onMove(event.key === 'ArrowLeft' ? -1 : 1)
    }
  }

  return (
    <div className="application-menu" role="none">
      <button
        ref={triggerRef}
        type="button"
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={`${id}-application-menu`}
        onClick={onToggle}
        onKeyDown={handleTriggerKeyDown}
      >
        {label}
      </button>
      <div
        id={`${id}-application-menu`}
        className="application-menu-panel"
        role="menu"
        aria-label={label}
        data-menu-panel={id}
        hidden={!open}
        onKeyDown={handleMenuKeyDown}
      >
        {children}
      </div>
    </div>
  )
}

interface AppMenuProps {
  busy: boolean
  sessionReady: boolean
  canUndo: boolean
  canRedo: boolean
  canRequestHint: boolean
  canApply: boolean
  candidatesVisible: boolean
  candidateEntry: boolean
  theme: Theme
  ratingMode: RatingModeDto
  variantPreset: VariantPreset
  canReconfigure: boolean
  onNew: () => void
  onImport: (returnFocus: HTMLButtonElement | null) => void
  onUndo: () => void
  onRedo: () => void
  onNextHint: () => void
  onApply: () => void
  onApplyAndNext: () => void
  onToggleCandidates: () => void
  onToggleCandidateEntry: () => void
  onTheme: (theme: Theme) => void
  onRatingMode: (mode: RatingModeDto) => void
  onVariantPreset: (preset: VariantPreset) => void
  onAbout: (returnFocus: HTMLButtonElement | null) => void
}

export function AppMenu({
  busy,
  sessionReady,
  canUndo,
  canRedo,
  canRequestHint,
  canApply,
  candidatesVisible,
  candidateEntry,
  theme,
  ratingMode,
  variantPreset,
  canReconfigure,
  onNew,
  onImport,
  onUndo,
  onRedo,
  onNextHint,
  onApply,
  onApplyAndNext,
  onToggleCandidates,
  onToggleCandidateEntry,
  onTheme,
  onRatingMode,
  onVariantPreset,
  onAbout,
}: AppMenuProps) {
  const [openMenu, setOpenMenu] = useState<MenuId | null>(null)
  const root = useRef<HTMLElement>(null)
  const triggers = useRef<Partial<Record<MenuId, HTMLButtonElement | null>>>({})

  const closeMenu = useCallback((restoreFocus = false) => {
    const current = openMenu
    setOpenMenu(null)
    if (restoreFocus && current) window.requestAnimationFrame(() => triggers.current[current]?.focus())
  }, [openMenu])

  const select = useCallback((action: () => void) => {
    setOpenMenu(null)
    action()
  }, [])

  const moveMenu = useCallback((from: MenuId, direction: -1 | 1) => {
    const index = MENU_ORDER.indexOf(from)
    const target = MENU_ORDER[(index + direction + MENU_ORDER.length) % MENU_ORDER.length]!
    const wasOpen = openMenu != null
    setOpenMenu(wasOpen ? target : null)
    window.requestAnimationFrame(() => triggers.current[target]?.focus())
  }, [openMenu])

  useEffect(() => {
    if (openMenu == null) return
    const handleOutsidePointer = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpenMenu(null)
    }
    document.addEventListener('pointerdown', handleOutsidePointer, true)
    return () => document.removeEventListener('pointerdown', handleOutsidePointer, true)
  }, [openMenu])

  const engineDisabled = busy || !sessionReady
  const configurationTitle = canReconfigure
    ? undefined
    : 'Start from or return to the original puzzle before changing solver settings.'

  const menuProps = (id: MenuId, label: string) => ({
    id,
    label,
    open: openMenu === id,
    triggerRef: (element: HTMLButtonElement | null) => { triggers.current[id] = element },
    onToggle: () => setOpenMenu((current) => current === id ? null : id),
    onMove: (direction: -1 | 1) => moveMenu(id, direction),
    onClose: closeMenu,
  })

  return (
    <nav ref={root} className="application-menubar" aria-label="Application menu">
      <div role="menubar" aria-label="Sukaku Forge commands">
        <ApplicationMenu {...menuProps('file', 'File')}>
          <MenuAction disabled={busy} onSelect={() => select(onNew)}>New blank puzzle</MenuAction>
          <MenuAction
            disabled={busy}
            onSelect={() => select(() => onImport(triggers.current.file ?? null))}
          >Import 81-character string…</MenuAction>
        </ApplicationMenu>
        <ApplicationMenu {...menuProps('edit', 'Edit')}>
          <MenuAction disabled={busy || !canUndo} onSelect={() => select(onUndo)}>Undo</MenuAction>
          <MenuAction disabled={busy || !canRedo} onSelect={() => select(onRedo)}>Redo</MenuAction>
          <div role="separator" />
          <MenuAction
            role="menuitemcheckbox"
            checked={candidateEntry}
            disabled={engineDisabled}
            onSelect={() => select(onToggleCandidateEntry)}
          >Candidate entry mode</MenuAction>
        </ApplicationMenu>
        <ApplicationMenu {...menuProps('tools', 'Tools')}>
          <MenuAction disabled={busy || !canRequestHint} onSelect={() => select(onNextHint)}>Next hint</MenuAction>
          <MenuAction disabled={busy || !canApply} onSelect={() => select(onApply)}>Apply hint</MenuAction>
          <MenuAction disabled={busy || !canApply} onSelect={() => select(onApplyAndNext)}>Apply and next</MenuAction>
        </ApplicationMenu>
        <ApplicationMenu {...menuProps('options', 'Options')}>
          <MenuAction
            role="menuitemcheckbox"
            checked={candidatesVisible}
            disabled={engineDisabled}
            onSelect={() => select(onToggleCandidates)}
          >Show candidates</MenuAction>
          <div role="separator" />
          <MenuAction role="menuitemradio" checked={theme === 'light'} onSelect={() => select(() => onTheme('light'))}>Light theme</MenuAction>
          <MenuAction role="menuitemradio" checked={theme === 'dark'} onSelect={() => select(() => onTheme('dark'))}>Dark theme</MenuAction>
          <div role="separator" />
          <MenuAction
            role="menuitemradio"
            checked={ratingMode === 'original'}
            disabled={!canReconfigure}
            title={configurationTitle}
            onSelect={() => select(() => onRatingMode('original'))}
          >Original rating</MenuAction>
          <MenuAction
            role="menuitemradio"
            checked={ratingMode === 'revised'}
            disabled={!canReconfigure}
            title={configurationTitle}
            onSelect={() => select(() => onRatingMode('revised'))}
          >Revised rating</MenuAction>
        </ApplicationMenu>
        <ApplicationMenu {...menuProps('variants', 'Variants')}>
          <MenuAction
            role="menuitemradio"
            checked={variantPreset === 'classic'}
            disabled={!canReconfigure}
            title={configurationTitle}
            onSelect={() => select(() => onVariantPreset('classic'))}
          >Classic Sudoku</MenuAction>
          <MenuAction
            role="menuitemradio"
            checked={variantPreset === 'anti-knight'}
            disabled={!canReconfigure}
            title={configurationTitle}
            onSelect={() => select(() => onVariantPreset('anti-knight'))}
          >Anti-knight</MenuAction>
          <MenuAction
            role="menuitemradio"
            checked={variantPreset === 'sudoku-x'}
            disabled={!canReconfigure}
            title={configurationTitle}
            onSelect={() => select(() => onVariantPreset('sudoku-x'))}
          >Sudoku X</MenuAction>
        </ApplicationMenu>
        <ApplicationMenu {...menuProps('help', 'Help')}>
          <MenuAction
            onSelect={() => select(() => onAbout(triggers.current.help ?? null))}
          >About Sukaku Forge</MenuAction>
        </ApplicationMenu>
      </div>
    </nav>
  )
}
