import { invoke } from '@tauri-apps/api/core'
import { controlStatusSchema, type ControlStatus } from '@/features/control/schema'
import { isTauriRuntime } from '@/lib/tauri'

const mockControlStatus: ControlStatus = {
  pendingMemoryProposals: 0,
}

export async function getControlStatus(): Promise<ControlStatus> {
  const payload = isTauriRuntime()
    ? await invoke<ControlStatus>('control_status')
    : mockControlStatus

  return controlStatusSchema.parse(payload)
}
