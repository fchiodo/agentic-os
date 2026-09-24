import { useMutation, useQueryClient } from '@tanstack/react-query'
import {
  BrainCircuit,
  FolderOpen,
  FileScan,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  ServerCog,
} from 'lucide-react'
import { useEffect, useState } from 'react'
import { flushSync } from 'react-dom'
import { NavLink, Outlet, useLocation } from 'react-router-dom'
import { NavBadge } from '@/components/ui/nav-badge'
import { StatusBadge } from '@/components/ui/status-badge'
import { refreshDashboardSnapshot } from '@/features/dashboard/api'
import {
  dashboardSnapshotQueryKey,
  useDashboardSnapshot,
} from '@/features/dashboard/use-dashboard-snapshot'
import { useControlStatus } from '@/features/control/use-control-status'
import { DocumentConverterRuntimeBridge } from '@/features/document-converter/runtime-bridge'
import { formatCompactNumber } from '@/lib/format'

const navigation = [
  {
    icon: FolderOpen,
    label: 'Catalog',
    summary: 'Agents, skills, MCP, workflows',
    to: '/catalog',
  },
  {
    badgeKey: 'pendingMemoryProposals' as const,
    icon: BrainCircuit,
    label: 'Memory',
    summary: 'Persistent context and notes',
    to: '/memory',
  },
  {
    icon: FileScan,
    label: 'Document Converter',
    summary: 'Local PDF and image OCR',
    to: '/document-converter',
  },
] as const

type RefreshToast = {
  description: string
  id: number
  title: string
  tone: 'danger' | 'success'
}

const minimumManualRefreshFeedbackMs = 900

function waitForNextPaint() {
  return new Promise<void>((resolve) => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        resolve()
      })
    })
  })
}

