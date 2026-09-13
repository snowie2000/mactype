[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$crateRoot = Join-Path $root 'control-center\src-tauri'
$exceptionsPath = Join-Path $root 'security\rust-advisory-exceptions.json'
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargoCommand) {
    $cargo = $cargoCommand.Source
}
else {
    $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
    if (-not (Test-Path -LiteralPath $cargo -PathType Leaf)) {
        throw 'Cargo was not found on PATH or in the standard rustup user directory.'
    }
}
$document = Get-Content -LiteralPath $exceptionsPath -Raw | ConvertFrom-Json
$exceptions = @($document.exceptions)

if ($exceptions.Count -ne 1) {
    throw 'Rust advisory exceptions must remain an explicit, individually reviewed list.'
}

$exception = $exceptions[0]
if ($exception.advisory -ne 'RUSTSEC-2024-0429' -or
    $exception.ghsa -ne 'GHSA-wrw7-89jp-8q8g' -or
    $exception.package -ne 'glib') {
    throw 'The checked advisory exception does not match Dependabot alert #1.'
}

$expectedTargets = @('i686-pc-windows-msvc', 'x86_64-pc-windows-msvc')
$actualTargets = @($exception.supportedTargets | Sort-Object)
if ($actualTargets.Count -ne $expectedTargets.Count -or
    (Compare-Object -ReferenceObject $expectedTargets -DifferenceObject $actualTargets)) {
    throw 'The advisory exception must cover both supported Windows Rust targets.'
}

$reviewAfter = [DateTime]::ParseExact(
    $exception.reviewAfter,
    'yyyy-MM-dd',
    [Globalization.CultureInfo]::InvariantCulture
)
if ([DateTime]::UtcNow.Date -gt $reviewAfter.Date) {
    throw "The $($exception.advisory) target-scope exception expired on $($exception.reviewAfter)."
}

function Invoke-CargoTree([string[]] $Arguments) {
    $output = @(& $cargo tree @Arguments 2>&1 | ForEach-Object { $_.ToString() })
    if ($LASTEXITCODE -ne 0) {
        throw "cargo tree failed: $($output -join [Environment]::NewLine)"
    }
    return $output -join [Environment]::NewLine
}

Push-Location $crateRoot
try {
    $allTargets = Invoke-CargoTree @(
        '--locked',
        '--target',
        'all',
        '--invert',
        "$($exception.package)@$($exception.lockedVersion)"
    )
    if ($allTargets -notmatch '(?m)^glib v0\.18\.5\r?$' -or $allTargets -notmatch '(?m)^.*tauri v') {
        throw 'The glib exception is stale or no longer originates from the Tauri platform graph.'
    }

    foreach ($target in $actualTargets) {
        $supportedTree = Invoke-CargoTree @('--locked', '--target', $target, '--invert', $exception.package)
        if ($supportedTree -match '(?m)^glib v') {
            throw "$($exception.advisory) is reachable on supported target $target."
        }
    }
}
finally {
    Pop-Location
}

Write-Host "PASS: $($exception.advisory) is absent from every supported Windows target"
