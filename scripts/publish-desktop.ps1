$ErrorActionPreference = 'Stop'

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$releaseDir = Join-Path $projectRoot 'frontend\src-tauri\target\release'
$exeSource = Join-Path $releaseDir 'navis-recon.exe'
$installerSource = Join-Path $releaseDir 'bundle\nsis\Navis-Recon_0.1.0_x64-setup.exe'
$exeTarget = Join-Path $projectRoot 'Navis-Recon.exe'
$installerTarget = Join-Path $projectRoot 'Navis-Recon-Setup.exe'
$shortcutTarget = Join-Path $projectRoot 'Navis-Recon.lnk'

if (-not (Test-Path -LiteralPath $exeSource)) {
    throw "Release executable not found: $exeSource"
}

Copy-Item -LiteralPath $exeSource -Destination $exeTarget -Force
if (Test-Path -LiteralPath $installerSource) {
    Copy-Item -LiteralPath $installerSource -Destination $installerTarget -Force
}

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutTarget)
$shortcut.TargetPath = $exeTarget
$shortcut.WorkingDirectory = $projectRoot
$shortcut.Description = 'Launch Navis-Recon standalone desktop app'
$shortcut.IconLocation = "$exeTarget,0"
$shortcut.Save()

Write-Host "Published desktop executable: $exeTarget"
if (Test-Path -LiteralPath $installerTarget) {
    Write-Host "Published installer: $installerTarget"
}
Write-Host "Published shortcut: $shortcutTarget"
