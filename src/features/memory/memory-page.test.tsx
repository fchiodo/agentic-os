import '@testing-library/jest-dom/vitest'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { MemoryPage } from '@/features/memory/memory-page'
import { arrayBufferToBase64 } from '@/features/memory/binary'

afterEach(cleanup)

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryPage />
    </QueryClientProvider>,
  )
}

describe('MemoryPage', () => {
  it('encodes PDF bytes losslessly for the Tauri import boundary', () => {
    const bytes = new Uint8Array([0x25, 0x50, 0x44, 0x46, 0x2d, 0x00, 0xff, 0x10])
    const encoded = arrayBufferToBase64(bytes.buffer)
    const decoded = Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0))
    expect(decoded).toEqual(bytes)
  })

  it('exposes a real stale toggle and runs a manual save through the pipeline', async () => {
    renderPage()

    const staleToggle = screen.getByRole('checkbox', { name: 'Include stale' })
    expect(staleToggle).not.toBeChecked()
    fireEvent.click(staleToggle)
    expect(staleToggle).toBeChecked()

    const saveButtons = screen.getAllByRole('button', { name: 'Save memory' })
    fireEvent.click(saveButtons.at(-1)!)
    fireEvent.change(screen.getByLabelText('Title'), {
      target: { value: 'Architecture decision' },
    })
    fireEvent.change(screen.getByLabelText('Body'), {
      target: { value: 'Use the governed local memory pipeline.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Run gate and save' }))

    await waitFor(() => {
      expect(
        screen.getByText('Saved, committed, indexed, and audited.'),
      ).toBeInTheDocument()
    })
  })

  it('asks memory and renders an answer with a clickable citation', async () => {
    renderPage()
    fireEvent.click(screen.getByRole('button', { name: 'Ask' }))
    fireEvent.change(screen.getByLabelText('Ask the Second Brain'), {
      target: { value: 'Why is the feed delta?' },
    })
    fireEvent.click(screen.getAllByRole('button', { name: 'Ask' }).at(-1)!)

    await waitFor(() => {
      expect(screen.getByText('Grounded answer')).toBeInTheDocument()
      expect(
        screen.getByRole('button', {
          name: /\[1\] PowerReviews feed is delta, not full/,
        }),
      ).toBeInTheDocument()
    })
  })

  it('imports an untruncated document and creates review proposals', async () => {
    renderPage()
    const importButtons = screen.getAllByRole('button', { name: 'Import document' })
    fireEvent.click(importButtons.at(-1)!)

    fireEvent.change(screen.getByLabelText('Document title'), {
      target: { value: 'Sierra Headless API' },
    })
    fireEvent.change(screen.getByLabelText('Full document'), {
      target: {
        value: '# Authentication\nSierra supports OAuth client credentials with short-lived JWT tokens.',
      },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Import and create proposals' }))

    await waitFor(() => {
      expect(screen.getByText('Source preserved and versioned')).toBeInTheDocument()
      expect(screen.getByText(/1 proposal\(s\) waiting for review/)).toBeInTheDocument()
      expect(screen.getByText(/Review and approve the proposed facts/)).toBeInTheDocument()
    })
  })

  it('accepts a PDF as binary instead of decoding it with File.text()', async () => {
    renderPage()
    fireEvent.click(screen.getAllByRole('button', { name: 'Import document' }).at(-1)!)
    fireEvent.click(screen.getByRole('tab', { name: 'File' }))

    const bytes = new TextEncoder().encode('%PDF-1.4\nminimal test body')
    const file = new File([bytes], 'sierra.pdf', { type: 'application/pdf' })
    Object.defineProperty(file, 'arrayBuffer', {
      value: async () => bytes.buffer,
    })
    const fileInput = screen
      .getByText('PDF or text document')
      .closest('label')
      ?.querySelector('input[type="file"]')
    expect(fileInput).not.toBeNull()
    fireEvent.change(fileInput!, {
      target: { files: [file] },
    })

    await waitFor(() => {
      expect(screen.getByDisplayValue('sierra')).toBeInTheDocument()
      expect(screen.getByText('sierra.pdf')).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole('button', { name: 'Import and create proposals' }))

    await waitFor(() => {
      expect(screen.getByText('Source preserved and versioned')).toBeInTheDocument()
    })
  })
})
