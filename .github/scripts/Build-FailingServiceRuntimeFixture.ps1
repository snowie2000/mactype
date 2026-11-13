[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $SourceRoot,
    [string] $OutputRoot = 'artifacts\service-runtime-failing-upgrade',
    [Parameter(Mandatory)]
    [string] $Version
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$source = [IO.Path]::GetFullPath((Join-Path $root $SourceRoot))
$output = [IO.Path]::GetFullPath((Join-Path $root $OutputRoot))
$artifactBoundary = [IO.Path]::GetFullPath((Join-Path $root 'artifacts'))
foreach ($path in @($source, $output)) {
    if (-not $path.StartsWith($artifactBoundary + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "The test-only service fixture must stay inside artifacts/: $path"
    }
}
if ($source.Equals($output, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The test-only failing fixture cannot overwrite its valid source payload.'
}
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$' -or $Version.Length -gt 64) {
    throw "Invalid failing fixture version: $Version"
}

$expectedFiles = @(
    'mactype-service-setup.exe',
    'payload\manifest.json',
    'payload\files\mactype-service.exe',
    'payload\files\mactype-injector32.exe',
    'payload\files\mactype-injector64.exe',
    'payload\files\MacType.dll',
    'payload\files\MacType64.dll'
)
if (-not (Test-Path -LiteralPath $source -PathType Container)) {
    throw "Valid source payload is missing: $source"
}
if (Test-Path -LiteralPath $output) {
    Remove-Item -LiteralPath $output -Recurse -Force
}
Copy-Item -LiteralPath $source -Destination $output -Recurse

$broker = Join-Path $output 'mactype-service-setup.exe'
$service = Join-Path $output 'payload\files\mactype-service.exe'
Copy-Item -LiteralPath $broker -Destination $service -Force

$manifestPath = Join-Path $output 'payload\manifest.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -AsHashtable
$manifest.version = $Version
foreach ($name in @($manifest.files.Keys)) {
    $hash = (Get-FileHash -LiteralPath (Join-Path $output "payload\files\$name") -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifest.files[$name] = "sha256:$hash"
}
[IO.File]::WriteAllText(
    $manifestPath,
    ($manifest | ConvertTo-Json -Depth 4 -Compress),
    [Text.UTF8Encoding]::new($false)
)

$actualFiles = @(Get-ChildItem -LiteralPath $output -Recurse -File | ForEach-Object {
    [IO.Path]::GetRelativePath($output, $_.FullName)
} | Sort-Object)
if (Compare-Object @($expectedFiles | Sort-Object) $actualFiles) {
    throw 'The test-only failing fixture contains an unexpected artifact set.'
}
if ((Get-FileHash -LiteralPath $broker -Algorithm SHA256).Hash -cne
    (Get-FileHash -LiteralPath $service -Algorithm SHA256).Hash) {
    throw 'The test-only service replacement was not exact.'
}

Write-Host "Created test-only failing service-start payload at $output."
