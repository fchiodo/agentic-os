import { createHashRouter, Navigate } from 'react-router-dom'
import { AppShell } from '@/components/layout/app-shell'
import { CatalogPage } from '@/features/catalog/catalog-page'
import { DocumentConverterPage } from '@/features/document-converter/document-converter-page'
import { MemoryPage } from '@/features/memory/memory-page'

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
        path: '/memory',
        element: <MemoryPage />,
      },
      {
        path: '/document-converter',
        element: <DocumentConverterPage />,
      },
      {
        path: '*',
        element: <Navigate replace to="/catalog" />,
      },
    ],
  },
])
