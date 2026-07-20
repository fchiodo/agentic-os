import { useEffect, useRef, useState } from 'react'
import type { TaskEvent } from '@/features/runner/schema'

type LiveLogProps = {
  events: TaskEvent[]
  max?: number
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function lineFor(event: TaskEvent): string {
  const payload = event.payload
  if (isRecord(payload)) {
    if (typeof payload.raw_line === 'string') {
      return payload.raw_line
    }
    if (typeof payload.message === 'string') {
      return `[${event.kind}] ${payload.message}`
    }
    if (typeof payload.command === 'string') {
      return `$ ${payload.command}`
    }
    if (typeof payload.status === 'string') {
      return `[${event.kind}] status: ${payload.status}`
    }
  }

  const serialized = JSON.stringify(payload)
  const truncated = serialized.length > 160 ? `${serialized.slice(0, 157)}...` : serialized
  return `[${event.kind}] ${truncated}`
}

export function LiveLog({ events, max = 30 }: LiveLogProps) {
  const [pinned, setPinned] = useState(true)
  const containerRef = useRef<HTMLDivElement>(null)
  const visible = events.slice(-max)

  useEffect(() => {
    const el = containerRef.current
    if (pinned && el) {
      el.scrollTop = el.scrollHeight
    }
  }, [visible.length, pinned])

  function handleScroll() {
    const el = containerRef.current
    if (!el) {
      return
    }
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 24
    setPinned(atBottom)
  }

  function jumpToLatest() {
    const el = containerRef.current
    if (el) {
      el.scrollTop = el.scrollHeight
    }
    setPinned(true)
  }

  if (visible.length === 0) {
    return null
  }

  return (
    <div className="live-log-wrap">
      <div className="code-panel live-log" ref={containerRef} onScroll={handleScroll}>
        {visible.map((event) => (
          <div key={`${event.seq}-${event.ts}-${event.kind}`} className="live-log-line">
            {lineFor(event)}
          </div>
        ))}
      </div>
      {!pinned ? (
        <button className="live-log-jump" onClick={jumpToLatest} type="button">
          Jump to latest
        </button>
      ) : null}
    </div>
  )
}
