import { createHashRouter, Navigate } from 'react-router-dom'
import { AppShell } from '@/components/layout/app-shell'
import { ApprovalsPage } from '@/features/approvals/approvals-page'
import { AuditPage } from '@/features/audit/audit-page'
import { CatalogPage } from '@/features/catalog/catalog-page'
import { MemoryPage } from '@/features/memory/memory-page'
import { RunnerPage } from '@/features/runner/runner-page'
import { UsagePage } from '@/features/usage/usage-page'

export const router = createHashRouter([
  {
    path: '/',
    element: <AppShell />,
    children: [
      {
        index: true,
        element: <Navigate replace to="/catalog" />,
      },
      {
        path: '/catalog',
        element: <CatalogPage />,
      },
      {
        path: '/runner',
        element: <RunnerPage />,
      },
      {
        path: '/approvals',
        element: <ApprovalsPage />,
      },
      {
        path: '/memory',
        element: <MemoryPage />,
      },
      {
        path: '/usage',
        element: <UsagePage />,
      },
      {
        path: '/audit',
        element: <AuditPage />,
      },
    ],
  },
])
