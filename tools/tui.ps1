# 星伴 FOSS TUI 工具（Windows PowerShell 5.1+ / PowerShell 7+ on Linux & macOS）
[CmdletBinding()]
param(
  [ValidateSet('menu','export','webview2','check')]
  [string]$Action = 'menu'
)

$ErrorActionPreference = 'Stop'

# Windows PowerShell 5.1 没有 $IsWindows，需要手动判定
$IsWin = if ($PSVersionTable.PSVersion.Major -ge 6) { $IsWindows } else { $true }
$IsMac = if ($PSVersionTable.PSVersion.Major -ge 6) { $IsMacOS } else { $false }

$ScriptDir = $PSScriptRoot
$AppRoot = if ($env:STARLIT_APP_ROOT) { $env:STARLIT_APP_ROOT } else { Split-Path $ScriptDir -Parent }

$defaultData = if ($IsWin) {
  Join-Path $env:APPDATA 'starlit-desktop'
} elseif ($IsMac) {
  Join-Path $HOME 'Library/Application Support/starlit-desktop'
} else {
  Join-Path $HOME '.local/share/starlit-desktop'
}
$DataRoot = if ($env:STARLIT_DATA_ROOT) { $env:STARLIT_DATA_ROOT } else { $defaultData }

$LogDir = Join-Path $DataRoot 'log'
$MediaDir = Join-Path (Join-Path $AppRoot 'resources') 'media'
$TempDir = [System.IO.Path]::GetTempPath()
$StarlitExe = if ($IsWin) { 'starlit-foss.exe' } else { 'starlit-foss' }
$WebView2Guid = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$WebView2Url = 'https://go.microsoft.com/fwlink/p/?LinkId=2124703'

function Write-Warn($text) { Write-Host "[WARN] $text" -ForegroundColor Yellow }
$BoxInner = 66

function Get-DisplayWidth([string]$text) {
  $width = 0
  foreach ($ch in $text.ToCharArray()) {
    $code = [int]$ch
    if (($code -ge 0x1100 -and $code -le 0x115F) -or
        ($code -ge 0x2E80 -and $code -le 0xA4CF) -or
        ($code -ge 0xAC00 -and $code -le 0xD7A3) -or
        ($code -ge 0xF900 -and $code -le 0xFAFF) -or
        ($code -ge 0xFE30 -and $code -le 0xFE4F) -or
        ($code -ge 0xFF00 -and $code -le 0xFF60) -or
        ($code -ge 0xFFE0 -and $code -le 0xFFE6)) {
      $width += 2
    } else {
      $width += 1
    }
  }
  return $width
}

function Format-BoxText([string]$text) {
  $out = ''
  $used = 0
  foreach ($ch in $text.ToCharArray()) {
    $w = if ((Get-DisplayWidth $ch) -eq 2) { 2 } else { 1 }
    if ($used + $w -gt ($BoxInner - 1)) { $out += '…'; $used += 1; break }
    $out += $ch
    $used += $w
  }
  return $out + (' ' * ($BoxInner - $used))
}

function Write-BoxLine([string]$text) {
  Write-Host "║ $(Format-BoxText $text) ║"
}

function Write-BoxSep {
  Write-Host ('╠' + ('═' * ($BoxInner + 2)) + '╣')
}

function Write-BoxTop([string]$title) {
  Write-Host ('╔' + ('═' * ($BoxInner + 2)) + '╗')
  Write-BoxLine $title
  Write-BoxSep
}

function Write-BoxBottom {
  Write-Host ('╚' + ('═' * ($BoxInner + 2)) + '╝')
}

function Pause-Menu {
  if ($Action -eq 'menu') { Read-Host '按 Enter 返回菜单…' | Out-Null }
}

