import { clsx } from 'clsx'
import { InfoTooltip } from '@/components/ui/info-tooltip'

type MetricCardProps = {
  hint?: string
  label: string
  tone?: 'accent' | 'neutral' | 'success' | 'warning'
  tooltipAlign?: 'center' | 'end'
  value: string
}

export function MetricCard({
  hint,
  label,
  tone = 'neutral',
  tooltipAlign = 'center',
  value,
}: MetricCardProps) {
  return (
    <article className={clsx('metric-card', `metric-card--${tone}`)}>
      <div className="metric-heading">
        <span className="metric-label">{label}</span>
        {hint ? <InfoTooltip align={tooltipAlign} content={hint} /> : null}
      </div>
      <strong className="metric-value">{value}</strong>
    </article>
  )
}
