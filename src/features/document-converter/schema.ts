import { z } from 'zod'

export const conversionOptionsSchema = z.object({
  processingMode: z.enum(['automatic', 'force-ocr', 'digital-only']),
  maxTokensPerPage: z.number().int().min(256).max(8192),
  preservePageImages: z.boolean(),
})

export type ConversionOptions = z.infer<typeof conversionOptionsSchema>

export const modelStatusSchema = z.object({
  id: z.string(),
  displayName: z.string(),
  version: z.string(),
  state: z.string(),
  installed: z.boolean(),
  checksumValid: z.boolean(),
  downloadSizeBytes: z.number().nonnegative(),
  installedSizeBytes: z.number().nonnegative(),
  installedPath: z.string().nullable(),
  architecture: z.string(),
  license: z.string(),
  errorCode: z.string().nullable(),
  errorMessage: z.string().nullable(),
})

export type ModelStatus = z.infer<typeof modelStatusSchema>

export const converterStatusSchema = z.object({
  ready: z.boolean(),
  architecture: z.string(),
  sidecarVersion: z.string(),
  protocolVersion: z.number().int(),
  engine: z.string(),
  engineVersion: z.string(),
  model: modelStatusSchema,
  activeJobs: z.number().int().nonnegative(),
  queuedJobs: z.number().int().nonnegative(),
  keepWarmSeconds: z.number().int().nonnegative(),
  localOnly: z.boolean(),
})

export type ConverterStatus = z.infer<typeof converterStatusSchema>

export const selectedDocumentSchema = z.object({
  path: z.string(),
  name: z.string(),
  sizeBytes: z.number().nonnegative(),
  pageCount: z.number().int().positive().nullable(),
  supported: z.boolean(),
  errorCode: z.string().nullable(),
  errorMessage: z.string().nullable(),
})

export type SelectedDocument = z.infer<typeof selectedDocumentSchema>

export const conversionJobSchema = z.object({
  id: z.string().uuid(),
  sourceName: z.string(),
  sourcePath: z.string(),
  sourceHash: z.string(),
  sourceSizeBytes: z.number().nonnegative(),
  destinationRoot: z.string(),
  outputPath: z.string().nullable(),
  markdownPath: z.string().nullable(),
  jsonPath: z.string().nullable(),
  engine: z.string(),
  engineVersion: z.string(),
  modelVersion: z.string(),
  status: z.string(),
  stage: z.string(),
  stageLabel: z.string().nullable(),
  totalPages: z.number().int().nonnegative().nullable(),
  processedPages: z.number().int().nonnegative(),
  pagesDigital: z.number().int().nonnegative(),
  pagesOcr: z.number().int().nonnegative(),
  assetCount: z.number().int().nonnegative(),
  processingMode: z.string(),
  options: conversionOptionsSchema,
  fingerprint: z.string(),
  warnings: z.array(z.string()),
  createdAt: z.string(),
  startedAt: z.string().nullable(),
  completedAt: z.string().nullable(),
  errorCode: z.string().nullable(),
  errorMessage: z.string().nullable(),
})

export type ConversionJob = z.infer<typeof conversionJobSchema>

export const createJobsResponseSchema = z.object({
  jobs: z.array(conversionJobSchema),
  rejected: z.array(z.object({
    path: z.string(),
    name: z.string(),
    code: z.string(),
    message: z.string(),
  })),
  duplicates: z.array(z.object({
    sourcePath: z.string(),
    previousJob: conversionJobSchema,
  })),
})

export type CreateJobsResponse = z.infer<typeof createJobsResponseSchema>

export const markdownPreviewSchema = z.object({
  jobId: z.string(),
  sourceName: z.string(),
  markdown: z.string(),
  outputPath: z.string(),
  assetCount: z.number().int().nonnegative(),
})

export type MarkdownPreviewData = z.infer<typeof markdownPreviewSchema>

export const modelProgressSchema = z.object({
  modelId: z.string(),
  stage: z.string(),
  downloadedBytes: z.number().nonnegative(),
  totalBytes: z.number().nonnegative(),
  percent: z.number().min(0).max(100),
  currentFile: z.string().nullable(),
})

export type ModelProgress = z.infer<typeof modelProgressSchema>

export const conversionProgressSchema = z.object({
  jobId: z.string(),
  sourceName: z.string(),
  status: z.string(),
  stage: z.string(),
  label: z.string(),
  page: z.number().int().nonnegative().nullable(),
  totalPages: z.number().int().nonnegative().nullable(),
  percent: z.number().min(0).max(100).nullable(),
})

export type ConversionProgress = z.infer<typeof conversionProgressSchema>
