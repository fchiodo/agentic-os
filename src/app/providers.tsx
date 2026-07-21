import { QueryClientProvider } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { RouterProvider } from 'react-router-dom'
import { createAppQueryClient } from '@/app/query-client'
import { router } from '@/app/router'
import { taskEventSchema } from '@/features/runner/schema'
import { isTauriRuntime } from '@/lib/tauri'
import { useTaskEventsStore } from '@/store/task-events'

const TASK_EVENT_CHANNEL = 'agent-control://task-event'

/**
 * Mounts once, subscribes to the global Tauri event channel every running
 * task streams onto, and pushes events into the Zustand store used by the
 * Runner live log. It deliberately does not invalidate or poll queries.
 * See UI-SPEC.md section 3 "Tauri command surface and event streaming".
 */
function TaskEventBridge() {
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
      }).then((fn) => {
        unlisten = fn
      })
    })

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [pushEvents])

  return null
}

export function AppProviders() {
  const [queryClient] = useState(createAppQueryClient)

  return (
    <QueryClientProvider client={queryClient}>
      <TaskEventBridge />
      <RouterProvider router={router} />
    </QueryClientProvider>
  )
}
