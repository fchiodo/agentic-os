import {
  ArchiveRestore,
  Brain,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Clock,
  FileText,
  FolderOpen,
  MessageCircleQuestion,
  Plus,
  Search,
  Shield,
  Tag,
  Trash2,
  Upload,
  Wrench,
  X,
} from 'lucide-react'
import { type FormEvent, useCallback, useMemo, useState } from 'react'
import { DiffView } from '@/components/ui/diff-view'
import { StatusBadge } from '@/components/ui/status-badge'
import { DocumentImportPanel } from '@/features/memory/document-import-panel'
import {
  useMemoryAsk,
  useMemoryConfirm,
  useMemoryMaintenanceRun,
  useMemoryProposals,
  useMemoryProposalsDecide,
  useMemoryRead,
  useMemoryReindex,
  useMemorySaveManual,
  useMemorySearch,
  useMemoryTree,
} from '@/features/memory/hooks'
import type {
  MemoryType,
  MemoryWriteProposal,
  Sensitivity,
  VaultNode,
} from '@/features/memory/schema'
import { formatCompactNumber, formatRelativeTime } from '@/lib/format'

const DOMAINS = ['work', 'planphysique', 'personal', 'family', 'finance', 'research'] as const

const TYPE_ICONS: Record<string, typeof FileText> = {
  fact: FileText,
  decision: FileText,
  preference: Tag,
  entity: Brain,
  episode: Clock,
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

function MemoryReader({ path, onClose }: { path: string; onClose: () => void }) {
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
            {fm.staleAfterDays && <div><dt>Stale after</dt><dd>{fm.staleAfterDays}d</dd></div>}
            {fm.expires && <div><dt>Expires</dt><dd>{fm.expires}</dd></div>}
            {data.gitLastCommit && <div><dt>Git</dt><dd className="mono">{data.gitLastCommit}</dd></div>}
          </dl>
          {fm.tags.length > 0 && <div className="tag-row">{fm.tags.map((tag) => <span className="tag-chip" key={tag}>{tag}</span>)}</div>}
        </>
      )}

      <div className="memory-reader-body"><MarkdownContent markdown={data.markdown} /></div>
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
        <label><span>Type</span><select onChange={(event) => setMemType(event.target.value as MemoryType)} value={memType}>{(['fact', 'decision', 'preference', 'entity', 'episode'] as MemoryType[]).map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
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

function AskMemory({ domain, includeStale, onSelect }: { domain?: string; includeStale: boolean; onSelect: (path: string) => void }) {
  const askMutation = useMemoryAsk()
  const [question, setQuestion] = useState('')
  const [askDomain, setAskDomain] = useState(domain ?? 'work')

  const submit = (event: FormEvent) => {
    event.preventDefault()
    askMutation.mutate({ question: question.trim(), domain: askDomain, includeStale })
  }

  return (
    <div className="memory-ask">
      <form className="memory-ask-form" onSubmit={submit}>
        <MessageCircleQuestion aria-hidden="true" size={20} />
        <input aria-label="Ask the Second Brain" onChange={(event) => setQuestion(event.target.value)} placeholder="What did we decide about the PowerReviews feed?" value={question} />
        <select aria-label="Answer domain" onChange={(event) => setAskDomain(event.target.value)} value={askDomain}>{DOMAINS.map((item) => <option key={item} value={item}>{DOMAIN_LABELS[item]}</option>)}</select>
        <button className="primary-button" disabled={askMutation.isPending || question.trim().length < 2} type="submit">{askMutation.isPending ? 'Reading…' : 'Ask'}</button>
      </form>
      {askMutation.error && <div className="inline-error" role="alert">{errorMessage(askMutation.error)}</div>}
      {askMutation.data && (
        <div className={`memory-answer ${askMutation.data.abstained ? 'memory-answer--abstained' : ''}`}>
          <p className="eyebrow">Grounded answer</p>
          <div className="memory-answer-copy">{askMutation.data.answer}</div>
          {askMutation.data.warnings.map((warning) => <div className="memory-answer-warning" key={warning}>{warning}</div>)}
          {askMutation.data.citations.length > 0 && (
            <div className="memory-citations">
              {askMutation.data.citations.map((citation) => (
                <button key={citation.id} onClick={() => onSelect(citation.vaultPath)} type="button">
                  <span>[{citation.number}] {citation.title}</span>
                  <span>{citation.status} · {Math.round(citation.score * 100)}%</span>
                </button>
              ))}
            </div>
          )}
        </div>
      )}
      {!askMutation.data && !askMutation.isPending && <div className="memory-welcome"><MessageCircleQuestion aria-hidden="true" className="memory-welcome-icon" size={48} /><h2>Ask with evidence</h2><p>Answers are extractive and cite exact vault files. If no evidence is present, the system abstains instead of inventing an answer.</p></div>}
    </div>
  )
}

type GateReport = { passed?: boolean; checks?: Array<{ name: string; passed: boolean; detail: string }> }

function ProposalCard({ proposal, onDecide }: { proposal: MemoryWriteProposal; onDecide: (id: string, decision: string) => void }) {
  const [expanded, setExpanded] = useState(false)
  const gate = useMemo<GateReport>(() => {
    try { return JSON.parse(proposal.gateReport) as GateReport } catch { return {} }
  }, [proposal.gateReport])
  return (
    <div className="proposal-card">
      <div className="proposal-card-head">
        <StatusBadge label={proposal.op} tone={proposal.op === 'supersede' ? 'warning' : proposal.op === 'create' ? 'success' : 'accent'} />
        <span className="proposal-card-path" title={proposal.vaultPath}>{proposal.vaultPath}</span>
        <StatusBadge label={proposal.status} tone={STATUS_TONE[proposal.status] ?? 'neutral'} />
      </div>
      <div className="proposal-card-meta"><span>{DOMAIN_LABELS[proposal.domain] ?? proposal.domain}</span><span>{formatRelativeTime(new Date(proposal.createdAt).getTime())}</span>{proposal.requiresApproval && <StatusBadge label="needs approval" tone="warning" />}</div>
      <button className="proposal-toggle" onClick={() => setExpanded((value) => !value)} type="button">{expanded ? 'Hide review' : 'Review gate and diff'}</button>
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
          <button className="primary-button" onClick={() => onDecide(proposal.id, 'approve')} type="button"><CheckCircle2 aria-hidden="true" size={14} />Approve</button>
          <button className="icon-button" onClick={() => onDecide(proposal.id, 'discard')} title="Discard" type="button"><Trash2 aria-hidden="true" size={14} /></button>
        </div>
      )}
    </div>
  )
}

