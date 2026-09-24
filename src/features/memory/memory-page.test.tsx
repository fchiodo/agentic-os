import '@testing-library/jest-dom/vitest'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { MemoryPage } from '@/features/memory/memory-page'
import { arrayBufferToBase64 } from '@/features/memory/binary'
import { resetMockMemoryState } from '@/features/memory/api'

vi.mock('@/features/memory/orbit-map', () => ({
  OrbitMapView: () => (
    <section>
      <h3>Interactive 3D brain</h3>
      <span>AgenticOS</span>
    </section>
  ),
}))

afterEach(() => {
  cleanup()
  resetMockMemoryState()
})

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
  it('shows memory metrics for vault coverage and governance workload', async () => {
    renderPage()

    expect(await screen.findByLabelText('Memory metrics')).toBeInTheDocument()
    expect(screen.getByText('Vault notes')).toBeInTheDocument()
    expect(await screen.findByText('2 active')).toBeInTheDocument()
    expect(screen.getByText('Pending review')).toBeInTheDocument()
    expect(await screen.findByText('1 requires approval')).toBeInTheDocument()
    expect(screen.getByText('Reviewed writes')).toBeInTheDocument()
    expect(await screen.findByText('1 approved')).toBeInTheDocument()
    expect(screen.getByText('Populated domains')).toBeInTheDocument()
    expect(await screen.findByText('1 of 6 active')).toBeInTheDocument()
  })

  it('keeps governance closed by default and opens the pending proposal rail on request', async () => {
    renderPage()

    const openGovernance = screen.getByRole('button', { name: 'Expand governance' })
    expect(openGovernance).toHaveAttribute('aria-expanded', 'false')
    const governanceRail = screen
      .getByText('Governance', { selector: '.memory-governance-title' })
      .closest('aside')
    expect(governanceRail).toHaveAttribute('aria-hidden', 'true')
    expect(governanceRail).toHaveAttribute('inert')

    fireEvent.click(openGovernance)

    expect(screen.getByRole('button', { name: 'Collapse governance' })).toHaveAttribute('aria-expanded', 'true')
    expect(governanceRail).toHaveAttribute('aria-hidden', 'false')
    expect(governanceRail).not.toHaveAttribute('inert')
    expect(
      await screen.findByText('1 write is waiting for review before it reaches the vault.'),
    ).toBeInTheDocument()
    expect(screen.getByText('Update PowerReviews feed decision')).toBeInTheDocument()
    expect(screen.getByText('Sensitive')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Approve' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Dismiss' })).toBeInTheDocument()
  })

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

  it('asks memory, renders a synthesized answer, and exposes governed actions', async () => {
    renderPage()
    fireEvent.click(screen.getByRole('button', { name: 'Ask' }))
    fireEvent.change(screen.getByLabelText('Ask the Second Brain'), {
      target: { value: 'Why is the feed delta?' },
    })
    fireEvent.click(screen.getAllByRole('button', { name: 'Ask' }).at(-1)!)

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Answer' })).toBeInTheDocument()
      expect(screen.getByText('Medium confidence · 1 source')).toBeInTheDocument()
      expect(
        screen.getByRole('button', {
          name: /Open citation 1: PowerReviews feed is delta, not full/,
        }),
      ).toBeInTheDocument()
      expect(screen.getByText(/AI-synthesized · citation verified · abstains without evidence/)).toBeInTheDocument()
    })

    fireEvent.click(screen.getAllByRole('button', { name: 'Save memory' }).at(-1)!)
    await waitFor(() => {
      expect(screen.getByText('Answer saved, indexed, and audited.')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: 'Flag' }))
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Flagged' })).toBeDisabled()
    })
  })

  it('stops an in-flight Ask without presenting it as a failure', async () => {
    renderPage()
    fireEvent.click(screen.getByRole('button', { name: 'Ask' }))
    fireEvent.change(screen.getByLabelText('Ask the Second Brain'), {
      target: { value: 'Why is the feed delta?' },
    })
    fireEvent.click(screen.getAllByRole('button', { name: 'Ask' }).at(-1)!)

    const stop = await screen.findByRole('button', { name: 'Stop' })
    fireEvent.click(stop)

    await waitFor(() => {
      expect(screen.getByText('Stopped')).toBeInTheDocument()
    })
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('switches to the 3D brain and back without losing the Memory page', async () => {
    renderPage()

    fireEvent.click(screen.getByRole('button', { name: '3D Brain' }))
    expect(await screen.findByRole('heading', { name: 'Interactive 3D brain' })).toBeInTheDocument()
    expect(screen.getByText('AgenticOS')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Library' }))
    expect(await screen.findByLabelText('Memory metrics')).toBeInTheDocument()
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
      .getByText('PDF, email (.eml/.msg), or text document')
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
      expect(screen.getByText('markitdown 0.1.6')).toBeInTheDocument()
      expect(screen.getByText('passed · 96/100')).toBeInTheDocument()
    })
  })
})
