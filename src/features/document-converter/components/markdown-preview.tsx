import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { readAsset } from '../api'

function LocalAsset({ alt, jobId, path }: { alt: string; jobId: string; path: string }) {
  const [source, setSource] = useState<string | null>(null)
  useEffect(() => {
    let active = true
    if (!/^assets\/[a-zA-Z0-9._-]+$/.test(path)) return
    void readAsset(jobId, path).then((value) => {
      if (active) setSource(value)
    }).catch(() => undefined)
    return () => { active = false }
  }, [jobId, path])
  return source
    ? <img alt={alt} className="converter-preview-image" src={source} />
    : <span className="converter-preview-asset">Local image: {alt}</span>
}

function inlineText(value: string): ReactNode {
  const parts = value.split(/(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/g)
  return parts.map((part, index) => {
    if (part.startsWith('`') && part.endsWith('`')) return <code key={index}>{part.slice(1, -1)}</code>
    if (part.startsWith('**') && part.endsWith('**')) return <strong key={index}>{part.slice(2, -2)}</strong>
    const link = part.match(/^\[([^\]]+)\]\(([^)]+)\)$/)
    if (link) return <span className="converter-preview-link" key={index} title="Links are not opened automatically">{link[1]} ({link[2]})</span>
    return part
  })
}

export function MarkdownPreview({ jobId, markdown }: { jobId: string; markdown: string }) {
  const nodes = useMemo(() => {
    const lines = markdown.split(/\r?\n/)
    const output: ReactNode[] = []
    let index = 0
    while (index < lines.length) {
      const line = lines[index]
      const heading = line.match(/^(#{1,6})\s+(.+)$/)
      const image = line.match(/^!\[([^\]]*)\]\(([^)]+)\)$/)
      if (!line.trim()) {
        index += 1
        continue
      }
      if (heading) {
        const level = heading[1].length
        const content = inlineText(heading[2])
        output.push(level <= 2 ? <h2 key={index}>{content}</h2> : <h3 key={index}>{content}</h3>)
        index += 1
        continue
      }
      if (image) {
        output.push(<LocalAsset alt={image[1] || 'Document image'} jobId={jobId} key={index} path={image[2]} />)
        index += 1
        continue
      }
      if (line.startsWith('```')) {
        const code: string[] = []
        index += 1
        while (index < lines.length && !lines[index].startsWith('```')) {
          code.push(lines[index])
          index += 1
        }
        output.push(<pre key={`code-${index}`}><code>{code.join('\n')}</code></pre>)
        index += 1
        continue
      }
      if (/^[-*•]\s+/.test(line)) {
        const items: string[] = []
        while (index < lines.length && /^[-*•]\s+/.test(lines[index])) {
          items.push(lines[index].replace(/^[-*•]\s+/, ''))
          index += 1
        }
        output.push(<ul key={`list-${index}`}>{items.map((item, itemIndex) => <li key={itemIndex}>{inlineText(item)}</li>)}</ul>)
        continue
      }
      if (line.includes('|') && lines[index + 1]?.includes('|')) {
        const rows: string[][] = []
        while (index < lines.length && lines[index].includes('|')) {
          const cells = lines[index].split('|').map((cell) => cell.trim()).filter(Boolean)
          if (!cells.every((cell) => /^:?-{3,}:?$/.test(cell))) rows.push(cells)
          index += 1
        }
        output.push(<div className="converter-table-scroll" key={`table-${index}`}><table><tbody>{rows.map((row, rowIndex) => <tr key={rowIndex}>{row.map((cell, cellIndex) => <td key={cellIndex}>{inlineText(cell)}</td>)}</tr>)}</tbody></table></div>)
        continue
      }
      const paragraph = [line]
      index += 1
      while (index < lines.length && lines[index].trim() && !/^(#{1,6})\s+/.test(lines[index])) {
        paragraph.push(lines[index])
        index += 1
      }
      output.push(<p key={`paragraph-${index}`}>{inlineText(paragraph.join('\n'))}</p>)
    }
    return output
  }, [jobId, markdown])

  return <article className="converter-markdown-preview">{nodes}</article>
}
