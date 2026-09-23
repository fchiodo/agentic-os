import '@testing-library/jest-dom/vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { describe, expect, it, vi } from 'vitest'

const auditState = vi.hoisted(() => ({
  traceRunId: null as string | null,
}))

vi.mock('@/features/audit/hooks', () => ({
  useAuditChain: () => ({ data: { ok: true, checkedRows: 2 } }),
  useAuditRuns: () => ({ data: [
    { runId: 'run-real', taskId: 'task-1', title: 'Real run', ts: '2026-09-23T09:00:00Z', status: 'completed', costUsd: null },
  ] }),
  useAuditTrace: (runId: string | null) => {
    auditState.traceRunId = runId
    return { data: runId ? [{ runId, seq: 1, ts: '2026-09-23T09:00:00Z', kind: 'task', summary: 'Real trace', detail: {}, tokens: null, costUsd: null }] : undefined }
  },
}))

import { AuditPage } from '@/features/audit/audit-page'

describe('AuditPage trace routing', () => {
  it('does not fall back to another run when the requested trace is missing', () => {
    render(<MemoryRouter initialEntries={['/audit?run=missing-run']}><AuditPage /></MemoryRouter>)

    expect(screen.getByRole('alert')).toHaveTextContent('Trace “missing-run” was not found. No different run was opened.')
    expect(auditState.traceRunId).toBeNull()
    expect(screen.queryByText('Real trace')).not.toBeInTheDocument()
  })
})
