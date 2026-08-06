// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ImportPuzzleDialog } from './Dialogs'

afterEach(cleanup)

describe('ImportPuzzleDialog', () => {
  it('submits a validated canonical 81-character value grid', () => {
    const onImport = vi.fn()
    render(<ImportPuzzleDialog onClose={vi.fn()} onImport={onImport} />)

    fireEvent.change(screen.getByLabelText('81-character puzzle'), {
      target: { value: `${'123456789'.repeat(8)}123456780` },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Import puzzle' }))

    expect(onImport).toHaveBeenCalledWith(`${'123456789'.repeat(8)}12345678.`)
  })

  it('keeps invalid input in the dialog and reports the validation error', () => {
    const onImport = vi.fn()
    render(<ImportPuzzleDialog onClose={vi.fn()} onImport={onImport} />)

    fireEvent.change(screen.getByLabelText('81-character puzzle'), { target: { value: '.'.repeat(80) } })
    fireEvent.click(screen.getByRole('button', { name: 'Import puzzle' }))

    expect(screen.getByRole('alert').textContent).toContain('exactly 81 characters')
    expect(onImport).not.toHaveBeenCalled()
  })
})
