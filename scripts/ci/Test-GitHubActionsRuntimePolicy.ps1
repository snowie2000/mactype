[CmdletBinding()]
param(
    [string] $WorkflowRoot
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$workflowRoot = if ($WorkflowRoot) {
    [System.IO.Path]::GetFullPath($WorkflowRoot)
} else {
    Join-Path $root '.github\workflows'
}
if (-not (Test-Path -LiteralPath $workflowRoot -PathType Container)) {
    throw "GitHub Actions workflow root is not a directory: $workflowRoot"
}
$failures = [System.Collections.Generic.List[string]]::new()

# These are the audited current major releases used by this repository whose
# JavaScript runtime is Node.js 24. Requiring the exact major prevents both a
# Node.js 20 regression and a typo that names an unreleased action major.
$supportedMajor = @{
    'actions/checkout'             = 7
    'actions/setup-node'           = 7
    'actions/upload-artifact'      = 7
    'actions/download-artifact'    = 8
    'microsoft/setup-msbuild'      = 3
    'pnpm/action-setup'            = 6
    'softprops/action-gh-release'  = 3
    'github/codeql-action/init'    = 4
    'github/codeql-action/analyze' = 4
}
$auditedComposite = @{
    'dtolnay/rust-toolchain' = 'stable'
}

foreach ($workflow in Get-ChildItem -LiteralPath $workflowRoot -File | Where-Object Extension -in @('.yml', '.yaml')) {
    $text = Get-Content -LiteralPath $workflow.FullName -Raw
    $relative = [System.IO.Path]::GetRelativePath($root, $workflow.FullName)

    foreach ($match in [regex]::Matches($text, '(?m)^\s*-?\s*uses:\s*([^\s#]+)')) {
        $reference = $match.Groups[1].Value.Trim('"', "'")
        if ($reference.StartsWith('./')) { continue }

        $separator = $reference.LastIndexOf('@')
        if ($separator -lt 1) { continue }
        $action = $reference.Substring(0, $separator)
        $version = $reference.Substring($separator + 1)

        if ($auditedComposite.ContainsKey($action)) {
            if ($version -ne $auditedComposite[$action]) {
                $failures.Add("$relative uses $reference; audited composite action $action must use $($auditedComposite[$action]).")
            }
            continue
        }
        if (-not $supportedMajor.ContainsKey($action)) {
            $failures.Add("$relative uses $reference; remote actions must be present in the closed allowlist before CI can trust their runtime.")
            continue
        }

        if ($version -notmatch '^v(?<major>\d+)(?:\.|$)') {
            $failures.Add("$relative uses $reference; the Node 24 policy requires an auditable vN action release.")
            continue
        }

        $major = [int]$Matches.major
        if ($major -ne $supportedMajor[$action]) {
            $failures.Add("$relative uses $reference; the audited Node.js 24 release for $action is v$($supportedMajor[$action]).")
        }
    }

    foreach ($match in [regex]::Matches($text, '(?m)^\s*node-version:\s*["'']?(?<major>\d+)')) {
        if ([int]$match.Groups['major'].Value -lt 24) {
            $failures.Add("$relative configures Node.js $($match.Groups['major'].Value); CI must use Node.js 24 or newer.")
        }
    }
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "ERROR: $_" -ForegroundColor Red }
    throw "GitHub Actions Node.js 24 policy failed with $($failures.Count) violation(s): $($failures -join '; ')"
}

Write-Host 'GitHub Actions Node.js 24 policy passed.'
