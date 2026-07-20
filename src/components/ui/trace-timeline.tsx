import { ChevronDown, ChevronRight } from 'lucide-react'
import { useState } from 'react'
import type { TraceEntry } from '@/features/audit/schema'
import { formatDateTime } from '@/lib/format'

type TraceTimelineProps = {
  entries: TraceEntry[]
}

function TraceRow({ entry }: { entry: TraceEntry }) {
  const [open, setOpen] = useState(false)
  const hasDetail = entry.detail !== null && entry.detail !== undefined

  return (
    <li className="trace-row">
      <button
        className="trace-row-toggle"
        disabled={!hasDetail}
        onClick={() => setOpen((value) => !value)}
        type="button"
      >
        <span aria-hidden="true" className="trace-row-chevron">
          {hasDetail ? (
            open ? (
              <ChevronDown size={14} />
            ) : (
              <ChevronRight size={14} />
            )
          ) : null}
        </span>
        <span className="tag-chip cell-lowercase trace-row-kind">{entry.kind}</span>
        <span className="trace-row-summary">{entry.summary}</span>
        <span className="row-subtle trace-row-time">
          {formatDateTime(new Date(entry.ts).getTime())}
        </span>
        {entry.tokens !== null || entry.costUsd !== null ? (
          <span className="row-subtle trace-row-cost">
            {entry.tokens !== null ? `${entry.tokens} tok` : ''}
            {entry.costUsd !== null ? ` · $${entry.costUsd.toFixed(2)}` : ''}
          </span>
        ) : null}
      </button>
      {open && hasDetail ? (
        <pre className="code-panel trace-row-detail">
          {JSON.stringify(entry.detail, null, 2)}
        </pre>
      ) : null}
    </li>
  )
}

export function TraceTimeline({ entries }: TraceTimelineProps) {
  return (
    <ul className="trace-timeline">
      {entries.map((entry) => (
        <TraceRow entry={entry} key={`${entry.runId}-${entry.seq}`} />
      ))}
    </ul>
  )
}
