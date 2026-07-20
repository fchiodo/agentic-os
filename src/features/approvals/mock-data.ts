import type { ApprovalRequest } from '@/features/approvals/schema'

export const mockApprovals: ApprovalRequest[] = [
  {
    id: 'approval-2',
    taskId: 'task-2',
    taskTitle: 'Implement SSE in the newsletter-ai BFF',
    domain: 'work',
    toolName: 'codex.exec',
    actionSummary: 'Task goal mentions a high-risk action (push, delete, send, deploy, or similar)',
    riskLevel: 'high',
    preview: {
      kind: 'text',
      content:
        'Push branch feature/sse-bff with the SSE streaming implementation for the newsletter-ai BFF.',
    },
    requestedAt: new Date(Date.now() - 8 * 60_000).toISOString(),
  },
]
