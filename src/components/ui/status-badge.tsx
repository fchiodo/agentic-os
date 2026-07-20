import { clsx } from 'clsx'

type StatusBadgeProps = {
  label: string
  tone?: 'accent' | 'danger' | 'neutral' | 'success' | 'warning'
}

export function StatusBadge({
  label,
  tone = 'neutral',
}: StatusBadgeProps) {
  return <span className={clsx('status-badge', `status-badge--${tone}`)}>{label}</span>
}