function Export-Logs {
  if (-not (Test-Path $LogDir)) {
    Write-BoxTop '导出日志'
    Write-BoxLine '日志目录不存在'
    Write-BoxLine $LogDir
    Write-BoxBottom
    Pause-Menu; return
  }

  if ($Action -eq 'menu') {
    $out = Read-Host '输出目录（默认当前目录）'
  } else {
    $out = ''
  }
  if ([string]::IsNullOrWhiteSpace($out)) { $out = (Get-Location).Path }
  New-Item -ItemType Directory -Force -Path $out | Out-Null

  $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
  $files = @(Get-ChildItem -Path $LogDir -Filter '*.log' -File)

  $zstd = Get-Command zstd -ErrorAction SilentlyContinue
  $tar = Get-Command tar -ErrorAction SilentlyContinue
  $useTarZst = (-not $IsWin) -and $zstd -and $tar

  if ($useTarZst) {
    $target = Join-Path $out "starlit-logs-$stamp.tar.zst"
    if ($files.Count -eq 0) {
      $tmp = Join-Path $TempDir "starlit-empty-$stamp"
      New-Item -ItemType Directory -Force -Path $tmp | Out-Null
      Set-Content -Path (Join-Path $tmp 'empty.txt') -Value 'no logs'
      & tar -C $tmp -cf - . | & zstd -19 -T0 -q -f -o $target
      Remove-Item -Recurse -Force $tmp
    } else {
      & tar -C $LogDir -cf - . | & zstd -19 -T0 -q -f -o $target
    }
  } else {
    $target = Join-Path $out "starlit-logs-$stamp.zip"
    if ($files.Count -eq 0) {
      $tmp = Join-Path $TempDir "starlit-empty-$stamp"
      New-Item -ItemType Directory -Force -Path $tmp | Out-Null
      Set-Content -Path (Join-Path $tmp 'empty.txt') -Value 'no logs'
      Compress-Archive -Path (Join-Path $tmp '*') -DestinationPath $target -Force
      Remove-Item -Recurse -Force $tmp
    } else {
      Compress-Archive -Path $files.FullName -DestinationPath $target -Force
    }
  }

  $size = '{0:N1} KB' -f ((Get-Item $target).Length / 1KB)
  Write-BoxTop '导出日志'
  Write-BoxLine "日志目录: $LogDir"
  Write-BoxLine "输出文件: $target"
  Write-BoxLine "大小: $size"
  Write-BoxBottom
  Pause-Menu
}

function Install-WebView2 {
  if (-not $IsWin) {
    Write-BoxTop '安装 WebView2'
    Write-BoxLine '当前平台不是 Windows'
    Write-BoxLine 'Linux/macOS 使用 WebKitGTK/WKWebView'
    Write-BoxLine '无需安装 WebView2'
    Write-BoxBottom
    Pause-Menu
    return
  }

  $paths = @(
    "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$WebView2Guid",
    "HKCU:\Software\Microsoft\EdgeUpdate\Clients\$WebView2Guid"
  )
  $installed = $null
  foreach ($p in $paths) {
    if (Test-Path $p) {
      $pv = (Get-ItemProperty $p -ErrorAction SilentlyContinue).pv
      if ($pv) { $installed = $pv; break }
    }
  }

  if ($installed) {
    Write-BoxTop '安装 WebView2'
    Write-BoxLine '状态: 已安装'
    Write-BoxLine "版本: $installed"
    Write-BoxBottom
    Pause-Menu; return
  }

  Write-BoxTop '安装 WebView2'
  Write-BoxLine '状态: 未安装'
  Write-BoxLine '准备下载官方 Evergreen 安装器'
  Write-BoxBottom

  $installer = Join-Path $TempDir 'MicrosoftEdgeWebview2Setup.exe'
  try {
    Invoke-WebRequest -Uri $WebView2Url -OutFile $installer -UseBasicParsing
    Start-Process -FilePath $installer -ArgumentList '/silent', '/install' -Wait
    Write-BoxTop '安装 WebView2'
    Write-BoxLine '状态: 安装完成'
    Write-BoxLine '可以重新启动星伴'
    Write-BoxBottom
  } catch {
    Write-BoxTop '安装 WebView2'
    Write-BoxLine '状态: 安装失败'
    Write-BoxLine $_.Exception.Message
    Write-BoxBottom
  }
  Pause-Menu
}

