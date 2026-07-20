import { invoke } from '@tauri-apps/api/core'
import { mockTaskDetails, mockTasks } from '@/features/runner/mock-data'
import {
  taskDetailSchema,
  taskEventSchema,
  taskSummarySchema,
  type TaskDetail,
  type TaskEvent,
  type TaskSubmitRequest,
  type TaskSummary,
} from '@/features/runner/schema'
import { isTauriRuntime } from '@/lib/tauri'

export async function listTasks(): Promise<TaskSummary[]> {
  const payload = isTauriRuntime()
    ? await invoke<TaskSummary[]>('tasks_list')
    : mockTasks

  return taskSummarySchema.array().parse(payload)
}

export async function getTask(id: string): Promise<TaskDetail> {
  const payload = isTauriRuntime()
    ? await invoke<TaskDetail>('tasks_get', { id })
    : (mockTaskDetails[id] ?? mockTaskDetails['task-1'])

  return taskDetailSchema.parse(payload)
}

export async function getTaskEventsSince(id: string, sinceSeq: number): Promise<TaskEvent[]> {
  if (!isTauriRuntime()) {
    return []
  }

  const payload = await invoke<TaskEvent[]>('tasks_events_since', { id, sinceSeq })
  return taskEventSchema.array().parse(payload)
}

export async function submitTask(request: TaskSubmitRequest): Promise<TaskSummary> {
  if (!isTauriRuntime()) {
    const created: TaskSummary = {
      id: `task-mock-${Date.now()}`,
      title: request.goal.slice(0, 64),
      goal: request.goal,
      domain: request.domain ?? 'work',
      agentId: request.agentId ?? null,
      harness: 'codex',
      status: 'planned',
      originKind: 'manual',
      ontologyCategoryId: null,
      currentStep: 0,
      stepCount: 3,
      costTokens: 0,
      costUsd: null,
      pendingApprovalId: null,
      riskLevel: 'low',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    }
    return created
  }

  const payload = await invoke<TaskSummary>('tasks_submit', { request })
  return taskSummarySchema.parse(payload)
}

export async function cancelTask(id: string): Promise<TaskSummary> {
  const payload = isTauriRuntime()
    ? await invoke<TaskSummary>('tasks_cancel', { id })
    : mockTasks[0]

  return taskSummarySchema.parse(payload)
}
