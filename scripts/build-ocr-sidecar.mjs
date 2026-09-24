import { createHash } from 'node:crypto'
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { execFileSync, spawnSync } from 'node:child_process'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const root = resolve(scriptDir, '..')
const sourceDir = join(root, 'tools', 'ocr-sidecar')
const buildRoot = join(root, 'src-tauri', 'target', 'ocr-sidecar')
const binariesDir = join(root, 'src-tauri', 'binaries')
const requirements = join(sourceDir, 'requirements.lock')
const entrypoint = join(sourceDir, 'main.py')
const requiredPythonVersion = readFileSync(join(sourceDir, '.python-version'), 'utf8').trim()
const triple = 'aarch64-apple-darwin'
const destination = join(binariesDir, `ocr-sidecar-${triple}`)

function run(program, args, options = {}) {
  return execFileSync(program, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
    ...options,
  })
}

function assertAppleSilicon() {
  if (process.platform !== 'darwin') {
    throw new Error('OCR sidecar v1 can only be built on macOS.')
  }
  const machine = run('uname', ['-m'], { capture: true }).trim()
  if (machine !== 'arm64' || process.arch !== 'arm64') {
    throw new Error(`OCR sidecar v1 requires native Apple Silicon; found ${machine}/${process.arch}.`)
  }
}

function findPython() {
  const candidates = process.env.AGENTIC_OS_OCR_PYTHON
    ? [process.env.AGENTIC_OS_OCR_PYTHON]
    : ['python3.10', '/opt/homebrew/bin/python3.10', 'python3']
  for (const candidate of candidates) {
    try {
      const details = JSON.parse(run(candidate, [
        '-c',
        'import json, platform, sys; print(json.dumps({"version": platform.python_version(), "major": sys.version_info.major, "minor": sys.version_info.minor, "machine": platform.machine()}))',
      ], { capture: true }))
      if (details.version === requiredPythonVersion && details.machine === 'arm64') {
        return { executable: candidate, version: details.version }
      }
    } catch {
      // Try the next explicitly supported interpreter.
    }
  }
  throw new Error(
    `OCR sidecar build requires native arm64 Python ${requiredPythonVersion}. ` +
    'Set AGENTIC_OS_OCR_PYTHON to its absolute path.',
  )
}

function sourceFiles(directory) {
  return readdirSync(directory)
    .filter((name) => name.endsWith('.py') || name.endsWith('.lock') || name.endsWith('.in'))
    .sort()
    .map((name) => join(directory, name))
}

function verifyHealth(binary) {
  const input = [
    JSON.stringify({ protocolVersion: 1, requestId: 'build-health', command: 'health' }),
    JSON.stringify({ protocolVersion: 1, requestId: 'build-shutdown', command: 'shutdown' }),
    '',
  ].join('\n')
  const result = spawnSync(binary, [], {
    cwd: root,
    encoding: 'utf8',
    input,
    env: {
      PATH: '/usr/bin:/bin',
      HOME: process.env.HOME,
      HF_HUB_OFFLINE: '1',
      TRANSFORMERS_OFFLINE: '1',
    },
    timeout: 60_000,
  })
  if (result.error || result.status !== 0) {
    throw new Error(`OCR sidecar health check failed: ${result.error?.message || result.stderr}`)
  }
  const responses = result.stdout.trim().split('\n').map((line) => JSON.parse(line))
  const health = responses.find((response) => response.type === 'health')
  if (
    health?.status !== 'ok' ||
    health?.protocolVersion !== 1 ||
    health?.architecture !== 'arm64'
  ) {
    throw new Error(`OCR sidecar returned an incompatible health response: ${result.stdout}`)
  }
}

assertAppleSilicon()
const python = findPython()
const fingerprint = createHash('sha256')
for (const file of [
  ...sourceFiles(sourceDir),
  join(sourceDir, '.python-version'),
  fileURLToPath(import.meta.url),
]) {
  fingerprint.update(readFileSync(file))
}
fingerprint.update(`${process.platform}:${process.arch}:${triple}:python-${python.version}`)
const expectedFingerprint = fingerprint.digest('hex')
const stamp = join(buildRoot, 'fingerprint')
const dependencyFingerprint = createHash('sha256')
  .update(readFileSync(requirements))
  .update(`${process.platform}:${process.arch}:${python.version}`)
  .digest('hex')
const dependencyStamp = join(buildRoot, 'dependency-fingerprint')

if (
  existsSync(destination) &&
  existsSync(stamp) &&
  readFileSync(stamp, 'utf8').trim() === expectedFingerprint
) {
  verifyHealth(destination)
  console.log('OCR sidecar up to date — skipping build')
  process.exit(0)
}

mkdirSync(buildRoot, { recursive: true })
mkdirSync(binariesDir, { recursive: true })

const venv = join(buildRoot, 'venv')
const venvPython = join(venv, 'bin', 'python')
if (
  !existsSync(venvPython) ||
  !existsSync(dependencyStamp) ||
  readFileSync(dependencyStamp, 'utf8').trim() !== dependencyFingerprint
) {
  rmSync(venv, { recursive: true, force: true })
  run(python.executable, ['-m', 'venv', venv])
  run(venvPython, [
    '-m',
    'pip',
    'install',
    '--disable-pip-version-check',
    '--require-hashes',
    '--requirement',
    requirements,
  ])
  writeFileSync(dependencyStamp, `${dependencyFingerprint}\n`)
} else {
  console.log('OCR Python environment up to date — reusing it')
}

const dist = join(buildRoot, 'dist')
rmSync(dist, { recursive: true, force: true })
rmSync(join(buildRoot, 'work'), { recursive: true, force: true })
rmSync(join(buildRoot, 'spec'), { recursive: true, force: true })
run(venvPython, [
  '-m',
  'PyInstaller',
  '--noconfirm',
  '--clean',
  '--onefile',
  '--name',
  'ocr-sidecar',
  '--distpath',
  dist,
  '--workpath',
  join(buildRoot, 'work'),
  '--specpath',
  join(buildRoot, 'spec'),
  '--paths',
  sourceDir,
  '--collect-all',
  'mlx',
  '--collect-data',
  'mlx_vlm',
  '--collect-data',
  'transformers',
  '--hidden-import',
  'mlx_vlm.models.paddleocr_vl',
  '--copy-metadata',
  'mlx',
  '--copy-metadata',
  'mlx-vlm',
  '--exclude-module',
  'scipy',
  entrypoint,
])

const built = join(dist, 'ocr-sidecar')
if (!existsSync(built) || statSync(built).size < 1024 * 1024) {
  throw new Error(`PyInstaller did not create a plausible executable at ${built}`)
}
const architectures = run('lipo', ['-archs', built], { capture: true }).trim().split(/\s+/)
if (!architectures.includes('arm64')) {
  throw new Error(`OCR sidecar is not arm64: ${architectures.join(', ')}`)
}

copyFileSync(built, destination)
chmodSync(destination, 0o755)
verifyHealth(destination)
writeFileSync(stamp, `${expectedFingerprint}\n`)
console.log(`Built OCR sidecar (${Math.ceil(statSync(destination).size / 1024 / 1024)} MiB): ${destination}`)