function Check-Integrity {
  $results = New-Object System.Collections.Generic.List[string]
  $failures = 0

  $exeCandidates = @(
    (Join-Path (Join-Path $AppRoot 'bin') $StarlitExe),
    (Join-Path $AppRoot $StarlitExe)
  )
  if ($IsWin -and $env:ProgramFiles) {
    $exeCandidates += (Join-Path (Join-Path (Join-Path $env:ProgramFiles '星伴 FOSS') $StarlitExe))
  }
  if (-not $IsWin) {
    $exeCandidates += (Join-Path $AppRoot 'src-tauri/target/release/starlit-foss')
    $exeCandidates += (Join-Path $AppRoot 'src-tauri/target/debug/starlit-foss')
  }
  $exe = $exeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
  if ($exe) { $results.Add("[OK] 可执行文件: $exe") }
  else { $results.Add('[FAIL] 未找到应用可执行文件'); $failures++ }

  $manifest = Join-Path $MediaDir 'manifest.json'
  if (-not (Test-Path $manifest)) {
    $results.Add('[FAIL] 缺少媒体清单'); $failures++
  } else {
    $data = Get-Content $manifest -Raw | ConvertFrom-Json
    $total = 0; $missing = 0; $bad = 0
    foreach ($entry in $data.archives) {
      $total++
      $file = Join-Path $MediaDir $entry.name
      if (-not (Test-Path $file)) { $missing++; continue }
      if ($entry.md5) {
        $hash = (Get-FileHash $file -Algorithm MD5).Hash.ToLower()
        $expected = $entry.md5.ToLower()
      } else {
        $hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
        $expected = $entry.sha256.ToLower()
      }
      if ($hash -ne $expected) { $bad++ }
    }
    if ($missing -eq 0 -and $bad -eq 0) { $results.Add("[OK] 媒体包完整: $total 个") }
    else { $results.Add("[FAIL] 媒体包异常: 缺失 $missing, 校验失败 $bad, 共 $total"); $failures++ }
  }

  $db = Join-Path (Join-Path $DataRoot 'data') 'companion.db'
  if (Test-Path $db) {
    $sqlite = Get-Command sqlite3 -ErrorAction SilentlyContinue
    if ($sqlite) {
      $result = & sqlite3 $db 'PRAGMA integrity_check;' 2>$null | Select-Object -First 1
      if ($result -eq 'ok') { $results.Add('[OK] 数据库完整性: ok') }
      else { $results.Add("[FAIL] 数据库完整性: $result"); $failures++ }
    } else {
      $results.Add('[WARN] 跳过数据库检查')
    }
  } else {
    $results.Add('[WARN] 数据库尚未创建')
  }

  if (Test-Path $LogDir) { $results.Add("[OK] 日志目录: $LogDir") }
  else { $results.Add('[WARN] 日志目录尚未创建') }

  Write-BoxTop '检查文件完整性'
  $results.Insert(0, "[INFO] 应用目录: $AppRoot")
  $results.Insert(1, "[INFO] 数据目录: $DataRoot")
  foreach ($line in $results) { Write-BoxLine $line }
  Write-BoxSep
  if ($failures -eq 0) { Write-BoxLine '结果: 全部检查通过' }
  else { Write-BoxLine "结果: 存在 $failures 项问题" }
  Write-BoxBottom
  Pause-Menu
}

function Show-Menu {
  while ($true) {
    Clear-Host
    Write-BoxTop '星伴 FOSS TUI 工具'
    Write-BoxLine "应用目录: $AppRoot"
    Write-BoxLine "数据目录: $DataRoot"
    Write-BoxSep
    Write-BoxLine '1  导出日志文件夹'
    Write-BoxLine '2  安装 WebView2（Windows）'
    Write-BoxLine '3  检查文件完整性'
    Write-BoxLine '0  退出'
    Write-BoxBottom
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
