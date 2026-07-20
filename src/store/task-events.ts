import { create } from 'zustand'
import type { TaskEvent } from '@/features/runner/schema'

const MAX_EVENTS_PER_TASK = 2000

// Stable empty-array reference for tasks with no events yet. Returning a
// fresh `[]` literal from a selector breaks useSyncExternalStore (Zustand
// calls the selector more than once per render to detect tearing, and a
// new reference each time reads as "changed" and loops the component).
const EMPTY_EVENTS: TaskEvent[] = []

type TaskEventsState = {
  eventsByTask: Record<string, TaskEvent[]>
  lastSeqByTask: Record<string, number>
  pushEvents: (events: TaskEvent[]) => void
  clearTask: (taskId: string) => void
}

export const useTaskEventsStore = create<TaskEventsState>((set) => ({
  eventsByTask: {},
  lastSeqByTask: {},
  pushEvents: (incoming) => {
    if (incoming.length === 0) {
      return
    }

    set((state) => {
      const eventsByTask = { ...state.eventsByTask }
      const lastSeqByTask = { ...state.lastSeqByTask }

      for (const event of incoming) {
        const existing = eventsByTask[event.taskId] ?? []
        // seq -1 marks a synthetic live status ping (not a persisted DB
        // row, see harness/codex.rs emit_status) - never dedupe those.
        const isDuplicate = event.seq >= 0 && existing.some((item) => item.seq === event.seq)
        const next = isDuplicate ? existing : [...existing, event]
        eventsByTask[event.taskId] =
          next.length > MAX_EVENTS_PER_TASK ? next.slice(next.length - MAX_EVENTS_PER_TASK) : next

        if (event.seq >= 0) {
          lastSeqByTask[event.taskId] = Math.max(lastSeqByTask[event.taskId] ?? 0, event.seq)
        }
      }

      return { eventsByTask, lastSeqByTask }
    })
  },
  clearTask: (taskId) => {
    set((state) => {
      const eventsByTask = { ...state.eventsByTask }
      const lastSeqByTask = { ...state.lastSeqByTask }
      delete eventsByTask[taskId]
      delete lastSeqByTask[taskId]
      return { eventsByTask, lastSeqByTask }
    })
  },
}))

export function selectTaskEvents(taskId: string) {
  return (state: TaskEventsState) => state.eventsByTask[taskId] ?? EMPTY_EVENTS
}

export function selectLastSeq(taskId: string) {
  return (state: TaskEventsState) => state.lastSeqByTask[taskId] ?? 0
}
