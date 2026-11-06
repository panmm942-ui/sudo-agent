param(
    [Parameter(Mandatory=$true)][ValidateSet('llm','voice','all','router')][string]$Action,
    [Parameter(Mandatory=$true)][string]$DataRoot
)
# Explicit, user-started setup for the optional local Sudo AI assistant.
# Every download is pinned (release tag / model revision) and checked against a published SHA-256.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# Shared helpers (Expand-SafeZip, Find-Uv, Run-Checked) from the vocal setup script.
$JarvisAction = $Action
. (Join-Path $PSScriptRoot 'vocal_setup.ps1') -Action runtime -DataRoot $DataRoot -LibraryOnly
$Action = $JarvisAction  # dot-sourcing rebinds the shared parameter names
# Settings shows "Step N of 4" and, for downloads, bytes done / total.
$Steps = 4
function Send-Status([string]$Phase, [string]$Message, [int]$Step = 0, [long]$Done = -1, [long]$Total = -1) {
    $status = @{phase=$Phase; component=$Action; message=$Message}
    if ($Step -gt 0) { $status.step = $Step; $status.steps = $Steps }
    if ($Total -gt 0) { $status.done = $Done; $status.total = $Total }
    [Console]::Out.WriteLine(($status | ConvertTo-Json -Compress))
}

$LlamaTag = 'b12128'
$LlamaAssets = @{
    '13' = @(@{name="llama-$LlamaTag-bin-win-cuda-13.4-x64.zip"; size=170771094; sha='b93b77ba35c63ae1264fb9eba3b5dfc1d088ffd854ce7eff53c9d97864cf55f1'},
             @{name='cudart-llama-bin-win-cuda-13.4-x64.zip'; size=423535356; sha='738f8c251ac22b70c3ae6f83a10cf222725df0395246a2cf58f32bdb85fbe668'})
    '12' = @(@{name="llama-$LlamaTag-bin-win-cuda-12.4-x64.zip"; size=288918265; sha='8cbd5e64dcb61160ea6dca16e7d353536de593851cefc5a2dc27f3478eae5e52'},
             @{name='cudart-llama-bin-win-cuda-12.4-x64.zip'; size=391443627; sha='8c79a9b226de4b3cacfd1f83d24f962d0773be79f1e7b75c6af4ded7e32ae1d6'})
}
$QwenRepo = 'HauhauCS/Qwen3.5-4B-Uncensored-HauhauCS-Aggressive'
$QwenRevision = 'c09cdbcdb1fefad6d335809d445621b5f5ba0c6e'
$QwenFile = 'Qwen3.5-4B-Uncensored-HauhauCS-Aggressive-Q4_K_M.gguf'
$QwenSha = '79e28ecacf84e75b6056cf4059636d435aa9eb67795780f7b7dbc7d32a962741'
# Optional fast commands: GLiNER2.5-multi-Decide (Apache-2.0), pinned to one revision.
$GlinerRepo = 'fastino/GLiNER2.5-multi-Decide'
$GlinerRevision = '36e46e4c94b7307e8bd59b23b6dfa36519a23269'
$GlinerFiles = @(
    @{path='config.json'; size=4106; sha='aa29cd584316b61b0bfed94d03ee1cfdaf28b46a37686293742929c49b8bd2f7'},
    @{path='encoder_config/config.json'; size=858; sha='d0ebbcb8b458e285a39e12cc315cbaf3d1c6f631e7281e6b22dd5b4071183f83'},
    @{path='tokenizer_config.json'; size=646; sha='fd4a31dc2f1f17e31638c5f0e783b81cdb2fbe6bddd116a8d9e5d50d78148cf1'},
    @{path='tokenizer.json'; size=16035853; sha='c62446df87ae18ec98b133f8f84fc449a07cc89bbf8ef192a4cb5f9c53777a7a'},
    @{path='model.safetensors'; size=1149461028; sha='9efe0f88c99f2aa794452e9559dc60e98d60d9fa2bf1b60cf2710411b6da5b4e'}
)

