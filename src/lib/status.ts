import type { TaskStatus } from '@/features/runner/schema'

type Tone = 'accent' | 'danger' | 'neutral' | 'success' | 'warning'

const taskStatusTone: Record<TaskStatus, Tone> = {
  created: 'neutral',
  classified: 'neutral',
  planned: 'neutral',
  running: 'accent',
  waiting_for_tool: 'accent',
  resuming: 'accent',
  verifying: 'accent',
  waiting_for_approval: 'warning',
  completed: 'success',
  failed: 'danger',
  cancelled: 'neutral',
  partially_completed: 'neutral',
}

const taskStatusLabel: Record<TaskStatus, string> = {
  created: 'Created',
  classified: 'Classified',
  planned: 'Planned',
  running: 'Running',
  waiting_for_tool: 'Waiting for tool',
  resuming: 'Resuming',
  verifying: 'Verifying',
  waiting_for_approval: 'Waiting for approval',
  completed: 'Completed',
  failed: 'Failed',
  cancelled: 'Cancelled',
  partially_completed: 'Partially completed',
}

export function toneForTaskStatus(status: TaskStatus): Tone {
  return taskStatusTone[status]
}

export function labelForTaskStatus(status: TaskStatus): string {
  return taskStatusLabel[status]
}

const riskTone: Record<'low' | 'medium' | 'high' | 'critical', Tone> = {
  low: 'neutral',
  medium: 'warning',
  high: 'danger',
  critical: 'danger',
}

export function toneForRisk(risk: 'low' | 'medium' | 'high' | 'critical'): Tone {
  return riskTone[risk]
}
