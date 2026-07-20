import {
  Brain,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Clock,
  FileText,
  FolderOpen,
  Search,
  Shield,
  Tag,
  Trash2,
  X,
} from 'lucide-react'
import { useCallback, useMemo, useState } from 'react'
import { StatusBadge } from '@/components/ui/status-badge'
import {
  useMemoryConfirm,
  useMemoryProposals,
  useMemoryProposalsDecide,
  useMemoryRead,
  useMemoryReindex,
  useMemorySearch,
  useMemoryTree,
} from '@/features/memory/hooks'
import type { VaultNode } from '@/features/memory/schema'
import { formatCompactNumber, formatRelativeTime } from '@/lib/format'

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
}

const DOMAIN_LABELS: Record<string, string> = {
  work: 'Work',
  planphysique: 'PlanPhysique',
  personal: 'Personal',
  family: 'Family',
  finance: 'Finance',
  research: 'Research',
}

// ---------------------------------------------------------------------------
// Vault tree node
// ---------------------------------------------------------------------------

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
          onClick={() => setExpanded((v) => !v)}
          style={{ paddingLeft: `${12 + depth * 16}px` }}
          type="button"
        >
          {expanded ? (
            <ChevronDown aria-hidden="true" size={14} />
          ) : (
            <ChevronRight aria-hidden="true" size={14} />
          )}
          {expanded ? (
            <FolderOpen aria-hidden="true" size={14} className="tree-icon--open" />
          ) : (
            <FolderOpen aria-hidden="true" size={14} />
          )}
          <span className="tree-label">{node.name}</span>
        </button>
        {expanded && node.children.length > 0 && (
          <div className="tree-children">
            {node.children.map((child) => (
              <TreeNode
                key={child.path}
                node={child}
                depth={depth + 1}
                onSelect={onSelect}
                selectedPath={selectedPath}
              />
            ))}
          </div>
        )}
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

// ---------------------------------------------------------------------------
// Search result card
// ---------------------------------------------------------------------------

function SearchResult({
  item,
  onSelect,
}: {
  item: { row: { id: string; title: string; memType: string; domain: string; vaultPath: string; status: string; summary?: string | null }; score: number; relevance: number; recency: number; trust: number }
  onSelect: (path: string) => void
}) {
  const Icon = TYPE_ICONS[item.row.memType] ?? FileText
  const scorePct = Math.round(item.score * 100)

  return (
    <button
      className="memory-search-result"
      onClick={() => onSelect(item.row.vaultPath)}
      type="button"
    >
      <div className="memory-search-result-head">
        <Icon aria-hidden="true" size={16} />
        <span className="memory-search-result-title">{item.row.title}</span>
        <StatusBadge label={item.row.memType} tone="accent" />
        <StatusBadge label={item.row.status} tone={STATUS_TONE[item.row.status] ?? 'neutral'} />
        <span className="memory-search-score">{scorePct}%</span>
      </div>
      {item.row.summary && (
        <p className="memory-search-result-summary">{item.row.summary}</p>
      )}
      <div className="memory-search-result-meta">
        <span>{item.row.domain}</span>
        <span className="memory-search-score-detail">
          rel {Math.round(item.relevance * 100)}% · rec {Math.round(item.recency * 100)}% · trust{' '}
          {Math.round(item.trust * 100)}%
        </span>
      </div>
    </button>
  )
}

// ---------------------------------------------------------------------------
// Memory reader
// ---------------------------------------------------------------------------

