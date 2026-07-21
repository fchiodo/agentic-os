import { createHash } from 'node:crypto'
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { execFileSync } from 'node:child_process'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const root = resolve(scriptDir, '..')
const sourceDir = join(root, 'tools', 'markitdown-sidecar')
const buildRoot = join(root, 'src-tauri', 'target', 'markitdown-sidecar')
const binariesDir = join(root, 'src-tauri', 'binaries')
const requirements = join(sourceDir, 'requirements.txt')
const entrypoint = join(sourceDir, 'main.py')

function run(program, args, options = {}) {
  return execFileSync(program, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
    ...options,
  })
}

function findPython() {
  const candidates = process.env.AGENTIC_OS_PYTHON
    ? [process.env.AGENTIC_OS_PYTHON]
    : process.platform === 'win32'
      ? ['python', 'python3']
      : ['python3.12', 'python3.11', 'python3.10', 'python3']
  for (const candidate of candidates) {
    try {
      const version = run(candidate, [
        '-c',
        'import sys; assert (3, 10) <= sys.version_info[:2] < (3, 15); print(f"{sys.version_info.major}.{sys.version_info.minor}")',
      ], { capture: true }).trim()
      return { executable: candidate, version }
    } catch {
      // Try the next interpreter. The sidecar build is intentionally strict.
    }
  }
  throw new Error('MarkItDown sidecar build requires Python 3.10 through 3.14.')
}

function hostTriple() {
  const rustc = process.env.RUSTC || 'rustc'
  const details = run(rustc, ['-Vv'], { capture: true })
  const host = details.split('\n').find((line) => line.startsWith('host: '))
  if (!host) throw new Error('Could not determine the Rust host target triple.')
  return host.slice('host: '.length).trim()
}

const python = findPython()
const triple = process.env.AGENTIC_OS_TARGET_TRIPLE || hostTriple()
const extension = process.platform === 'win32' ? '.exe' : ''
const destination = join(binariesDir, `markitdown-sidecar-${triple}${extension}`)
const fingerprint = createHash('sha256')
  .update(readFileSync(requirements))
  .update(readFileSync(entrypoint))
  .update(readFileSync(fileURLToPath(import.meta.url)))
  .update(`${process.platform}:${process.arch}:${triple}:python-${python.version}`)
  .digest('hex')
const stamp = join(buildRoot, 'fingerprint')

if (existsSync(destination) && existsSync(stamp) && readFileSync(stamp, 'utf8').trim() === fingerprint) {
  console.log(`MarkItDown sidecar is current: ${destination}`)
  process.exit(0)
}

rmSync(buildRoot, { recursive: true, force: true })
mkdirSync(buildRoot, { recursive: true })
mkdirSync(binariesDir, { recursive: true })

const venv = join(buildRoot, 'venv')
run(python.executable, ['-m', 'venv', venv])
const venvPython = process.platform === 'win32'
  ? join(venv, 'Scripts', 'python.exe')
  : join(venv, 'bin', 'python')
run(venvPython, [
  '-m',
  'pip',
  'install',
  '--disable-pip-version-check',
  '--requirement',
  requirements,
])

const dist = join(buildRoot, 'dist')
run(venvPython, [
  '-m',
  'PyInstaller',
  '--noconfirm',
  '--clean',
  '--onefile',
  '--name',
  'markitdown-sidecar',
  '--distpath',
  dist,
  '--workpath',
  join(buildRoot, 'work'),
  '--specpath',
  join(buildRoot, 'spec'),
  '--collect-all',
  'markitdown',
  '--collect-all',
  'pdfminer',
  '--collect-all',
  'pdfplumber',
  entrypoint,
])

const built = join(dist, `markitdown-sidecar${extension}`)
if (!existsSync(built)) throw new Error(`PyInstaller did not create ${built}`)
copyFileSync(built, destination)
if (process.platform !== 'win32') chmodSync(destination, 0o755)
writeFileSync(stamp, `${fingerprint}\n`)
console.log(`Built MarkItDown ${readFileSync(requirements, 'utf8').split('\n')[0]} sidecar: ${destination}`)
