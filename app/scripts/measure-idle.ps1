# Samples CPU % and memory of sudo-island.exe and its msedgewebview2.exe descendants every
# second and prints mean/max, plus a per-process breakdown. Usage: .\measure-idle.ps1 [-Seconds 60] [-Id <pid>]
# -Id picks one instance when several sudo-island.exe run (e.g. a debug build next to the release).
# "private MB" is private bytes (commit charge, PrivateMemorySize64); "working set MB" is the
# total working set, which double-counts pages shared between the WebView2 processes.
param([int]$Seconds = 60, [int]$Id = 0)

$ErrorActionPreference = 'Stop'
$cores = [Environment]::ProcessorCount

function Get-TreePids {
    $filter = if ($Id) { "ProcessId = $Id" } else { "Name = 'sudo-island.exe'" }
    $roots = @(Get-CimInstance Win32_Process -Filter $filter)
    if (-not $roots) { throw 'sudo-island.exe is not running' }
    if ($roots.Count -gt 1) { throw "$($roots.Count) sudo-island.exe instances running; pass -Id <pid>" }
    $root = $roots[0]
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
    $pids = [System.Collections.Generic.HashSet[int]]::new()
    [void]$pids.Add([int]$root.ProcessId)
    do {
        $added = $false
        foreach ($p in $all) {
            if ($pids.Contains([int]$p.ParentProcessId) -and $pids.Add([int]$p.ProcessId)) { $added = $true }
        }
    } while ($added)
    return $pids
}

$pids = Get-TreePids
$prev = @{}
foreach ($id in $pids) { $p = Get-Process -Id $id -ErrorAction SilentlyContinue; if ($p) { $prev[$id] = $p.TotalProcessorTime.TotalMilliseconds } }
$cpu = @(); $ram = @(); $wss = @()
$perProc = @{}
$clock = [Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt $Seconds; $i++) {
    $t0 = $clock.Elapsed.TotalMilliseconds
    Start-Sleep -Milliseconds 1000
    $dt = $clock.Elapsed.TotalMilliseconds - $t0
    $busy = 0.0; $priv = 0; $ws = 0
    foreach ($id in @($pids)) {
        $p = Get-Process -Id $id -ErrorAction SilentlyContinue
        if (-not $p) { continue }
        $now = $p.TotalProcessorTime.TotalMilliseconds
        $d = if ($prev.ContainsKey($id)) { $now - $prev[$id] } else { 0 }
        $busy += $d
        $prev[$id] = $now
        $priv += $p.PrivateMemorySize64
        $ws += $p.WorkingSet64
        if (-not $perProc.ContainsKey($id)) { $perProc[$id] = @{ name = $p.ProcessName; cpu = 0.0; priv = 0.0; n = 0 } }
        $e = $perProc[$id]; $e.cpu += $d; $e.priv += $p.PrivateMemorySize64 / 1MB; $e.n++
    }
    $cpu += 100.0 * $busy / ($dt * $cores)
    $ram += $priv / 1MB
    $wss += $ws / 1MB
}
$elapsed = $clock.Elapsed.TotalMilliseconds

$c = $cpu | Measure-Object -Average -Maximum
$r = $ram | Measure-Object -Average -Maximum
$w = $wss | Measure-Object -Average -Maximum
"processes: $($pids.Count) (sudo-island.exe + descendants), $Seconds samples"
"cpu %  (of all $cores logical cores)  mean {0:N2}  max {1:N2}" -f $c.Average, $c.Maximum
"private MB                       mean {0:N1}  max {1:N1}" -f $r.Average, $r.Maximum
"working set MB (shared counted)  mean {0:N1}  max {1:N1}" -f $w.Average, $w.Maximum
""
"per process (mean):"
$perProc.GetEnumerator() | ForEach-Object {
    [pscustomobject]@{
        pid        = $_.Key
        name       = $_.Value.name
        'cpu %'    = [math]::Round(100.0 * $_.Value.cpu / ($elapsed * $cores), 2)
        'private MB' = [math]::Round($_.Value.priv / [math]::Max(1, $_.Value.n), 1)
    }
} | Sort-Object 'private MB' -Descending | Format-Table -AutoSize | Out-String -Width 120
