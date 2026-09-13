Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-OpenServiceRepositoryTextFiles {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Root,
        [Parameter(Mandatory)] [string[]] $RelativeRoots
    )

    $files = foreach ($relativeRoot in $RelativeRoots) {
        $path = Join-Path $Root $relativeRoot
        if (-not (Test-Path -LiteralPath $path)) { continue }
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            Get-Item -LiteralPath $path
            continue
        }
        Get-ChildItem -LiteralPath $path -Recurse -File | Where-Object {
            $_.FullName -notmatch '[\/](?:target|dist|node_modules|artifacts|build|gen)[\/]'
        }
    }
    $files | Sort-Object FullName -Unique
}

function Get-OpenServiceFunctionRegion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Text,
        [Parameter(Mandatory)] [string] $FunctionName
    )

    $name = [regex]::Escape($FunctionName)
    $pattern = "(?ms)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+$name\b.*?(?=^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+\w+\b|\z)"
    $match = [regex]::Match($Text, $pattern)
    if ($match.Success) { return $match.Value }
    return $null
}

function Get-OpenServiceEnumRegion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Text,
        [Parameter(Mandatory)] [string] $EnumName
    )

    $name = [regex]::Escape($EnumName)
    $match = [regex]::Match(
        $Text,
        "(?ms)\benum\s+$name\s*\{.*?\}"
    )
    if ($match.Success) { return $match.Value }
    return $null
}

Export-ModuleMember -Function @(
    'Get-OpenServiceRepositoryTextFiles',
    'Get-OpenServiceFunctionRegion',
    'Get-OpenServiceEnumRegion'
)
