import {
  ArchiveRestore,
  Brain,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Clock,
  Copy,
  FileText,
  Flag,
  FolderOpen,
  Gauge,
  Layers,
  Loader2,
  MessageCircleQuestion,
  Network,
  Plus,
  Save,
  Search,
  Shield,
  Square,
  Tag,
  Upload,
  Wrench,
  X,
} from 'lucide-react'
import { lazy, Suspense, type FormEvent, Fragment, type ReactNode, useCallback, useEffect, useMemo, useState } from 'react'
import { isAskStoppedError } from '@/features/memory/api'
import { DiffView } from '@/components/ui/diff-view'
import { StatusBadge } from '@/components/ui/status-badge'
import { DocumentImportPanel } from '@/features/memory/document-import-panel'
import {
  useMemoryAsk,
  useMemoryAnswerFeedback,
  useMemoryConfirm,
  useMemoryLint,
  useMemoryMaintenanceRun,
  useMemoryOperations,
  useMemoryProposals,
  useMemoryProposalsDecide,
  useMemoryRead,
  useMemoryReindex,
  useMemoryRetrievalBenchmark,
  useMemoryRetrievalEvalCaseSave,
  useMemorySaveManual,
  useMemorySearch,
  useMemoryTree,
} from '@/features/memory/hooks'
import type {
  MemoryAnswer,
  MemoryAskProgress,
  MemoryCitation,
  MemoryType,
  MemoryWriteProposal,
  Sensitivity,
  VaultNode,
} from '@/features/memory/schema'
import { formatCompactNumber, formatRelativeTime } from '@/lib/format'

const OrbitMapView = lazy(() => import('@/features/memory/orbit-map').then((module) => ({ default: module.OrbitMapView })))

const DOMAINS = ['work', 'planphysique', 'personal', 'family', 'finance', 'research'] as const

const TYPE_ICONS: Record<string, typeof FileText> = {
  fact: FileText,
  decision: FileText,
  preference: Tag,
  entity: Brain,
  episode: Clock,
  synthesis: Layers,
}

const STATUS_TONE: Record<string, 'accent' | 'neutral' | 'success' | 'warning'> = {
  active: 'success',
  stale: 'warning',
  expired: 'neutral',
  pending: 'warning',
  approved: 'success',
  auto_applied: 'accent',
  discarded: 'neutral',
}

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

const CONFIDENCE_LABELS: Record<MemoryAnswer['confidence'], string> = {
  high: 'High confidence',
  medium: 'Medium confidence',
  low: 'Low confidence',
  insufficient: 'Insufficient evidence',
}

function countVaultFiles(nodes: VaultNode[]): number {
  return nodes.reduce((count, node) => count + (node.isDir ? countVaultFiles(node.children) : 1), 0)
}

function countVaultFilesByStatus(nodes: VaultNode[], status: string): number {
  return nodes.reduce((total, node) => {
    if (!node.isDir) return total + (node.status === status ? 1 : 0)
    return total + countVaultFilesByStatus(node.children, status)
  }, 0)
}

function countPopulatedDomains(nodes: VaultNode[]): number {
  return nodes.filter((node) => node.isDir && countVaultFiles([node]) > 0).length
}

