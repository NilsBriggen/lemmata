# Builds the example document on Windows in the situations that have gone wrong
# for users, and prints how long each took. Run by .github/workflows/windows.yml
# after MiKTeX is installed:
#
#   pwsh scripts/windows-smoke.ps1 target/debug/lemmata.exe
#
# Scenarios (each builds a fresh copy, then rebuilds it after an edit):
#   latexmk with Perl      the normal path
#   Perl missing           latexmk cannot start; Lemmata builds with pdflatex itself
#   Perl fails to start    a Perl that exits with an error (stands in for a blocked one)
param([Parameter(Mandatory = $true)][string]$Cli)
$ErrorActionPreference = "Stop"
$Cli = (Resolve-Path $Cli).Path
$work = Join-Path $env:RUNNER_TEMP "smoke"
New-Item -ItemType Directory -Force $work | Out-Null
$results = @()
$failures = @()

function Fresh([string]$name) {
    $dir = Join-Path $work $name
    if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    Copy-Item templates/example $dir -Recurse
    Remove-Item (Join-Path $dir "template.toml")
    return $dir
}

function Build([string]$dir, [string]$scenario, [string]$step, [bool]$expectFallback) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $text = (& $Cli build $dir 2>&1 | Out-String)
    $code = $LASTEXITCODE
    $sw.Stop()
    # This build's own result: the PDF of an earlier build may still be there.
    $pdf = ($code -eq 0) -and ($text -match "main\.pdf in \d+ ms")
    $fellBack = $text -match "built with pdflatex directly"
    Write-Host "::group::$scenario / $step ($([math]::Round($sw.Elapsed.TotalSeconds, 1)) s)"
    Write-Host $text
    Write-Host "::endgroup::"
    if (-not $pdf) { $script:failures += "${scenario} / ${step}: the build wrote no PDF" }
    if ($fellBack -ne $expectFallback) { $script:failures += "${scenario} / ${step}: expected fallback=$expectFallback, got $fellBack" }
    $script:results += [pscustomobject]@{ Scenario = $scenario; Step = $step; Seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1); Pdf = $pdf; Direct = $fellBack }
}

# After a good build, a rebuild following an edit calls pdflatex directly on purpose (no latexmk, no
# "could not run" note), whichever way the first build went.
function Rebuild([string]$dir, [string]$scenario) {
    Add-Content (Join-Path $dir "main.tex") "`n% edit"
    Build $dir $scenario "after an edit" $false
}

# TeX detection is what the start screen waits for.
$sw = [Diagnostics.Stopwatch]::StartNew()
& $Cli doctor 2>&1 | Out-String | Write-Host
$sw.Stop()
$results += [pscustomobject]@{ Scenario = "lemmata doctor"; Step = "TeX detection"; Seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1); Pdf = $null; Direct = $null }

# The first build installs packages on the fly; keep that out of the comparison. MiKTeX's mirrors time out
# now and then, so it gets a few tries.
$warm = Fresh "warmup"
foreach ($i in 1..3) {
    & $Cli build $warm 2>&1 | Out-String | Write-Host
    if (Test-Path (Join-Path $warm ".lemmata/build/main.pdf")) { break }
}
if (-not (Test-Path (Join-Path $warm ".lemmata/build/main.pdf"))) { $failures += "warm-up: no PDF after 3 tries (MiKTeX could not install the packages)" }

$original = $env:PATH

# 1. latexmk with Perl on the PATH, as on a machine that works.
$dir = Fresh "with-perl"
Build $dir "latexmk with Perl" "first build" $false
Rebuild $dir "latexmk with Perl"

# 2. No Perl anywhere. Lemmata also looks in Strawberry's and Git's default folders, so hide those too.
$hidden = @()
foreach ($p in @("C:\Strawberry", "C:\Program Files\Git\usr\bin\perl.exe")) {
    if (Test-Path $p) { Rename-Item $p ((Split-Path $p -Leaf) + ".off"); $hidden += $p }
}
try {
    $env:PATH = (($original -split ";") | Where-Object { $_ -notmatch "Strawberry|perl|\\Git\\usr|msys" }) -join ";"
    $dir = Fresh "no-perl"
    Build $dir "Perl missing" "first build" $true
    Rebuild $dir "Perl missing"
} finally {
    foreach ($p in $hidden) { Rename-Item ($p + ".off") (Split-Path $p -Leaf) -ErrorAction Continue }
    $env:PATH = $original
}

# 3. A Perl that starts and fails, first on the PATH.
$fake = Join-Path $work "fake-perl"
New-Item -ItemType Directory -Force $fake | Out-Null
Set-Content (Join-Path $fake "perl.rs") 'fn main() { eprintln!("fake perl: blocked"); std::process::exit(1); }'
& rustc (Join-Path $fake "perl.rs") -o (Join-Path $fake "perl.exe")
$env:PATH = "$fake;$env:PATH"
$dir = Fresh "failing-perl"
Build $dir "Perl fails to start" "first build" $true
Rebuild $dir "Perl fails to start"
$env:PATH = $original

$table = $results | Format-Table -AutoSize | Out-String
Write-Host $table
$md = "| Scenario | Step | Seconds | PDF | Built without latexmk |`n|---|---|---|---|---|`n"
foreach ($r in $results) { $md += "| $($r.Scenario) | $($r.Step) | $($r.Seconds) | $($r.Pdf) | $($r.Direct) |`n" }
if ($env:GITHUB_STEP_SUMMARY) { Add-Content $env:GITHUB_STEP_SUMMARY "## Windows build timings`n`n$md" }
if ($failures.Count) {
    $failures | ForEach-Object { Write-Host "::error::$_" }
    exit 1
}
