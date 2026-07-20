import type { AuditRunSummary, TraceEntry } from '@/features/audit/schema'

export const mockAuditRuns: AuditRunSummary[] = [
  {
    runId: 'task-3',
    taskId: 'task-3',
    title: 'Summarize the Databricks sync meeting',
    ts: new Date(Date.now() - 36 * 60_000).toISOString(),
    status: 'completed',
    costUsd: 0.18,
  },
  {
    runId: 'task-1',
    taskId: 'task-1',
    title: 'QA skate-classics-era-launch against the Vans style guide',
    ts: new Date(Date.now() - 5_000).toISOString(),
    status: 'running',
    costUsd: null,
  },
]

export const mockTrace: Record<string, TraceEntry[]> = {
  'task-3': [
    {
      runId: 'task-3',
      seq: 1,
      ts: new Date(Date.now() - 40 * 60_000).toISOString(),
      kind: 'input',
      summary: 'Task submitted',
      detail: { goal: 'Summarize the Databricks sync transcript' },
      tokens: null,
      costUsd: null,
    },
    {
      runId: 'task-3',
      seq: 2,
      ts: new Date(Date.now() - 39 * 60_000).toISOString(),
      kind: 'policy_decision',
      summary: 'Read-only task, auto-approved',
      detail: { riskLevel: 'low', sandboxMode: 'read-only' },
      tokens: null,
      costUsd: null,
    },
    {
      runId: 'task-3',
      seq: 3,
      ts: new Date(Date.now() - 36 * 60_000).toISOString(),
      kind: 'output',
      summary: 'Task completed',
      detail: { status: 'completed' },
      tokens: 8100,
      costUsd: 0.18,
    },
  ],
}
