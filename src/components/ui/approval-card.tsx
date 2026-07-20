import { GitBranch } from 'lucide-react'
import { useState } from 'react'
import { DiffView } from '@/components/ui/diff-view'
import { StatusBadge } from '@/components/ui/status-badge'
import type { ApprovalRequest } from '@/features/approvals/schema'
import { toneForRisk } from '@/lib/status'

type ApprovalCardProps = {
  approval: ApprovalRequest
  onDecide: (decision: 'approve' | 'deny', note?: string) => void
  pending?: boolean
}

export function ApprovalCard({ approval, onDecide, pending = false }: ApprovalCardProps) {
  const [note, setNote] = useState('')
  const [showNoteField, setShowNoteField] = useState(false)

  return (
    <article className="surface approval-card">
      <div className="panel-heading">
        <div>
          <p className="row-title">{approval.actionSummary}</p>
          <p className="row-subtle">{approval.taskTitle}</p>
        </div>
        <div className="tag-row">
          <span className="tag-chip cell-lowercase">{approval.domain}</span>
          <StatusBadge label={approval.riskLevel} tone={toneForRisk(approval.riskLevel)} />
        </div>
      </div>

      {approval.preview ? (
        approval.preview.kind === 'diff' ? (
          <DiffView unifiedDiff={approval.preview.content} />
        ) : approval.preview.kind === 'command' ? (
          <div className="code-panel">{approval.preview.content}</div>
        ) : (
          <p className="body-copy approval-preview-text">{approval.preview.content}</p>
        )
      ) : null}

      <div className="approval-action-row">
        <span className="approval-tool-name">
          <GitBranch aria-hidden="true" size={15} />
          {approval.toolName}
        </span>

        <div className="inline-actions">
          {showNoteField ? (
            <input
              autoFocus
              className="approval-note-input"
              onChange={(event) => setNote(event.target.value)}
              placeholder="Reason for denial (optional)"
              value={note}
            />
          ) : null}
          <button
            className="icon-button"
            disabled={pending}
            onClick={() => {
              if (showNoteField) {
                onDecide('deny', note || undefined)
              } else {
                setShowNoteField(true)
              }
            }}
            type="button"
          >
            {showNoteField ? 'Confirm deny' : 'Deny'}
          </button>
          <button
            className="primary-button"
            disabled={pending}
            onClick={() => onDecide('approve')}
            type="button"
          >
            Approve
          </button>
        </div>
      </div>
    </article>
  )
}
