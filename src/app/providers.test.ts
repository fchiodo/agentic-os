import { describe, expect, it } from 'vitest'
import { createAppQueryClient } from '@/app/query-client'

describe('AppProviders query policy', () => {
  it('disables periodic and reconnect-driven query refreshes globally', () => {
    const options = createAppQueryClient().getDefaultOptions().queries

    expect(options?.refetchInterval).toBe(false)
    expect(options?.refetchOnReconnect).toBe(false)
    expect(options?.refetchOnWindowFocus).toBe(false)
    expect(options?.staleTime).toBe(Infinity)
  })
})
