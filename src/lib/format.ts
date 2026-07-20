export function formatCompactNumber(value: number) {
  return new Intl.NumberFormat('en-US', {
    maximumFractionDigits: 1,
    notation: value >= 10_000 ? 'compact' : 'standard',
  }).format(value)
}

export function formatDateTime(value: number) {
  return new Intl.DateTimeFormat('en-US', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(value)
}

export function formatRelativeTime(value: number) {
  const diffInSeconds = Math.round((value - Date.now()) / 1000)
  const absSeconds = Math.abs(diffInSeconds)

  if (absSeconds < 60) {
    return 'just now'
  }

  if (absSeconds < 3_600) {
    return new Intl.RelativeTimeFormat('en-US', {
      numeric: 'auto',
    }).format(Math.round(diffInSeconds / 60), 'minute')
  }

  if (absSeconds < 86_400) {
    return new Intl.RelativeTimeFormat('en-US', {
      numeric: 'auto',
    }).format(Math.round(diffInSeconds / 3_600), 'hour')
  }

  return new Intl.RelativeTimeFormat('en-US', {
    numeric: 'auto',
  }).format(Math.round(diffInSeconds / 86_400), 'day')
}
