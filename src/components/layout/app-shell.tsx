import { useMutation, useQueryClient } from '@tanstack/react-query'
import {
  Activity,
  BrainCircuit,
  Blocks,
  FolderOpen,
  RefreshCw,
  ServerCog,
} from 'lucide-react'
import { useEffect, useState } from 'react'
import { flushSync } from 'react-dom'
import { NavLink, Outlet, useLocation } from 'react-router-dom'
import { MetricCard } from '@/components/ui/metric-card'
import { StatusBadge } from '@/components/ui/status-badge'
import { refreshDashboardSnapshot } from '@/features/dashboard/api'
import {
  dashboardSnapshotQueryKey,
  useDashboardSnapshot,
} from '@/features/dashboard/use-dashboard-snapshot'
import { formatCompactNumber, formatRelativeTime } from '@/lib/format'

const navigation = [
  {
    icon: FolderOpen,
    label: 'Catalog',
    summary: 'Agents, skills, MCP, workflows',
    to: '/catalog',
  },
  {
    icon: Blocks,
    label: 'Runner',
    summary: 'Prompt and routine staging',
    to: '/runner',
  },
  {
    icon: Activity,
    label: 'Usage',
    summary: 'Tokens, workspaces, threads',
    to: '/usage',
  },
  {
    icon: BrainCircuit,
    label: 'Memory',
    summary: 'Persistent context and notes',
    to: '/memory',
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
  const queryClient = useQueryClient()
  const { data, error, isFetching, isLoading } = useDashboardSnapshot()
  const showsDataSources = location.pathname === '/catalog'
  const [isManualRefreshActive, setIsManualRefreshActive] = useState(false)
  const [refreshToast, setRefreshToast] = useState<RefreshToast | null>(null)
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

  const metrics = data
    ? [
        {
          hint:
            'Total number of local artifacts discovered by the native scanner, including agents, skills, plugins, prompts, MCP servers, workflows, routines, and automations.',
          label: 'Catalog items',
          value: formatCompactNumber(data.catalog.totalItems),
          tone: 'neutral' as const,
        },
        {
          hint:
            'Conversation threads found in the local state store and available for usage summaries.',
          label: 'Tracked threads',
          value: formatCompactNumber(data.usage.trackedThreads),
          tone: 'accent' as const,
        },
        {
          hint:
            'Combined token usage aggregated across the tracked local threads in this workspace view.',
          label: 'Total tokens',
          value: formatCompactNumber(data.usage.totalTokens),
          tone: 'success' as const,
        },
        {
          hint:
            'Activity or log entries recorded during the last 24 hours across the connected local sources.',
          label: 'Logs / 24h',
          tooltipAlign: 'end' as const,
          value: formatCompactNumber(data.usage.logEntries24h),
          tone: 'warning' as const,
        },
      ]
    : []

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-block">
          <div className="brand-mark">
            <ServerCog aria-hidden="true" size={20} />
          </div>
          <div>
            <p className="eyebrow">Local Control Plane</p>
            <h1>Agent Control</h1>
          </div>
        </div>

        <nav className="sidebar-nav" aria-label="Primary navigation">
          {navigation.map((item) => {
            const Icon = item.icon

            return (
              <NavLink
                key={item.to}
                className={({ isActive }) =>
                  isActive ? 'nav-link is-active' : 'nav-link'
                }
                to={item.to}
              >
                <Icon aria-hidden="true" size={18} />
                <span className="nav-copy">
                  <span className="nav-label">{item.label}</span>
                  <span className="nav-summary">{item.summary}</span>
                </span>
              </NavLink>
            )
          })}
        </nav>

        <section className="sidebar-section">
          <div className="panel-heading">
            <h2>Runtime</h2>
          </div>
          <dl className="meta-list">
            <div>
              <dt>Platform</dt>
              <dd>{data?.runtime.platform ?? 'Loading'}</dd>
            </div>
            <div>
              <dt>Home</dt>
              <dd>{data?.runtime.codexHome ?? 'Unavailable'}</dd>
            </div>
            <div>
              <dt>Last scan</dt>
              <dd>
                {data ? formatRelativeTime(data.generatedAt) : 'Waiting for scan'}
              </dd>
            </div>
            <div>
              <dt>View</dt>
              <dd>{location.pathname.replace('/', '') || 'catalog'}</dd>
            </div>
          </dl>
        </section>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="topbar-copy">
            <p className="eyebrow">Desktop inventory</p>
            <h2>Agents, skills, MCP, workflows, and telemetry in one place</h2>
          </div>

          <div className="topbar-actions">
            {isLoading ? (
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
          </div>
        </header>

        <section className="metric-strip" aria-label="Runtime metrics">
          {metrics.map((metric) => (
            <MetricCard
              key={metric.label}
              hint={metric.hint}
              label={metric.label}
              tone={metric.tone}
              tooltipAlign={metric.tooltipAlign}
              value={metric.value}
            />
          ))}
        </section>

        {error ? (
          <section className="alert-banner" role="alert">
            <strong>Native data sources did not load cleanly.</strong>
            <span>{error instanceof Error ? error.message : 'Unknown error'}</span>
          </section>
        ) : null}

        <div className={showsDataSources ? 'main-grid' : 'main-grid main-grid--single'}>
          <main className="page-content">
            <Outlet />
          </main>

          {showsDataSources ? (
            <aside className="inspector">
              <section className="surface">
                <div className="panel-heading">
                  <h2>Data sources</h2>
                </div>

                <ul className="source-list">
                  {data?.sources.map((source) => (
                    <li key={source.id} className="source-row">
                      <div>
                        <p className="row-title">{source.label}</p>
                        <p className="row-subtle">{source.path}</p>
                      </div>
                      <StatusBadge
                        label={source.status}
                        tone={source.status === 'available' ? 'success' : 'warning'}
                      />
                    </li>
                  )) ?? <li className="row-subtle">Waiting for source inventory.</li>}
                </ul>
              </section>
            </aside>
          ) : null}
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
