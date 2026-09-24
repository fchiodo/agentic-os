import { existsSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const binary = join(root, 'src-tauri', 'binaries', 'ocr-sidecar-aarch64-apple-darwin')
if (!existsSync(binary)) {
  throw new Error('OCR sidecar is missing. Run pnpm prepare:ocr first.')
}

const requests = [
  { protocolVersion: 1, requestId: 'check-health', command: 'health' },
  { protocolVersion: 1, requestId: 'check-capabilities', command: 'capabilities' },
]
const modelPath = process.env.AGENTIC_OS_OCR_MODEL
const imagePath = process.env.AGENTIC_OS_OCR_IMAGE
if (modelPath && imagePath) {
  requests.push({
    protocolVersion: 1,
    requestId: 'check-inference',
    command: 'convert-image',
    modelPath,
    imagePath,
    maxTokens: 256,
  })
}
requests.push({ protocolVersion: 1, requestId: 'check-shutdown', command: 'shutdown' })

const result = spawnSync(binary, [], {
  cwd: root,
  encoding: 'utf8',
  input: `${requests.map((request) => JSON.stringify(request)).join('\n')}\n`,
  env: {
    PATH: '/usr/bin:/bin',
    HOME: process.env.HOME,
    HF_HUB_OFFLINE: '1',
    TRANSFORMERS_OFFLINE: '1',
  },
  timeout: modelPath && imagePath ? 300_000 : 60_000,
})
if (result.error || result.status !== 0) {
  throw new Error(result.error?.message || result.stderr || `sidecar exited ${result.status}`)
}

const messages = result.stdout.trim().split('\n').map((line) => JSON.parse(line))
for (const message of messages) {
  if (message.type === 'error') throw new Error(`${message.code}: ${message.message}`)
}
const health = messages.find((message) => message.type === 'health')
if (health?.status !== 'ok' || health?.protocolVersion !== 1) {
  throw new Error(`Incompatible health response: ${result.stdout}`)
}
const completed = messages.find((message) => message.requestId === 'check-inference')
if (modelPath && imagePath && (!completed?.text || completed.type !== 'completed')) {
  throw new Error('Real OCR smoke test did not return text')
}

console.log(JSON.stringify({ health, realInference: Boolean(completed?.text) }, null, 2))
