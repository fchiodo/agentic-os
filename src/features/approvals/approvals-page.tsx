import { useEffect, useMemo, useState } from 'react'
import { ApprovalCard } from '@/components/ui/approval-card'
import { SectionEmptyState } from '@/components/ui/section-empty-state'
import { useApprovals, useDecideApproval } from '@/features/approvals/hooks'

export function ApprovalsPage() {
  const { data: approvals } = useApprovals()
  const decideMutation = useDecideApproval()
  const [focusedIndex, setFocusedIndex] = useState(0)

  const sorted = useMemo(
    () =>
      [...(approvals ?? [])].sort(
        (a, b) => new Date(a.requestedAt).getTime() - new Date(b.requestedAt).getTime(),
      ),
    [approvals],
  )

  // Clamped at render time rather than via an effect + setState, so a
  // shrinking list (an approval got decided elsewhere) never points
  // focus past the end for even one extra render.
  const clampedFocusedIndex = Math.min(focusedIndex, Math.max(0, sorted.length - 1))

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (sorted.length === 0) {
        return
      }
      const target = event.target as HTMLElement | null
      if (target && ['INPUT', 'TEXTAREA'].includes(target.tagName)) {
        return
      }

      if (event.key === 'ArrowDown') {
        event.preventDefault()
        setFocusedIndex((index) => Math.min(index + 1, sorted.length - 1))
      } else if (event.key === 'ArrowUp') {
        event.preventDefault()
        setFocusedIndex((index) => Math.max(index - 1, 0))
      } else if (event.key === 'a' || event.key === 'A') {
        const approval = sorted[clampedFocusedIndex]
        if (approval) {
          decideMutation.mutate({ id: approval.id, decision: 'approve' })
        }
      } else if (event.key === 'd' || event.key === 'D') {
        const approval = sorted[clampedFocusedIndex]
        if (approval) {
          decideMutation.mutate({ id: approval.id, decision: 'deny' })
        }
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [sorted, clampedFocusedIndex, decideMutation])

  return (
    <section className="page-section approvals-page">
      <p aria-atomic="true" aria-live="polite" className="sr-only">
        {sorted.length} approval{sorted.length === 1 ? '' : 's'} waiting
      </p>

      {sorted.length === 0 ? (
        <SectionEmptyState
          body="Auto-approved actions still show up in Audit — nothing here needs a decision from you."
          title="Nothing is waiting on you."
        />
      ) : (
        <div className="approvals-list">
          {sorted.map((approval, index) => (
            <div
              className={
                index === clampedFocusedIndex ? 'approval-card-wrap is-focused' : 'approval-card-wrap'
              }
              key={approval.id}
            >
              <ApprovalCard
                approval={approval}
                onDecide={(decision, note) =>
                  decideMutation.mutate({ id: approval.id, decision, note })
                }
                pending={decideMutation.isPending}
              />
            </div>
          ))}
        </div>
      )}
    </section>
  )
}
