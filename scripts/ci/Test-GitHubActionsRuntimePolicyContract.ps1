[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$policy = Join-Path $root 'scripts\ci\Test-GitHubActionsRuntimePolicy.ps1'
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) "mactype-actions-policy-$([guid]::NewGuid().ToString('N'))"

function Set-WorkflowFixture([string] $Uses) {
    $workflow = @"
name: policy fixture
on: workflow_dispatch
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: $Uses
"@
    [System.IO.File]::WriteAllText(
        (Join-Path $fixtureRoot 'fixture.yml'),
        $workflow,
        [System.Text.UTF8Encoding]::new($false)
    )
}

function Assert-PolicyRejects([string] $Uses, [string] $ExpectedMessage) {
    Set-WorkflowFixture -Uses $Uses
    $messages = @()
    try {
        $messages = @(& $policy -WorkflowRoot $fixtureRoot *>&1)
    } catch {
        $messages += $_
        $combined = ($messages | Out-String) + $_.Exception.Message
        if ($combined -match [regex]::Escape($ExpectedMessage)) {
            return
        }
        throw "Policy rejected '$Uses' for the wrong reason: $combined"
    }
    throw "Policy accepted forbidden remote action '$Uses'."
}

try {
    New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
    Assert-PolicyRejects -Uses 'example/old-node-action@v1' -ExpectedMessage 'closed allowlist'
    Assert-PolicyRejects -Uses 'actions/checkout@v6' -ExpectedMessage 'audited Node.js 24 release'

    Set-WorkflowFixture -Uses 'dtolnay/rust-toolchain@stable'
    & $policy -WorkflowRoot $fixtureRoot
} finally {
    if (Test-Path -LiteralPath $fixtureRoot) {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
    }
}

Write-Host 'GitHub Actions runtime policy contract passed.'
