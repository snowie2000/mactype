[CmdletBinding()]
param(
    [string] $CoreRoot = 'artifacts\open-core',
    [string] $OutputRoot = 'artifacts\service-runtime',
    [string] $Version = '0.2.0'
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$core = [System.IO.Path]::GetFullPath((Join-Path $root $CoreRoot))
$output = [System.IO.Path]::GetFullPath((Join-Path $root $OutputRoot))
$artifactBoundary = [System.IO.Path]::GetFullPath((Join-Path $root 'artifacts'))
if (-not $output.StartsWith($artifactBoundary + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Service runtime output must stay inside artifacts/: $output"
}
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$' -or $Version.Length -gt 64) {
    throw "Invalid service runtime version: $Version"
}
$packageBaseVersion = ($Version -split '[+-]', 2)[0]

function Assert-ExactRelativeFileSet(
    [string] $Path,
    [string[]] $Expected,
    [string] $FailureMessage
) {
    $actual = @(Get-ChildItem -LiteralPath $Path -Recurse -File | ForEach-Object {
        [System.IO.Path]::GetRelativePath($Path, $_.FullName)
    } | Sort-Object)
    $difference = @(Compare-Object @($Expected | Sort-Object) $actual)
    if ($difference.Count -ne 0) {
        $details = $difference | ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" }
        throw "$FailureMessage`: $($details -join ', ')"
    }
}

$ExpectedServiceRuntimeFiles = @(
    'mactype-service-setup.exe',
    'payload\manifest.json',
    'payload\files\mactype-service.exe',
    'payload\files\mactype-injector32.exe',
    'payload\files\mactype-injector64.exe',
    'payload\files\MacType.dll',
    'payload\files\MacType64.dll'
)

$manifestPath = Join-Path $root 'service-runtime\Cargo.toml'
$metadata = ((& cargo metadata --format-version 1 --no-deps --manifest-path $manifestPath) -join "`n") | ConvertFrom-Json
$runtimePackages = @($metadata.packages | Where-Object name -in @('mactype-service-host', 'mactype-service-setup'))
if ($runtimePackages.Count -ne 2 -or ($runtimePackages | Where-Object version -ne $packageBaseVersion)) {
    throw "Service runtime package base version does not match payload version $Version."
}

$hadServiceRuntimeVersion = Test-Path Env:\MACTYPE_SERVICE_RUNTIME_VERSION
$previousServiceRuntimeVersion = $env:MACTYPE_SERVICE_RUNTIME_VERSION
try {
    $env:MACTYPE_SERVICE_RUNTIME_VERSION = $Version
    cargo build `
        --manifest-path $manifestPath `
        --release `
        -p mactype-service-host `
        -p mactype-service-setup
}
finally {
    if ($hadServiceRuntimeVersion) {
        $env:MACTYPE_SERVICE_RUNTIME_VERSION = $previousServiceRuntimeVersion
    }
    else {
        Remove-Item Env:\MACTYPE_SERVICE_RUNTIME_VERSION -ErrorAction SilentlyContinue
    }
}

$target = Join-Path $root 'service-runtime\target\release'
$setupSource = Join-Path $target 'mactype-service-setup.exe'
$payloadSources = [ordered]@{
    'mactype-service.exe'     = (Join-Path $target 'mactype-service.exe')
    'mactype-injector32.exe' = (Join-Path $core 'mactype-injector32.exe')
    'mactype-injector64.exe' = (Join-Path $core 'mactype-injector64.exe')
    'MacType.dll'             = (Join-Path $core 'MacType.dll')
    'MacType64.dll'           = (Join-Path $core 'MacType64.dll')
}

foreach ($source in @($setupSource) + @($payloadSources.Values)) {
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Required service runtime build output is missing: $source"
    }
}

$payloadRoot = Join-Path $output 'payload'
$payloadFiles = Join-Path $payloadRoot 'files'
if (Test-Path -LiteralPath $output) {
    Remove-Item -LiteralPath $output -Recurse -Force
}
New-Item -ItemType Directory -Path $payloadFiles -Force | Out-Null
Copy-Item -LiteralPath $setupSource -Destination (Join-Path $output 'mactype-service-setup.exe') -Force

$manifestFiles = [ordered]@{}
foreach ($entry in $payloadSources.GetEnumerator()) {
    $destination = Join-Path $payloadFiles $entry.Key
    Copy-Item -LiteralPath $entry.Value -Destination $destination -Force
    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifestFiles[$entry.Key] = "sha256:$hash"
}

$manifest = [ordered]@{
    schema = 1
    version = $Version
    files = $manifestFiles
} | ConvertTo-Json -Depth 4 -Compress
[System.IO.File]::WriteAllText((Join-Path $payloadRoot 'manifest.json'), $manifest, [System.Text.UTF8Encoding]::new($false))

Assert-ExactRelativeFileSet `
    -Path $output `
    -Expected $ExpectedServiceRuntimeFiles `
    -FailureMessage 'Unexpected service-runtime artifact set'

Write-Host "Open service runtime payload produced at $output."
