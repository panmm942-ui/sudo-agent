param(
    [Parameter(Mandatory=$true)][ValidateSet('driver','runtime','open-driver')][string]$Action,
    [Parameter(Mandatory=$true)][string]$DataRoot,
    [switch]$LibraryOnly
)
# Invoked only by a user setup action. Never installs a driver or changes audio routing.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Send-Status([string]$Phase, [string]$Message) {
    [Console]::Out.WriteLine((@{phase=$Phase; component=$Action; message=$Message} | ConvertTo-Json -Compress))
}

function Expand-SafeZip([string]$Archive, [string]$Destination, [long]$MaximumBytes = 134217728) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $targetRoot = [IO.Path]::GetFullPath($Destination).TrimEnd('\') + '\'
    if (Test-Path -LiteralPath $Destination) { throw 'Extraction requires a new empty staging folder.' }
    $archiveHandle = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        [long]$totalBytes = 0
        $seenPaths = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
        foreach ($entry in $archiveHandle.Entries) {
            $entryName = $entry.FullName.Replace('/', '\')
            if ([string]::IsNullOrWhiteSpace($entryName) -or [IO.Path]::IsPathRooted($entryName) -or $entryName.Contains(':') -or $entryName -match '(^|\\)\.\.?($|\\)' -or $entryName -match '[. ]($|\\)') { throw 'Unsafe path in downloaded archive.' }
            $entryTarget = [IO.Path]::GetFullPath((Join-Path $Destination $entryName))
            if (-not $entryTarget.StartsWith($targetRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Archive entry escapes its setup folder.' }
            # Reject Unix symbolic links and Windows reparse entries.
            if ((($entry.ExternalAttributes -shr 16) -band 0xF000) -eq 0xA000 -or ($entry.ExternalAttributes -band 0x400) -ne 0) { throw 'Links are not allowed in setup archives.' }
            if (-not $seenPaths.Add($entryTarget)) { throw 'Duplicate path in downloaded archive.' }
            $totalBytes += $entry.Length
            if ($totalBytes -gt $MaximumBytes -or $archiveHandle.Entries.Count -gt 2048) { throw 'Downloaded archive exceeds setup limits.' }
        }
    } finally { $archiveHandle.Dispose() }
    [IO.Compression.ZipFile]::ExtractToDirectory($Archive, $Destination)
}

function Assert-VBCableSignature([string]$Installer) {
    if (-not (Test-Path -LiteralPath $Installer -PathType Leaf)) { throw 'VB-CABLE installer is missing. Choose Download driver first.' }
    $signature = Get-AuthenticodeSignature -LiteralPath $Installer
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch '(^|,\s*)(CN|O)=BUREL VINCENT( Entrepreneur individuel)?(,|$)') {
        throw 'VB-CABLE signature is not valid for its expected publisher, BUREL VINCENT. Setup was not opened.'
    }
}

function Show-Driver([string]$Installer) {
    Assert-VBCableSignature $Installer
    # Explorer accepts /select,<fully quoted file>, so users see the actual executable.
    Start-Process -FilePath 'explorer.exe' -ArgumentList ('/select,"' + $Installer + '"') -WindowStyle Normal | Out-Null
}

function Prepare-Driver {
    $driverFolder = Join-Path $DataRoot 'vocal-driver\Pack45'
    $installer = Join-Path $driverFolder 'VBCABLE_Setup_x64.exe'
    if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
        Send-Status 'downloading' 'Downloading VB-CABLE from VB-Audio (about 1.3 MB)...'
        $driverRoot = Join-Path $DataRoot 'vocal-driver'
        New-Item -ItemType Directory -Force -Path $driverRoot | Out-Null
        $staging = Join-Path $driverRoot ('staging-' + [Guid]::NewGuid().ToString('N'))
        $archive = $staging + '.zip'
        Invoke-WebRequest -UseBasicParsing -Uri 'https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip' -OutFile $archive -TimeoutSec 180
        Expand-SafeZip $archive $staging 16777216
        Assert-VBCableSignature (Join-Path $staging 'VBCABLE_Setup_x64.exe')
        # Preserve an incomplete earlier download, then publish the verified full package.
        if (Test-Path -LiteralPath $driverFolder) {
            $previous = [IO.Path]::GetFullPath($driverFolder + '.previous-' + [Guid]::NewGuid().ToString('N'))
            $expectedRoot = [IO.Path]::GetFullPath($driverRoot).TrimEnd('\') + '\'
            if (-not $previous.StartsWith($expectedRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid driver staging path.' }
            Move-Item -LiteralPath $driverFolder -Destination $previous
        }
        Move-Item -LiteralPath $staging -Destination $driverFolder
    }
    Show-Driver $installer
    Send-Status 'ready' 'Driver installer selected in Explorer. Right-click it, choose Run as administrator, click Install Driver, and restart Windows when requested. Then refresh devices.'
}

function Find-Uv {
    $localUv = Join-Path $DataRoot 'vocal-tools\uv.exe'
    if (Test-Path -LiteralPath $localUv -PathType Leaf) { return $localUv }
    # uv is installed privately: no profile, PATH, registry or global Python changes.
    Send-Status 'downloading' 'Downloading the portable Python setup tool from Astral...'
    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/astral-sh/uv/releases/latest' -Headers @{'User-Agent'='Sudo-optional-vocal-setup'} -TimeoutSec 90
    $assetName = 'uv-x86_64-pc-windows-msvc.zip'
    $asset = @($release.assets | Where-Object name -eq $assetName)
    $checksum = @($release.assets | Where-Object name -eq ($assetName + '.sha256'))
    if ($asset.Count -ne 1 -or $checksum.Count -ne 1) { throw 'Astral did not publish the expected Windows tool and checksum. Retry later.' }
    $expectedPrefix = 'https://github.com/astral-sh/uv/releases/download/'
    if (-not $asset[0].browser_download_url.StartsWith($expectedPrefix) -or -not $checksum[0].browser_download_url.StartsWith($expectedPrefix)) { throw 'Unexpected portable-tool download source.' }
    $toolsRoot = Join-Path $DataRoot 'vocal-tools'
    New-Item -ItemType Directory -Force -Path $toolsRoot | Out-Null
    $staging = Join-Path $toolsRoot ('staging-' + [Guid]::NewGuid().ToString('N'))
    $archive = $staging + '.zip'
    Invoke-WebRequest -UseBasicParsing -Uri $asset[0].browser_download_url -OutFile $archive -TimeoutSec 180
    $checksumBody = (Invoke-WebRequest -UseBasicParsing -Uri $checksum[0].browser_download_url -TimeoutSec 90).Content
    # Windows PowerShell 5.1 returns application/octet-stream bodies as bytes, not text.
    $checksumText = if ($checksumBody -is [byte[]]) { [Text.Encoding]::ASCII.GetString($checksumBody) } else { [string]$checksumBody }
    $expected = [regex]::Match($checksumText, '(?i)\b[a-f0-9]{64}\b').Value
    if (-not $expected -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) { throw 'Portable-tool checksum verification failed.' }
    Expand-SafeZip $archive $staging
    $binary = Join-Path $staging 'uv.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Portable Python tool is missing from its archive.' }
    Copy-Item -LiteralPath $binary -Destination $localUv
    return $localUv
}

function Run-Checked([string]$Program, [string[]]$Arguments) {
    # Do not leak arbitrary dependency stdout into the app's JSON control channel.
    if (-not (Test-Path -LiteralPath $Program -PathType Leaf)) { throw 'Optional setup executable is missing.' }
    $previousErrorAction = $ErrorActionPreference
    try {
        # Windows PowerShell 5.1 cannot redirect stdout to stderr with 1>&2.
        $ErrorActionPreference = 'Continue'
        & $Program @Arguments | ForEach-Object { [Console]::Error.WriteLine($_) }
        $programExit = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousErrorAction }
    if ($programExit -ne 0) { throw 'Optional dependency setup failed. Retry, or inspect vocal-setup.log in the displayed data folder.' }
}

function Prepare-Runtime {
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'This optional runtime currently requires Windows x64.' }
    $nvidia = Get-Command 'nvidia-smi.exe' -ErrorAction SilentlyContinue
    if ($null -eq $nvidia) { throw 'A compatible NVIDIA GPU and its current graphics driver are required. No runtime packages were downloaded.' }
    $gpu = & $nvidia.Source '--query-gpu=name' '--format=csv,noheader' 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $gpu) { throw 'The NVIDIA graphics driver is unavailable. No runtime packages were downloaded.' }
    foreach ($folder in 'tmp', 'uv-cache', 'uv-python', 'vocal-models', 'vocal-bin') { New-Item -ItemType Directory -Force -Path (Join-Path $DataRoot $folder) | Out-Null }
    $env:UV_CACHE_DIR = Join-Path $DataRoot 'uv-cache'
    $env:UV_PYTHON_INSTALL_DIR = Join-Path $DataRoot 'uv-python'
    $env:UV_PYTHON_PREFERENCE = 'only-managed'
    $env:TEMP = Join-Path $DataRoot 'tmp'
    $env:TMP = $env:TEMP
    $env:CUDA_CACHE_PATH = Join-Path $DataRoot 'cuda-cache'
    $env:HF_HOME = Join-Path $DataRoot 'hf'
    $env:PYTHONNOUSERSITE = '1'
    $env:PYTHONPATH = ''
    $uv = Find-Uv
    $python = Join-Path $DataRoot 'vocal-env\Scripts\python.exe'
    if (-not (Test-Path -LiteralPath $python -PathType Leaf)) {
        Send-Status 'python' 'Installing portable Python 3.12 in your Sudo data folder...'
        Run-Checked $uv @('venv', '--python', '3.12', '--python-preference', 'only-managed', (Join-Path $DataRoot 'vocal-env'))
    }
    # PyTorch's CUDA 12.6 build covers GPUs up to the RTX 40 series; RTX 50 (Blackwell, compute
    # capability 10+) needs its CUDA 13.0 build, which in turn needs a CUDA 13 driver.
    $flavor = 'cu126'
    $capability = (& $nvidia.Source '--query-gpu=compute_cap' '--format=csv,noheader' 2>$null | Select-Object -First 1)
    $parsed = 0.0
    if ($capability -and [double]::TryParse(([string]$capability).Trim(), [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$parsed) -and $parsed -ge 10.0) {
        $smi = (& $nvidia.Source) -join "`n"
        $cuda = [regex]::Match($smi, 'CUDA (?:UMD )?Version:\s*(\d+)\.(\d+)')
        if (-not $cuda.Success -or [int]$cuda.Groups[1].Value -lt 13) { throw 'This NVIDIA GPU needs a newer graphics driver (one that supports CUDA 13). Update it from nvidia.com or the NVIDIA app, then retry. Nothing was installed.' }
        $flavor = 'cu130'
    }
    $lock = Join-Path $PSScriptRoot 'vocal-runtime.lock.txt'
    if ($flavor -ne 'cu126') {
        $pinned = (Get-Content -LiteralPath $lock -Raw) -replace '\+cu126', "+$flavor"
        $lock = Join-Path $env:TEMP "vocal-runtime.$flavor.lock.txt"
        Set-Content -LiteralPath $lock -Value $pinned -Encoding ASCII
    }
    Send-Status 'dependencies' 'Installing GPU and audio libraries. This can download several GB and take several minutes; Cancel stops setup.'
    Run-Checked $uv @('pip', 'install', '--python', $python, '--index-url', 'https://pypi.org/simple', '--extra-index-url', "https://download.pytorch.org/whl/$flavor", '--index-strategy', 'unsafe-best-match', '-r', $lock)
    Send-Status 'model' 'Checking GPU libraries and downloading the instrumental model (about 67 MB)...'
    Run-Checked $python @('-u', (Join-Path $PSScriptRoot 'vocal_prepare.py'), '--data', $DataRoot)
    Send-Status 'ready' 'Optional runtime is ready. Install VB-CABLE if needed, then refresh devices. Vocal mode stays off until you enable it.'
}

if ($LibraryOnly) { return }
try {
    $DataRoot = [IO.Path]::GetFullPath($DataRoot)
    switch ($Action) {
        'driver' { Prepare-Driver }
        'open-driver' { Show-Driver (Join-Path $DataRoot 'vocal-driver\Pack45\VBCABLE_Setup_x64.exe'); Send-Status 'ready' 'Installer selected in Explorer. Right-click and run as administrator; install the driver and restart Windows when requested. Then refresh devices.' }
        'runtime' { Prepare-Runtime }
    }
} catch {
    Send-Status 'error' $_.Exception.Message
    Write-Error -ErrorAction Continue $_
    exit 1
}
