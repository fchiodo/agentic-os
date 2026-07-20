import { FileText, GitBranch, Mail, Wand2 } from 'lucide-react'
import { ApprovalCard } from '@/components/ui/approval-card'
import { LiveLog } from '@/components/ui/live-log'
import { StatusBadge } from '@/components/ui/status-badge'
import { StepChecklist } from '@/components/ui/step-checklist'
import type { ApprovalRequest } from '@/features/approvals/schema'
import type { ArtifactRef, TaskDetail, TaskEvent } from '@/features/runner/schema'
import { formatCompactNumber, formatRelativeTime } from '@/lib/format'
import { labelForTaskStatus, toneForTaskStatus } from '@/lib/status'

type TaskCardProps = {
  approval?: ApprovalRequest | null
  events: TaskEvent[]
  onApprovalDecide?: (decision: 'approve' | 'deny', note?: string) => void
  onCancel?: () => void
  onDistill?: () => void
  task: TaskDetail
}

function artifactIcon(kind: ArtifactRef['kind']) {
  switch (kind) {
    case 'draft':
      return Mail
    case 'diff':
      return GitBranch
    default:
      return FileText
  }
}

export function TaskCard({
  approval,
  events,
  onApprovalDecide,
  onCancel,
  onDistill,
  task,
}: TaskCardProps) {
  const showLog = task.status === 'running' || task.status === 'waiting_for_tool'
  const showApproval = task.status === 'waiting_for_approval' && approval && onApprovalDecide

  return (
    <article className="surface task-card">
      <div className="panel-heading">
        <div>
          <p className="row-title">{task.title}</p>
          <p className="row-subtle">
            {task.harness} · {task.agentId ?? 'auto'} · {task.domain}
          </p>
        </div>
        <StatusBadge label={labelForTaskStatus(task.status)} tone={toneForTaskStatus(task.status)} />
      </div>

      <StepChecklist steps={task.steps} />

      {showLog ? <LiveLog events={events} /> : null}

      {showApproval && approval ? (
        <ApprovalCard
          approval={approval}
          onDecide={(decision, note) => onApprovalDecide?.(decision, note)}
        />
      ) : null}

      {task.status === 'completed' && task.artifacts.length > 0 ? (
        <div className="tag-row">
          {task.artifacts.map((artifact) => {
            const Icon = artifactIcon(artifact.kind)
            return (
              <span className="tag-chip" key={artifact.id}>
                <Icon aria-hidden="true" size={14} />
                {artifact.label}
              </span>
            )
          })}
        </div>
      ) : null}

      {task.status === 'failed' ? (
        <p className="row-subtle task-failure-reason">
          The agent could not complete this task. Check Audit for the full trace.
        </p>
      ) : null}

      <div className="task-card-footer">
        <span className="row-subtle">{formatRelativeTime(new Date(task.updatedAt).getTime())}</span>
        <span className="row-subtle">
          {formatCompactNumber(task.costTokens)} tok
          {task.costUsd !== null ? ` · $${task.costUsd.toFixed(2)}` : ''}
        </span>
        <div className="inline-actions task-card-actions">
          {task.status === 'completed' && onDistill ? (
            <button className="icon-button" onClick={onDistill} type="button">
              <Wand2 aria-hidden="true" size={14} />
              Distill to skill
            </button>
          ) : null}
          {(task.status === 'running' || task.status === 'waiting_for_approval') && onCancel ? (
            <button className="icon-button" onClick={onCancel} type="button">
              Cancel
            </button>
          ) : null}
        </div>
      </div>
    </article>
  )
}
