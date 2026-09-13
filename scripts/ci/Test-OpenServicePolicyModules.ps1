[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$modulePath = Join-Path $PSScriptRoot 'lib\OpenServiceProductPolicy.psm1'
if (-not (Test-Path -LiteralPath $modulePath -PathType Leaf)) {
    throw 'OpenServiceProductPolicy.psm1 is missing.'
}
Import-Module $modulePath -Force

$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) `
    "mactype-open-service-policy-$PID-$([guid]::NewGuid().ToString('N'))"
try {
    $legacyRoot = Join-Path $fixtureRoot `
        'control-center\src-tauri\src\machine_integration\legacy_mactray'
    $modelRoot = Join-Path $fixtureRoot 'control-center\src-tauri\src\machine_integration'
    $frontendRoot = Join-Path $fixtureRoot 'control-center\src\app'
    New-Item -ItemType Directory -Path $legacyRoot, $frontendRoot -Force | Out-Null

    [System.IO.File]::WriteAllText(
        (Join-Path $legacyRoot 'broker.rs'),
        'fn retired() { run_elevated(); }'
    )
    [System.IO.File]::WriteAllText(
        (Join-Path $modelRoot 'model.rs'),
        @'
enum MachineAction { Install, Rollback }
enum PublicMachineAction { Install, Rollback }
'@
    )
    [System.IO.File]::WriteAllText(
        (Join-Path $frontendRoot 'model.ts'),
        'export type SystemServiceAction = "install" | "rollback";'
    )

    $failures = @(Test-OpenServiceProductPolicy -Root $fixtureRoot)
    foreach ($expected in @(
        'machine_integration.*legacy_mactray.*run_elevated',
        'PublicMachineAction.*Rollback',
        'SystemServiceAction.*rollback'
    )) {
        if (-not ($failures -match $expected)) {
            throw "Product policy did not reject /$expected/. Failures: $($failures -join '; ')"
        }
    }

    [System.IO.File]::WriteAllText(
        (Join-Path $legacyRoot 'broker.rs'),
        'fn detection_only() { status(); }'
    )
    [System.IO.File]::WriteAllText(
        (Join-Path $modelRoot 'model.rs'),
        @'
enum MachineAction { Install, Rollback }
enum PublicMachineAction { Install }
fn rollback_transaction(action: MachineAction) { let _ = action; }
'@
    )
    [System.IO.File]::WriteAllText(
        (Join-Path $frontendRoot 'model.ts'),
        'export type SystemServiceAction = "install" | "stop";'
    )

    $allowedFailures = @(Test-OpenServiceProductPolicy -Root $fixtureRoot)
    if ($allowedFailures -match 'PublicMachineAction.*Rollback|SystemServiceAction.*rollback|retired normal-operation broker/API') {
        throw "Product policy rejected internal rollback or safe legacy detection: $($allowedFailures -join '; ')"
    }
} finally {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host 'Open service responsibility-scoped product policy tests passed.'
