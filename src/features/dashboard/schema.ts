import { z } from 'zod'

export const catalogKindSchema = z.enum([
  'agent',
  'automation',
  'mcp',
  'plugin',
  'prompt',
  'routine',
  'skill',
  'workflow',
])

export const sourceStatusSchema = z.enum(['available', 'missing'])

export const catalogItemSchema = z.object({
  id: z.string(),
  kind: catalogKindSchema,
  name: z.string(),
  displayName: z.string(),
  summary: z.string().nullable(),
  path: z.string(),
  origin: z.string(),
  group: z.string(),
  tags: z.array(z.string()),
  version: z.string().nullable(),
  category: z.string().nullable(),
  updatedAt: z.number().nullable(),
  provider: z.string(),
  detector: z.string(),
  entrypoint: z.string().nullable(),
  confidence: z.number(),
})

export const catalogCountsSchema = z.object({
  agent: z.number(),
  automation: z.number(),
  mcp: z.number(),
  plugin: z.number(),
  prompt: z.number(),
  routine: z.number(),
  skill: z.number(),
  workflow: z.number(),
})

export const catalogSectionSchema = z.object({
  counts: catalogCountsSchema,
  items: z.array(catalogItemSchema),
  totalItems: z.number(),
})

export const sourceDescriptorSchema = z.object({
  id: z.string(),
  label: z.string(),
  kind: z.string(),
  path: z.string(),
  status: sourceStatusSchema,
})

export const runtimeInfoSchema = z.object({
  codexHome: z.string(),
  platform: z.string(),
})

export const dashboardSnapshotSchema = z.object({
  generatedAt: z.number(),
  catalog: catalogSectionSchema,
  sources: z.array(sourceDescriptorSchema),
  runtime: runtimeInfoSchema,
})

export type CatalogItem = z.infer<typeof catalogItemSchema>
export type CatalogKind = z.infer<typeof catalogKindSchema>
export type DashboardSnapshot = z.infer<typeof dashboardSnapshotSchema>
