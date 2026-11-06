<#
.SYNOPSIS
  Downloads the latest Sudo release from GitHub, verifies its SHA-256 checksum and runs the installer.

.DESCRIPTION
  One-command install (PowerShell):
    irm https://raw.githubusercontent.com/panmm942-ui/sudo-agent/master/scripts/install.ps1 | iex

  Steps:
    1. Reads the latest release from the GitHub API.
    2. Downloads the x64 setup (or portable) executable and SHA256SUMS.txt.
    3. Refuses to continue unless the file's SHA-256 matches the published checksum.
    4. Starts the per-user NSIS installer (or places the portable exe in a folder you choose).

  No administrator rights are needed. The release is not code-signed, so Windows SmartScreen
  may ask for confirmation the first time the installer runs.

.PARAMETER Portable
  Download the portable executable instead of the installer.

.PARAMETER Destination
  Folder for the download. Defaults to the Downloads folder for -Portable, otherwise a temp folder.

.PARAMETER DownloadOnly
  Download and verify only; do not run the installer.

.PARAMETER Version
  A specific tag such as v0.2.7. Defaults to the latest release.
#>
[CmdletBinding()]
param(
    [switch]$Portable,
    [string]$Destination,
    [switch]$DownloadOnly,
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is far faster without the progress bar.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = 'panmm942-ui/sudo-agent'
$api = if ($Version) { "https://api.github.com/repos/$repo/releases/tags/$Version" } else { "https://api.github.com/repos/$repo/releases/latest" }
$headers = @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'sudo-install-script' }

if ([Environment]::OSVersion.Platform -ne 'Win32NT' -or -not [Environment]::Is64BitOperatingSystem) {
    throw 'Sudo requires 64-bit Windows 10 or 11.'
}

Write-Host "Looking up the $(if ($Version) { $Version } else { 'latest' }) Sudo release..."
$release = Invoke-RestMethod -Uri $api -Headers $headers
$pattern = if ($Portable) { '_x64-portable\.exe$' } else { '_x64-setup\.exe$' }
$asset = $release.assets | Where-Object { $_.name -match $pattern } | Select-Object -First 1
$sums = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS.txt' } | Select-Object -First 1
if (-not $asset) { throw "Release $($release.tag_name) has no asset matching $pattern." }
if (-not $sums) { throw "Release $($release.tag_name) has no SHA256SUMS.txt; refusing to install an unverified file." }

if (-not $Destination) {
    $Destination = if ($Portable) { Join-Path ([Environment]::GetFolderPath('UserProfile')) 'Downloads' } else { Join-Path ([IO.Path]::GetTempPath()) "sudo-install-$($release.tag_name)" }
}
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$file = Join-Path $Destination $asset.name

Write-Host "Downloading $($asset.name) ($([math]::Round($asset.size / 1MB, 1)) MB)..."
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $file -Headers @{ 'User-Agent' = 'sudo-install-script' } -UseBasicParsing
$sumsText = (Invoke-WebRequest -Uri $sums.browser_download_url -Headers @{ 'User-Agent' = 'sudo-install-script' } -UseBasicParsing).Content
if ($sumsText -is [byte[]]) { $sumsText = [Text.Encoding]::UTF8.GetString($sumsText) }

$expected = $null
foreach ($line in ($sumsText -split "`r?`n")) {
    if ($line -match '^\s*([0-9a-fA-F]{64})\s+\*?(.+?)\s*$' -and $Matches[2] -eq $asset.name) { $expected = $Matches[1].ToLowerInvariant() }
}
if (-not $expected) { Remove-Item -LiteralPath $file -Force; throw "SHA256SUMS.txt does not list $($asset.name)." }
$actual = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) {
    Remove-Item -LiteralPath $file -Force
    throw "Checksum mismatch for $($asset.name): expected $expected, got $actual. The download was deleted."
}
Write-Host "Checksum verified: $actual"

if ($DownloadOnly) {
    Write-Host "Saved to $file"
    return
}
if ($Portable) {
    Write-Host "Starting the portable app from $file"
    Start-Process -FilePath $file
    return
}
Write-Host 'Starting the installer...'
Start-Process -FilePath $file -Wait
Write-Host "Done. Sudo $($release.tag_name) is installed; it appears at the top of your screen."
