type DiffViewProps = {
  unifiedDiff: string
}

function lineToneClass(line: string): string {
  if (line.startsWith('+++') || line.startsWith('---')) {
    return 'diff-line diff-line--file'
  }
  if (line.startsWith('@@')) {
    return 'diff-line diff-line--hunk'
  }
  if (line.startsWith('+')) {
    return 'diff-line diff-line--add'
  }
  if (line.startsWith('-')) {
    return 'diff-line diff-line--remove'
  }
  return 'diff-line'
}

export function DiffView({ unifiedDiff }: DiffViewProps) {
  const lines = unifiedDiff.split('\n')

  return (
    <div className="code-panel diff-view">
      {lines.map((line, index) => (
        <div key={`${index}-${line}`} className={lineToneClass(line)}>
          {line.length > 0 ? line : ' '}
        </div>
      ))}
    </div>
  )
}
