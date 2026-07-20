import { useMemo, useState } from 'react'
import { MetricCard } from '@/components/ui/metric-card'
import { SectionEmptyState } from '@/components/ui/section-empty-state'
import { TaskCard } from '@/components/ui/task-card'
import { useApprovals, useDecideApproval } from '@/features/approvals/hooks'
import {
  useCancelTask,
  useSubmitTask,
  useTaskDetail,
  useTaskEventSync,
  useTasks,
} from '@/features/runner/hooks'
import type { Domain, TaskSummary } from '@/features/runner/schema'
import { formatCompactNumber } from '@/lib/format'
import { selectTaskEvents, useTaskEventsStore } from '@/store/task-events'

const domainOptions: Domain[] = [
  'work',
  'planphysique',
  'personal',
  'family',
  'finance',
  'research',
]

const statusPriority: Record<TaskSummary['status'], number> = {
  waiting_for_approval: 0,
  running: 1,
  waiting_for_tool: 1,
  resuming: 1,
  verifying: 1,
  created: 2,
  classified: 2,
  planned: 2,
  completed: 3,
  failed: 3,
  cancelled: 3,
  partially_completed: 3,
}

function sortTasks(tasks: TaskSummary[]): TaskSummary[] {
  return [...tasks].sort((a, b) => {
    const priorityDiff = statusPriority[a.status] - statusPriority[b.status]
    if (priorityDiff !== 0) {
      return priorityDiff
    }
    return new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()
  })
}

function isToday(iso: string): boolean {
  const date = new Date(iso)
  const now = new Date()
  return (
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate()
  )
}

function TaskCardContainer({
  onCancel,
  onDecide,
  taskId,
}: {
  onCancel: (id: string) => void
  onDecide: (id: string, decision: 'approve' | 'deny', note?: string) => void
  taskId: string
}) {
  const { data: task } = useTaskDetail(taskId)
  const { data: approvals } = useApprovals()
  useTaskEventSync(taskId)
  const events = useTaskEventsStore(selectTaskEvents(taskId))

  if (!task) {
    return null
  }

  const approval = approvals?.find((item) => item.taskId === taskId) ?? null

  return (
    <TaskCard
      approval={approval}
      events={events}
      onApprovalDecide={
        approval ? (decision, note) => onDecide(approval.id, decision, note) : undefined
      }
      onCancel={() => onCancel(taskId)}
      task={task}
    />
  )
}

export function RunnerPage() {
  const { data: tasks } = useTasks()
  const submitMutation = useSubmitTask()
  const cancelMutation = useCancelTask()
  const decideMutation = useDecideApproval()
  const [goal, setGoal] = useState('')
  const [domain, setDomain] = useState<Domain>('work')

  const sortedTasks = useMemo(() => sortTasks(tasks ?? []), [tasks])

  const runningCount = tasks?.filter((task) => task.status === 'running').length ?? 0
  const waitingCount =
    tasks?.filter((task) => task.status === 'waiting_for_approval').length ?? 0
  const completedTodayTasks =
    tasks?.filter((task) => task.status === 'completed' && isToday(task.updatedAt)) ?? []
  const spentTodayUsd = (tasks ?? [])
    .filter((task) => isToday(task.updatedAt))
    .reduce((sum, task) => sum + (task.costUsd ?? 0), 0)

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault()
    const trimmed = goal.trim()
    if (!trimmed) {
      return
    }
    submitMutation.mutate(
      { goal: trimmed, domain },
      {
        onSuccess: () => setGoal(''),
      },
    )
  }

  return (
    <section className="page-section runner-page">
      <section className="metric-strip" aria-label="Runner metrics">
        <MetricCard label="Running" tone="accent" value={String(runningCount)} />
        <MetricCard label="Waiting for approval" tone="warning" value={String(waitingCount)} />
        <MetricCard
          label="Completed today"
          tone="success"
          value={String(completedTodayTasks.length)}
        />
        <MetricCard
          hint="Sum of estimated cost across tasks last updated today."
          label="Spent today"
          tone="neutral"
          value={`$${spentTodayUsd.toFixed(2)}`}
        />
      </section>

      <div className="runner-grid">
        <div className="runner-task-list">
          {sortedTasks.length === 0 ? (
            <SectionEmptyState
              body="Describe a goal in the composer, or run something from Catalog."
              title="No tasks yet"
            />
          ) : (
            sortedTasks.map((task) => (
              <TaskCardContainer
                key={task.id}
                onCancel={(id) => cancelMutation.mutate(id)}
                onDecide={(id, decision, note) => decideMutation.mutate({ id, decision, note })}
                taskId={task.id}
              />
            ))
          )}
        </div>

        <aside className="inspector">
          <section className="surface">
            <div className="panel-heading">
              <h2>New task</h2>
            </div>
            <form className="field-stack runner-composer-form" onSubmit={handleSubmit}>
              <label>
                <span>Goal</span>
                <textarea
                  onChange={(event) => setGoal(event.target.value)}
                  placeholder="Describe what you want done, e.g. QA the latest newsletter campaign against the Vans style guide"
                  value={goal}
                />
              </label>
              <label>
                <span>Domain</span>
                <select
                  onChange={(event) => setDomain(event.target.value as Domain)}
                  value={domain}
                >
                  {domainOptions.map((option) => (
                    <option key={option} value={option}>
                      {option}
                    </option>
                  ))}
                </select>
              </label>
              <button
                className="primary-button"
                disabled={submitMutation.isPending || goal.trim().length === 0}
                type="submit"
              >
                {submitMutation.isPending ? 'Submitting…' : 'Run'}
              </button>
              {submitMutation.isError ? (
                <p className="row-subtle runner-composer-error">
                  {submitMutation.error instanceof Error
                    ? submitMutation.error.message
                    : 'The task could not be submitted.'}
                </p>
              ) : null}
            </form>
          </section>

          <section className="surface">
            <div className="panel-heading">
              <h2>How risk gating works</h2>
            </div>
            <p className="body-copy runner-help-copy">
              Every goal is screened before anything runs. Read-only work starts immediately.
              Anything that looks like it writes, pushes, sends, or deletes waits in Approvals
              first — see {formatCompactNumber(waitingCount)} pending above.
            </p>
          </section>
        </aside>
      </div>
    </section>
  )
}
