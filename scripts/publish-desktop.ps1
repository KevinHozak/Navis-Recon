$ErrorActionPreference = 'Stop'

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$releaseDir = Join-Path $projectRoot 'frontend\src-tauri\target\release'
$exeSource = Join-Path $releaseDir 'navis-recon.exe'
$configPath = Join-Path $projectRoot 'frontend\src-tauri\tauri.conf.json'
$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$exeTarget = Join-Path $projectRoot 'Navis-Recon.exe'
$installerTarget = Join-Path $projectRoot 'Navis-Recon-Setup.exe'
$shortcutTarget = Join-Path $projectRoot 'Navis-Recon.lnk'

if (-not (Test-Path -LiteralPath $exeSource -PathType Leaf)) {
    throw "Release executable not found: $exeSource"
}

# The desktop build requires NSIS. Resolve the configured release before changing
# any root artifacts; a newer timestamp does not establish the installer version.
$installerDir = Join-Path $releaseDir 'bundle\nsis'
$installerPrefix = "$($config.productName)_$($config.version)_"
if ([string]::IsNullOrWhiteSpace($config.productName) -or [string]::IsNullOrWhiteSpace($config.version)) {
    throw "productName and version are required in $configPath"
}
$installerCandidates = @(if (Test-Path -LiteralPath $installerDir -PathType Container) {
    Get-ChildItem -LiteralPath $installerDir -File | Where-Object {
        $_.Name.StartsWith($installerPrefix, [StringComparison]::OrdinalIgnoreCase) -and
        $_.Name.EndsWith('-setup.exe', [StringComparison]::OrdinalIgnoreCase)
    }
})
if ($installerCandidates.Count -ne 1) {
    throw "Expected exactly one NSIS installer for $($config.productName) $($config.version) in $installerDir; found $($installerCandidates.Count). Run a matching desktop build and resolve ambiguous artifacts."
}
$installerSource = $installerCandidates[0].FullName

Copy-Item -LiteralPath $exeSource -Destination $exeTarget -Force
Copy-Item -LiteralPath $installerSource -Destination $installerTarget -Force

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
