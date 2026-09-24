import { createHash } from 'node:crypto'
import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
} from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { execFileSync } from 'node:child_process'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const root = resolve(scriptDir, '..')
const manifestPath = join(root, 'tools', 'ocr-sidecar', 'python-runtime.json')
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
const managedRoot = join(root, '.build', 'ocr-python')
const runtimeDirectory = join(
  managedRoot,
  `${manifest.pythonVersion}-${manifest.build}-${manifest.architecture}`,
)
const managedPython = join(runtimeDirectory, 'bin', 'python3')

function run(program, args, options = {}) {
  return execFileSync(program, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
    ...options,
  })
}

function assertManifest() {
  const requiredPython = readFileSync(
    join(root, 'tools', 'ocr-sidecar', '.python-version'),
    'utf8',
  ).trim()
  if (
    manifest.schemaVersion !== 1 ||
    manifest.pythonVersion !== requiredPython ||
    manifest.architecture !== 'aarch64-apple-darwin' ||
    !manifest.downloadUrl.startsWith('https://') ||
    !Number.isSafeInteger(manifest.downloadSize) ||
    !/^[a-f0-9]{64}$/.test(manifest.sha256)
  ) {
    throw new Error(`Invalid OCR Python runtime manifest: ${manifestPath}`)
  }
}

export function inspectOcrPython(executable) {
  try {
    const details = JSON.parse(run(executable, [
      '-c',
      'import json, platform; print(json.dumps({"version": platform.python_version(), "machine": platform.machine()}))',
    ], { capture: true }))
    if (details.version === manifest.pythonVersion && details.machine === 'arm64') {
      return { executable, version: details.version, managed: executable === managedPython }
    }
  } catch {
    // The caller decides whether an invalid candidate is fatal or can be skipped.
  }
  return null
}

export function ensureManagedOcrPython() {
  assertManifest()
  const existing = inspectOcrPython(managedPython)
  if (existing) {
    console.log(`Managed OCR Python ${existing.version} is ready — skipping download`)
    return existing
  }

  if (existsSync(runtimeDirectory)) {
    rmSync(runtimeDirectory, { recursive: true, force: true })
  }
  mkdirSync(managedRoot, { recursive: true })

  const operationId = `${process.pid}-${Date.now()}`
  const archive = join(managedRoot, `.python-${operationId}.tar.gz`)
  const staging = join(managedRoot, `.install-${operationId}`)
  mkdirSync(staging, { recursive: true })

  try {
    console.log(
      `Downloading private OCR build runtime: CPython ${manifest.pythonVersion} ` +
      `(${Math.ceil(manifest.downloadSize / 1024 / 1024)} MiB)`,
    )
    run('curl', [
      '--fail',
      '--location',
      '--retry',
      '3',
      '--retry-all-errors',
      '--output',
      archive,
      manifest.downloadUrl,
    ])

    const size = statSync(archive).size
    if (size !== manifest.downloadSize) {
      throw new Error(
        `OCR Python download size mismatch: expected ${manifest.downloadSize}, received ${size}.`,
      )
    }
    const digest = createHash('sha256').update(readFileSync(archive)).digest('hex')
    if (digest !== manifest.sha256) {
      throw new Error(
        `OCR Python checksum mismatch: expected ${manifest.sha256}, received ${digest}.`,
      )
    }

    const entries = run('tar', ['-tzf', archive], { capture: true })
      .split('\n')
      .filter(Boolean)
    if (
      entries.length === 0 ||
      entries.some((entry) =>
        entry.startsWith('/') ||
        entry.split('/').includes('..') ||
        (entry !== 'python' && !entry.startsWith('python/')),
      )
    ) {
      throw new Error('OCR Python archive contains an unsafe path.')
    }

    run('tar', ['-xzf', archive, '-C', staging])
    const stagedRuntime = join(staging, 'python')
    const stagedPython = join(stagedRuntime, 'bin', 'python3')
    if (!inspectOcrPython(stagedPython)) {
      throw new Error('Downloaded OCR Python runtime failed version or architecture validation.')
    }
    renameSync(stagedRuntime, runtimeDirectory)
    const installed = inspectOcrPython(managedPython)
    if (!installed) {
      throw new Error('Installed OCR Python runtime failed final validation.')
    }
    console.log(`Managed OCR Python installed: ${managedPython}`)
    return installed
  } finally {
    rmSync(archive, { force: true })
    rmSync(staging, { recursive: true, force: true })
  }
}

export function getOcrPythonManifest() {
  assertManifest()
  return manifest
}

const invokedDirectly = process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
if (invokedDirectly) {
  ensureManagedOcrPython()
}