function Get-Verified([string]$Uri, [string]$Target, [string]$Sha256, [string]$Label, [int]$Step, [long]$Size) {
    # Resumable download through Windows' own curl.exe, published only after its hash matches.
    if ((Test-Path -LiteralPath $Target -PathType Leaf) -and (Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash -eq $Sha256) { return }
    New-Item -ItemType Directory -Force -Path (Split-Path $Target) | Out-Null
    $part = $Target + '.part'
    $start = New-Object Diagnostics.ProcessStartInfo (Join-Path $env:SystemRoot 'System32\curl.exe')
    $start.Arguments = '--fail --location --silent --show-error --retry 3 --continue-at - --output "' + $part + '" "' + $Uri + '"'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $curl = [Diagnostics.Process]::Start($start)
    while (-not $curl.WaitForExit(800)) {
        $done = if (Test-Path -LiteralPath $part) { (Get-Item -LiteralPath $part).Length } else { 0 }
        Send-Status 'downloading' $Label $Step $done $Size
    }
    if ($curl.ExitCode -ne 0) { throw "Download failed ($Label). Check your internet connection and press Set up Sudo AI again: finished parts are kept." }
    Send-Status 'verifying' 'Checking the download is complete and genuine...' $Step $Size $Size
    if ((Get-FileHash -LiteralPath $part -Algorithm SHA256).Hash -ne $Sha256) { Remove-Item -LiteralPath $part; throw "Checksum mismatch for $(Split-Path $Target -Leaf). Retry setup." }
    Move-Item -LiteralPath $part -Destination $Target -Force
}

function Run-WithStatus([string]$Program, [string[]]$Arguments) {
    # Like Run-Checked, but the child's own JSON status lines reach the app (progress).
    $previous = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        & $Program @Arguments 2>&1 | ForEach-Object {
            $line = [string]$_
            if ($_ -isnot [Management.Automation.ErrorRecord] -and $line.StartsWith('{"phase"')) { [Console]::Out.WriteLine($line) }
            else { [Console]::Error.WriteLine($line) }
        }
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previous }
    if ($code -ne 0) { throw 'Speech model setup failed. Press Set up Sudo AI again; see jarvis-setup.log if it keeps failing.' }
}

function Cuda-Version {
    $nvidia = Get-Command 'nvidia-smi.exe' -ErrorAction SilentlyContinue
    if ($null -eq $nvidia) { throw 'An NVIDIA GPU with a current driver is required. Nothing was downloaded.' }
    $text = (& $nvidia.Source) -join "`n"
    $match = [regex]::Match($text, 'CUDA (?:UMD )?Version:\s*(\d+)\.(\d+)')
    if (-not $match.Success) { throw 'The NVIDIA driver did not report its CUDA version. Update the graphics driver.' }
    return [version]"$($match.Groups[1].Value).$($match.Groups[2].Value)"
}

function Prepare-Llm {
    $cuda = Cuda-Version
    # A CUDA 13.4 build's PTX needs a driver that supports 13.4 itself ("unsupported toolchain" otherwise).
    $flavor = if ($cuda -ge [version]'13.4') { '13' } elseif ($cuda.Major -ge 12) { '12' } else { throw 'The NVIDIA driver is too old for the local model (CUDA 12 or newer). Update it and retry.' }
    $build = "$LlamaTag-cuda$flavor"
    $marker = Join-Path $DataRoot 'jarvis-llm\jarvis-build.txt'
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf) -or (Get-Content -LiteralPath $marker -Raw).Trim() -ne $build) {
        $root = Join-Path $DataRoot 'jarvis-downloads'
        $staging = Join-Path $DataRoot ('jarvis-llm-staging-' + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Force -Path $staging | Out-Null
        foreach ($asset in $LlamaAssets[$flavor]) {
            $zip = Join-Path $root $asset.name
            Get-Verified "https://github.com/ggml-org/llama.cpp/releases/download/$LlamaTag/$($asset.name)" $zip $asset.sha 'Downloading the AI model server' 1 $asset.size
            $unpacked = Join-Path $staging ([IO.Path]::GetFileNameWithoutExtension($asset.name))
            Expand-SafeZip $zip $unpacked 2147483648
            Get-ChildItem -LiteralPath $unpacked -Recurse -File | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $staging }
            Remove-Item -LiteralPath $unpacked -Recurse -Force
        }
        if (-not (Test-Path -LiteralPath (Join-Path $staging 'llama-server.exe'))) { throw 'llama-server.exe is missing from the downloaded package.' }
        $final = Join-Path $DataRoot 'jarvis-llm'
        if (Test-Path -LiteralPath $final) { Remove-Item -LiteralPath $final -Recurse -Force }
        Set-Content -LiteralPath (Join-Path $staging 'jarvis-build.txt') -Value $build -Encoding ASCII
        Move-Item -LiteralPath $staging -Destination $final
    }
    Get-Verified "https://huggingface.co/$QwenRepo/resolve/$QwenRevision/$QwenFile" (Join-Path $DataRoot "jarvis-models\$QwenFile") $QwenSha 'Downloading the Qwen3.5 4B AI model' 2 2707513696
}