function proposalTitle(proposal: MemoryWriteProposal): string {
  const title = proposal.newContent.match(/^title:\s*(.+)$/m)?.[1]?.replace(/^['"]|['"]$/g, '')
  if (title && title.trim().length > 0) return title.trim()
  const fileName = proposal.vaultPath.split('/').at(-1)?.replace(/\.md$/i, '')
  return fileName ?? proposal.vaultPath
}

function savedAnswerBody(answer: MemoryAnswer): string {
  const sourceList = answer.citations
    .map((citation) => `[${citation.number}] ${citation.vaultPath}`)
    .join('\n')
  const value = sourceList ? `${answer.answer}\n\nSources:\n${sourceList}` : answer.answer
  return [...value].slice(0, 1_200).join('')
}

function proposalPreviewLines(unifiedDiff: string): Array<{ tone: 'add' | 'remove'; text: string }> {
  return unifiedDiff
    .split('\n')
    .filter((line) => (line.startsWith('+') || line.startsWith('-')) && !line.startsWith('+++') && !line.startsWith('---'))
    .slice(0, 3)
    .map((line) => ({
      tone: line.startsWith('+') ? 'add' : 'remove',
      text: line,
    }))
}

function MemoryMetricsStrip({
  activeVaultCount,
  approvalRequiredCount,
  approvedCount,
  pendingCount,
  populatedDomainsCount,
  reviewedCount,
  vaultItemCount,
}: {
  activeVaultCount: number
  approvalRequiredCount: number
  approvedCount: number
  pendingCount: number
  populatedDomainsCount: number
  reviewedCount: number
  vaultItemCount: number
}) {
  const items = [
    {
      detail: `${formatCompactNumber(activeVaultCount)} active`,
      label: 'Vault notes',
      tone: 'neutral',
      value: formatCompactNumber(vaultItemCount),
    },
    {
      detail: `${formatCompactNumber(approvalRequiredCount)} requires approval`,
      label: 'Pending review',
      tone: pendingCount > 0 ? 'warning' : 'neutral',
      value: formatCompactNumber(pendingCount),
    },
    {
      detail: `${formatCompactNumber(approvedCount)} approved`,
      label: 'Reviewed writes',
      tone: reviewedCount > 0 ? 'success' : 'neutral',
      value: formatCompactNumber(reviewedCount),
    },
    {
      detail: `${formatCompactNumber(populatedDomainsCount)} of ${DOMAINS.length} active`,
      label: 'Populated domains',
      tone: populatedDomainsCount > 0 ? 'accent' : 'neutral',
      value: formatCompactNumber(populatedDomainsCount),
    },
  ] as const

  return (
    <section aria-label="Memory metrics" className="memory-metric-strip">
      {items.map((item) => (
        <article className="memory-metric-cell" key={item.label}>
          <span className="memory-metric-label">{item.label}</span>
          <div className="memory-metric-value-row">
            <strong className="memory-metric-value">{item.value}</strong>
            <span className={`memory-metric-detail ${item.tone ? `is-${item.tone}` : ''}`}>{item.detail}</span>
          </div>
        </article>
      ))}
    </section>
  )
}

function MemoryControlToolbar({
  includeStale,
  mode,
  onSave,
  onToggleStale,
  setMode,
}: {
  includeStale: boolean
  mode: 'search' | 'ask'
  onSave: () => void
  onToggleStale: (checked: boolean) => void
  setMode: (mode: 'search' | 'ask') => void
}) {
  return (
    <div className="memory-control-topbar">
      <div className="memory-control-segment">
        <button className={mode === 'ask' ? 'is-active' : ''} onClick={() => setMode('ask')} type="button">
          <MessageCircleQuestion aria-hidden="true" size={15} />
          Ask
        </button>
        <button className={mode === 'search' ? 'is-active' : ''} onClick={() => setMode('search')} type="button">
          <Search aria-hidden="true" size={15} />
          Search
        </button>
      </div>
      <div className="memory-control-actions">
        <label className="memory-toggle-pill">
          <input checked={includeStale} onChange={(event) => onToggleStale(event.target.checked)} type="checkbox" />
          <span className="memory-toggle-pill-box" aria-hidden="true" />
          <span>Include stale</span>
        </label>
        <button className="secondary-button memory-toolbar-button" onClick={onSave} type="button">
          <Plus aria-hidden="true" size={15} />
          Save memory
        </button>
      </div>
    </div>
  )
}

function TreeNode({
  node,
  depth,
  onSelect,
  selectedPath,
}: {
  node: VaultNode
  depth: number
  onSelect: (path: string) => void
  selectedPath: string | null
}) {
  const [expanded, setExpanded] = useState(depth < 1)
  const isSelected = selectedPath === node.path

  if (node.isDir) {
    return (
      <div>
        <button
          className={`tree-node tree-node--dir ${isSelected ? 'is-selected' : ''}`}
          onClick={() => setExpanded((value) => !value)}
          style={{ paddingLeft: `${12 + depth * 16}px` }}
          type="button"
        >
          {expanded ? <ChevronDown aria-hidden="true" size={14} /> : <ChevronRight aria-hidden="true" size={14} />}
          <FolderOpen aria-hidden="true" className={expanded ? 'tree-icon--open' : undefined} size={14} />
          <span className="tree-label">{node.name}</span>
        </button>
        {expanded && node.children.map((child) => (
          <TreeNode
            key={child.path}
            depth={depth + 1}
            node={child}
            onSelect={onSelect}
            selectedPath={selectedPath}
          />
        ))}
      </div>
    )
  }

  return (
    <button
      className={`tree-node tree-node--file ${isSelected ? 'is-selected' : ''}`}
      onClick={() => onSelect(node.path)}
      style={{ paddingLeft: `${12 + depth * 16}px` }}
      type="button"
    >
      <FileText aria-hidden="true" size={14} />
      <span className="tree-label">{node.name}</span>
      {node.status && node.status !== 'active' && (
        <StatusBadge label={node.status} tone={STATUS_TONE[node.status] ?? 'neutral'} />
      )}
    </button>
  )
}

function MarkdownContent({ markdown }: { markdown: string }) {
  return (
    <div className="markdown-body">
      {markdown.split('\n').map((line, index) => {
        const key = `${index}-${line.slice(0, 16)}`
        if (line.startsWith('### ')) return <h4 key={key}>{line.slice(4)}</h4>
        if (line.startsWith('## ')) return <h3 key={key}>{line.slice(3)}</h3>
        if (line.startsWith('# ')) return <h2 key={key}>{line.slice(2)}</h2>
        if (line.startsWith('- ')) return <div className="markdown-list-line" key={key}>• {line.slice(2)}</div>
        if (!line.trim()) return <br key={key} />
        return <div key={key}>{line}</div>
      })}
    </div>
  )
}

/**
 * The verifier joins approved claims into one flat string with inline [n]
 * markers. Rendering splits it back into claims so each reads as its own
 * paragraph — with the markers as clickable citation chips — instead of a
 * wall of text.
 */
function splitAnswerClaims(answer: string): string[] {
  const segments = answer.match(/.*?(?:\[\d+\]\s*)+/gs)
  if (!segments) return [answer.trim()]
  const rest = answer.slice(segments.join('').length)
  const claims = [...segments, rest]
    // Joining artifacts (separator dots between claims, an orphaned final
    // period) must never render as bullets of their own.
    .map((segment) => segment.replace(/^[\s.;:]+/, '').trim())
    .filter((segment) => /[\p{L}\p{N}]/u.test(segment))
  return claims.length > 0 ? claims : [answer.trim()]
}

function renderInlineText(text: string, keyPrefix: string): ReactNode[] {
  return text
    .split(/(\*\*[^*]+\*\*|`[^`]+`)/g)
    .filter(Boolean)
    .map((part, index) => {
      const key = `${keyPrefix}-${index}`
      if (part.startsWith('**') && part.endsWith('**')) return <strong key={key}>{part.slice(2, -2)}</strong>
      if (part.startsWith('`') && part.endsWith('`')) return <code key={key}>{part.slice(1, -1)}</code>
      return <Fragment key={key}>{part}</Fragment>
    })
}

function AnswerRichText({ answer, citations, onSelect }: { answer: string; citations: MemoryCitation[]; onSelect: (path: string) => void }) {
  const claims = useMemo(() => splitAnswerClaims(answer), [answer])
  const byNumber = useMemo(() => new Map(citations.map((citation) => [citation.number, citation])), [citations])

  const renderClaim = (claim: string, claimKey: string) =>
    claim
      .split(/(\[\d+\])/g)
      .filter(Boolean)
      .map((part, index) => {
        const key = `${claimKey}-${index}`
        const marker = /^\[(\d+)\]$/.exec(part)
        if (!marker) return <Fragment key={key}>{renderInlineText(part, key)}</Fragment>
        const citation = byNumber.get(Number(marker[1]))
        if (!citation) return <sup className="memory-cite-chip memory-cite-chip--plain" key={key}>{marker[1]}</sup>
        return (
          <sup key={key}>
            <button
              aria-label={`Citation ${citation.number}: ${citation.title}`}
              className="memory-cite-chip"
              onClick={() => onSelect(citation.vaultPath)}
              title={`[${citation.number}] ${citation.vaultPath} — “${citation.excerpt}”`}
              type="button"
            >
              {citation.number}
            </button>
          </sup>
        )
      })

  if (claims.length <= 1) {
    return <p className="memory-answer-copy">{renderClaim(claims[0] ?? answer, 'claim-0')}</p>
  }
  return (
    <div className="memory-answer-flow">
      {claims.map((claim, index) => (
        <p className="memory-answer-claim" key={`claim-${index}-${claim.slice(0, 24)}`}>{renderClaim(claim, `claim-${index}`)}</p>
      ))}
    </div>
  )
}

function MemoryReader({ path, onClose, onSelect }: { path: string; onClose: () => void; onSelect?: (path: string) => void }) {
  const readQuery = useMemoryRead(path)
  const confirmMutation = useMemoryConfirm()

  if (readQuery.isLoading) return <div className="memory-reader-empty"><p>Loading…</p></div>
  if (readQuery.error) return <div className="memory-reader-empty" role="alert"><p>{errorMessage(readQuery.error)}</p></div>
  if (!readQuery.data) return <div className="memory-reader-empty"><p>Could not load file.</p></div>

  const { data } = readQuery
  const fm = data.frontmatter
  return (
    <div className="memory-reader">
      <div className="memory-reader-head">
        <div className="memory-reader-head-left">
          <FileText aria-hidden="true" size={18} />
          <div>
            <p className="eyebrow">{fm ? DOMAIN_LABELS[fm.domain] ?? fm.domain : 'Unindexed file'}</p>
            <h2 className="memory-reader-title">{fm?.title ?? path}</h2>
          </div>
        </div>
        <div className="memory-reader-head-right">
          {fm && <StatusBadge label={fm.memType} tone="accent" />}
          <StatusBadge label={data.status} tone={STATUS_TONE[data.status] ?? 'neutral'} />
          {fm?.sensitivity === 'sensitive' && <StatusBadge label="sensitive" tone="warning" />}
          <button aria-label="Close reader" className="icon-button" onClick={onClose} type="button"><X aria-hidden="true" size={16} /></button>
        </div>
      </div>

      {fm && (
        <>
          <dl className="memory-reader-meta">
            <div><dt>Confidence</dt><dd>{Math.round(fm.confidence * 100)}%</dd></div>
            <div><dt>Confirmations</dt><dd>{fm.confirmations ?? 0}</dd></div>
            <div><dt>Provenance</dt><dd>{fm.provenance.source}</dd></div>
            <div><dt>Created</dt><dd>{new Date(fm.created).toLocaleDateString()}</dd></div>
            {fm.validFrom && <div><dt>Valid from</dt><dd>{fm.validFrom}</dd></div>}
            {fm.validUntil && <div><dt>Valid until</dt><dd>{fm.validUntil}</dd></div>}
            {fm.supersedes && <div><dt>Supersedes</dt><dd className="mono">{fm.supersedes}</dd></div>}
            {fm.supersededBy && <div><dt>Superseded by</dt><dd className="mono">{fm.supersededBy}</dd></div>}
            {fm.staleAfterDays && <div><dt>Stale after</dt><dd>{fm.staleAfterDays}d</dd></div>}
            {fm.expires && <div><dt>Expires</dt><dd>{fm.expires}</dd></div>}
            {data.gitLastCommit && <div><dt>Git</dt><dd className="mono">{data.gitLastCommit}</dd></div>}
          </dl>
          {fm.tags.length > 0 && <div className="tag-row">{fm.tags.map((tag) => <span className="tag-chip" key={tag}>{tag}</span>)}</div>}
          {fm.sources.length > 0 && <div className="memory-source-chain"><strong>Original sources</strong>{fm.sources.map((source) => <code key={source} title={source}>{source}</code>)}</div>}
        </>
      )}

      <div className="memory-reader-body"><MarkdownContent markdown={data.markdown} /></div>
      {fm && fm.related.length > 0 && onSelect && (
        <div className="memory-reader-related">
          <span className="memory-citations-label">Linked memories</span>
          {fm.related.map((relatedPath) => (
            <button className="memory-reader-related-link" key={relatedPath} onClick={() => onSelect(relatedPath)} type="button">
              <FileText aria-hidden="true" size={14} />
              <span>{relatedPath}</span>
            </button>
          ))}
        </div>
      )}
      {fm && data.status === 'stale' && (
        <div className="memory-reader-actions">
          <button className="primary-button" disabled={confirmMutation.isPending} onClick={() => confirmMutation.mutate(fm.id)} type="button">
            <CheckCircle2 aria-hidden="true" size={16} />
            {confirmMutation.isPending ? 'Confirming…' : 'Confirm still true'}
          </button>
          {confirmMutation.error && <span className="inline-error" role="alert">{errorMessage(confirmMutation.error)}</span>}
        </div>
      )}
    </div>
  )
}

function SearchResult({ item, onSelect }: {
  item: { row: { id: string; title: string; memType: string; domain: string; vaultPath: string; status: string; summary?: string | null }; score: number; relevance: number; recency: number; trust: number }
  onSelect: (path: string) => void
}) {
  const Icon = TYPE_ICONS[item.row.memType] ?? FileText
  return (
    <button className="memory-search-result" onClick={() => onSelect(item.row.vaultPath)} type="button">
      <div className="memory-search-result-head">
        <Icon aria-hidden="true" size={16} />
        <span className="memory-search-result-title">{item.row.title}</span>
        <StatusBadge label={item.row.memType} tone="accent" />
        <StatusBadge label={item.row.status} tone={STATUS_TONE[item.row.status] ?? 'neutral'} />
        <span className="memory-search-score">{Math.round(item.score * 100)}%</span>
      </div>
      {item.row.summary && <p className="memory-search-result-summary">{item.row.summary}</p>}
      <div className="memory-search-result-meta">
        <span>{DOMAIN_LABELS[item.row.domain] ?? item.row.domain}</span>
        <span className="memory-search-score-detail" title="Relevance · recency · trust">
          rel {Math.round(item.relevance * 100)}% · rec {Math.round(item.recency * 100)}% · trust {Math.round(item.trust * 100)}%
        </span>
      </div>
    </button>
  )
}

function SaveMemoryForm({ defaultDomain, onClose }: { defaultDomain?: string; onClose: () => void }) {
  const saveMutation = useMemorySaveManual()
  const [domain, setDomain] = useState(defaultDomain ?? 'work')
  const [memType, setMemType] = useState<MemoryType>('fact')
  const [title, setTitle] = useState('')
  const [body, setBody] = useState('')
  const [tags, setTags] = useState('')
  const [sensitivity, setSensitivity] = useState<Sensitivity>('normal')

  const submit = (event: FormEvent) => {
    event.preventDefault()
    saveMutation.mutate({
      domain,
      memType,
      title: title.trim(),
      body: body.trim(),
      tags: tags.split(',').map((tag) => tag.trim()).filter(Boolean),
      sensitivity,
      source: 'manual',
    })
  }

  return (
    <form className="memory-compose surface" onSubmit={submit}>
      <div className="panel-heading">
        <div><p className="eyebrow">Admission pipeline</p><h2>Save to memory</h2></div>
        <button aria-label="Close form" className="icon-button" onClick={onClose} type="button"><X aria-hidden="true" size={16} /></button>
      </div>
      <p className="row-subtle">The gate checks secrets, provenance, duplication, sensitivity, and domain isolation before anything reaches the vault.</p>
      <div className="memory-compose-grid">
        <label><span>Domain</span><select onChange={(event) => setDomain(event.target.value)} value={domain}>{DOMAINS.map((item) => <option key={item} value={item}>{DOMAIN_LABELS[item]}</option>)}</select></label>
        <label><span>Type</span><select onChange={(event) => setMemType(event.target.value as MemoryType)} value={memType}>{(['fact', 'decision', 'preference', 'entity', 'episode', 'synthesis'] as MemoryType[]).map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
        <label><span>Sensitivity</span><select onChange={(event) => setSensitivity(event.target.value as Sensitivity)} value={sensitivity}><option value="normal">normal</option><option value="sensitive">sensitive</option></select></label>
        <label className="memory-compose-title"><span>Title</span><input maxLength={200} onChange={(event) => setTitle(event.target.value)} required value={title} /></label>
        <label className="memory-compose-wide"><span>Body</span><textarea maxLength={memType === 'episode' || memType === 'entity' ? undefined : 1200} onChange={(event) => setBody(event.target.value)} required rows={8} value={body} /></label>
        <label className="memory-compose-wide"><span>Tags <small>comma separated</small></span><input onChange={(event) => setTags(event.target.value)} placeholder="project, vendor, architecture" value={tags} /></label>
      </div>
      {saveMutation.error && <div className="inline-error" role="alert">{errorMessage(saveMutation.error)}</div>}
      {saveMutation.data && (
        <div className="memory-operation-result" role="status">
          <CheckCircle2 aria-hidden="true" size={16} />
          {saveMutation.data.status === 'auto_applied' ? 'Saved, committed, indexed, and audited.' : 'Proposal created and waiting for approval.'}
        </div>
      )}
      <div className="memory-compose-actions">
        <button className="primary-button" disabled={saveMutation.isPending || !title.trim() || !body.trim()} type="submit"><Plus aria-hidden="true" size={16} />{saveMutation.isPending ? 'Checking…' : 'Run gate and save'}</button>
      </div>
    </form>
  )
}

function AskProgressPanel({ events, pending, stopped = false }: { events: MemoryAskProgress[]; pending: boolean; stopped?: boolean }) {
  const [elapsedSeconds, setElapsedSeconds] = useState(0)

  useEffect(() => {
    if (!pending) return
    const startedAt = Date.now()
    const timer = window.setInterval(() => {
      setElapsedSeconds(Math.floor((Date.now() - startedAt) / 1000))
    }, 250)
    return () => window.clearInterval(timer)
  }, [pending])

  return (
    <div aria-live="polite" className="memory-ask-progress" role="status">
      <div className="memory-ask-progress-header">
        <span className="memory-ask-progress-title">
          {pending ? 'Synthesizing with evidence' : stopped ? 'Stopped' : 'Stopped before completing'}
        </span>
        {pending && <span className="memory-ask-progress-elapsed">{elapsedSeconds}s</span>}
      </div>
      <ol className="memory-ask-progress-steps">
        {events.length === 0 && pending && (
          <li className="memory-ask-progress-step memory-ask-progress-step--active">
            <Loader2 aria-hidden="true" className="memory-ask-progress-spinner" size={14} />
            <span>Contacting the local synthesis engine…</span>
          </li>
        )}
        {events.map((event, index) => {
          const isActive = pending && index === events.length - 1
          return (
            <li
              className={`memory-ask-progress-step ${isActive ? 'memory-ask-progress-step--active' : 'memory-ask-progress-step--done'}`}
              key={`${event.at}-${index}`}
            >
              {isActive
                ? <Loader2 aria-hidden="true" className="memory-ask-progress-spinner" size={14} />
                : <CheckCircle2 aria-hidden="true" size={14} />}
              <span>{event.label}</span>
            </li>
          )
        })}
      </ol>
    </div>
  )
}

function AskMemory({
  domain,
  header,
  includeStale,
  onSelect,
}: {
  domain?: string
  header?: ReactNode
  includeStale: boolean
  onSelect: (path: string) => void
}) {
  const askMutation = useMemoryAsk()
  const saveMutation = useMemorySaveManual()
  const feedbackMutation = useMemoryAnswerFeedback()
  const evalCaseMutation = useMemoryRetrievalEvalCaseSave()
  const [question, setQuestion] = useState('')
  const [askDomain, setAskDomain] = useState(domain ?? 'work')
  const [copiedAnswerId, setCopiedAnswerId] = useState<string | null>(null)
  const [copyError, setCopyError] = useState<string | null>(null)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    setCopiedAnswerId(null)
    setCopyError(null)
    saveMutation.reset()
    feedbackMutation.reset()
    evalCaseMutation.reset()
    askMutation.mutate({ question: question.trim(), domain: askDomain, includeStale })
  }

  const copyAnswer = async (answer: MemoryAnswer) => {
    try {
      if (!navigator.clipboard) throw new Error('Clipboard access is unavailable.')
      await navigator.clipboard.writeText(answer.answer)
      setCopiedAnswerId(answer.id)
      setCopyError(null)
    } catch (error) {
      setCopyError(errorMessage(error))
    }
  }

  const saveAnswer = (answer: MemoryAnswer) => {
    // Saved answers are first-class synthesis notes linked to the memories
    // they cited: answered questions compound instead of evaporating.
    const citedMemoryPaths = [...new Set(
      answer.citations
        .filter((citation) => citation.sourceKind === 'memory')
        .map((citation) => citation.vaultPath),
    )]
    saveMutation.mutate({
      domain: answer.domain,
      memType: 'synthesis',
      title: `Answer: ${answer.question}`.slice(0, 200),
      body: savedAnswerBody(answer),
      tags: ['ask', 'grounded-answer'],
      sensitivity: 'normal',
      source: `memory-ask:${answer.id}`,
      confidence: answer.confidenceScore,
      related: citedMemoryPaths,
    })
  }

  const flagAnswer = (answer: MemoryAnswer) => {
    feedbackMutation.mutate({
      answerId: answer.id,
      question: answer.question,
      domain: answer.domain,
      feedback: 'flagged',
    })
  }

  const addBenchmarkCase = (answer: MemoryAnswer) => {
    evalCaseMutation.mutate({
      question: answer.question,
      domain: answer.domain,
      expectedSources: [...new Set(answer.citations.map((citation) => citation.vaultPath))],
    })
  }

  return (
    <div className="memory-ask">
      <div className="surface memory-control-panel">
        {header}
        <form className="memory-control-form memory-control-form--ask memory-ask-form" onSubmit={submit}>
          <label className="memory-control-query">
            <span className="memory-control-query-icon" aria-hidden="true">
              <MessageCircleQuestion size={18} />
            </span>
            <input aria-label="Ask the Second Brain" onChange={(event) => setQuestion(event.target.value)} placeholder="What did we decide about the PowerReviews feed?" value={question} />
          </label>
          <label className="memory-control-domain">
            <select aria-label="Answer domain" onChange={(event) => setAskDomain(event.target.value)} value={askDomain}>{DOMAINS.map((item) => <option key={item} value={item}>{DOMAIN_LABELS[item]}</option>)}</select>
          </label>
          {askMutation.isPending
            ? (
              <button className="secondary-button memory-control-submit memory-ask-stop" onClick={() => askMutation.stop()} type="button">
                <Square aria-hidden="true" size={12} />
                Stop
              </button>
            )
            : <button className="primary-button memory-control-submit" disabled={question.trim().length < 2} type="submit">Ask</button>}
        </form>
      </div>
      {(askMutation.isPending || (askMutation.error !== null && askMutation.progress.length > 0)) && (
        <AskProgressPanel
          events={askMutation.progress}
          pending={askMutation.isPending}
          stopped={askMutation.error !== null && isAskStoppedError(askMutation.error)}
        />
      )}
      {askMutation.error && !isAskStoppedError(askMutation.error) && <div className="inline-error" role="alert">{errorMessage(askMutation.error)}</div>}
      {askMutation.data && (
        <div className={`memory-answer ${askMutation.data.abstained ? 'memory-answer--abstained' : ''}`}>
          <div className="memory-answer-header">
            <div className="memory-answer-title">
              <h2>Answer</h2>
              <span>{DOMAIN_LABELS[askMutation.data.domain] ?? askMutation.data.domain}</span>
            </div>
            <div className={`memory-answer-confidence memory-answer-confidence--${askMutation.data.confidence}`}>
              <span aria-hidden="true" />
              {CONFIDENCE_LABELS[askMutation.data.confidence]}
              {!askMutation.data.abstained && ` · ${askMutation.data.sourceCount} source${askMutation.data.sourceCount === 1 ? '' : 's'}`}
            </div>
          </div>
          <AnswerRichText answer={askMutation.data.answer} citations={askMutation.data.citations} onSelect={onSelect} />
          {askMutation.data.warnings.map((warning) => <div className="memory-answer-warning" key={warning}>{warning}</div>)}
          {askMutation.data.citations.length > 0 && (
            <div className="memory-citations">
              <span className="memory-citations-label">Cited from your vault</span>
              {askMutation.data.citations.map((citation) => (
                <button aria-label={`Open citation ${citation.number}: ${citation.title}`} key={`${citation.id}-${citation.number}`} onClick={() => onSelect(citation.vaultPath)} type="button">
                  <FileText aria-hidden="true" size={16} />
                  <span className="memory-citation-copy">
                    <strong>[{citation.number}] {citation.vaultPath}</strong>
                    <small title={citation.excerpt}>“{citation.excerpt}”</small>
                  </span>
                  <span className="memory-citation-score">{Math.round(citation.score * 100)}%</span>
                </button>
              ))}
            </div>
          )}
          <div className="memory-answer-footer">
            <div className="memory-answer-actions">
              <button className="primary-button" disabled={askMutation.data.abstained || saveMutation.isPending || Boolean(saveMutation.data)} onClick={() => saveAnswer(askMutation.data)} type="button">
                <Save aria-hidden="true" size={14} />
                {saveMutation.isPending ? 'Checking…' : saveMutation.data ? 'Saved' : 'Save memory'}
              </button>
              <button className="secondary-button" onClick={() => void copyAnswer(askMutation.data)} type="button">
                <Copy aria-hidden="true" size={14} />
                {copiedAnswerId === askMutation.data.id ? 'Copied' : 'Copy'}
              </button>
              <button className="secondary-button" disabled={feedbackMutation.isPending || feedbackMutation.isSuccess} onClick={() => flagAnswer(askMutation.data)} type="button">
                <Flag aria-hidden="true" size={14} />
                {feedbackMutation.isPending ? 'Flagging…' : feedbackMutation.isSuccess ? 'Flagged' : 'Flag'}
              </button>
              <button className="secondary-button" disabled={askMutation.data.abstained || askMutation.data.citations.length === 0 || evalCaseMutation.isPending || evalCaseMutation.isSuccess} onClick={() => addBenchmarkCase(askMutation.data)} type="button">
                <Gauge aria-hidden="true" size={14} />
                {evalCaseMutation.isPending ? 'Adding…' : evalCaseMutation.isSuccess ? 'Benchmark case added' : 'Use as benchmark case'}
              </button>
            </div>
            <span>
              AI-synthesized · citation verified · abstains without evidence
              {askMutation.durationMs !== null && ` · ${Math.max(1, Math.round(askMutation.durationMs / 1000))}s`}
            </span>
          </div>
          {saveMutation.data && <div className="memory-operation-result" role="status"><CheckCircle2 aria-hidden="true" size={16} />{saveMutation.data.status === 'auto_applied' ? 'Answer saved, indexed, and audited.' : 'Memory proposal created and waiting for approval.'}</div>}
          {evalCaseMutation.error && <div className="inline-error" role="alert">{errorMessage(evalCaseMutation.error)}</div>}
          {saveMutation.error && <div className="inline-error" role="alert">{errorMessage(saveMutation.error)}</div>}
          {copyError && <div className="inline-error" role="alert">{copyError}</div>}
          {feedbackMutation.error && <div className="inline-error" role="alert">{errorMessage(feedbackMutation.error)}</div>}
        </div>
      )}
      {!askMutation.data && !askMutation.isPending && !askMutation.error && <div className="memory-welcome"><MessageCircleQuestion aria-hidden="true" className="memory-welcome-icon" size={48} /><h2>Ask with evidence</h2><p>The AI synthesizes the relevant passages, then a local verifier removes uncited claims. Without sufficient evidence, it abstains instead of inventing an answer.</p></div>}
    </div>
  )
}

type GateReport = { passed?: boolean; checks?: Array<{ name: string; passed: boolean; detail: string }> }

function ProposalCard({ proposal, onDecide }: { proposal: MemoryWriteProposal; onDecide: (id: string, decision: string) => void }) {
  const [expanded, setExpanded] = useState(false)
  const gate = useMemo<GateReport>(() => {
    try { return JSON.parse(proposal.gateReport) as GateReport } catch { return {} }
  }, [proposal.gateReport])
  const previewLines = useMemo(() => proposalPreviewLines(proposal.unifiedDiff), [proposal.unifiedDiff])
  const leadPill = proposal.sensitivity === 'sensitive'
    ? { label: 'Sensitive', tone: 'warning' as const }
    : { label: proposal.op, tone: 'neutral' as const }

  return (
    <div className={`proposal-card ${proposal.status === 'pending' ? 'proposal-card--pending' : 'proposal-card--activity'}`}>
      <div className="proposal-card-meta">
        <span className={`proposal-card-pill ${leadPill.tone === 'warning' ? 'proposal-card-pill--warning' : ''}`}>{leadPill.label}</span>
        <span className="proposal-card-time">{formatRelativeTime(new Date(proposal.createdAt).getTime())}</span>
      </div>
      <h3 className="proposal-card-title">{proposalTitle(proposal)}</h3>
      <div className="proposal-card-preview">
        <span className="proposal-card-path" title={proposal.vaultPath}>{proposal.vaultPath}</span>
        {previewLines.length > 0 ? (
          previewLines.map((line, index) => (
            <span className={`proposal-card-diffline is-${line.tone}`} key={`${proposal.id}-${index}-${line.text.slice(0, 16)}`}>
              {line.text}
            </span>
          ))
        ) : <span className="proposal-card-placeholder">Review the gate report and diff before deciding.</span>}
      </div>
      {(gate.checks?.length ?? 0) > 0 && (
        <button className="proposal-toggle" onClick={() => setExpanded((value) => !value)} type="button">
          <ChevronRight aria-hidden="true" className={expanded ? 'is-expanded' : ''} size={13} />
          {expanded ? 'Hide gate checks' : 'Show gate checks'}
        </button>
      )}
      {expanded && (
        <div className="proposal-review">
          <div className="proposal-checks">
            {gate.checks?.map((check) => <div key={check.name}><span>{check.passed ? '✓' : '×'} {check.name}</span><small>{check.detail}</small></div>)}
          </div>
          <DiffView unifiedDiff={proposal.unifiedDiff} />
        </div>
      )}
      {proposal.status === 'pending' && proposal.requiresApproval && (
        <div className="proposal-actions">
          <button className="proposal-action proposal-action--approve" onClick={() => onDecide(proposal.id, 'approve')} type="button">Approve</button>
          <button className="proposal-action proposal-action--dismiss" onClick={() => onDecide(proposal.id, 'discard')} type="button">Dismiss</button>
        </div>
      )}
      {proposal.status !== 'pending' && (
        <div className="proposal-card-statusline">
          <StatusBadge label={proposal.status.replace('_', ' ')} tone={STATUS_TONE[proposal.status] ?? 'neutral'} />
          <span>{DOMAIN_LABELS[proposal.domain] ?? proposal.domain}</span>
        </div>
      )}
    </div>
  )
}

function GovernanceRail({
  activity,
  collapsed,
  decideError,
  onDecide,
  pending,
  railTab,
  setRailTab,
}: {
  activity: MemoryWriteProposal[]
  collapsed: boolean
  decideError: unknown
  onDecide: (id: string, decision: string) => void
  pending: MemoryWriteProposal[]
  railTab: 'pending' | 'activity'
  setRailTab: (value: 'pending' | 'activity') => void
}) {
  const visibleProposals = railTab === 'pending' ? pending : activity

  return (
    <aside aria-hidden={collapsed} className={`memory-governance-rail surface ${collapsed ? 'is-closed' : ''}`}>
      <div className="memory-governance-header">
        <Shield aria-hidden="true" size={17} />
        <span className="memory-governance-title">Governance</span>
        {pending.length > 0 && <span className="memory-governance-count">{pending.length}</span>}
      </div>
      <div className="memory-governance-tabs">
        <button className={railTab === 'pending' ? 'is-active' : ''} onClick={() => setRailTab('pending')} type="button">Pending</button>
        <button className={railTab === 'activity' ? 'is-active' : ''} onClick={() => setRailTab('activity')} type="button">Activity</button>
      </div>
      {railTab === 'pending' && pending.length > 0 && (
        <div className="memory-governance-banner">
          <Flag aria-hidden="true" size={15} />
          <span>
            {pending.length} write{pending.length === 1 ? '' : 's'} {pending.length === 1 ? 'is' : 'are'} waiting for review before {pending.length === 1 ? 'it reaches' : 'they reach'} the vault.
          </span>
        </div>
      )}
      {decideError && <div className="inline-error memory-rail-error" role="alert">{errorMessage(decideError)}</div>}
      <div className="memory-governance-list">
        {visibleProposals.map((proposal) => <ProposalCard key={proposal.id} onDecide={onDecide} proposal={proposal} />)}
        {visibleProposals.length === 0 && <div className="empty-state"><h3>{railTab === 'pending' ? 'Nothing to review' : 'No activity yet'}</h3><p>{railTab === 'pending' ? 'Sensitive and truth-changing writes appear here.' : 'Automatic and decided writes appear here.'}</p></div>}
      </div>
      <p className="memory-governance-footer">Sensitive and truth-changing writes appear here before they are committed.</p>
    </aside>
  )
}

export function MemoryPage() {
  const [view, setView] = useState<'library' | 'map'>('library')
  const [searchText, setSearchText] = useState('')
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [domainFilter, setDomainFilter] = useState<string | undefined>()
  const [includeStale, setIncludeStale] = useState(false)
  const [mode, setMode] = useState<'search' | 'ask'>('search')
  const [showComposer, setShowComposer] = useState(false)
  const [showImporter, setShowImporter] = useState(false)
  const [selectedImportId, setSelectedImportId] = useState<string | null>(null)
  const [railTab, setRailTab] = useState<'pending' | 'activity'>('pending')
  const [governanceOpen, setGovernanceOpen] = useState(true)

  const treeQuery = useMemoryTree(domainFilter)
  const searchQuery = useMemorySearch(searchText, domainFilter, includeStale)
  const proposalsQuery = useMemoryProposals()
  const decideMutation = useMemoryProposalsDecide()
  const reindexMutation = useMemoryReindex()
  const maintenanceMutation = useMemoryMaintenanceRun()
  const operationsQuery = useMemoryOperations()
  const benchmarkMutation = useMemoryRetrievalBenchmark()
  const lintMutation = useMemoryLint()

  const vaultItemCount = useMemo(() => countVaultFiles(treeQuery.data ?? []), [treeQuery.data])
  const activeVaultCount = useMemo(() => countVaultFilesByStatus(treeQuery.data ?? [], 'active'), [treeQuery.data])
  const populatedDomainsCount = useMemo(() => countPopulatedDomains(treeQuery.data ?? []), [treeQuery.data])
  const pending = useMemo(() => proposalsQuery.data?.filter((proposal) => proposal.status === 'pending') ?? [], [proposalsQuery.data])
  const operationsNeedingAttention = useMemo(() => operationsQuery.data?.filter((operation) => operation.status === 'needs_attention') ?? [], [operationsQuery.data])
  const activity = useMemo(() => proposalsQuery.data?.filter((proposal) => proposal.status !== 'pending') ?? [], [proposalsQuery.data])
  const approvalRequiredCount = useMemo(() => pending.filter((proposal) => proposal.requiresApproval).length, [pending])
  const approvedCount = useMemo(() => activity.filter((proposal) => proposal.status === 'approved').length, [activity])
  const selectPath = useCallback((path: string) => {
    setSelectedPath(path)
    setShowComposer(false)
    setShowImporter(false)
    setSelectedImportId(null)
    setView('library')
  }, [])
  const openImporter = useCallback(() => { setSelectedImportId(null); setShowImporter(true); setShowComposer(false); setSelectedPath(null); setView('library') }, [])
  const openImportedSource = useCallback((importId: string, domain?: string) => {
    if (domain) setDomainFilter(domain)
    setSelectedImportId(importId)
    setShowImporter(true)
    setShowComposer(false)
    setSelectedPath(null)
    setView('library')
  }, [])
  const openComposer = useCallback(() => { setShowComposer(true); setShowImporter(false); setSelectedPath(null); setView('library') }, [])
  const controlToolbar = (
    <MemoryControlToolbar
      includeStale={includeStale}
      mode={mode}
      onSave={openComposer}
      onToggleStale={setIncludeStale}
      setMode={setMode}
    />
  )

  return (
    <section className="page-section memory-page">
      <div className="surface memory-view-header">
        <div><p className="eyebrow">Second Brain</p><h2>{view === 'map' ? '3D Brain' : 'Governed memory'}</h2></div>
        <div className="memory-view-switch" aria-label="Memory view">
          <button className={view === 'library' ? 'is-active' : ''} onClick={() => setView('library')} type="button"><Brain aria-hidden="true" size={15} />Library</button>
          <button className={view === 'map' ? 'is-active' : ''} onClick={() => setView('map')} type="button"><Network aria-hidden="true" size={15} />3D Brain</button>
        </div>
      </div>
      {operationsNeedingAttention.length > 0 && <div className="inline-error" role="alert">{operationsNeedingAttention.length} interrupted memory operation{operationsNeedingAttention.length === 1 ? '' : 's'} need attention. The journal has preserved the exact stage and no conflicting state was guessed.</div>}
      {view === 'map' ? (
        <Suspense fallback={<div className="orbit-loading"><Network aria-hidden="true" size={34} /><p>Loading graph renderer…</p></div>}>
          <OrbitMapView onOpenMemory={selectPath} onOpenSource={openImportedSource} />
        </Suspense>
      ) : (
        <>
          <MemoryMetricsStrip
            activeVaultCount={activeVaultCount}
            approvalRequiredCount={approvalRequiredCount}
            approvedCount={approvedCount}
            pendingCount={pending.length}
            populatedDomainsCount={populatedDomainsCount}
            reviewedCount={activity.length}
            vaultItemCount={vaultItemCount}
          />
      <div
        className="memory-layout"
        style={{
          gridTemplateColumns: `280px minmax(0, 1fr) ${governanceOpen ? '340px' : '0px'} 32px`,
        }}
      >
        <aside className="memory-sidebar surface">
          <div className="panel-heading">
            <div><p className="eyebrow">Local vault</p><h2>Second Brain</h2></div>
            <div className="panel-heading-actions">
              <button aria-label="Import document" className="icon-button" onClick={openImporter} title="Import document" type="button"><Upload aria-hidden="true" size={16} /></button>
              <button aria-label="Save a memory" className="icon-button" onClick={openComposer} type="button"><Plus aria-hidden="true" size={16} /></button>
            </div>
          </div>
          <div className="memory-domain-strip">
            <button className={`segment-button ${!domainFilter ? 'is-active' : ''}`} onClick={() => setDomainFilter(undefined)} type="button">All</button>
            {DOMAINS.map((domain) => <button className={`segment-button ${domainFilter === domain ? 'is-active' : ''}`} key={domain} onClick={() => setDomainFilter(domain)} type="button">{DOMAIN_LABELS[domain]}</button>)}
          </div>
          <div className="memory-tree">
            {treeQuery.isLoading && <span className="row-subtle">Loading vault…</span>}
            {treeQuery.error && <span className="inline-error" role="alert">{errorMessage(treeQuery.error)}</span>}
            {treeQuery.data?.map((node) => <TreeNode depth={0} key={node.path} node={node} onSelect={selectPath} selectedPath={selectedPath} />)}
            {treeQuery.data?.length === 0 && <div className="empty-state"><h3>Empty vault</h3><p>Save a fact or decision to begin.</p></div>}
          </div>
          <div className="memory-sidebar-tools">
            <button disabled={reindexMutation.isPending} onClick={() => reindexMutation.mutate()} type="button"><ArchiveRestore aria-hidden="true" size={14} />{reindexMutation.isPending ? 'Indexing…' : 'Reindex'}</button>
            <button disabled={maintenanceMutation.isPending} onClick={() => maintenanceMutation.mutate()} type="button"><Wrench aria-hidden="true" size={14} />{maintenanceMutation.isPending ? 'Running…' : 'Maintenance'}</button>
            <button disabled={benchmarkMutation.isPending} onClick={() => benchmarkMutation.mutate()} type="button"><Gauge aria-hidden="true" size={14} />{benchmarkMutation.isPending ? 'Measuring…' : 'Benchmark'}</button>
            <button disabled={lintMutation.isPending} onClick={() => lintMutation.mutate({ deep: true, domain: domainFilter })} title="Check links, orphans, staleness, and contradictions" type="button"><Shield aria-hidden="true" size={14} />{lintMutation.isPending ? 'Linting…' : 'Lint'}</button>
          </div>
          {(reindexMutation.data || maintenanceMutation.data) && <div className="memory-maintenance-result" role="status">{reindexMutation.data && `${reindexMutation.data.indexed} indexed · ${reindexMutation.data.drifted} drifted · ${reindexMutation.data.orphaned} orphaned`}{maintenanceMutation.data && `${maintenanceMutation.data.expired} archived · ${maintenanceMutation.data.markedStale} stale · ${maintenanceMutation.data.consolidationProposals} consolidation proposals · ${maintenanceMutation.data.deferredExpirations} deferred`}</div>}
          {(reindexMutation.error || maintenanceMutation.error || lintMutation.error || benchmarkMutation.error) && <div className="inline-error" role="alert">{errorMessage(reindexMutation.error ?? maintenanceMutation.error ?? lintMutation.error ?? benchmarkMutation.error)}</div>}
          {benchmarkMutation.data && <div className="memory-maintenance-result" role="status">{benchmarkMutation.data.cases === 0 ? 'No confirmed benchmark cases yet. Use a verified Ask answer to add one.' : `${benchmarkMutation.data.cases} realistic cases · hit@5 FTS ${(benchmarkMutation.data.baseline.sourceHitRateAtFive * 100).toFixed(0)}% · candidate ${(benchmarkMutation.data.candidate.sourceHitRateAtFive * 100).toFixed(0)}% · production ${(benchmarkMutation.data.production.sourceHitRateAtFive * 100).toFixed(0)}% · p95 ${benchmarkMutation.data.production.latencyP95Ms.toFixed(1)} ms`}<small>Corpus {benchmarkMutation.data.corpusMemories} · fuzzy scan {benchmarkMutation.data.fuzzyScanCount} · semantic {benchmarkMutation.data.semanticBackend}</small></div>}
          {lintMutation.data && (
            <div className="memory-lint-result" role="status">
              <span className="memory-lint-summary">
                Lint: {lintMutation.data.scanned} notes · {lintMutation.data.findings.length === 0 ? 'no issues' : `${lintMutation.data.findings.length} finding${lintMutation.data.findings.length === 1 ? '' : 's'}`}
              </span>
              {lintMutation.data.findings.map((finding, index) => (
                <div className={`memory-lint-finding memory-lint-finding--${finding.severity}`} key={`${finding.kind}-${index}`}>
                  <span className="memory-lint-kind">{finding.kind.replace('_', ' ')}</span>
                  <p>{finding.detail}</p>
                  {finding.paths.map((findingPath) => (
                    <button className="memory-lint-path" key={findingPath} onClick={() => selectPath(findingPath)} type="button">{findingPath}</button>
                  ))}
                </div>
              ))}
            </div>
          )}
          <div className="memory-sidebar-footer"><span className="row-subtle">{formatCompactNumber(proposalsQuery.data?.length ?? 0)} writes</span>{pending.length > 0 && <StatusBadge label={`${pending.length} pending`} tone="warning" />}</div>
        </aside>

        <main className="memory-main">
          {showImporter ? <DocumentImportPanel defaultDomain={domainFilter} initialImportId={selectedImportId} key={selectedImportId ?? 'new-import'} onClose={() => { setShowImporter(false); setSelectedImportId(null) }} /> : showComposer ? <SaveMemoryForm defaultDomain={domainFilter} onClose={() => setShowComposer(false)} /> : selectedPath ? <MemoryReader onClose={() => setSelectedPath(null)} onSelect={selectPath} path={selectedPath} /> : (
            <>
              {mode === 'search' ? (
                <>
                  <div className="surface memory-control-panel">
                    {controlToolbar}
                    <div className="memory-control-form memory-control-form--search">
                      <label className="memory-control-query">
                        <span className="memory-control-query-icon" aria-hidden="true">
                          <Search size={18} />
                        </span>
                        <input aria-label="Search memory" onChange={(event) => setSearchText(event.target.value)} placeholder="Search titles, facts, decisions, people…" type="search" value={searchText} />
                      </label>
                    </div>
                  </div>
                  <div className="memory-search-results">
                    {searchQuery.error && <div className="inline-error" role="alert">{errorMessage(searchQuery.error)}</div>}
                    {searchText.length >= 2 && searchQuery.data?.map((item) => <SearchResult item={item} key={item.row.id} onSelect={selectPath} />)}
                    {searchText.length >= 2 && searchQuery.data?.length === 0 && <div className="empty-state"><h3>No evidence found</h3><p>Try other terms, another domain, or include stale memories.</p></div>}
                    {searchText.length < 2 && <div className="memory-welcome"><Brain aria-hidden="true" className="memory-welcome-icon" size={48} /><h2>Your governed memory</h2><p>Markdown is the source of truth; SQLite powers retrieval; Git and the audit chain preserve every change.</p></div>}
                  </div>
                </>
              ) : <AskMemory domain={domainFilter} header={controlToolbar} includeStale={includeStale} onSelect={selectPath} />}
            </>
          )}
        </main>

        <GovernanceRail
          activity={activity}
          collapsed={!governanceOpen}
          decideError={decideMutation.error}
          onDecide={(id, decision) => decideMutation.mutate({ id, decision })}
          pending={pending}
          railTab={railTab}
          setRailTab={setRailTab}
        />

        <div className="memory-governance-tab-col">
          <button
            aria-expanded={governanceOpen}
            aria-label={governanceOpen ? 'Collapse governance' : 'Expand governance'}
            className="memory-governance-tab"
            onClick={() => setGovernanceOpen((value) => !value)}
            type="button"
          >
            {governanceOpen ? <ChevronRight aria-hidden="true" size={14} /> : <ChevronLeft aria-hidden="true" size={14} />}
            <span className="memory-governance-tab-label">Governance</span>
            {pending.length > 0 && <span className="memory-governance-tab-count">{pending.length}</span>}
          </button>
        </div>
      </div>
        </>
      )}
    </section>
  )
}
