import {
  AlertTriangle,
  CheckCircle2,
  ClipboardPaste,
  ExternalLink,
  FileUp,
  History,
  Link2,
  Upload,
  X,
} from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { StatusBadge } from '@/components/ui/status-badge'
import { arrayBufferToBase64 } from '@/features/memory/binary'
import {
  useMemoryDocumentImports,
  useMemoryDocumentSourceRead,
  useMemoryImportDocument,
} from '@/features/memory/hooks'
import type { DocumentImportRequest, DocumentInputKind, MemoryWriteProposal } from '@/features/memory/schema'
import { formatRelativeTime } from '@/lib/format'

const DOMAINS = ['work', 'planphysique', 'personal', 'family', 'finance', 'research'] as const
const MAX_DOCUMENT_BYTES = 2 * 1024 * 1024

const DOMAIN_LABELS: Record<string, string> = {
  work: 'Work',
  planphysique: 'PlanPhysique',
  personal: 'Personal',
  family: 'Family',
  finance: 'Finance',
  research: 'Research',
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function proposalTitle(proposal: MemoryWriteProposal): string {
  const match = proposal.newContent.match(/^title:\s*(.+)$/m)
  return match?.[1]?.replace(/^['"]|['"]$/g, '') ?? proposal.vaultPath
}

function byteLength(value: string): number {
  return new TextEncoder().encode(value).length
}

export function DocumentImportPanel({
  defaultDomain,
  onClose,
}: {
  defaultDomain?: string
  onClose: () => void
}) {
  const [domain, setDomain] = useState(defaultDomain ?? 'work')
  const [inputKind, setInputKind] = useState<DocumentInputKind>('text')
  const [title, setTitle] = useState('')
  const [content, setContent] = useState('')
  const [contentEncoding, setContentEncoding] = useState<DocumentImportRequest['contentEncoding']>()
  const [mimeType, setMimeType] = useState('')
  const [fileBytes, setFileBytes] = useState(0)
  const [sourceUrl, setSourceUrl] = useState('')
  const [fileName, setFileName] = useState('')
  const [fileError, setFileError] = useState<string | null>(null)
  const [selectedImportId, setSelectedImportId] = useState<string | null>(null)
  const importMutation = useMemoryImportDocument()
  const historyQuery = useMemoryDocumentImports(domain)
  const sourceQuery = useMemoryDocumentSourceRead(selectedImportId)
  const documentBytes = inputKind === 'url'
    ? 0
    : inputKind === 'file'
      ? fileBytes
      : byteLength(content)
  const canSubmit = title.trim().length > 0
    && (inputKind === 'url' ? sourceUrl.trim().length > 0 : content.trim().length > 0)
    && documentBytes <= MAX_DOCUMENT_BYTES
    && !fileError

  const chooseKind = (kind: DocumentInputKind) => {
    setInputKind(kind)
    importMutation.reset()
    setFileError(null)
    setContent('')
    setContentEncoding(undefined)
    setMimeType('')
    setFileBytes(0)
    setFileName('')
  }

  const selectFile = async (file: File | undefined) => {
    setFileError(null)
    importMutation.reset()
    if (!file) return
    if (file.size > MAX_DOCUMENT_BYTES) {
      setFileError('The file exceeds the 2 MiB limit.')
      return
    }
    try {
      const normalizedName = file.name.toLocaleLowerCase()
      const isPdf = file.type.toLocaleLowerCase() === 'application/pdf' || normalizedName.endsWith('.pdf')
      if (isPdf) {
        const bytes = await file.arrayBuffer()
        const signature = new TextDecoder('ascii').decode(bytes.slice(0, 5))
        if (signature !== '%PDF-') {
          setFileError('This file has a PDF name or type but does not contain a valid PDF signature.')
          return
        }
        setContent(arrayBufferToBase64(bytes))
        setContentEncoding('base64')
        setMimeType('application/pdf')
      } else {
        const text = await file.text()
        if (text.includes('\0')) {
          setFileError('This binary file type is not supported. Choose a PDF or a UTF-8 text document.')
          return
        }
        if (byteLength(text) > MAX_DOCUMENT_BYTES) {
          setFileError('The decoded file exceeds the 2 MiB limit.')
          return
        }
        setContent(text)
        setContentEncoding('utf8')
        setMimeType(file.type || 'text/plain')
      }
      setFileBytes(file.size)
      setFileName(file.name)
      if (!title.trim()) setTitle(file.name.replace(/\.[^.]+$/, ''))
    } catch (error) {
      setFileError(`Could not read this file: ${errorMessage(error)}`)
    }
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!canSubmit) return
    importMutation.mutate({
      domain,
      inputKind,
      title: title.trim(),
      content: inputKind === 'url' ? undefined : content,
      contentEncoding: inputKind === 'file' ? contentEncoding : undefined,
      mimeType: inputKind === 'file' ? mimeType : undefined,
      sourceUrl: inputKind === 'url' ? sourceUrl.trim() : undefined,
      fileName: inputKind === 'file' ? fileName : undefined,
    })
  }

  return (
    <div className="document-import surface">
      <div className="panel-heading document-import-heading">
        <div>
          <p className="eyebrow">Full source + governed extraction</p>
          <h2>Import document</h2>
        </div>
        <button aria-label="Close import" className="icon-button" onClick={onClose} type="button">
          <X aria-hidden="true" size={16} />
        </button>
      </div>

      <div className="document-import-explainer">
        <span><strong>1</strong> Preserve the complete source</span>
        <span><strong>2</strong> Extract up to 10 atomic facts locally</span>
        <span><strong>3</strong> Approve each fact in Governance</span>
      </div>

      <div className="document-import-layout">
        <form className="document-import-form" onSubmit={submit}>
          <div className="document-kind-tabs" role="tablist" aria-label="Document source type">
            <button aria-selected={inputKind === 'text'} className={inputKind === 'text' ? 'is-active' : ''} onClick={() => chooseKind('text')} role="tab" type="button"><ClipboardPaste aria-hidden="true" size={15} />Paste text</button>
            <button aria-selected={inputKind === 'file'} className={inputKind === 'file' ? 'is-active' : ''} onClick={() => chooseKind('file')} role="tab" type="button"><FileUp aria-hidden="true" size={15} />File</button>
            <button aria-selected={inputKind === 'url'} className={inputKind === 'url' ? 'is-active' : ''} onClick={() => chooseKind('url')} role="tab" type="button"><Link2 aria-hidden="true" size={15} />URL</button>
          </div>

          <div className="document-import-fields">
            <label>
              <span>Domain</span>
              <select onChange={(event) => setDomain(event.target.value)} value={domain}>
                {DOMAINS.map((item) => <option key={item} value={item}>{DOMAIN_LABELS[item]}</option>)}
              </select>
            </label>
            <label className="document-import-title">
              <span>Document title</span>
              <input maxLength={200} onChange={(event) => setTitle(event.target.value)} placeholder="Sierra Headless API" required value={title} />
            </label>

            {inputKind === 'file' && (
              <label className="document-file-picker">
                <span>PDF or text document</span>
                <input accept=".pdf,.md,.mdx,.txt,.json,.yaml,.yml,.html,.htm,.xml,application/pdf,text/*,application/json,application/xml" onChange={(event) => { void selectFile(event.target.files?.[0]) }} type="file" />
                <span className="document-file-drop"><Upload aria-hidden="true" size={20} />{fileName || 'Choose a PDF or UTF-8 text file up to 2 MiB'}</span>
              </label>
            )}
            {inputKind === 'url' && (
              <label className="document-import-wide">
                <span>Public URL</span>
                <input onChange={(event) => setSourceUrl(event.target.value)} placeholder="https://docs.example.com/api.md" type="url" value={sourceUrl} />
                <small>Redirects, local addresses, credentials, binary responses, and sources over 2 MiB are blocked.</small>
              </label>
            )}
            {inputKind === 'text' && (
              <label className="document-import-wide">
                <span>Full document</span>
                <textarea onChange={(event) => setContent(event.target.value)} placeholder="Paste the complete document here. It will not be truncated." rows={14} value={content} />
              </label>
            )}
            {inputKind !== 'url' && (
              <div className={`document-byte-count ${documentBytes > MAX_DOCUMENT_BYTES ? 'is-over' : ''}`}>
                {(documentBytes / 1024).toFixed(1)} KiB / 2,048 KiB
              </div>
            )}
          </div>

          {(fileError || importMutation.error) && <div className="inline-error" role="alert">{fileError ?? errorMessage(importMutation.error)}</div>}
          {importMutation.data && (
            <div className="document-import-result" role="status">
              <div className="document-import-result-head">
                <CheckCircle2 aria-hidden="true" size={18} />
                <div><strong>Source preserved and versioned</strong><span>{importMutation.data.proposals.length} proposal(s) waiting for review · {importMutation.data.import.byteCount.toLocaleString()} bytes</span></div>
              </div>
              <code>{importMutation.data.import.sourcePath}</code>
              {importMutation.data.warnings.map((warning) => <div className="document-warning" key={warning}><AlertTriangle aria-hidden="true" size={14} />{warning}</div>)}
              {importMutation.data.proposals.length > 0 && (
                <div className="document-candidate-list">
                  {importMutation.data.proposals.map((proposal) => <span key={proposal.id}>{proposalTitle(proposal)}</span>)}
                </div>
              )}
              <p>Review and approve the proposed facts in the Governance panel on the right.</p>
            </div>
          )}

          <div className="memory-compose-actions">
            <button className="primary-button" disabled={!canSubmit || importMutation.isPending} type="submit">
              <Upload aria-hidden="true" size={16} />
              {importMutation.isPending ? 'Preserving and extracting…' : 'Import and create proposals'}
            </button>
          </div>
        </form>

        <aside className="document-history">
          <div className="document-history-head"><History aria-hidden="true" size={16} /><strong>Source history</strong></div>
          {historyQuery.isLoading && <p className="row-subtle">Loading sources…</p>}
          {historyQuery.error && <div className="inline-error" role="alert">{errorMessage(historyQuery.error)}</div>}
          <div className="document-history-list">
            {historyQuery.data?.map((item) => (
              <button className={selectedImportId === item.id ? 'is-active' : ''} key={item.id} onClick={() => setSelectedImportId(item.id)} type="button">
                <span className="document-history-title">{item.title}</span>
                <span><StatusBadge label={item.inputKind} tone="neutral" /><StatusBadge label={item.status.replace('_', ' ')} tone={item.status === 'completed' ? 'success' : item.status === 'pending' ? 'warning' : 'neutral'} /></span>
                <small>{item.candidateCount} facts · {formatRelativeTime(new Date(item.createdAt).getTime())}</small>
              </button>
            ))}
            {historyQuery.data?.length === 0 && <div className="empty-state"><h3>No sources yet</h3><p>Your complete imported documents will be listed here.</p></div>}
          </div>
          {selectedImportId && (
            <div className="document-source-preview">
              <div><strong>{sourceQuery.data?.import.originalPath ? 'Extracted PDF text' : 'Original source'}</strong>{sourceQuery.data?.gitLastCommit && <code>{sourceQuery.data.gitLastCommit}</code>}</div>
              {sourceQuery.isLoading && <p className="row-subtle">Reading snapshot…</p>}
              {sourceQuery.error && <div className="inline-error" role="alert">{errorMessage(sourceQuery.error)}</div>}
              {sourceQuery.data && <pre>{sourceQuery.data.content}</pre>}
              {sourceQuery.data?.import.originalPath && <code>Original: {sourceQuery.data.import.originalPath}</code>}
              {sourceQuery.data?.import.sourceRef.startsWith('http') && <a href={sourceQuery.data.import.sourceRef} rel="noreferrer" target="_blank">Open recorded URL <ExternalLink aria-hidden="true" size={13} /></a>}
            </div>
          )}
        </aside>
      </div>
    </div>
  )
}