function Prepare-Voice {
    foreach ($folder in 'tmp', 'uv-cache', 'uv-python', 'jarvis-models') { New-Item -ItemType Directory -Force -Path (Join-Path $DataRoot $folder) | Out-Null }
    $env:UV_CACHE_DIR = Join-Path $DataRoot 'uv-cache'
    $env:UV_PYTHON_INSTALL_DIR = Join-Path $DataRoot 'uv-python'
    $env:UV_PYTHON_PREFERENCE = 'only-managed'
    $env:TEMP = Join-Path $DataRoot 'tmp'
    $env:TMP = $env:TEMP
    $env:HF_HOME = Join-Path $DataRoot 'hf'
    $env:PYTHONNOUSERSITE = '1'
    $env:PYTHONPATH = ''
    $null = Cuda-Version
    $uv = Find-Uv
    $python = Join-Path $DataRoot 'jarvis-env\Scripts\python.exe'
    if (-not (Test-Path -LiteralPath $python -PathType Leaf)) {
        Send-Status 'python' 'Installing a private copy of Python (does not change your system)...' 3
        Run-Checked $uv @('venv', '--python', '3.12', '--python-preference', 'only-managed', (Join-Path $DataRoot 'jarvis-env'))
    }
    Send-Status 'dependencies' 'Installing speech libraries (about 1.7 GB, a few minutes)...' 3
    Run-Checked $uv @('pip', 'install', '--python', $python, '--index-url', 'https://pypi.org/simple', '-r', (Join-Path $PSScriptRoot 'jarvis-runtime.lock.txt'))
    Send-Status 'model' 'Downloading speech recognition and voice models' 4
    Run-WithStatus $python @('-u', '-I', (Join-Path $PSScriptRoot 'jarvis_prepare.py'), '--data', $DataRoot)
}

function Prepare-Router {
    # CPU only: no GPU needed. A separate environment, so the speech libraries are never touched.
    $script:Steps = 2
    foreach ($folder in 'tmp', 'uv-cache', 'uv-python', 'jarvis-models') { New-Item -ItemType Directory -Force -Path (Join-Path $DataRoot $folder) | Out-Null }
    $env:UV_CACHE_DIR = Join-Path $DataRoot 'uv-cache'
    $env:UV_PYTHON_INSTALL_DIR = Join-Path $DataRoot 'uv-python'
    $env:UV_PYTHON_PREFERENCE = 'only-managed'
    $env:TEMP = Join-Path $DataRoot 'tmp'
    $env:TMP = $env:TEMP
    $env:PYTHONNOUSERSITE = '1'
    $env:PYTHONPATH = ''
    $uv = Find-Uv
    $python = Join-Path $DataRoot 'jarvis-router-env\Scripts\python.exe'
    if (-not (Test-Path -LiteralPath $python -PathType Leaf)) {
        Send-Status 'python' 'Preparing a private Python for fast commands (does not change your system)...' 1
        Run-Checked $uv @('venv', '--python', '3.12', '--python-preference', 'only-managed', (Join-Path $DataRoot 'jarvis-router-env'))
    }
    Send-Status 'dependencies' 'Installing the command classifier libraries (about 250 MB download)...' 1
    Run-Checked $uv @('pip', 'install', '--python', $python, '--index-url', 'https://download.pytorch.org/whl/cpu', '--extra-index-url', 'https://pypi.org/simple', '-r', (Join-Path $PSScriptRoot 'jarvis-router.lock.txt'))
    $folder = Join-Path $DataRoot 'jarvis-models\gliner'
    foreach ($file in $GlinerFiles) {
        $target = Join-Path $folder ($file.path -replace '/', '\')
        Get-Verified "https://huggingface.co/$GlinerRepo/resolve/$GlinerRevision/$($file.path)" $target $file.sha 'Downloading the GLiNER command classifier' 2 $file.size
    }
}

try {
    $DataRoot = [IO.Path]::GetFullPath($DataRoot)
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'Sudo AI currently requires Windows x64.' }
    if ($Action -in 'llm', 'all') { Prepare-Llm }
    if ($Action -in 'voice', 'all') { Prepare-Voice }
    if ($Action -eq 'router') { Prepare-Router; Send-Status 'ready' 'Fast commands are ready. They start the next time you wake Sudo.'; exit 0 }
    Send-Status 'ready' 'Sudo AI is ready. Hold Right Ctrl and speak, or type a question.'
} catch {
    Send-Status 'error' $_.Exception.Message
    Write-Error -ErrorAction Continue $_
    exit 1
}
