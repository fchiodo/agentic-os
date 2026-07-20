type NavBadgeProps = {
  count: number
}

export function NavBadge({ count }: NavBadgeProps) {
  if (count <= 0) {
    return null
  }

  return <span className="nav-badge">{count > 99 ? '99+' : count}</span>
}
