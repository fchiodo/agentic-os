type ComingSoonPageProps = {
  body: string
  eyebrow: string
  title?: string
}

export function ComingSoonPage({
  body,
  eyebrow,
  title = 'Coming soon',
}: ComingSoonPageProps) {
  return (
    <section className="page-section coming-soon-page">
      <section className="surface coming-soon-surface">
        <div className="coming-soon-copy">
          <p className="eyebrow">{eyebrow}</p>
          <h2>{title}</h2>
          <p>{body}</p>
        </div>
      </section>
    </section>
  )
}
