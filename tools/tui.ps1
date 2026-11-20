# 星伴 FOSS TUI 工具（Windows PowerShell 5.1+）
[CmdletBinding()]
param(
  [ValidateSet('menu','export','webview2','check')]
  [string]$Action = 'menu'
)

$ErrorActionPreference = 'Stop'
$ScriptDir = $PSScriptRoot
$AppRoot = if ($env:STARLIT_APP_ROOT) { $env:STARLIT_APP_ROOT } else { Split-Path $ScriptDir -Parent }
$DataRoot = if ($env:STARLIT_DATA_ROOT) { $env:STARLIT_DATA_ROOT } else { Join-Path $env:APPDATA 'starlit-desktop' }
$LogDir = Join-Path $DataRoot 'log'
$MediaDir = Join-Path $AppRoot 'resources\media'
$WebView2Guid = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$WebView2Url = 'https://go.microsoft.com/fwlink/p/?LinkId=2124703'

function Write-Ok($text)   { Write-Host "[OK] $text" -ForegroundColor Green }
function Write-Warn($text) { Write-Host "[WARN] $text" -ForegroundColor Yellow }
function Write-Err($text)  { Write-Host "[FAIL] $text" -ForegroundColor Red }
function Write-Title($text){ Write-Host "`n$text" -ForegroundColor Cyan }
function Pause-Menu {
  if ($Action -eq 'menu') { Read-Host '按 Enter 返回菜单…' | Out-Null }
}

function Export-Logs {
  Write-Title '导出日志'
  if (-not (Test-Path $LogDir)) { Write-Err "日志目录不存在：$LogDir"; Pause-Menu; return }
  if ($Action -eq 'menu') {
    $out = Read-Host "输出目录（默认当前目录）"
  } else {
    $out = ''
  }
  if ([string]::IsNullOrWhiteSpace($out)) { $out = (Get-Location).Path }
  New-Item -ItemType Directory -Force -Path $out | Out-Null
  $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
  $target = Join-Path $out "starlit-logs-$stamp.zip"
  $files = Get-ChildItem -Path $LogDir -Filter '*.log' -File
  if ($files.Count -eq 0) {
    Write-Warn '日志目录为空，生成空压缩包。'
    $tmp = Join-Path $env:TEMP "starlit-empty-$stamp"
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    Set-Content -Path (Join-Path $tmp 'empty.txt') -Value 'no logs'
    Compress-Archive -Path (Join-Path $tmp '*') -DestinationPath $target -Force
    Remove-Item -Recurse -Force $tmp
  } else {
    Compress-Archive -Path $files.FullName -DestinationPath $target -Force
  }
  Write-Ok "已导出：$target"
  Get-Item $target | Select-Object FullName, Length | Format-List
  Pause-Menu
}

function Install-WebView2 {
  Write-Title '安装 WebView2'
  $paths = @(
    "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$WebView2Guid",
    "HKCU:\Software\Microsoft\EdgeUpdate\Clients\$WebView2Guid"
  )
  foreach ($p in $paths) {
    if (Test-Path $p) {
      $pv = (Get-ItemProperty $p -ErrorAction SilentlyContinue).pv
      if ($pv) { Write-Ok "已安装 WebView2 Runtime：$pv"; Pause-Menu; return }
    }
  }
  Write-Warn '未检测到 WebView2 Runtime，准备下载官方 Evergreen 安装器。'
  $installer = Join-Path $env:TEMP 'MicrosoftEdgeWebview2Setup.exe'
  try {
    Invoke-WebRequest -Uri $WebView2Url -OutFile $installer -UseBasicParsing
    Write-Ok "下载完成：$installer"
    Start-Process -FilePath $installer -ArgumentList '/silent', '/install' -Wait
    Write-Ok '安装完成。'
  } catch {
    Write-Err "安装失败：$($_.Exception.Message)"
  }
  Pause-Menu
}

function Check-Integrity {
  Write-Title '检查文件完整性'
  $failures = 0

  Write-Host "应用目录：$AppRoot"
  Write-Host "数据目录：$DataRoot"

  $exeCandidates = @(
    (Join-Path $AppRoot 'bin\starlit-foss.exe'),
    (Join-Path $AppRoot 'starlit-foss.exe'),
    (Join-Path $env:ProgramFiles '星伴 FOSS\starlit-foss.exe')
  )
  $exe = $exeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
  if ($exe) { Write-Ok "可执行文件：$exe" }
  else { Write-Err '未找到应用可执行文件'; $failures++ }

  $manifest = Join-Path $MediaDir 'manifest.json'
  if (-not (Test-Path $manifest)) {
    Write-Err "缺少媒体清单：$manifest"; $failures++
  } else {
    $data = Get-Content $manifest -Raw | ConvertFrom-Json
    $total = 0; $missing = 0; $bad = 0
    foreach ($entry in $data.archives) {
      $total++
      $file = Join-Path $MediaDir $entry.name
      if (-not (Test-Path $file)) {
        Write-Err "缺失：$($entry.name)"; $missing++; continue
      }
      $hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
      if ($hash -ne $entry.sha256.ToLower()) {
        Write-Err "校验失败：$($entry.name)"; $bad++
      }
    }
    if ($missing -eq 0 -and $bad -eq 0) { Write-Ok "媒体包完整：$total 个" }
    else { Write-Err "媒体包异常：缺失 $missing，校验失败 $bad（共 $total）"; $failures++ }
  }

  $db = Join-Path $DataRoot 'data\companion.db'
  if (Test-Path $db) {
    $sqlite = Get-Command sqlite3 -ErrorAction SilentlyContinue
    if ($sqlite) {
      $result = & sqlite3 $db 'PRAGMA integrity_check;' 2>$null | Select-Object -First 1
      if ($result -eq 'ok') { Write-Ok '数据库完整性：ok' }
      else { Write-Err "数据库完整性检查失败：$result"; $failures++ }
    } else {
      Write-Warn '未找到 sqlite3，跳过数据库检查'
    }
  } else {
    Write-Warn '数据库尚未创建'
  }

  if (Test-Path $LogDir) { Write-Ok "日志目录：$LogDir" }
  else { Write-Warn '日志目录尚未创建' }

  if ($failures -eq 0) { Write-Host "`n全部检查通过。" -ForegroundColor Green }
  else { Write-Host "`n存在 $failures 项问题。" -ForegroundColor Red }
  Pause-Menu
}

function Show-Menu {
  while ($true) {
    Clear-Host
    Write-Host '星伴 FOSS TUI 工具' -ForegroundColor Cyan
    Write-Host "应用目录:`t$AppRoot"
    Write-Host "数据目录:`t$DataRoot"
    Write-Host ''
    Write-Host "1`t导出日志文件夹"
    Write-Host "2`t安装 WebView2"
    Write-Host "3`t检查文件完整性"
    Write-Host "0`t退出"
    $choice = Read-Host '请选择'
    switch ($choice) {
      '1' { Export-Logs }
      '2' { Install-WebView2 }
      '3' { Check-Integrity }
      '0' { return }
      default { Write-Warn '无效选项' }
    }
  }
}

switch ($Action) {
  'export'   { Export-Logs }
  'webview2' { Install-WebView2 }
  'check'    { Check-Integrity }
  default    { Show-Menu }
}
