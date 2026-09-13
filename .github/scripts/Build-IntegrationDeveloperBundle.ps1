[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $ApplicationExe,
    [Parameter(Mandatory)]
    [string] $PreviewHelper,
    [Parameter(Mandatory)]
    [string] $CoreRoot,
    [Parameter(Mandatory)]
    [string] $ServiceRuntimeRoot,
    [Parameter(Mandatory)]
    [string] $OutputRoot
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$output = [IO.Path]::GetFullPath((Join-Path $repositoryRoot $OutputRoot))
$separators = [char[]]@([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
$repositoryPrefix = $repositoryRoot.TrimEnd($separators) + [IO.Path]::DirectorySeparatorChar
if (-not $output.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Integration bundle output must stay inside the repository: $output"
}

$distributionRoot = Join-Path $repositoryRoot 'distribution'
foreach ($requiredFile in @(
    $ApplicationExe,
    $PreviewHelper,
    (Join-Path $CoreRoot 'MacType.dll'),
    (Join-Path $CoreRoot 'MacType64.dll'),
    (Join-Path $CoreRoot 'MacType.Core.dll'),
    (Join-Path $CoreRoot 'MacType64.Core.dll'),
    (Join-Path $CoreRoot 'MacLoader.exe'),
    (Join-Path $CoreRoot 'MacLoader64.exe'),
    (Join-Path $ServiceRuntimeRoot 'mactype-service-setup.exe'),
    (Join-Path (Join-Path $ServiceRuntimeRoot 'payload') 'manifest.json'),
    (Join-Path $distributionRoot 'INTEGRATION_DEVELOPER_README.md')
)) {
    if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
        throw "Integration bundle input is missing: $requiredFile"
    }
}

if (Test-Path -LiteralPath $output) {
    Remove-Item -LiteralPath $output -Recurse -Force
}
$tree = Join-Path $output 'installation-tree'
New-Item -ItemType Directory -Path $tree -Force | Out-Null

$rootFiles = [ordered]@{
    $ApplicationExe = 'MacType Control Center.exe'
    $PreviewHelper = 'mactype-preview32.exe'
    (Join-Path $CoreRoot 'MacType.dll') = 'MacType.dll'
    (Join-Path $CoreRoot 'MacType64.dll') = 'MacType64.dll'
    (Join-Path $CoreRoot 'MacType.Core.dll') = 'MacType.Core.dll'
    (Join-Path $CoreRoot 'MacType64.Core.dll') = 'MacType64.Core.dll'
    (Join-Path $CoreRoot 'MacLoader.exe') = 'MacLoader.exe'
    (Join-Path $CoreRoot 'MacLoader64.exe') = 'MacLoader64.exe'
    (Join-Path $distributionRoot 'MacType.ini') = 'MacType.ini'
    (Join-Path $distributionRoot 'THIRD_PARTY_NOTICES.md') = 'THIRD_PARTY_NOTICES.md'
    (Join-Path $repositoryRoot 'LICENSE') = 'LICENSE.txt'
}
foreach ($entry in $rootFiles.GetEnumerator()) {
    Copy-Item -LiteralPath $entry.Key -Destination (Join-Path $tree $entry.Value)
}

Copy-Item -LiteralPath (Join-Path $distributionRoot 'ini') -Destination (Join-Path $tree 'ini') -Recurse
Copy-Item -LiteralPath (Join-Path $distributionRoot 'languages') -Destination (Join-Path $tree 'languages') -Recurse
Copy-Item -LiteralPath $ServiceRuntimeRoot -Destination (Join-Path $tree 'service-runtime') -Recurse
Copy-Item -LiteralPath (Join-Path $distributionRoot 'INTEGRATION_DEVELOPER_README.md') -Destination (Join-Path $output 'README.md')

$installedServiceRuntime = Join-Path $tree 'service-runtime'
$installedPayload = Join-Path (Join-Path $installedServiceRuntime 'payload') 'files'
$manifest = Get-Content -LiteralPath (
    Join-Path (Join-Path $installedServiceRuntime 'payload') 'manifest.json'
) -Raw | ConvertFrom-Json
foreach ($payloadName in $manifest.files.PSObject.Properties.Name) {
    $payloadPath = Join-Path $installedPayload $payloadName
    if (-not (Test-Path -LiteralPath $payloadPath -PathType Leaf)) {
        throw "Integration bundle does not reproduce the manifest payload: $payloadName"
    }
}

Write-Host "Integration/Developer bundle staged at $output"
