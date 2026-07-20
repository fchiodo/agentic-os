import { invoke } from '@tauri-apps/api/core'
import { mockApprovals } from '@/features/approvals/mock-data'
import {
  approvalRequestSchema,
  type ApprovalDecisionInput,
  type ApprovalRequest,
} from '@/features/approvals/schema'
import { isTauriRuntime } from '@/lib/tauri'

export async function listApprovals(): Promise<ApprovalRequest[]> {
  const payload = isTauriRuntime()
    ? await invoke<ApprovalRequest[]>('approvals_list')
    : mockApprovals

  return approvalRequestSchema.array().parse(payload)
}

export async function decideApproval(decision: ApprovalDecisionInput): Promise<ApprovalRequest> {
  const payload = isTauriRuntime()
    ? await invoke<ApprovalRequest>('approvals_decide', { decision })
    : (mockApprovals.find((item) => item.id === decision.id) ?? mockApprovals[0])

  return approvalRequestSchema.parse(payload)
}
