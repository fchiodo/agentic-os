import { MetricCard } from '@/components/ui/metric-card'
import { SectionEmptyState } from '@/components/ui/section-empty-state'
import { useDashboardSnapshot } from '@/features/dashboard/use-dashboard-snapshot'
import { useTasks } from '@/features/runner/hooks'
import type { Domain } from '@/features/runner/schema'
import { formatCompactNumber } from '@/lib/format'

type DomainRollup = {
  costUsd: number
  domain: Domain
  runs: number
  tokens: number
}

export function UsagePage() {
  const { data: snapshot } = useDashboardSnapshot()
  const { data: tasks } = useTasks()

  const byDomain = (tasks ?? []).reduce<Record<string, DomainRollup>>((acc, task) => {
    const existing = acc[task.domain] ?? { costUsd: 0, domain: task.domain, runs: 0, tokens: 0 }
    acc[task.domain] = {
      costUsd: existing.costUsd + (task.costUsd ?? 0),
      domain: task.domain,
      runs: existing.runs + 1,
      tokens: existing.tokens + task.costTokens,
    }
    return acc
  }, {})

  const domainRollups = Object.values(byDomain).sort((a, b) => b.tokens - a.tokens)
  const totalRunnerTokens = domainRollups.reduce((sum, row) => sum + row.tokens, 0)
  const totalRunnerCost = domainRollups.reduce((sum, row) => sum + row.costUsd, 0)

  return (
    <section className="page-section usage-page">
      <section className="metric-strip" aria-label="Runner cost metrics">
        <MetricCard
          hint="Total tokens spent across every Runner task, all time."
          label="Runner tokens"
          tone="accent"
          value={formatCompactNumber(totalRunnerTokens)}
        />
        <MetricCard
          hint="Sum of estimated cost across every Runner task, all time."
          label="Runner cost"
          tone="neutral"
          value={`$${totalRunnerCost.toFixed(2)}`}
        />
        <MetricCard
          label="Runner tasks"
          tone="success"
          value={String(tasks?.length ?? 0)}
        />
        <MetricCard
          hint="Codex conversation threads discovered on this machine."
          label="Codex threads"
          tone="warning"
          value={formatCompactNumber(snapshot?.usage.trackedThreads ?? 0)}
        />
      </section>

      <div className="usage-grid-wide">
        <section className="surface">
          <div className="panel-heading">
            <h2>Runner cost by domain</h2>
          </div>
          {domainRollups.length === 0 ? (
            <SectionEmptyState
              body="Run a task in Runner and its cost will roll up here by domain."
              title="No Runner activity yet"
            />
          ) : (
            <ul className="workspace-list">
              {domainRollups.map((row) => (
                <li className="workspace-row" key={row.domain}>
                  <div className="workspace-row-copy">
                    <p className="row-title cell-lowercase">{row.domain}</p>
                    <p className="row-subtle workspace-meta">
                      {row.runs} run{row.runs === 1 ? '' : 's'}
                    </p>
                  </div>
                  <span className="token-pill workspace-token-pill">
                    {formatCompactNumber(row.tokens)} tok
                  </span>
                  <span className="row-subtle">${row.costUsd.toFixed(2)}</span>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="surface">
          <div className="panel-heading">
            <h2>Codex workspaces</h2>
          </div>
          {snapshot && snapshot.usage.topWorkspaces.length > 0 ? (
            <ul className="workspace-list">
              {snapshot.usage.topWorkspaces.map((workspace) => (
                <li className="workspace-row" key={workspace.cwd}>
                  <div className="workspace-row-copy">
                    <p className="row-title workspace-path">{workspace.cwd}</p>
                    <p className="row-subtle workspace-meta">
                      {workspace.threadCount} thread{workspace.threadCount === 1 ? '' : 's'}
                    </p>
                  </div>
                  <span className="token-pill workspace-token-pill">
                    {formatCompactNumber(workspace.tokenTotal)} tok
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <SectionEmptyState
              body="Codex thread telemetry from ~/.codex will show up here once available."
              title="No Codex usage detected"
            />
          )}
        </section>
      </div>
    </section>
  )
}
