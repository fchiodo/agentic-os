import { Info } from 'lucide-react'
import { clsx } from 'clsx'

type InfoTooltipProps = {
  align?: 'center' | 'end'
  className?: string
  content: string
}

export function InfoTooltip({
  align = 'center',
  className,
  content,
}: InfoTooltipProps) {
  return (
    <span className={clsx('info-tooltip', `info-tooltip--${align}`, className)}>
      <span aria-hidden="true" className="info-tooltip__trigger">
        <Info size={12} />
      </span>
      <span className="info-tooltip__bubble" role="tooltip">
        {content}
      </span>
    </span>
  )
}
