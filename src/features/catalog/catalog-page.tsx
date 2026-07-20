import {
  ArrowUpRight,
  Bot,
  Blocks,
  Cable,
  ChevronLeft,
  ChevronRight,
  FileText,
  GitBranchPlus,
  Hammer,
  Play,
  Search,
  Wand2,
  X,
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { InfoTooltip } from '@/components/ui/info-tooltip'
import { SectionEmptyState } from '@/components/ui/section-empty-state'
import { StatusBadge } from '@/components/ui/status-badge'
import { useDashboardSnapshot } from '@/features/dashboard/use-dashboard-snapshot'
import type { CatalogItem, CatalogKind } from '@/features/dashboard/schema'
import {
  formatCompactNumber,
  formatDateTime,
  formatRelativeTime,
} from '@/lib/format'
import { useWorkbenchStore } from '@/store/workbench'

const catalogOrder = [
  'skill',
  'plugin',
  'agent',
  'routine',
  'workflow',
  'prompt',
  'mcp',
  'automation',
] as const

const kindMeta: Record<
  CatalogKind,
  {
    icon: typeof Wand2
    hint: string
    label: string
    tone: 'accent' | 'neutral' | 'success' | 'warning'
  }
> = {
  agent: {
    hint:
      'Dedicated agent profile for a specialized role or execution context in the local workspace.',
    icon: Bot,
    label: 'Agent',
    tone: 'success',
  },
  automation: {
    hint:
      'Local executable script or tool-oriented automation that can support agentic tasks or repeated operations.',
    icon: Hammer,
    label: 'Automation',
    tone: 'warning',
  },
  mcp: {
    hint:
      'Model Context Protocol server definition discovered from local config or project manifests.',
    icon: Cable,
    label: 'MCP',
    tone: 'neutral',
  },
  plugin: {
    hint:
      'Installed extension package that can add custom commands, specialized agents, hooks, or integrations.',
    icon: Blocks,
    label: 'Plugin',
    tone: 'neutral',
  },
  prompt: {
    hint:
      'Prompt template or instruction document that shapes how a local agent or assistant should behave.',
    icon: FileText,
    label: 'Prompt',
    tone: 'accent',
  },
  routine: {
    hint:
      'Repeatable local workflow or script you can trigger for an operational task.',
    icon: Play,
    label: 'Routine',
    tone: 'warning',
  },
  skill: {
    hint:
      'Reusable skill pack that extends the coding agent with specialized knowledge, workflows, or tools.',
    icon: Wand2,
    label: 'Skill',
    tone: 'accent',
  },
  workflow: {
    hint:
      'Structured orchestration flow such as n8n exports, graph pipelines, or multi-step local automation.',
    icon: GitBranchPlus,
    label: 'Workflow',
    tone: 'success',
  },
}

export function CatalogPage() {
  const { data } = useDashboardSnapshot()
  const [page, setPage] = useState(1)
  const [isDetailOpen, setIsDetailOpen] = useState(false)
  const paginationChunkSize = 5
  const {
    catalogFilter,
    catalogSearch,
    selectedCatalogId,
    setCatalogFilter,
    setCatalogSearch,
    setSelectedCatalogId,
  } = useWorkbenchStore()

  const filteredItems = useMemo(() => {
    const items = data?.catalog.items ?? []
    const searchNeedle = catalogSearch.trim().toLowerCase()

    return items.filter((item) => {
      const matchesFilter =
        catalogFilter === 'all' ? true : item.kind === catalogFilter
      const matchesSearch =
        searchNeedle.length === 0
          ? true
          : [
              item.displayName,
              item.origin,
              item.summary ?? '',
              item.group,
              item.provider,
              item.detector,
              item.entrypoint ?? '',
              item.category ?? '',
              item.tags.join(' '),
            ]
              .join(' ')
              .toLowerCase()
              .includes(searchNeedle)

      return matchesFilter && matchesSearch
    })
  }, [catalogFilter, catalogSearch, data?.catalog.items])

  const selectedItem =
    (data?.catalog.items ?? []).find((item) => item.id === selectedCatalogId) ?? null

  const activeCatalog = data?.catalog
  const pageSize = 8
  const pageCount = Math.max(1, Math.ceil(filteredItems.length / pageSize))
  const currentPage = Math.min(page, pageCount)
  const paginatedItems = useMemo(() => {
    const start = (currentPage - 1) * pageSize

    return filteredItems.slice(start, start + pageSize)
  }, [currentPage, filteredItems])
  const pageWindowStart =
    Math.floor((currentPage - 1) / paginationChunkSize) * paginationChunkSize + 1
  const pageWindowEnd = Math.min(pageCount, pageWindowStart + paginationChunkSize - 1)
  const visiblePageNumbers = useMemo(() => {
    return Array.from(
      { length: pageWindowEnd - pageWindowStart + 1 },
      (_, index) => pageWindowStart + index,
    )
  }, [pageWindowEnd, pageWindowStart])

  useEffect(() => {
    if (!isDetailOpen) {
      return
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsDetailOpen(false)
      }
    }

    window.addEventListener('keydown', onKeyDown)

    return () => {
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [isDetailOpen])

  const handleOpenDetails = (itemId: string) => {
    setSelectedCatalogId(itemId)
    setIsDetailOpen(true)
  }

  const renderCatalogIcon = (item: CatalogItem) => {
    const Icon = kindMeta[item.kind].icon

    return (
      <span className={`catalog-kind-icon catalog-kind-icon--${item.kind}`}>
        <Icon aria-hidden="true" size={16} />
      </span>
    )
  }

  return (
    <section className="page-section catalog-page">
      <section className="surface catalog-console">
        <div className="catalog-console-heading">
          <div>
            <p className="eyebrow">Inventory</p>
            <h2>Catalog inventory</h2>
          </div>

          <div className="catalog-console-meta">
            <span className="row-subtle">
              {formatCompactNumber(filteredItems.length)} visible
            </span>
            <StatusBadge
              label={`${formatCompactNumber(activeCatalog?.totalItems ?? 0)} total`}
              tone="neutral"
            />
          </div>
        </div>

        <div className="catalog-toolbar">
          <div className="catalog-kind-strip" aria-label="Catalog composition">
            {catalogOrder.map((kind) => {
              const Icon = kindMeta[kind].icon

              return (
                <button
                  key={kind}
                  aria-pressed={catalogFilter === kind}
                  className={
                    catalogFilter === kind
                      ? 'catalog-kind-pill is-active'
                      : 'catalog-kind-pill'
                  }
                  onClick={() => {
                    setCatalogFilter(catalogFilter === kind ? 'all' : kind)
                    setPage(1)
                  }}
                  type="button"
                >
                  <Icon
                    aria-hidden="true"
                    className={`catalog-kind-pill-glyph catalog-kind-pill-glyph--${kind}`}
                    size={15}
                  />
                  <span className="catalog-kind-pill-label">
                    {kindMeta[kind].label}
                  </span>
                  <strong>{formatCompactNumber(activeCatalog?.counts[kind] ?? 0)}</strong>
                  <InfoTooltip
                    className="catalog-filter-tooltip"
                    content={kindMeta[kind].hint}
                  />
                </button>
              )
            })}
          </div>

          <div className="search-field catalog-search-field">
            <Search aria-hidden="true" size={16} />
            <input
              aria-label="Search catalog"
              onChange={(event) => {
                setCatalogSearch(event.target.value)
                setPage(1)
              }}
              placeholder="Filter by name, group, description"
              type="search"
              value={catalogSearch}
            />
          </div>
        </div>

        <div className="catalog-table-shell">
          <div className="catalog-head-row" role="row">
            <span>#</span>
            <span>Name</span>
            <span>Type</span>
            <span>Description</span>
            <span>Group</span>
            <span>Updated</span>
            <span className="catalog-arrow-cell" aria-hidden="true" />
          </div>

          {filteredItems.length === 0 ? (
            <SectionEmptyState
              body="Tighten or relax the filter once local sources are available."
              title="No inventory entries"
            />
          ) : (
            <div className="catalog-list" role="table" aria-label="Catalog inventory">
              {paginatedItems.map((item, index) => (
                <div
                  key={item.id}
                  className={
                    isDetailOpen && selectedItem?.id === item.id
                      ? 'catalog-entry is-selected'
                      : 'catalog-entry'
                  }
                  role="row"
                >
                  <span className="catalog-index-cell">
                    {(currentPage - 1) * pageSize + index + 1}
                  </span>

                  <span className="catalog-name-cell">
                    {renderCatalogIcon(item)}
                    <span className="catalog-name-copy">
                      <strong>{item.displayName}</strong>
                      <small>{item.origin}</small>
                    </span>
                  </span>

                  <span className="catalog-type-cell">
                    <StatusBadge
                      label={kindMeta[item.kind].label}
                      tone={kindMeta[item.kind].tone}
                    />
                  </span>

                  <span className="catalog-description-cell">
                    {item.summary ?? 'No embedded summary available.'}
                  </span>

                  <span className="catalog-group-cell">{item.group}</span>

                  <span className="catalog-updated-cell">
                    {item.updatedAt ? formatRelativeTime(item.updatedAt) : 'n/a'}
                  </span>

                  <span className="catalog-arrow-cell">
                    <button
                      aria-label={`Open details for ${item.displayName}`}
                      className="catalog-action-button"
                      onClick={() => handleOpenDetails(item.id)}
                      type="button"
                    >
                      <ArrowUpRight aria-hidden="true" size={16} />
                    </button>
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="catalog-table-footer">
          <span className="row-subtle">
            {formatCompactNumber(filteredItems.length)} of{' '}
            {formatCompactNumber(activeCatalog?.totalItems ?? 0)} local items in view
          </span>

          <div className="catalog-pagination" aria-label="Catalog pagination">
            <button
              aria-label="Previous page"
              className="catalog-pagination-button"
              disabled={currentPage === 1}
              onClick={() => setPage((current) => Math.max(1, current - 1))}
              type="button"
            >
              <ChevronLeft aria-hidden="true" size={14} />
            </button>

            {pageWindowStart > 1 ? (
              <span className="catalog-pagination-gap" aria-hidden="true">
                ...
              </span>
            ) : null}

            {visiblePageNumbers.map((pageNumber) => (
              <button
                key={pageNumber}
                aria-current={pageNumber === currentPage ? 'page' : undefined}
                className={
                  pageNumber === currentPage
                    ? 'catalog-pagination-button is-active'
                    : 'catalog-pagination-button'
                }
                onClick={() => setPage(pageNumber)}
                type="button"
              >
                {pageNumber}
              </button>
            ))}

            {pageWindowEnd < pageCount ? (
              <span className="catalog-pagination-gap" aria-hidden="true">
                ...
              </span>
            ) : null}

            <button
              aria-label="Next page"
              className="catalog-pagination-button"
              disabled={currentPage === pageCount}
              onClick={() => setPage((current) => Math.min(pageCount, current + 1))}
              type="button"
            >
              <ChevronRight aria-hidden="true" size={14} />
            </button>
          </div>
        </div>
      </section>

      {isDetailOpen && selectedItem ? (
        <div
          aria-hidden="true"
          className="catalog-modal-backdrop"
          onClick={() => setIsDetailOpen(false)}
        >
          <section
            aria-labelledby="catalog-detail-title"
            aria-modal="true"
            className="surface catalog-modal"
            onClick={(event) => event.stopPropagation()}
            role="dialog"
          >
            <div className="panel-heading">
              <div className="catalog-modal-title">
                <div className="catalog-name-cell">
                  {renderCatalogIcon(selectedItem)}
                  <div>
                    <p className="eyebrow">Details</p>
                    <h2 id="catalog-detail-title">{selectedItem.displayName}</h2>
                  </div>
                </div>

                <StatusBadge
                  label={kindMeta[selectedItem.kind].label}
                  tone={kindMeta[selectedItem.kind].tone}
                />
              </div>

              <button
                aria-label="Close details"
                className="catalog-action-button"
                onClick={() => setIsDetailOpen(false)}
                type="button"
              >
                <X aria-hidden="true" size={16} />
              </button>
            </div>

            <p className="body-copy">
              {selectedItem.summary ?? 'No embedded summary was available for this item.'}
            </p>

            <dl className="detail-list catalog-detail-grid">
              <div>
                <dt>Provider</dt>
                <dd>{selectedItem.provider}</dd>
              </div>
              <div>
                <dt>Detector</dt>
                <dd>{selectedItem.detector}</dd>
              </div>
              <div>
                <dt>Origin</dt>
                <dd>{selectedItem.origin}</dd>
              </div>
              <div>
                <dt>Path</dt>
                <dd>{selectedItem.path}</dd>
              </div>
              <div>
                <dt>Entrypoint</dt>
                <dd>{selectedItem.entrypoint ?? 'n/a'}</dd>
              </div>
              <div>
                <dt>Group</dt>
                <dd>{selectedItem.group}</dd>
              </div>
              <div>
                <dt>Category</dt>
                <dd>{selectedItem.category ?? 'Uncategorized'}</dd>
              </div>
              <div>
                <dt>Version</dt>
                <dd>{selectedItem.version ?? 'n/a'}</dd>
              </div>
              <div>
                <dt>Updated</dt>
                <dd>
                  {selectedItem.updatedAt
                    ? formatDateTime(selectedItem.updatedAt)
                    : 'n/a'}
                </dd>
              </div>
              <div>
                <dt>Confidence</dt>
                <dd>{`${Math.round(selectedItem.confidence * 100)}%`}</dd>
              </div>
            </dl>

            <div className="tag-row">
              {selectedItem.tags.length > 0 ? (
                selectedItem.tags.map((tag) => (
                  <span key={tag} className="tag-chip">
                    {tag}
                  </span>
                ))
              ) : (
                <span className="row-subtle">No tags</span>
              )}
            </div>
          </section>
        </div>
      ) : null}
    </section>
  )
}
