import '@testing-library/jest-dom/vitest'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { createMemoryRouter, RouterProvider } from 'react-router-dom'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AppShell } from '@/components/layout/app-shell'
import { useControlStatus } from '@/features/control/use-control-status'
import { useDashboardSnapshot } from '@/features/dashboard/use-dashboard-snapshot'

vi.mock('@/features/dashboard/use-dashboard-snapshot', () => ({
  dashboardSnapshotQueryKey: ['dashboard-snapshot'],
  useDashboardSnapshot: vi.fn(),
}))

vi.mock('@/features/control/use-control-status', () => ({
  controlStatusQueryKey: ['control-status'],
  useControlStatus: vi.fn(),
}))

afterEach(() => {
  vi.clearAllMocks()
})

function renderShell() {
  vi.mocked(useDashboardSnapshot).mockReturnValue({
    data: {
      activity: {
        recentJobs: [],
        recentThreads: [],
      },
      catalog: {
        counts: {
          agent: 1,
          automation: 0,
          mcp: 0,
          plugin: 0,
          prompt: 0,
          routine: 0,
          skill: 0,
          workflow: 0,
        },
        items: [],
        totalItems: 1,
      },
      generatedAt: Date.now(),
      runtime: {
        codexHome: '~/.codex',
        platform: 'darwin arm64',
      },
      sources: [],
      usage: {
        activeThreads: 0,
        distinctWorkspaces: 0,
        logEntries24h: 0,
        topWorkspaces: [],
        totalTokens: 0,
        trackedThreads: 0,
        trend: [],
      },
    },
    error: null,
    isFetching: false,
    isLoading: false,
  } as ReturnType<typeof useDashboardSnapshot>)

  vi.mocked(useControlStatus).mockReturnValue({
    data: {
      auditChainOk: true,
      pendingApprovals: 0,
      pendingMemoryProposals: 0,
      runningTasks: 0,
      spentTodayUsd: 0,
    },
  } as ReturnType<typeof useControlStatus>)

  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  })

  const router = createMemoryRouter(
    [
      {
        path: '/',
        element: <AppShell />,
        children: [
          {
            path: '/catalog',
            element: <div>Catalog body</div>,
          },
        ],
      },
    ],
    {
      initialEntries: ['/catalog'],
    },
  )

  return render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  )
}

describe('AppShell', () => {
  it('does not render the data sources column on catalog', () => {
    renderShell()

    expect(screen.queryByText('Data sources')).not.toBeInTheDocument()
    expect(screen.getByText('Catalog body')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Refresh' })).toBeInTheDocument()
  })
})
