import fs from 'node:fs'
import path from 'node:path'

const root = process.cwd()
const versionPath = path.join(root, 'version.json')

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'))
}

function writeTextIfChanged(filePath, text) {
  const current = fs.existsSync(filePath) ? fs.readFileSync(filePath, 'utf8') : null
  if (current === text) return false
  fs.writeFileSync(filePath, text)
  return true
}

function writeJson(filePath, data) {
  return writeTextIfChanged(filePath, `${JSON.stringify(data, null, 2)}\n`)
}

function replaceJsonStringProperty(text, propertyName, value) {
  const escapedName = propertyName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const pattern = new RegExp(`("${escapedName}"\\s*:\\s*")([^"]+)(")`)
  return text.replace(pattern, `$1${value}$3`)
}

function replacePackageVersion(text, packageName, version) {
  const escapedName = packageName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const pattern = new RegExp(
    `(\\[\\[package\\]\\]\\s+name\\s*=\\s*"${escapedName}"\\s+version\\s*=\\s*")([^"]+)(")`,
    'm'
  )
  return text.replace(pattern, `$1${version}$3`)
}

const versionConfig = readJson(versionPath)
const version = String(versionConfig.version || '').trim()

if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`version.json 中的版本号无效：${version}`)
}

const packageJsonPath = path.join(root, 'package.json')
const packageJson = readJson(packageJsonPath)
packageJson.version = version
const changedFiles = []
if (writeJson(packageJsonPath, packageJson)) changedFiles.push('package.json')

const packageLockPath = path.join(root, 'package-lock.json')
if (fs.existsSync(packageLockPath)) {
  const packageLock = readJson(packageLockPath)
  packageLock.version = version
  if (packageLock.packages?.['']) {
    packageLock.packages[''].version = version
  }
  if (writeJson(packageLockPath, packageLock)) changedFiles.push('package-lock.json')
}

const tauriConfigPath = path.join(root, 'src-tauri', 'tauri.conf.json')
let tauriConfig = fs.readFileSync(tauriConfigPath, 'utf8')
tauriConfig = replaceJsonStringProperty(tauriConfig, 'version', version)
if (writeTextIfChanged(tauriConfigPath, tauriConfig)) {
  changedFiles.push(path.join('src-tauri', 'tauri.conf.json'))
}

const cargoTomlPath = path.join(root, 'src-tauri', 'Cargo.toml')
let cargoToml = fs.readFileSync(cargoTomlPath, 'utf8')
cargoToml = cargoToml.replace(
  /(^\[package\][\s\S]*?^version\s*=\s*")([^"]+)(")/m,
  `$1${version}$3`
)
if (writeTextIfChanged(cargoTomlPath, cargoToml)) {
  changedFiles.push(path.join('src-tauri', 'Cargo.toml'))
}

const cargoLockPath = path.join(root, 'src-tauri', 'Cargo.lock')
if (fs.existsSync(cargoLockPath)) {
  let cargoLock = fs.readFileSync(cargoLockPath, 'utf8')
  cargoLock = replacePackageVersion(cargoLock, 'kiro-hub', version)
  if (writeTextIfChanged(cargoLockPath, cargoLock)) {
    changedFiles.push(path.join('src-tauri', 'Cargo.lock'))
  }
}

if (changedFiles.length > 0) {
  console.log(`已同步版本号：${version}（${changedFiles.join(', ')}）`)
} else {
  console.log(`版本号已是最新：${version}`)
}
