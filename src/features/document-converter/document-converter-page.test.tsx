import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { DocumentConverterPage } from './document-converter-page'
import { useConverterStore } from './store'

const installModel = vi.fn()
const chooseFiles = vi.fn()
const getPreview = vi.fn()

const status = {
  ready: false,
  architecture: 'aarch64',
  sidecarVersion: '0.2.1',
  protocolVersion: 1,
  engine: 'paddleocr-vl',
  engineVersion: '0.2.1',
  model: {
    id: 'paddleocr-vl',
    displayName: 'PaddleOCR-VL 1.6',
    version: '1.6',
    state: 'not-installed',
    installed: false,
    checksumValid: false,
    downloadSizeBytes: 1930426592,
    installedSizeBytes: 1930426592,
    installedPath: null,
    architecture: 'aarch64-apple-darwin',
    license: 'Apache-2.0',
    errorCode: null,
    errorMessage: null,
  },
  activeJobs: 0,
  queuedJobs: 0,
  keepWarmSeconds: 300,
  localOnly: true,
}

vi.mock('./api', () => ({
  chooseFiles: (...args: unknown[]) => chooseFiles(...args),
  chooseDestination: vi.fn().mockResolvedValue(null),
  inspectPaths: vi.fn().mockResolvedValue([]),
  createJobs: vi.fn().mockResolvedValue({ jobs: [], rejected: [], duplicates: [] }),
  cancelJob: vi.fn(),
  cancelAll: vi.fn(),
  retryJob: vi.fn(),
  deleteHistoryEntry: vi.fn(),
  getPreview: (...args: unknown[]) => getPreview(...args),
  readAsset: vi.fn(),
  openOutput: vi.fn(),
  importToMemory: vi.fn(),
  cancelModelDownload: vi.fn(),
}))

vi.mock('./hooks', () => ({
  converterJobsKey: ['document-converter', 'jobs'],
  useConverterStatus: () => ({ data: status, error: null }),
  useConversionJobs: () => ({ data: [], isLoading: false }),
  useConverterEvents: () => undefined,
  useInstallModel: () => ({ isPending: false, error: null, mutate: installModel }),
  useRepairModel: () => ({ isPending: false, error: null, mutate: vi.fn() }),
  useRemoveModel: () => ({ isPending: false, error: null, mutate: vi.fn() }),
}))

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(<QueryClientProvider client={client}><DocumentConverterPage /></QueryClientProvider>)
}

afterEach(cleanup)

describe('DocumentConverterPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useConverterStore.setState({
      selected: [],
      destinationRoot: null,
      options: { processingMode: 'automatic', maxTokensPerPage: 2048, preservePageImages: false },
      previewJobId: null,
      modelProgress: null,
      nativeDragActive: false,
      progressByJob: {},
    })
  })

  it('shows the real model-not-installed flow', () => {
    renderPage()
    expect(screen.getByText('Document Converter')).toBeTruthy()
    expect(screen.getByText('Processing is local. Documents are never uploaded.')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Install Document AI' }))
    expect(installModel).toHaveBeenCalledOnce()
  })

  it('selects multiple files and exposes Force OCR mode', async () => {
    chooseFiles.mockResolvedValue([
      { path: '/tmp/report.pdf', name: 'report.pdf', sizeBytes: 1000, pageCount: 2, supported: true, errorCode: null, errorMessage: null },
      { path: '/tmp/bad.xlsx', name: 'bad.xlsx', sizeBytes: 20, pageCount: null, supported: false, errorCode: 'UNSUPPORTED_FILE', errorMessage: 'Unsupported format' },
    ])
    renderPage()
    fireEvent.click(screen.getByRole('button', { name: 'Choose files' }))
    await waitFor(() => expect(screen.getByText('report.pdf')).toBeTruthy())
    expect(screen.getByText('bad.xlsx')).toBeTruthy()
    const mode = screen.getByLabelText('Processing mode')
    fireEvent.change(mode, { target: { value: 'force-ocr' } })
    expect(useConverterStore.getState().options.processingMode).toBe('force-ocr')
  })
})
