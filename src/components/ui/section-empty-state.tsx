type SectionEmptyStateProps = {
  body: string
  title: string
}

export function SectionEmptyState({
  body,
  title,
}: SectionEmptyStateProps) {
  return (
    <div className="empty-state">
      <h3>{title}</h3>
      <p>{body}</p>
    </div>
  )
}
