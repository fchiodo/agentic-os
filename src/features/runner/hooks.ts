import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'
import { controlStatusQueryKey } from '@/features/control/use-control-status'
import {
  cancelTask,
  getTask,
  getTaskEventsSince,
  listTasks,
  submitTask,
} from '@/features/runner/api'
import type { TaskSubmitRequest } from '@/features/runner/schema'
import { useTaskEventsStore } from '@/store/task-events'

export const tasksQueryKey = ['tasks'] as const
export const taskDetailQueryKey = (id: string) => ['tasks', 'detail', id] as const

export function useTasks() {
  return useQuery({
    queryKey: tasksQueryKey,
    queryFn: listTasks,
  })
}

export function useTaskDetail(id: string | null) {
  return useQuery({
    queryKey: id ? taskDetailQueryKey(id) : ['tasks', 'detail', 'none'],
    queryFn: () => getTask(id as string),
    enabled: Boolean(id),
  })
}

export function useSubmitTask() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (request: TaskSubmitRequest) => submitTask(request),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: tasksQueryKey })
      void queryClient.invalidateQueries({ queryKey: controlStatusQueryKey })
    },
  })
}

export function useCancelTask() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: string) => cancelTask(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: tasksQueryKey })
    },
  })
}

/**
 * Closes any event gap for a task the moment it becomes visible - the
 * global listener (see app/providers.tsx) only catches events emitted
 * while the app is running, so a task opened after being created
 * elsewhere (or after a reconnect) needs one tasks_events_since call to
 * back-fill. seq is monotonic per UI-SPEC.md section 3, so this is safe
 * to call every time the task is opened.
 */
export function useTaskEventSync(taskId: string | null) {
  const pushEvents = useTaskEventsStore((state) => state.pushEvents)
  const lastSeq = useTaskEventsStore((state) =>
    taskId ? (state.lastSeqByTask[taskId] ?? 0) : 0,
  )

  useEffect(() => {
    if (!taskId) {
      return
    }

    let cancelled = false

    void getTaskEventsSince(taskId, lastSeq).then((events) => {
      if (!cancelled && events.length > 0) {
        pushEvents(events)
      }
    })

    return () => {
      cancelled = true
    }
    // Deliberately only re-run when taskId changes - lastSeq updates on
    // every push and would otherwise re-trigger this on every event.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId])
}