function MemoryReader({
  path,
  onClose,
}: {
  path: string
  onClose: () => void
}) {
  const { data, isLoading } = useMemoryRead(path)
  const confirmMutation = useMemoryConfirm()

  if (isLoading) {
    return (
      <div className="memory-reader-empty">
        <p>Loading…</p>
      </div>
    )
  }

  if (!data) {
    return (
      <div className="memory-reader-empty">
        <p>Could not load file.</p>
      </div>
    )
  }

  const fm = data.frontmatter

  return (
    <div className="memory-reader">
      <div className="memory-reader-head">
        <div className="memory-reader-head-left">
          <FileText aria-hidden="true" size={18} />
          <div>
            <p className="eyebrow">{fm ? DOMAIN_LABELS[fm.domain] ?? fm.domain : 'Unknown'}</p>
            <h2 className="memory-reader-title">{fm?.title ?? path}</h2>
          </div>
        </div>
        <div className="memory-reader-head-right">
          {fm && (
            <>
              <StatusBadge label={fm.memType} tone="accent" />
              <StatusBadge
                label={data.status}
                tone={STATUS_TONE[data.status] ?? 'neutral'}
              />
              {fm.sensitivity === 'sensitive' && (
                <StatusBadge label="sensitive" tone="warning" />
              )}
            </>
          )}
          <button className="icon-button" onClick={onClose} type="button" aria-label="Close reader">
            <X aria-hidden="true" size={16} />
          </button>
        </div>
      </div>

      {fm && (
        <dl className="memory-reader-meta">
          <div>
            <dt>Confidence</dt>
            <dd>{Math.round(fm.confidence * 100)}%</dd>
          </div>
          <div>
            <dt>Confirmations</dt>
            <dd>{fm.confirmations ?? 0}</dd>
          </div>
          <div>
            <dt>Provenance</dt>
            <dd>{fm.provenance.source}</dd>
          </div>
          <div>
            <dt>Created</dt>
            <dd>{new Date(fm.created).toLocaleDateString()}</dd>
          </div>
          {fm.staleAfterDays && (
            <div>
              <dt>Stale after</dt>
              <dd>{fm.staleAfterDays}d</dd>
            </div>
          )}
          {fm.expires && (
            <div>
              <dt>Expires</dt>
              <dd>{fm.expires}</dd>
            </div>
          )}
          {data.gitLastCommit && (
            <div>
              <dt>Git</dt>
              <dd className="mono">{data.gitLastCommit}</dd>
            </div>
          )}
        </dl>
      )}

      {fm && fm.tags.length > 0 && (
        <div className="tag-row">
          {fm.tags.map((tag) => (
            <span key={tag} className="tag-chip">
              {tag}
            </span>
          ))}
        </div>
      )}

      <div className="memory-reader-body">
        <div className="markdown-body">{data.markdown}</div>
      </div>

      {fm && data.status === 'stale' && (
        <div className="memory-reader-actions">
          <button
            className="primary-button"
            onClick={() => confirmMutation.mutate(fm.id)}
            disabled={confirmMutation.isPending}
            type="button"
          >
            <CheckCircle2 aria-hidden="true" size={16} />
            {confirmMutation.isPending ? 'Confirming…' : 'Confirm still true'}
          </button>
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Proposal card
// ---------------------------------------------------------------------------

function ProposalCard({
  proposal,
  onDecide,
}: {
  proposal: {
    id: string
    vaultPath: string
    domain: string
    op: string
    sensitivity: string
    requiresApproval: boolean
    status: string
    createdAt: string
    gateReport: string
  }
  onDecide: (id: string, decision: string) => void
}) {
  const [expanded, setExpanded] = useState(false)
  const opTone: Record<string, 'accent' | 'success' | 'warning'> = {
    create: 'success',
    update: 'accent',
    supersede: 'warning',
  }

  return (
    <div className="proposal-card">
      <div className="proposal-card-head">
        <StatusBadge label={proposal.op} tone={opTone[proposal.op] ?? 'neutral'} />
        <span className="proposal-card-path">{proposal.vaultPath}</span>
        <StatusBadge label={proposal.status} tone={STATUS_TONE[proposal.status] ?? 'neutral'} />
      </div>
      <div className="proposal-card-meta">
        <span>{proposal.domain}</span>
        <span>{formatRelativeTime(new Date(proposal.createdAt).getTime())}</span>
        {proposal.requiresApproval && (
          <StatusBadge label="needs approval" tone="warning" />
        )}
      </div>
      <button
        className="proposal-toggle"
        onClick={() => setExpanded((v) => !v)}
        type="button"
      >
        {expanded ? 'Hide' : 'Show'} gate report
      </button>
      {expanded && (
        <pre className="proposal-gate-report">{proposal.gateReport}</pre>
      )}
      {proposal.status === 'pending' && proposal.requiresApproval && (
        <div className="proposal-actions">
          <button
            className="primary-button"
            onClick={() => onDecide(proposal.id, 'approve')}
            type="button"
          >
            <CheckCircle2 aria-hidden="true" size={14} />
            Approve
          </button>
          <button
            className="icon-button"
            onClick={() => onDecide(proposal.id, 'discard')}
            type="button"
          >
            <Trash2 aria-hidden="true" size={14} />
            Discard
          </button>
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export function MemoryPage() {
  const [searchQuery, setSearchQuery] = useState('')
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [showProposals, setShowProposals] = useState(false)
  const [domainFilter, setDomainFilter] = useState<string | undefined>()

  const treeQuery = useMemoryTree(domainFilter)
  const searchQuery_ = useMemorySearch(searchQuery, domainFilter)
  const proposalsQuery = useMemoryProposals()
  const decideMutation = useMemoryProposalsDecide()
  const reindexMutation = useMemoryReindex()

  const pendingProposals = useMemo(
    () => proposalsQuery.data?.filter((p) => p.status === 'pending') ?? [],
    [proposalsQuery.data],
  )

  const handleSelect = useCallback((path: string) => {
    setSelectedPath(path)
    setSearchQuery('')
  }, [])

  const handleDecide = useCallback(
    (id: string, decision: string) => {
      decideMutation.mutate({ id, decision })
    },
    [decideMutation],
  )

  const domains = ['work', 'planphysique', 'personal', 'family', 'finance', 'research']

  return (
    <section className="page-section memory-page">
      <div className="memory-layout">
        {/* ---- Left: tree ---- */}
        <aside className="memory-sidebar surface">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Vault</p>
              <h2>Memory tree</h2>
            </div>
            <button
              className="icon-button"
              onClick={() => reindexMutation.mutate()}
              disabled={reindexMutation.isPending}
              type="button"
              aria-label="Reindex vault"
            >
              <Brain aria-hidden="true" size={16} />
            </button>
          </div>

          <div className="memory-domain-strip">
            <button
              className={`segment-button ${!domainFilter ? 'is-active' : ''}`}
              onClick={() => setDomainFilter(undefined)}
              type="button"
            >
              All
            </button>
            {domains.map((d) => (
              <button
                key={d}
                className={`segment-button ${domainFilter === d ? 'is-active' : ''}`}
                onClick={() => setDomainFilter(d)}
                type="button"
              >
                {DOMAIN_LABELS[d] ?? d}
              </button>
            ))}
          </div>

          <div className="memory-tree">
            {treeQuery.data?.map((node) => (
              <TreeNode
                key={node.path}
                node={node}
                depth={0}
                onSelect={handleSelect}
                selectedPath={selectedPath}
              />
            ))}
            {treeQuery.data && treeQuery.data.length === 0 && (
              <div className="empty-state">
                <h3>Empty vault</h3>
                <p>No memory files yet. Save something to get started.</p>
              </div>
            )}
          </div>

          <div className="memory-sidebar-footer">
            <span className="row-subtle">
              {formatCompactNumber(proposalsQuery.data?.length ?? 0)} proposals
            </span>
            {pendingProposals.length > 0 && (
              <StatusBadge label={`${pendingProposals.length} pending`} tone="warning" />
            )}
          </div>
        </aside>

        {/* ---- Center: search + reader ---- */}
        <main className="memory-main">
          {!selectedPath ? (
            <>
              <div className="surface memory-search-bar">
                <Search aria-hidden="true" size={18} />
                <input
                  aria-label="Search memory"
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder="Search the vault…"
                  type="search"
                  value={searchQuery}
                />
                <div className="memory-search-toggle">
                  <label className="memory-toggle-label">
                    <input
                      type="checkbox"
                      checked={searchQuery_.data !== undefined}
                      readOnly
                    />
                    Include stale
                  </label>
                </div>
              </div>

              <div className="memory-search-results">
                {searchQuery.length >= 2 && searchQuery_.data && searchQuery_.data.length > 0 ? (
                  searchQuery_.data.map((item) => (
                    <SearchResult
                      key={item.row.id}
                      item={item}
                      onSelect={handleSelect}
                    />
                  ))
                ) : searchQuery.length >= 2 && searchQuery_.data?.length === 0 ? (
                  <div className="empty-state">
                    <h3>No results</h3>
                    <p>Try different keywords or broaden your search.</p>
                  </div>
                ) : (
                  <div className="memory-welcome">
                    <Brain aria-hidden="true" size={48} className="memory-welcome-icon" />
                    <h2>Second Brain</h2>
                    <p>
                      Your personal knowledge vault. Search across domains, browse the tree, or
                      save new memories manually.
                    </p>
                  </div>
                )}
              </div>
            </>
          ) : (
            <MemoryReader path={selectedPath} onClose={() => setSelectedPath(null)} />
          )}
        </main>

        {/* ---- Right: proposals rail ---- */}
        <aside className={`memory-proposals-rail surface ${showProposals ? 'is-open' : ''}`}>
          <button
            className="memory-proposals-toggle"
            onClick={() => setShowProposals((v) => !v)}
            type="button"
          >
            <Shield aria-hidden="true" size={16} />
            <span>Proposals</span>
            {pendingProposals.length > 0 && (
              <span className="memory-proposals-count">{pendingProposals.length}</span>
            )}
          </button>

          {showProposals && (
            <div className="memory-proposals-list">
              {proposalsQuery.data && proposalsQuery.data.length > 0 ? (
                proposalsQuery.data.map((p) => (
                  <ProposalCard key={p.id} proposal={p} onDecide={handleDecide} />
                ))
              ) : (
                <div className="empty-state">
                  <h3>No proposals</h3>
                  <p>Memory write proposals will appear here.</p>
                </div>
              )}
            </div>
          )}
        </aside>
      </div>
    </section>
  )
}
