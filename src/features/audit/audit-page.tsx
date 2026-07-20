import { useState } from 'react'
import { SectionEmptyState } from '@/components/ui/section-empty-state'
import { StatusBadge } from '@/components/ui/status-badge'
import { TraceTimeline } from '@/components/ui/trace-timeline'
import { useAuditChain, useAuditRuns, useAuditTrace } from '@/features/audit/hooks'
import { formatCompactNumber, formatRelativeTime } from '@/lib/format'
import { labelForTaskStatus, toneForTaskStatus } from '@/lib/status'
import type { TaskStatus } from '@/features/runner/schema'

function isKnownTaskStatus(value: string): value is TaskStatus {
  return [
    'created',
    'classified',
    'planned',
    'running',
    'waiting_for_tool',
    'waiting_for_approval',
    'resuming',
    'verifying',
    'completed',
    'failed',
    'cancelled',
    'partially_completed',
  ].includes(value)
}

export function AuditPage() {
  const { data: runs } = useAuditRuns()
  const { data: chain } = useAuditChain()
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null)

  const effectiveRunId = selectedRunId ?? runs?.[0]?.runId ?? null
  const { data: trace } = useAuditTrace(effectiveRunId)

  return (
    <section className="page-section audit-page">
      <div className="panel-heading">
        <h2>Run history</h2>
        {chain ? (
          <StatusBadge
            label={
              chain.ok
                ? `Audit chain verified · ${formatCompactNumber(chain.checkedRows)} rows`
                : `Chain broken at row ${chain.brokenAt ?? '?'}`
            }
            tone={chain.ok ? 'success' : 'danger'}
          />
        ) : null}
      </div>

      {!runs || runs.length === 0 ? (
        <SectionEmptyState
          body="Run something from Runner and its full trace will show up here."
          title="No runs yet"
        />
      ) : (
        <div className="audit-grid">
          <ul className="workspace-list audit-run-list">
            {runs.map((run) => (
              <li key={run.runId}>
                <button
                  className={
                    run.runId === effectiveRunId
                      ? 'workspace-row audit-run-row is-selected'
                      : 'workspace-row audit-run-row'
                  }
                  onClick={() => setSelectedRunId(run.runId)}
                  type="button"
                >
                  <div className="workspace-row-copy">
                    <p className="row-title">{run.title}</p>
                    <p className="row-subtle workspace-meta">
                      {formatRelativeTime(new Date(run.ts).getTime())}
                      {run.costUsd !== null ? ` · $${run.costUsd.toFixed(2)}` : ''}
                    </p>
                  </div>
                  <StatusBadge
                    label={isKnownTaskStatus(run.status) ? labelForTaskStatus(run.status) : run.status}
                    tone={isKnownTaskStatus(run.status) ? toneForTaskStatus(run.status) : 'neutral'}
                  />
                </button>
              </li>
            ))}
          </ul>

          <section className="surface audit-trace-panel">
            <div className="panel-heading">
              <h2>Trace</h2>
            </div>
            {trace && trace.length > 0 ? (
              <TraceTimeline entries={trace} />
            ) : (
              <p className="row-subtle">Select a run to see its full trace.</p>
            )}
          </section>
        </div>
      )}
    </section>
  )
}
