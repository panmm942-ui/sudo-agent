# Dot-source before working on sudo (PowerShell):  . <repo>\dev.ps1
# Points every tool cache this project uses at one data root, $env:SUDO_DATA:
#   - an existing $env:SUDO_DATA is kept;
#   - else F:\sudo-data, then D:\sudo-data, if present (the dev box: C: is nearly full);
#   - else <repo>\.data (gitignored), e.g. on a single-drive laptop.
if (-not $env:SUDO_DATA) {
    if (Test-Path -PathType Container 'F:\sudo-data\cargo') { $env:SUDO_DATA = 'F:\sudo-data' }
    elseif (Test-Path -PathType Container 'D:\sudo-data') { $env:SUDO_DATA = 'D:\sudo-data' }
    else { $env:SUDO_DATA = Join-Path $PSScriptRoot '.data' }
}
$data = $env:SUDO_DATA
foreach ($d in 'uv-cache', 'uv-python', 'hf', 'cuda-cache', 'tmp') {
    New-Item -ItemType Directory -Force (Join-Path $data $d) | Out-Null
}
$env:UV_CACHE_DIR      = "$data\uv-cache"
$env:UV_PYTHON_INSTALL_DIR = "$data\uv-python"
$env:HF_HOME           = "$data\hf"
$env:CUDA_CACHE_PATH   = "$data\cuda-cache"
$env:CUDA_CACHE_MAXSIZE = '4294967296'
$env:RUSTUP_HOME       = "$data\rustup"
$env:CARGO_HOME        = "$data\cargo"
$env:npm_config_cache  = "$data\npm-cache"
$env:PLAYWRIGHT_BROWSERS_PATH = "$data\ms-playwright"
$env:TEMP = "$data\tmp"; $env:TMP = "$data\tmp"
if ($env:Path -notlike "*$data\cargo\bin*") { $env:Path = "$data\cargo\bin;$env:Path" }
