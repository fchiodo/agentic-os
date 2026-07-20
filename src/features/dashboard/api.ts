import { invoke } from '@tauri-apps/api/core'
import { mockDashboardSnapshot } from '@/features/dashboard/mock-data'
import {
  dashboardSnapshotSchema,
  type DashboardSnapshot,
} from '@/features/dashboard/schema'
import { isTauriRuntime } from '@/lib/tauri'

export async function getDashboardSnapshot(): Promise<DashboardSnapshot> {
  const payload = isTauriRuntime()
    ? await invoke<DashboardSnapshot>('get_app_snapshot')
    : mockDashboardSnapshot

  return dashboardSnapshotSchema.parse(payload)
}

export async function refreshDashboardSnapshot(): Promise<DashboardSnapshot> {
  const payload = isTauriRuntime()
    ? await invoke<DashboardSnapshot>('refresh_app_snapshot')
    : mockDashboardSnapshot

  return dashboardSnapshotSchema.parse(payload)
}
