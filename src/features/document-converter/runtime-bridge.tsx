import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { isTauriRuntime } from '@/lib/tauri'
import * as api from './api'
import { converterJobsKey } from './hooks'
import { conversionProgressSchema, modelProgressSchema } from './schema'
import { useConverterStore } from './store'

type RuntimeBridgeProps = {
  dropEnabled: boolean
}

/**
 * Owns the native converter listeners for the lifetime of the app shell.
 * Route changes must not repeatedly register and tear down Tauri listeners:
 * on macOS a drag/drop subscription expands to four native listeners, so
 * rapid navigation otherwise creates overlapping registrations.
 */
export function DocumentConverterRuntimeBridge({ dropEnabled }: RuntimeBridgeProps) {
  const queryClient = useQueryClient()
  const dropEnabledRef = useRef(dropEnabled)
  const addSelected = useConverterStore((state) => state.addSelected)
  const setConversionProgress = useConverterStore((state) => state.setConversionProgress)
  const setModelProgress = useConverterStore((state) => state.setModelProgress)
  const setNativeDragActive = useConverterStore((state) => state.setNativeDragActive)

  useEffect(() => {
    dropEnabledRef.current = dropEnabled
    if (!dropEnabled) setNativeDragActive(false)
  }, [dropEnabled, setNativeDragActive])

  useEffect(() => {
    if (!isTauriRuntime()) return

    let disposed = false
    const activeUnlisteners = new Set<UnlistenFn>()
    const register = (registration: Promise<UnlistenFn>) => {
      void registration.then((unlisten) => {
        if (disposed) unlisten()
        else activeUnlisteners.add(unlisten)
      }).catch(() => {
        // A failed listener is retried on the next app-shell mount. Keeping
        // the rejection handled prevents a native setup failure from taking
        // down the React tree.
      })
    }
    const refreshJobs = () => {
      void queryClient.invalidateQueries({ queryKey: converterJobsKey })
    }

    register(listen('document-converter:model-progress', (event) => {
      const parsed = modelProgressSchema.safeParse(event.payload)
      if (parsed.success) setModelProgress(parsed.data)
    }))
    register(listen('document-converter:conversion-progress', (event) => {
      const parsed = conversionProgressSchema.safeParse(event.payload)
      if (parsed.success) setConversionProgress(parsed.data)
      refreshJobs()
    }))
    register(listen('document-converter:conversion-completed', refreshJobs))
    register(listen('document-converter:conversion-failed', refreshJobs))
    register(getCurrentWebview().onDragDropEvent((event) => {
      if (!dropEnabledRef.current) return
      if (event.payload.type === 'enter' || event.payload.type === 'over') {
        setNativeDragActive(true)
      } else if (event.payload.type === 'leave') {
        setNativeDragActive(false)
      } else if (event.payload.type === 'drop') {
        setNativeDragActive(false)
        if (event.payload.paths.length > 0) {
          void api.inspectPaths(event.payload.paths).then(addSelected).catch(() => {
            // File validation errors remain local to the converter and must
            // not become unhandled rejections during route transitions.
          })
        }
      }
    }))

    return () => {
      disposed = true
      setNativeDragActive(false)
      for (const unlisten of activeUnlisteners) unlisten()
      activeUnlisteners.clear()
    }
  }, [addSelected, queryClient, setConversionProgress, setModelProgress, setNativeDragActive])

  return null
}
