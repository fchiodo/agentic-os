import { useMutation, useQueryClient } from '@tanstack/react-query'
import {
  AlertCircle,
  CheckCircle2,
  Clipboard,
  Download,
  FileImage,
  FileText,
  FolderOpen,
  HardDrive,
  History,
  Loader2,
  Play,
  RefreshCw,
  ScanText,
  ShieldCheck,
  Square,
  Trash2,
  Upload,
  X,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useState, type DragEvent } from 'react'
import * as api from './api'
import { MarkdownPreview } from './components/markdown-preview'
import {
  converterJobsKey,
  useConversionJobs,
  useConverterStatus,
  useInstallModel,
  useRemoveModel,
  useRepairModel,
} from './hooks'
import type { ConversionJob, ConversionProgress, ModelProgress, SelectedDocument } from './schema'
import { useConverterStore } from './store'

const activeStatuses = new Set(['queued', 'preparing', 'rendering', 'ocr', 'reconstructing', 'writing'])
const domains = ['work', 'planphysique', 'personal', 'family', 'finance', 'research']

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${units[unit]}`
}

function formatDate(value: string | null): string {
  if (!value) return '—'
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function ModelCard({
  progress,
}: {
  progress: ModelProgress | null
}) {
  const status = useConverterStatus()
  const install = useInstallModel()
  const repair = useRepairModel()
  const remove = useRemoveModel()
  const model = status.data?.model
  const pending = install.isPending || repair.isPending
  const error = install.error ?? repair.error ?? remove.error ?? status.error

  if (model?.checksumValid) {
    return (
      <section className="converter-model-card is-ready" aria-label="Document AI status">
        <div className="converter-model-icon"><CheckCircle2 aria-hidden="true" size={22} /></div>
        <div className="converter-model-copy">
          <strong>Document AI ready</strong>
          <span>{model.displayName} · version {model.version} · {formatBytes(model.installedSizeBytes)}</span>
        </div>
        <details className="converter-model-actions">
          <summary>Manage</summary>
          <div>
            <button className="converter-button secondary" disabled={repair.isPending} onClick={() => repair.mutate()} type="button">
              <RefreshCw aria-hidden="true" size={15} /> Repair
            </button>
            <button className="converter-button danger" disabled={remove.isPending} onClick={() => remove.mutate()} type="button">
              <Trash2 aria-hidden="true" size={15} /> Remove
            </button>
          </div>
        </details>
      </section>
    )
  }

  return (
    <section className="converter-model-card" aria-label="Install Document AI">
      <div className="converter-model-icon"><Download aria-hidden="true" size={22} /></div>
      <div className="converter-model-copy">
        <strong>{model?.installed ? 'Document AI needs repair' : 'Document AI'}</strong>
        <span>To convert scanned PDFs and images locally, install {model?.displayName ?? 'the document AI model'}.</span>
        <span className="converter-privacy-line"><ShieldCheck aria-hidden="true" size={14} /> Processing is local. Documents are never uploaded.</span>
        {model ? <span>Download {formatBytes(model.downloadSizeBytes)} · {model.license}</span> : null}
        {pending && progress ? (
          <div className="converter-download-progress">
            <div className="converter-progress-track"><span style={{ width: `${progress.percent}%` }} /></div>
            <span>{progress.stage === 'installed' ? 'Installed' : `Downloading ${formatBytes(progress.downloadedBytes)} / ${formatBytes(progress.totalBytes)}`}</span>
          </div>
        ) : null}
        {error ? <span className="converter-error" role="alert">{errorText(error)}</span> : null}
      </div>
      <div className="converter-model-cta">
        {pending ? (
          <>
            <button className="converter-button primary" disabled type="button"><Loader2 className="is-spinning" size={16} /> Installing</button>
            <button className="converter-button secondary" onClick={() => void api.cancelModelDownload()} type="button">Cancel</button>
          </>
        ) : model?.installed ? (
          <button className="converter-button primary" onClick={() => repair.mutate()} type="button"><RefreshCw size={16} /> Repair</button>
        ) : (
          <button className="converter-button primary" onClick={() => install.mutate()} type="button"><Download size={16} /> Install Document AI</button>
        )}
      </div>
    </section>
  )
}

function DropZone({ onFiles }: { onFiles: (files: SelectedDocument[]) => void }) {
  const [browserDragging, setBrowserDragging] = useState(false)
  const nativeDragActive = useConverterStore((state) => state.nativeDragActive)
  const setNativeDragActive = useConverterStore((state) => state.setNativeDragActive)
  const dragging = browserDragging || nativeDragActive
  const choose = useMutation({ mutationFn: api.chooseFiles, onSuccess: onFiles })

  const inspectDropped = useCallback((paths: string[]) => {
    if (paths.length > 0) void api.inspectPaths(paths).then(onFiles)
  }, [onFiles])

  const browserDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    setBrowserDragging(false)
    setNativeDragActive(false)
    const paths = Array.from(event.dataTransfer.files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((path): path is string => Boolean(path))
    inspectDropped(paths)
  }

  return (
    <div
      className={`converter-drop-zone ${dragging ? 'is-dragging' : ''}`}
      onDragEnter={(event) => { event.preventDefault(); setBrowserDragging(true) }}
      onDragLeave={() => setBrowserDragging(false)}
      onDragOver={(event) => event.preventDefault()}
      onDrop={browserDrop}
    >
      <Upload aria-hidden="true" size={30} />
      <strong>Drop documents here</strong>
      <span>PDF · PNG · JPG</span>
      <button className="converter-button secondary" disabled={choose.isPending} onClick={() => choose.mutate()} type="button">
        {choose.isPending ? <Loader2 className="is-spinning" size={16} /> : <FolderOpen size={16} />}
        Choose files
      </button>
      {choose.error ? <span className="converter-error" role="alert">{errorText(choose.error)}</span> : null}
    </div>
  )
}

function QueueItem({ job, progress, onCancel }: {
  job: ConversionJob
  progress?: ConversionProgress
  onCancel: (id: string) => void
}) {
  const percent = progress?.percent ?? (job.totalPages ? job.processedPages / job.totalPages * 100 : null)
  return (
    <article className="converter-job-row">
      <div className="converter-file-icon"><FileText aria-hidden="true" size={18} /></div>
      <div className="converter-job-main">
        <div className="converter-job-title"><strong>{job.sourceName}</strong><span className={`converter-status is-${job.status}`}>{job.status}</span></div>
        <span>{progress?.label ?? job.stageLabel ?? job.stage}</span>
        {job.totalPages ? <span>Page {job.processedPages} / {job.totalPages}</span> : null}
        {percent !== null ? <div className="converter-progress-track"><span style={{ width: `${Math.min(percent, 100)}%` }} /></div> : null}
      </div>
      <button aria-label={`Cancel ${job.sourceName}`} className="converter-icon-button" onClick={() => onCancel(job.id)} type="button"><Square size={15} /></button>
    </article>
  )
}

function PreviewPanel({ jobId, onClose }: { jobId: string; onClose: () => void }) {
  const preview = useMutation({ mutationFn: () => api.getPreview(jobId) })
  const [domain, setDomain] = useState('personal')
  const importMutation = useMutation({ mutationFn: () => api.importToMemory(jobId, domain) })
  useEffect(() => { preview.mutate() }, [jobId]) // eslint-disable-line react-hooks/exhaustive-deps
  const copy = async () => {
    if (preview.data) await navigator.clipboard.writeText(preview.data.markdown)
  }
  return (
    <div className="converter-preview-backdrop" role="presentation">
      <section aria-label="Markdown preview" aria-modal="true" className="converter-preview-panel" role="dialog">
        <header>
          <div><span className="eyebrow">Canonical Markdown</span><h2>{preview.data?.sourceName ?? 'Preview'}</h2></div>
          <button aria-label="Close preview" className="converter-icon-button" onClick={onClose} type="button"><X size={18} /></button>
        </header>
        <div className="converter-preview-toolbar">
          <button className="converter-button secondary" disabled={!preview.data} onClick={() => void copy()} type="button"><Clipboard size={15} /> Copy Markdown</button>
          <select aria-label="Memory domain" onChange={(event) => setDomain(event.target.value)} value={domain}>{domains.map((value) => <option key={value} value={value}>{value}</option>)}</select>
          <button className="converter-button primary" disabled={!preview.data || importMutation.isPending} onClick={() => importMutation.mutate()} type="button"><ScanText size={15} /> Import to Memory</button>
          {importMutation.isSuccess ? <span className="converter-success">Imported for review</span> : null}
        </div>
        <div className="converter-preview-body">
          {preview.isPending ? <Loader2 className="is-spinning" /> : null}
          {preview.error ? <p className="converter-error">{errorText(preview.error)}</p> : null}
          {preview.data ? <MarkdownPreview jobId={jobId} markdown={preview.data.markdown} /> : null}
        </div>
      </section>
    </div>
  )
}

export function DocumentConverterPage() {
  const queryClient = useQueryClient()
  const status = useConverterStatus()
  const jobsQuery = useConversionJobs()
  const modelProgress = useConverterStore((state) => state.modelProgress)
  const progress = useConverterStore((state) => state.progressByJob)
  const [notice, setNotice] = useState<string | null>(null)
  const selected = useConverterStore((state) => state.selected)
  const destinationRoot = useConverterStore((state) => state.destinationRoot)
  const options = useConverterStore((state) => state.options)
  const previewJobId = useConverterStore((state) => state.previewJobId)
  const addSelected = useConverterStore((state) => state.addSelected)
  const removeSelected = useConverterStore((state) => state.removeSelected)
  const clearSelected = useConverterStore((state) => state.clearSelected)
  const setDestinationRoot = useConverterStore((state) => state.setDestinationRoot)
  const setProcessingMode = useConverterStore((state) => state.setProcessingMode)
  const setPreservePageImages = useConverterStore((state) => state.setPreservePageImages)
  const setPreviewJobId = useConverterStore((state) => state.setPreviewJobId)
  const create = useMutation({
    mutationFn: api.createJobs,
    onSuccess: async (result) => {
      clearSelected()
      if (result.duplicates.length > 0) setNotice(`${result.duplicates.length} document(s) were converted before; new conversions were queued.`)
      if (result.rejected.length > 0) setNotice(`${result.rejected.length} file(s) could not be queued.`)
      await queryClient.invalidateQueries({ queryKey: converterJobsKey })
    },
  })
  const cancel = useMutation({
    mutationFn: api.cancelJob,
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: converterJobsKey }),
  })
  const retry = useMutation({
    mutationFn: api.retryJob,
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: converterJobsKey }),
  })
  const removeHistory = useMutation({
    mutationFn: api.deleteHistoryEntry,
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: converterJobsKey }),
  })

  const jobs = jobsQuery.data ?? []
  const active = jobs.filter((job) => activeStatuses.has(job.status))
  const history = jobs.filter((job) => !activeStatuses.has(job.status))
  const validSelected = selected.filter((document) => document.supported)
  const totalBytes = validSelected.reduce((total, document) => total + document.sizeBytes, 0)
  const totalPages = validSelected.reduce((total, document) => total + (document.pageCount ?? 0), 0)
  const diagnostics = status.data ? [
    `Architecture: ${status.data.architecture}`,
    `Sidecar: ${status.data.sidecarVersion}`,
    `Protocol: ${status.data.protocolVersion}`,
    `Engine: ${status.data.engine}`,
    `Model: ${status.data.model.state}`,
    `Model version: ${status.data.model.version}`,
    `Checksum: ${status.data.model.checksumValid ? 'valid' : 'not verified'}`,
    `Status: ${status.data.ready ? 'Ready' : 'Not ready'}`,
  ].join('\n') : ''
  const overall = useMemo(() => {
    const totals = active.reduce((result, job) => {
      result.done += job.processedPages
      result.total += job.totalPages ?? 0
      return result
    }, { done: 0, total: 0 })
    return totals.total > 0 ? totals.done / totals.total * 100 : null
  }, [active])

  return (
    <section className="page-section document-converter-page">
      <header className="converter-page-header">
        <div>
          <p className="eyebrow">Local document understanding</p>
          <h1>Document Converter</h1>
          <p>Convert PDF and images to structured Markdown, entirely on this Mac.</p>
        </div>
        <div className="converter-local-pill"><ShieldCheck size={16} /><span><strong>Local processing</strong>Document content is not uploaded.</span></div>
      </header>

      <ModelCard progress={modelProgress} />

      <div className="converter-layout">
        <div className="converter-primary-column">
          <section className="surface converter-input-card">
            <div className="converter-section-heading"><div><h2>Convert documents</h2><p>Select one or more files. OCR jobs run one at a time to protect memory.</p></div></div>
            <DropZone onFiles={addSelected} />
            {selected.length > 0 ? (
              <div className="converter-selection">
                {selected.map((document) => (
                  <article className={`converter-selected-row ${document.supported ? '' : 'is-rejected'}`} key={document.path}>
                    <div className="converter-file-icon">{/\.pdf$/i.test(document.name) ? <FileText size={18} /> : <FileImage size={18} />}</div>
                    <div><strong>{document.name}</strong><span>{document.supported ? `${document.pageCount ?? 1} page${document.pageCount === 1 ? '' : 's'} · ${formatBytes(document.sizeBytes)}` : document.errorMessage}</span></div>
                    {document.supported ? <CheckCircle2 aria-label="Supported" className="converter-valid" size={17} /> : <AlertCircle aria-label="Unsupported" size={17} />}
                    <button aria-label={`Remove ${document.name}`} className="converter-icon-button" onClick={() => removeSelected(document.path)} type="button"><X size={16} /></button>
                  </article>
                ))}
                <div className="converter-options">
                  <label><span>Processing mode</span><select onChange={(event) => setProcessingMode(event.target.value as typeof options.processingMode)} value={options.processingMode}><option value="automatic">Automatic</option><option value="force-ocr">Force OCR</option><option value="digital-only">Digital text only</option></select></label>
                  <label className="converter-check"><input checked={options.preservePageImages} onChange={(event) => setPreservePageImages(event.target.checked)} type="checkbox" /> Preserve rendered page images in assets</label>
                  <div className="converter-destination"><span>Output: {destinationRoot ?? '~/Documents/'}</span><button className="converter-text-button" onClick={() => void api.chooseDestination().then((value) => { if (value) setDestinationRoot(value) })} type="button">Change destination</button></div>
                </div>
                <footer className="converter-selection-footer">
                  <span>{validSelected.length} document{validSelected.length === 1 ? '' : 's'} · {totalPages} pages · {formatBytes(totalBytes)}</span>
                  <button className="converter-button primary" disabled={validSelected.length === 0 || !status.data?.ready || create.isPending} onClick={() => create.mutate({ inputPaths: validSelected.map((document) => document.path), destinationRoot, options })} type="button"><Play size={16} /> Convert {validSelected.length > 1 ? 'all' : ''}</button>
                </footer>
              </div>
            ) : null}
            {create.error ? <p className="converter-error" role="alert">{errorText(create.error)}</p> : null}
            {notice ? <p className="converter-notice">{notice}</p> : null}
          </section>

          {active.length > 0 ? (
            <section className="surface converter-current-card">
              <div className="converter-section-heading"><div><h2>Current conversions</h2><p>{active.filter((job) => job.status === 'queued').length} queued</p></div><button className="converter-text-button" onClick={() => void api.cancelAll()} type="button">Cancel all</button></div>
              {overall !== null ? <div className="converter-progress-track is-overall"><span style={{ width: `${overall}%` }} /></div> : null}
              <div className="converter-job-list">{active.map((job) => <QueueItem job={job} key={job.id} onCancel={(id) => cancel.mutate(id)} progress={progress[job.id]} />)}</div>
            </section>
          ) : null}

          <section className="surface converter-history-card">
            <div className="converter-section-heading"><div><h2>Recent conversions</h2><p>History is stored locally. Removing an entry does not delete output files.</p></div><History aria-hidden="true" size={20} /></div>
            {jobsQuery.isLoading ? <Loader2 className="is-spinning" /> : null}
            {history.length === 0 && !jobsQuery.isLoading ? <p className="converter-empty">Completed and failed conversions will appear here.</p> : null}
            <div className="converter-history-list">
              {history.map((job) => (
                <article className="converter-history-row" key={job.id}>
                  <div className="converter-file-icon">{job.status === 'completed' ? <CheckCircle2 size={18} /> : <AlertCircle size={18} />}</div>
                  <div className="converter-history-main"><strong>{job.sourceName}</strong><span>{job.status} · {job.engine} · {formatDate(job.completedAt ?? job.createdAt)}</span>{job.status === 'completed' ? <span>{job.pagesDigital} digital · {job.pagesOcr} OCR · {job.assetCount} assets</span> : <span className="converter-error">{job.errorMessage}</span>}</div>
                  <div className="converter-history-actions">
                    {job.status === 'completed' ? <><button className="converter-text-button" onClick={() => setPreviewJobId(job.id)} type="button">Preview</button><button className="converter-icon-button" title="Open folder" onClick={() => void api.openOutput(job.id)} type="button"><FolderOpen size={16} /></button></> : <button className="converter-text-button" onClick={() => retry.mutate(job.id)} type="button">Retry</button>}
                    <button aria-label={`Remove ${job.sourceName} from history`} className="converter-icon-button" onClick={() => removeHistory.mutate(job.id)} type="button"><Trash2 size={15} /></button>
                  </div>
                </article>
              ))}
            </div>
          </section>
        </div>

        <aside className="surface converter-diagnostics">
          <div className="converter-section-heading"><div><h2>Document AI Diagnostics</h2><p>No document content is included.</p></div><HardDrive size={19} /></div>
          {status.error ? <p className="converter-error">{errorText(status.error)}</p> : <pre>{diagnostics || 'Loading diagnostics…'}</pre>}
          <button className="converter-button secondary" disabled={!diagnostics} onClick={() => void navigator.clipboard.writeText(diagnostics)} type="button"><Clipboard size={15} /> Copy diagnostics</button>
          <div className="converter-trust-note"><ShieldCheck size={18} /><p><strong>Private by design</strong><br />Internet is needed only while installing the versioned model. Conversion works offline afterwards.</p></div>
        </aside>
      </div>
      {previewJobId ? <PreviewPanel jobId={previewJobId} onClose={() => setPreviewJobId(null)} /> : null}
    </section>
  )
}
