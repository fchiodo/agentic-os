import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { MarkdownPreview } from './markdown-preview'

vi.mock('../api', () => ({
  readAsset: vi.fn().mockResolvedValue('data:image/png;base64,AA=='),
}))

afterEach(cleanup)

describe('MarkdownPreview', () => {
  it('renders structure without executing raw HTML or remote resources', () => {
    const { container } = render(
      <MarkdownPreview
        jobId="job-1"
        markdown={'# Report\n\n<script>window.pwned = true</script>\n\n![remote](https://example.com/tracker.png)\n\n[Website](https://example.com)'}
      />,
    )
    expect(screen.getByRole('heading', { name: 'Report' })).toBeTruthy()
    expect(container.querySelector('script')).toBeNull()
    expect(container.querySelector('a')).toBeNull()
    expect(container.querySelector('img[src^="http"]')).toBeNull()
    expect(screen.getByText(/<script>window\.pwned/)).toBeTruthy()
  })

  it('renders tables as semantic local markup', () => {
    render(
      <MarkdownPreview
        jobId="job-1"
        markdown={'| Region | Revenue |\n| --- | ---: |\n| EMEA | €2.4B |'}
      />,
    )
    expect(screen.getByRole('table')).toBeTruthy()
    expect(screen.getByText('EMEA')).toBeTruthy()
  })
})
