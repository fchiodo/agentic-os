import { QueryClient, QueryClientProvider, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { RouterProvider } from 'react-router-dom'
import { router } from '@/app/router'
import { approvalsQueryKey } from '@/features/approvals/hooks'
import { controlStatusQueryKey } from '@/features/control/use-control-status'
import { taskEventSchema } from '@/features/runner/schema'
import { tasksQueryKey } from '@/features/runner/hooks'
import { isTauriRuntime } from '@/lib/tauri'
import { useTaskEventsStore } from '@/store/task-events'

const TASK_EVENT_CHANNEL = 'agent-control://task-event'

// Kinds that mean "something list-level changed" and should invalidate
// the lighter-weight queries (task list, approvals, control status)
// rather than only the per-task detail/event stream. Everything else is
// purely a live-log line for the task that is currently open.
const LIST_INVALIDATING_KINDS = new Set(['status_changed', 'cost_update'])

/**
 * Mounts once, subscribes to the global Tauri event channel every running
 * task streams onto, and fans events out to the Zustand event store (for
 * the live log) plus TanStack Query invalidation (for list-level views).
 * See UI-SPEC.md section 3 "Tauri command surface and event streaming".
 */
function TaskEventBridge() {
  const queryClient = useQueryClient()
  const pushEvents = useTaskEventsStore((state) => state.pushEvents)

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    let unlisten: (() => void) | undefined
    let cancelled = false

    void import('@tauri-apps/api/event').then(({ listen }) => {
      if (cancelled) {
        return
      }

      void listen(TASK_EVENT_CHANNEL, (message) => {
        const parsed = taskEventSchema.safeParse(message.payload)
        if (!parsed.success) {
          return
        }

        pushEvents([parsed.data])

        if (LIST_INVALIDATING_KINDS.has(parsed.data.kind)) {
          void queryClient.invalidateQueries({ queryKey: tasksQueryKey })
          void queryClient.invalidateQueries({ queryKey: controlStatusQueryKey })
        }

        if (parsed.data.kind === 'status_changed') {
          void queryClient.invalidateQueries({ queryKey: approvalsQueryKey })
        }
      }).then((fn) => {
        unlisten = fn
      })
    })

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [queryClient, pushEvents])

  return null
}

export function AppProviders() {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            refetchInterval: 30_000,
            refetchOnWindowFocus: false,
            retry: 1,
            staleTime: 15_000,
          },
        },
      }),
  )

  return (
    <QueryClientProvider client={queryClient}>
      <TaskEventBridge />
      <RouterProvider router={router} />
    </QueryClientProvider>
  )
}
