$ErrorActionPreference = 'Stop'

$repoRoot = $PSScriptRoot
$keyPath = Join-Path $repoRoot '.tauri-updater-key'
$passwordPath = Join-Path $repoRoot '.tauri-updater-password'
$bundleDir = Join-Path $repoRoot 'src-tauri\target\release\bundle\msi'
$packageJsonPath = Join-Path $repoRoot 'package.json'

function Assert-CargoAvailable {
  if (Get-Command cargo -ErrorAction SilentlyContinue) {
    return
  }

  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
  $processPath = $env:Path
  $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'

  $pathParts = @($processPath, $userPath, $machinePath)
  if (Test-Path -LiteralPath $cargoBin) {
    $pathParts += $cargoBin
  }

  $env:Path = ($pathParts | Where-Object { $_ -and $_.Trim() -ne '' } | Select-Object -Unique) -join ';'

  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw '未在 PATH 中找到 cargo，请通过 rustup 安装 Rust 后重新打开终端。'
  }
}

function Get-MsiRows {
  param(
    [Parameter(Mandatory = $true)]
    [string]$MsiPath,
    [Parameter(Mandatory = $true)]
    [string]$Sql
  )

  $installer = New-Object -ComObject WindowsInstaller.Installer
  $database = $installer.OpenDatabase($MsiPath, 0)
  $view = $database.OpenView($Sql)
  $view.Execute()

  $rows = @()
  while ($record = $view.Fetch()) {
    $values = @()
    for ($i = 1; $i -le $record.FieldCount(); $i++) {
      $values += $record.StringData($i)
    }
    $rows += [pscustomobject]@{
      Fields = [string[]]$values
    }
  }

  return @($rows)
}

function Test-Flag {
  param(
    [Parameter(Mandatory = $true)]
    [int]$Value,
    [Parameter(Mandatory = $true)]
    [int]$Flag
  )

  return (($Value -band $Flag) -eq $Flag)
}

function Assert-MsiUpgradeMetadata {
  param(
    [Parameter(Mandatory = $true)]
    [string]$MsiPath,
    [Parameter(Mandatory = $true)]
    [string]$ExpectedVersion
  )

  $propertySql = "SELECT ``Property``, ``Value`` FROM ``Property`` WHERE ``Property``='ProductVersion' OR ``Property``='UpgradeCode'"
  $propertyRows = Get-MsiRows -MsiPath $MsiPath -Sql $propertySql

  $properties = @{}
  foreach ($row in $propertyRows) {
    if (-not $row -or -not $row.Fields) {
      continue
    }
    $fields = $row.Fields
    $properties[$fields[0]] = $fields[1]
  }

  if (-not $properties.ProductVersion) {
    throw "MSI 缺少 ProductVersion 属性：$MsiPath"
  }

  if ($properties.ProductVersion -ne $ExpectedVersion) {
    throw "MSI 的 ProductVersion $($properties.ProductVersion) 与 package.json 版本 $ExpectedVersion 不一致"
  }

  if (-not $properties.UpgradeCode) {
    throw "MSI 缺少 UpgradeCode 属性：$MsiPath"
  }

  $upgradeSql = 'SELECT `UpgradeCode`, `VersionMin`, `VersionMax`, `Attributes`, `ActionProperty` FROM `Upgrade`'
  $upgradeRows = Get-MsiRows -MsiPath $MsiPath -Sql $upgradeSql

  $sameVersionRow = $null
  foreach ($row in $upgradeRows) {
    if (-not $row -or -not $row.Fields) {
      continue
    }
    $fields = $row.Fields
    $attributes = 0
    [void][int]::TryParse($fields[3], [ref]$attributes)
    if (
      $fields[0] -eq $properties.UpgradeCode -and
      $fields[2] -eq $ExpectedVersion -and
      $fields[4] -eq 'WIX_UPGRADE_DETECTED' -and
      (Test-Flag -Value $attributes -Flag 512)
    ) {
      $sameVersionRow = [pscustomobject]@{
        UpgradeCode    = $fields[0]
        VersionMin     = $fields[1]
        VersionMax     = $fields[2]
        Attributes     = $attributes
        ActionProperty = $fields[4]
      }
      break
    }
  }

  if (-not $sameVersionRow) {
    throw "MSI 缺少同版本覆盖升级检测条目（应为 VersionMax=$ExpectedVersion 且带「包含最大版本」标志）。"
  }

  Write-Host ''
  Write-Host '已校验 MSI 升级元数据：'
  Write-Host " - 产品版本(ProductVersion): $($properties.ProductVersion)"
  Write-Host " - 升级代码(UpgradeCode): $($properties.UpgradeCode)"
  Write-Host " - 同版本升级条目: VersionMax=$($sameVersionRow.VersionMax), Attributes=$($sameVersionRow.Attributes), ActionProperty=$($sameVersionRow.ActionProperty)"
}

if (-not (Test-Path -LiteralPath $keyPath)) {
  throw "缺少签名密钥文件：$keyPath"
}

if (-not (Test-Path -LiteralPath $passwordPath)) {
  throw "缺少签名密码文件：$passwordPath"
}

Push-Location $repoRoot
try {
  $package = Get-Content -LiteralPath $packageJsonPath -Raw | ConvertFrom-Json
  $expectedVersion = [string]$package.version

  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -LiteralPath $keyPath -Raw
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content -LiteralPath $passwordPath -Raw).Trim()

  Assert-CargoAvailable
  npm run tauri build

  if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
  }

  $artifacts = @()
  if (Test-Path -LiteralPath $bundleDir) {
    $artifacts = Get-ChildItem -LiteralPath $bundleDir -File |
      Where-Object { $_.Extension -in '.msi', '.sig' } |
      Sort-Object Name
  }

  if ($artifacts.Count -eq 0) {
    throw "构建完成，但在 $bundleDir 中未找到 MSI 产物"
  }

  $msiArtifact = $artifacts | Where-Object Extension -eq '.msi' | Select-Object -First 1
  if (-not $msiArtifact) {
    throw "构建完成，但在 $bundleDir 中未找到 MSI 安装包"
  }

  Assert-MsiUpgradeMetadata -MsiPath $msiArtifact.FullName -ExpectedVersion $expectedVersion

  Write-Host ''
  Write-Host '产物：'
  foreach ($artifact in $artifacts) {
    Write-Host " - $($artifact.FullName)"
  }
}
finally {
  Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
  Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
  Pop-Location
}
