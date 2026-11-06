# Synthetic archive/signature tests only. No network, driver launch or installation.
$ErrorActionPreference = 'Stop'
$taskTempRoot = [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) ('sudo-vocal-setup-test-' + [Guid]::NewGuid().ToString('N'))))
New-Item -ItemType Directory -Path $taskTempRoot | Out-Null
. (Join-Path $PSScriptRoot '..\src-tauri\resources\vocal_setup.ps1') -Action driver -DataRoot $taskTempRoot -LibraryOnly
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.IO.Compression
$script:passed = 0
function Assert-True([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message }; $script:passed++ }
function Make-Zip([string]$Name, [string[]]$Entries, [int]$Attributes=0) {
    $path=Join-Path $taskTempRoot ($Name + '.zip')
    $handle=[IO.Compression.ZipFile]::Open($path, [IO.Compression.ZipArchiveMode]::Create)
    try { foreach ($name in $Entries) { $entry=$handle.CreateEntry($name); if($Attributes){$entry.ExternalAttributes=$Attributes}; $writer=New-Object IO.StreamWriter($entry.Open()); try{$writer.Write('safe data')}finally{$writer.Dispose()} } } finally { $handle.Dispose() }
    return $path
}
function Reject-Zip([string]$Name, [string[]]$Entries, [int]$Attributes=0, [long]$Limit=134217728) {
    $path=Make-Zip $Name $Entries $Attributes
    $failed=$false
    try { Expand-SafeZip $path (Join-Path $taskTempRoot $Name) $Limit } catch { $failed=$true }
    Assert-True $failed ('Expected unsafe ZIP rejection: ' + $Name)
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $taskTempRoot $Name))) 'Unsafe archive must not create an extraction folder.'
}
try {
    $valid=Make-Zip 'valid' @('VBCABLE_Setup_x64.exe','sub/readme.txt')
    Expand-SafeZip $valid (Join-Path $taskTempRoot 'valid')
    Assert-True ((Get-Content -LiteralPath (Join-Path $taskTempRoot 'valid\sub\readme.txt')) -eq 'safe data') 'Safe nested archive must extract.'
    Reject-Zip 'traversal' @('../escape.exe')
    Reject-Zip 'backslash-traversal' @('sub\..\escape.exe')
    Reject-Zip 'rooted' @('/escape.exe')
    Reject-Zip 'drive' @('C:\escape.exe')
    Reject-Zip 'stream' @('safe.exe:payload')
    Reject-Zip 'duplicate' @('same.exe','SAME.exe')
    Reject-Zip 'trailing-dot' @('sub./escape.exe')
    Reject-Zip 'symlink' @('link') ([int](-1610612736))
    Reject-Zip 'reparse' @('link') 1024
    Reject-Zip 'oversize' @('too-large.txt') 0 2
    $signatureRejected=$false
    try{Assert-VBCableSignature (Join-Path $taskTempRoot 'valid\VBCABLE_Setup_x64.exe')}catch{$signatureRejected=$true}
    Assert-True $signatureRejected 'Unsigned synthetic installer must be rejected.'
    $missingRejected=$false
    try{Assert-VBCableSignature (Join-Path $taskTempRoot 'missing.exe')}catch{$missingRejected=$true}
    Assert-True $missingRejected 'Missing installer must be rejected.'
    $testShell=Join-Path $PSHOME 'powershell.exe'
    Run-Checked $testShell @('-NoLogo','-NoProfile','-NonInteractive','-Command','[Console]::WriteLine(42)')
    Assert-True $true 'Successful dependency process must complete.'
    $childFailure=$false
    try{Run-Checked $testShell @('-NoLogo','-NoProfile','-NonInteractive','-Command','exit 7')}catch{$childFailure=$true}
    Assert-True $childFailure 'Failed dependency process must stop setup.'
    Write-Output ('PASS: ' + $script:passed + ' optional setup assertions; no downloads or driver actions.')
} finally {
    $expectedParent=[IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if($taskTempRoot.StartsWith($expectedParent,[StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($taskTempRoot).StartsWith('sudo-vocal-setup-test-')) { Remove-Item -LiteralPath $taskTempRoot -Recurse -Force }
}
