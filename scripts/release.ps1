#Requires -Version 7.0
<#
.SYNOPSIS
Build a local release, replace dist/AI-Picker/aipicker.exe and restart it.
.EXAMPLE
.\scripts\release.ps1 -Jobs 4
.EXAMPLE
.\scripts\release.ps1 -SkipBuild
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [ValidateRange(1, 128)][int]$Jobs = 4,
    [switch]$Lto,
    [switch]$BuildOnly,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
if ($BuildOnly -and $SkipBuild) { throw 'BuildOnly and SkipBuild cannot be combined.' }
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$built = Join-Path $repo 'target\release\aipicker.exe'
$installed = Join-Path $repo 'dist\AI-Picker\aipicker.exe'
$desktop = Join-Path $PSScriptRoot 'desktop.ps1'
$shell = (Get-Process -Id $PID).Path

# Refuse directory junctions/symlinks that could redirect writes outside this checkout.
foreach ($relative in @('target', 'target\release', 'dist', 'dist\AI-Picker', 'dist\AI-Picker\aipicker.exe')) {
    $candidate = Join-Path $repo $relative
    if ((Test-Path -LiteralPath $candidate) -and
        ((Get-Item -LiteralPath $candidate -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Release path must not be a junction or symbolic link: $candidate"
    }
}
if (!$PSCmdlet.ShouldProcess($installed, 'Build release and update/restart AI Picker (unless BuildOnly)')) { return }

if (!$SkipBuild) {
    $previousLto = [Environment]::GetEnvironmentVariable('CARGO_PROFILE_RELEASE_LTO', 'Process')
    $previousCodegen = [Environment]::GetEnvironmentVariable('CARGO_PROFILE_RELEASE_CODEGEN_UNITS', 'Process')
    try {
        $env:CARGO_PROFILE_RELEASE_LTO = if ($Lto) { 'thin' } else { 'false' }
        $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '16'
        Push-Location -LiteralPath $repo
        try {
            $messages = & cargo build --release --locked --jobs $Jobs --target-dir (Join-Path $repo 'target') --message-format=json
            if ($LASTEXITCODE -ne 0) { throw "Release build failed (exit $LASTEXITCODE); running app was not stopped." }
            $artifacts = @($messages | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object {
                $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'aipicker' -and $_.executable
            })
            if ($artifacts.Count -ne 1) { throw 'Cargo did not report exactly one aipicker executable.' }
            $built = $artifacts[0].executable
        } finally { Pop-Location }
    } finally {
        [Environment]::SetEnvironmentVariable('CARGO_PROFILE_RELEASE_LTO', $previousLto, 'Process')
        [Environment]::SetEnvironmentVariable('CARGO_PROFILE_RELEASE_CODEGEN_UNITS', $previousCodegen, 'Process')
    }
}
if (!(Test-Path -LiteralPath $built -PathType Leaf)) { throw "Release artifact missing: $built" }
$expectedHash = (Get-FileHash -LiteralPath $built -Algorithm SHA256).Hash
if ($BuildOnly) { Write-Output "Built: $built (SHA256 $expectedHash)"; return }

function Get-InstalledProcess {
    @(Get-Process -Name aipicker -ErrorAction SilentlyContinue | Where-Object {
        $_.Path -and [string]::Equals($_.Path, $installed, [StringComparison]::OrdinalIgnoreCase)
    })
}
function Read-Desktop([int]$AppId) {
    $output = & $shell -NoProfile -ExecutionPolicy Bypass -File $desktop -Action Inspect -ProcessId $AppId
    if ($LASTEXITCODE -ne 0) { throw "Cannot inspect AI Picker PID $AppId." }
    $json = ($output | Where-Object { $_ -notmatch '^Tray registered:' }) -join "`n"
    $windows = @($json | ConvertFrom-Json)
    $main = $windows | Where-Object Title -eq 'AI Picker' | Select-Object -First 1
    if (!$main -or !($output -contains 'Tray registered: True')) { throw "Window/tray not ready for PID $AppId." }
    return $main
}
function Stop-Installed([Diagnostics.Process]$App) {
    if ($App.HasExited) { return }
    if (![string]::Equals($App.Path, $installed, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Process path changed; refusing to stop it.'
    }
    & $shell -NoProfile -ExecutionPolicy Bypass -File $desktop -Action Exit -ProcessId $App.Id | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Graceful exit failed for PID $($App.Id). Close it using the tray Exit menu, then retry." }
    if (!$App.WaitForExit(10000)) { throw "PID $($App.Id) did not exit; executable was not replaced." }
}

$current = @(Get-InstalledProcess)
if ($current.Count -gt 1) { throw 'Multiple copies of the installed executable are running. Leave one running and retry.' }
$arguments = ''
$wasVisible = $true
if ($current.Count -eq 1) {
    # Preserve the raw argument string, including Windows quoting and spaces.
    $command = (Get-CimInstance Win32_Process -Filter "ProcessId = $($current[0].Id)").CommandLine
    if (!$command -or $command -notmatch '^\s*(?:"[^"]+"|\S+)(?<arguments>.*)$') {
        throw 'Cannot read launch arguments; update cancelled before stopping the app.'
    }
    $arguments = $Matches.arguments.TrimStart()
    if ($arguments -match '--data-dir') {
        if ($arguments -notmatch '(?:^|\s)--data-dir\s+(?:"(?<directory>[^"]+)"|(?<directory>\S+))(?:\s|$)' -or
            $Matches.directory -notmatch '^(?:[A-Za-z]:[\\/]|\\\\)') {
            throw 'Relative or unsupported --data-dir quoting: restart with an absolute --data-dir before updating.'
        }
    }
    $wasVisible = (Read-Desktop $current[0].Id).Visible
}

$backup = $null
$newApp = $null
$replaced = $false
function Start-Installed {
    $launch = @{ FilePath = $installed; WorkingDirectory = (Split-Path $installed); WindowStyle = 'Hidden'; PassThru = $true }
    if ($arguments) { $launch.ArgumentList = $arguments }
    $app = Start-Process @launch
    # Keep the exact handle for cleanup even when startup validation fails.
    $script:newApp = $app
    Start-Sleep -Seconds 3
    if ($app.HasExited) { throw "AI Picker exited during startup (exit $($app.ExitCode))." }
    if (![string]::Equals($app.Path, $installed, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected launched executable path.' }
    $main = Read-Desktop $app.Id
    if ($main.Visible -ne $wasVisible) {
        $action = if ($wasVisible) { 'Tray' } else { 'Close' }
        & $shell -NoProfile -ExecutionPolicy Bypass -File $desktop -Action $action -ProcessId $app.Id | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Could not restore window visibility.' }
        Start-Sleep -Milliseconds 500
        if ((Read-Desktop $app.Id).Visible -ne $wasVisible) { throw 'Window visibility was not restored.' }
    }
    return $app
}

try {
    New-Item -ItemType Directory -Path (Split-Path $installed) -Force | Out-Null
    if (Test-Path -LiteralPath $installed) {
        $backup = "$installed.backup-$([Guid]::NewGuid().ToString('N'))"
        Copy-Item -LiteralPath $installed -Destination $backup
    }
    if ($current.Count -eq 1) { Stop-Installed $current[0] }
    if (@(Get-InstalledProcess).Count -ne 0) { throw 'An installed instance started during update; retry after it exits.' }
    # Mark before copying so a partial copy also triggers restoration.
    $replaced = $true
    Copy-Item -LiteralPath $built -Destination $installed -Force
    if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash -ne $expectedHash) { throw 'Installed executable hash mismatch.' }
    $newApp = Start-Installed
    Write-Output "Updated and running: $installed (PID $($newApp.Id), SHA256 $expectedHash)"
    if ($backup) { Write-Output "Previous executable: $backup" }
} catch {
    $failure = $_
    if ($replaced) {
        try {
            if ($newApp -and !$newApp.HasExited) { Stop-Installed $newApp }
            if (@(Get-InstalledProcess).Count -ne 0) { throw 'An instance is still running; automatic rollback cannot replace it.' }
            if ($backup) {
                Copy-Item -LiteralPath $backup -Destination $installed -Force
                if ($current.Count -eq 1) { $null = Start-Installed }
                Write-Warning "Previous executable restored from $backup"
            }
        } catch { Write-Warning "Rollback failed: $_. Backup: $backup" }
    }
    throw $failure
}
