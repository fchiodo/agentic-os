import { invoke } from '@tauri-apps/api/core'
import { controlStatusSchema, type ControlStatus } from '@/features/control/schema'
import { isTauriRuntime } from '@/lib/tauri'

const mockControlStatus: ControlStatus = {
  pendingApprovals: 1,
  pendingMemoryProposals: 0,
  runningTasks: 1,
  spentTodayUsd: 0.27,
  auditChainOk: true,
}

export async function getControlStatus(): Promise<ControlStatus> {
  const payload = isTauriRuntime()
    ? await invoke<ControlStatus>('control_status')
    : mockControlStatus

  return controlStatusSchema.parse(payload)
}
