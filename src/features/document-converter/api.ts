import { invoke } from '@tauri-apps/api/core'
import {
  conversionJobSchema,
  converterStatusSchema,
  createJobsResponseSchema,
  markdownPreviewSchema,
  modelStatusSchema,
  selectedDocumentSchema,
  type ConversionJob,
  type ConversionOptions,
  type ConverterStatus,
  type CreateJobsResponse,
  type MarkdownPreviewData,
  type ModelStatus,
  type SelectedDocument,
} from './schema'

export function converterError(error: unknown): Error {
  const raw = error instanceof Error ? error.message : String(error)
  const separator = raw.indexOf('|')
  const code = separator >= 0 ? raw.slice(0, separator) : 'DOCUMENT_CONVERTER_ERROR'
  const message = separator >= 0 ? raw.slice(separator + 1) : raw
  return Object.assign(new Error(message), { code })
}

async function native<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    throw converterError(error)
  }
}

export async function getConverterStatus(): Promise<ConverterStatus> {
  return converterStatusSchema.parse(await native('document_converter_get_status'))
}

export async function getModelStatus(): Promise<ModelStatus> {
  return modelStatusSchema.parse(await native('document_converter_get_model_status'))
}

export async function installModel(): Promise<ModelStatus> {
  return modelStatusSchema.parse(await native('document_converter_install_model'))
}

export async function repairModel(): Promise<ModelStatus> {
  return modelStatusSchema.parse(await native('document_converter_repair_model'))
}

export async function removeModel(): Promise<ModelStatus> {
  return modelStatusSchema.parse(await native('document_converter_remove_model'))
}

export async function cancelModelDownload(): Promise<void> {
  await native('document_converter_cancel_model_download')
}

export async function chooseFiles(): Promise<SelectedDocument[]> {
  return selectedDocumentSchema.array().parse(await native('document_converter_choose_files'))
}

export async function inspectPaths(paths: string[]): Promise<SelectedDocument[]> {
  return selectedDocumentSchema.array().parse(
    await native('document_converter_inspect_paths', { paths }),
  )
}

export async function chooseDestination(): Promise<string | null> {
  return native<string | null>('document_converter_choose_destination')
}

export async function createJobs(request: {
  inputPaths: string[]
  destinationRoot: string | null
  options: ConversionOptions
}): Promise<CreateJobsResponse> {
  return createJobsResponseSchema.parse(
    await native('document_converter_create_jobs', { request }),
  )
}

export async function listJobs(limit = 50): Promise<ConversionJob[]> {
  return conversionJobSchema.array().parse(
    await native('document_converter_list_jobs', { limit }),
  )
}

export async function cancelJob(jobId: string): Promise<ConversionJob> {
  return conversionJobSchema.parse(
    await native('document_converter_cancel_job', { jobId }),
  )
}

export async function cancelAll(): Promise<void> {
  await native('document_converter_cancel_all')
}

export async function retryJob(jobId: string): Promise<ConversionJob> {
  return conversionJobSchema.parse(
    await native('document_converter_retry_job', { jobId }),
  )
}

export async function deleteHistoryEntry(jobId: string): Promise<void> {
  await native('document_converter_delete_history_entry', { jobId })
}

export async function getPreview(jobId: string): Promise<MarkdownPreviewData> {
  return markdownPreviewSchema.parse(
    await native('document_converter_get_preview', { jobId }),
  )
}

export async function readAsset(jobId: string, relativePath: string): Promise<string> {
  const asset = await native<{ mimeType: string; base64: string }>(
    'document_converter_read_asset',
    { jobId, relativePath },
  )
  return `data:${asset.mimeType};base64,${asset.base64}`
}

export async function openOutput(jobId: string, revealMarkdown = false): Promise<void> {
  await native('document_converter_open_output', { jobId, revealMarkdown })
}

export async function importToMemory(jobId: string, domain: string): Promise<void> {
  await native('document_converter_import_to_memory', {
    request: { jobId, domain },
  })
}
