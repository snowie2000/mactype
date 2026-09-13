[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$policies = [ordered]@{
    'OpenServiceProductPolicy.psm1' = 'Test-OpenServiceProductPolicy'
    'OpenServiceRuntimePolicy.psm1' = 'Test-OpenServiceRuntimePolicy'
    'OpenServiceWorkflowPolicy.psm1' = 'Test-OpenServiceWorkflowPolicy'
    'OpenServiceDocumentationPolicy.psm1' = 'Test-OpenServiceDocumentationPolicy'
}

$failures = [System.Collections.Generic.List[string]]::new()
foreach ($entry in $policies.GetEnumerator()) {
    Import-Module (Join-Path $PSScriptRoot "lib\$($entry.Key)") -Force
    foreach ($failure in @(& $entry.Value -Root $root)) {
        $failures.Add([string] $failure)
    }
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "ERROR: $_" -ForegroundColor Red }
    throw "Open service contract policy failed with $($failures.Count) violation(s)."
}

Write-Host 'Open service static contract policy passed.'
