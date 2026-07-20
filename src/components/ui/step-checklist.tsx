import { Check, Circle, X } from 'lucide-react'
import type { TaskStep } from '@/features/runner/schema'

type StepChecklistProps = {
  max?: number
  steps: TaskStep[]
}

export function StepChecklist({ max = 6, steps }: StepChecklistProps) {
  const visible = steps.slice(0, max)
  const hiddenCount = steps.length - visible.length

  return (
    <ul className="step-checklist">
      {visible.map((step) => (
        <li key={step.index} className={`step-row step-row--${step.status}`}>
          <span aria-hidden="true" className="step-row-icon">
            {step.status === 'done' ? (
              <Check size={14} />
            ) : step.status === 'failed' ? (
              <X size={14} />
            ) : step.status === 'active' ? (
              <span className="step-row-dot" />
            ) : (
              <Circle size={14} />
            )}
          </span>
          <span className="step-row-title">{step.title}</span>
        </li>
      ))}
      {hiddenCount > 0 ? (
        <li className="step-row step-row--pending step-row--more">
          <span className="step-row-icon" aria-hidden="true">
            <Circle size={14} />
          </span>
          <span className="step-row-title">{hiddenCount} more</span>
        </li>
      ) : null}
    </ul>
  )
}