export function MemoryPage() {
  const [searchText, setSearchText] = useState('')
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [domainFilter, setDomainFilter] = useState<string | undefined>()
  const [includeStale, setIncludeStale] = useState(false)
  const [mode, setMode] = useState<'search' | 'ask'>('search')
  const [showComposer, setShowComposer] = useState(false)
  const [showImporter, setShowImporter] = useState(false)
  const [railTab, setRailTab] = useState<'pending' | 'activity'>('pending')

  const treeQuery = useMemoryTree(domainFilter)
  const searchQuery = useMemorySearch(searchText, domainFilter, includeStale)
  const proposalsQuery = useMemoryProposals()
  const decideMutation = useMemoryProposalsDecide()
  const reindexMutation = useMemoryReindex()
  const maintenanceMutation = useMemoryMaintenanceRun()

  const pending = useMemo(() => proposalsQuery.data?.filter((proposal) => proposal.status === 'pending') ?? [], [proposalsQuery.data])
  const activity = useMemo(() => proposalsQuery.data?.filter((proposal) => proposal.status !== 'pending') ?? [], [proposalsQuery.data])
  const visibleProposals = railTab === 'pending' ? pending : activity
  const selectPath = useCallback((path: string) => { setSelectedPath(path); setShowComposer(false); setShowImporter(false) }, [])

  return (
    <section className="page-section memory-page">
      <div className="memory-layout">
        <aside className="memory-sidebar surface">
          <div className="panel-heading">
            <div><p className="eyebrow">Local vault</p><h2>Second Brain</h2></div>
            <div className="panel-heading-actions">
              <button aria-label="Import document" className="icon-button" onClick={() => { setShowImporter(true); setShowComposer(false); setSelectedPath(null) }} title="Import document" type="button"><Upload aria-hidden="true" size={16} /></button>
              <button aria-label="Save a memory" className="icon-button" onClick={() => { setShowComposer(true); setShowImporter(false); setSelectedPath(null) }} type="button"><Plus aria-hidden="true" size={16} /></button>
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
          </div>
          {(reindexMutation.data || maintenanceMutation.data) && <div className="memory-maintenance-result" role="status">{reindexMutation.data && `${reindexMutation.data.indexed} indexed · ${reindexMutation.data.drifted} drifted · ${reindexMutation.data.orphaned} orphaned`}{maintenanceMutation.data && `${maintenanceMutation.data.expired} archived · ${maintenanceMutation.data.markedStale} stale`}</div>}
          {(reindexMutation.error || maintenanceMutation.error) && <div className="inline-error" role="alert">{errorMessage(reindexMutation.error ?? maintenanceMutation.error)}</div>}
          <div className="memory-sidebar-footer"><span className="row-subtle">{formatCompactNumber(proposalsQuery.data?.length ?? 0)} writes</span>{pending.length > 0 && <StatusBadge label={`${pending.length} pending`} tone="warning" />}</div>
        </aside>

        <main className="memory-main">
          {showImporter ? <DocumentImportPanel defaultDomain={domainFilter} onClose={() => setShowImporter(false)} /> : showComposer ? <SaveMemoryForm defaultDomain={domainFilter} onClose={() => setShowComposer(false)} /> : selectedPath ? <MemoryReader onClose={() => setSelectedPath(null)} path={selectedPath} /> : (
            <>
              <div className="surface memory-mode-bar">
                <div className="memory-mode-switch"><button className={mode === 'search' ? 'is-active' : ''} onClick={() => setMode('search')} type="button"><Search aria-hidden="true" size={15} />Search</button><button className={mode === 'ask' ? 'is-active' : ''} onClick={() => setMode('ask')} type="button"><MessageCircleQuestion aria-hidden="true" size={15} />Ask</button></div>
                <label className="memory-toggle-label"><input checked={includeStale} onChange={(event) => setIncludeStale(event.target.checked)} type="checkbox" />Include stale</label>
                <div className="memory-mode-actions"><button className="secondary-button" onClick={() => { setShowImporter(true); setShowComposer(false) }} type="button"><Upload aria-hidden="true" size={15} />Import document</button><button className="primary-button" onClick={() => { setShowComposer(true); setShowImporter(false) }} type="button"><Plus aria-hidden="true" size={15} />Save memory</button></div>
              </div>
              {mode === 'search' ? (
                <>
                  <div className="surface memory-search-bar"><Search aria-hidden="true" size={18} /><input aria-label="Search memory" onChange={(event) => setSearchText(event.target.value)} placeholder="Search titles, facts, decisions, people…" type="search" value={searchText} /></div>
                  <div className="memory-search-results">
                    {searchQuery.error && <div className="inline-error" role="alert">{errorMessage(searchQuery.error)}</div>}
                    {searchText.length >= 2 && searchQuery.data?.map((item) => <SearchResult item={item} key={item.row.id} onSelect={selectPath} />)}
                    {searchText.length >= 2 && searchQuery.data?.length === 0 && <div className="empty-state"><h3>No evidence found</h3><p>Try other terms, another domain, or include stale memories.</p></div>}
                    {searchText.length < 2 && <div className="memory-welcome"><Brain aria-hidden="true" className="memory-welcome-icon" size={48} /><h2>Your governed memory</h2><p>Markdown is the source of truth; SQLite powers retrieval; Git and the audit chain preserve every change.</p></div>}
                  </div>
                </>
              ) : <AskMemory domain={domainFilter} includeStale={includeStale} onSelect={selectPath} />}
            </>
          )}
        </main>

        <aside className="memory-proposals-rail surface">
          <div className="memory-proposals-toggle"><Shield aria-hidden="true" size={16} /><span>Governance</span>{pending.length > 0 && <span className="memory-proposals-count">{pending.length}</span>}</div>
          <div className="memory-rail-tabs"><button className={railTab === 'pending' ? 'is-active' : ''} onClick={() => setRailTab('pending')} type="button">Pending</button><button className={railTab === 'activity' ? 'is-active' : ''} onClick={() => setRailTab('activity')} type="button">Activity</button></div>
          {decideMutation.error && <div className="inline-error memory-rail-error" role="alert">{errorMessage(decideMutation.error)}</div>}
          <div className="memory-proposals-list">
            {visibleProposals.map((proposal) => <ProposalCard key={proposal.id} onDecide={(id, decision) => decideMutation.mutate({ id, decision })} proposal={proposal} />)}
            {visibleProposals.length === 0 && <div className="empty-state"><h3>{railTab === 'pending' ? 'Nothing to review' : 'No activity yet'}</h3><p>{railTab === 'pending' ? 'Sensitive and truth-changing writes appear here.' : 'Automatic and decided writes appear here.'}</p></div>}
          </div>
        </aside>
      </div>
    </section>
  )
}