export function AppShell() {
  const location = useLocation()
  const isDocumentConverter = location.pathname.startsWith('/document-converter')
  const queryClient = useQueryClient()
  const { error, isFetching, isLoading } = useDashboardSnapshot()
  const { data: controlStatus } = useControlStatus()
  const [isManualRefreshActive, setIsManualRefreshActive] = useState(false)
  const [refreshToast, setRefreshToast] = useState<RefreshToast | null>(null)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const refreshMutation = useMutation({
    mutationFn: refreshDashboardSnapshot,
    onSuccess: (snapshot) => {
      queryClient.setQueryData(dashboardSnapshotQueryKey, snapshot)
      setRefreshToast({
        description: `${formatCompactNumber(
          snapshot.catalog.totalItems,
        )} local items synced from the latest scan.`,
        id: Date.now(),
        title: 'Catalog refreshed',
        tone: 'success',
      })
    },
    onError: (mutationError) => {
      setRefreshToast({
        description:
          mutationError instanceof Error
            ? mutationError.message
            : 'The local inventory could not be refreshed.',
        id: Date.now(),
        title: 'Refresh failed',
        tone: 'danger',
      })
    },
  })
  const isRefreshing =
    isFetching || refreshMutation.isPending || isManualRefreshActive

  useEffect(() => {
    if (!refreshToast) {
      return
    }

    const timeoutId = window.setTimeout(() => {
      setRefreshToast((currentToast) =>
        currentToast?.id === refreshToast.id ? null : currentToast,
      )
    }, 4200)

    return () => {
      window.clearTimeout(timeoutId)
    }
  }, [refreshToast])

  const handleRefreshClick = async () => {
    if (isRefreshing) {
      return
    }

    flushSync(() => {
      setIsManualRefreshActive(true)
    })

    await waitForNextPaint()

    const startedAt = Date.now()

    try {
      await refreshMutation.mutateAsync()
    } catch {
      // Toast state is handled by the mutation callbacks.
    } finally {
      const elapsed = Date.now() - startedAt
      const remainingDelay = Math.max(
        0,
        minimumManualRefreshFeedbackMs - elapsed,
      )

      if (remainingDelay > 0) {
        await new Promise((resolve) => {
          window.setTimeout(resolve, remainingDelay)
        })
      }

      setIsManualRefreshActive(false)
    }
  }

  return (
    <div
      className="app-shell"
      style={{ gridTemplateColumns: `${sidebarCollapsed ? '76px' : '280px'} minmax(0, 1fr)` }}
    >
      <DocumentConverterRuntimeBridge dropEnabled={isDocumentConverter} />
      <aside className={`sidebar ${sidebarCollapsed ? 'is-collapsed' : ''}`}>
        <div className="brand-block">
          <div className="brand-mark">
            <ServerCog aria-hidden="true" size={20} />
          </div>
          {!sidebarCollapsed && (
            <div>
              <p className="eyebrow">Local Control Plane</p>
              <h1>Agentic OS</h1>
            </div>
          )}
          <button
            aria-label={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            className="sidebar-toggle"
            onClick={() => setSidebarCollapsed((value) => !value)}
            title={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            type="button"
          >
            {sidebarCollapsed ? <PanelLeftOpen aria-hidden="true" size={16} /> : <PanelLeftClose aria-hidden="true" size={16} />}
          </button>
        </div>

        <nav className="sidebar-nav" aria-label="Primary navigation">
          {navigation.map((item) => {
            const Icon = item.icon
            const badgeCount =
              'badgeKey' in item && controlStatus ? controlStatus[item.badgeKey] : 0

            return (
              <NavLink
                key={item.to}
                className={({ isActive }) =>
                  isActive ? 'nav-link is-active' : 'nav-link'
                }
                title={sidebarCollapsed ? item.label : undefined}
                to={item.to}
              >
                <Icon aria-hidden="true" size={18} />
                {!sidebarCollapsed && (
                  <span className="nav-copy">
                    <span className="nav-label">{item.label}</span>
                    <span className="nav-summary">{item.summary}</span>
                  </span>
                )}
                <NavBadge count={badgeCount} />
              </NavLink>
            )
          })}
        </nav>

      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="topbar-copy">
            <p className="eyebrow">
              {isDocumentConverter ? 'Local document workspace' : 'Desktop inventory'}
            </p>
            <h2>
              {isDocumentConverter
                ? 'Private conversion, model management, and document history'
                : 'Agents, skills, MCP, workflows, and memory in one place'}
            </h2>
          </div>

          <div className="topbar-actions">
            {isDocumentConverter ? (
              <StatusBadge label="Local only" tone="success" />
            ) : isLoading ? (
              <StatusBadge label="Scanning" tone="neutral" />
            ) : error ? (
              <StatusBadge label="Source issue" tone="danger" />
            ) : refreshMutation.isPending ? (
              <StatusBadge label="Scanning" tone="accent" />
            ) : isFetching ? (
              <StatusBadge label="Refreshing" tone="accent" />
            ) : (
              <StatusBadge label="Healthy" tone="success" />
            )}

            {!isDocumentConverter && (
              <button
                className="icon-button"
                disabled={isRefreshing}
                onClick={() => {
                  void handleRefreshClick()
                }}
                type="button"
              >
                <span
                  className={
                    isManualRefreshActive
                      ? 'refresh-button-icon is-spinning'
                      : 'refresh-button-icon'
                  }
                >
                  <RefreshCw
                    aria-hidden="true"
                    className="refresh-button-glyph"
                    size={16}
                  />
                </span>
                <span>Refresh</span>
              </button>
            )}
          </div>
        </header>

        {error ? (
          <section className="alert-banner" role="alert">
            <strong>Native data sources did not load cleanly.</strong>
            <span>{error instanceof Error ? error.message : 'Unknown error'}</span>
          </section>
        ) : null}

        <div className="main-grid main-grid--single">
          <main className="page-content">
            <Outlet />
          </main>
        </div>
      </div>

      {refreshToast ? (
        <div className="toast-stack" aria-atomic="true" aria-live="polite">
          <section
            className={`toast-message toast-message--${refreshToast.tone}`}
            role={refreshToast.tone === 'danger' ? 'alert' : 'status'}
          >
            <strong className="toast-title">{refreshToast.title}</strong>
            <p className="toast-description">{refreshToast.description}</p>
          </section>
        </div>
      ) : null}
    </div>
  )
}
