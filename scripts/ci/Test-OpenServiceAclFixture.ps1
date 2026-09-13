[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$supportModulePath = Join-Path $PSScriptRoot 'lib\OpenServiceTestSupport.psm1'
$modulePath = Join-Path $PSScriptRoot 'lib\OpenServiceAclFixture.psm1'
Import-Module $supportModulePath -Force
Import-Module $modulePath -Force

$readOnlyRights = [Security.AccessControl.FileSystemRights]::ReadAndExecute -bor
    [Security.AccessControl.FileSystemRights]::Synchronize
if (Test-OpenServiceAclWriteCapability -Rights $readOnlyRights) {
    throw 'ReadAndExecute with Synchronize was incorrectly classified as write-capable.'
}
foreach ($writeCapableRights in @(
    [Security.AccessControl.FileSystemRights]::Modify,
    [Security.AccessControl.FileSystemRights]::FullControl
)) {
    if (-not (Test-OpenServiceAclWriteCapability -Rights $writeCapableRights)) {
        throw "$writeCapableRights was incorrectly classified as read-only."
    }
}

$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) `
    "mactype-open-service-acl-$PID-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
$targetPath = Join-Path $temporaryRoot 'mactype-service.exe'
[System.IO.File]::WriteAllText(
    $targetPath,
    'acl regression fixture',
    [System.Text.UTF8Encoding]::new($false)
)
$targetHash = Get-LowerFileSha256 -Path $targetPath
if ($targetHash -notmatch '^[0-9a-f]{64}$') {
    throw 'Importing the ACL fixture hid the caller-owned test-support commands.'
}
$missingServiceName = "MacTypeControlCenterAclFixture$PID"

try {
    $failureMessage = $null
    try {
        Invoke-OpenServiceAclRepairFixture `
            -Path $targetPath `
            -ServiceName $missingServiceName `
            -RepairAction {
                throw 'machine operation configure-service-recovery-policy failed: Access is denied. (os error 5)'
            }
    } catch {
        $failureMessage = $_.Exception.Message
    }

    if (-not $failureMessage) {
        throw 'The failing ACL repair fixture unexpectedly succeeded.'
    }
    foreach ($requiredDiagnostic in @(
        'fixture=exact-users-modify-repair',
        'phase=repair',
        'innerError=machine operation configure-service-recovery-policy failed: Access is denied. (os error 5)',
        "target=$targetPath",
        'targetExists=True',
        'targetAclSddl=',
        'explicitUsersModify=True',
        "serviceName=$missingServiceName",
        'scQueryexExitCode=1060',
        'icaclsExitCode=0'
    )) {
        if (-not $failureMessage.Contains($requiredDiagnostic)) {
            throw "ACL repair failure diagnostics are missing '$requiredDiagnostic'. Message: $failureMessage"
        }
    }

    $postRepairFailure = $null
    try {
        Invoke-OpenServiceAclRepairFixture `
            -Path $targetPath `
            -ServiceName $missingServiceName `
            -RepairAction { }
    } catch {
        $postRepairFailure = $_.Exception.Message
    }
    if (-not $postRepairFailure) {
        throw 'The ACL fixture accepted repair success while explicit Users:M remained.'
    }
    foreach ($requiredDiagnostic in @(
        'fixture=exact-users-modify-repair',
        'phase=post-repair-verification',
        'innerError=repair returned success but the explicit Users:M rule remains',
        'explicitUsersModify=True'
    )) {
        if (-not $postRepairFailure.Contains($requiredDiagnostic)) {
            throw "ACL post-repair diagnostics are missing '$requiredDiagnostic'. Message: $postRepairFailure"
        }
    }

    $callbackReceiptPath = Join-Path $temporaryRoot 'repair-callback.receipt'
    $repairContext = [pscustomobject]@{
        TargetPath = $targetPath
        ReceiptPath = $callbackReceiptPath
        Token = 'explicit-repair-context'
    }
    Invoke-OpenServiceAclRepairFixture `
        -Path $targetPath `
        -ServiceName $missingServiceName `
        -RepairContext $repairContext `
        -RepairAction {
            param($context)

            if (-not $context -or $context.Token -cne 'explicit-repair-context') {
                throw 'The ACL repair callback did not receive its explicit context.'
            }
            [System.IO.File]::WriteAllText(
                $context.ReceiptPath,
                $context.TargetPath,
                [System.Text.UTF8Encoding]::new($false)
            )
            & "$env:SystemRoot\System32\icacls.exe" `
                $context.TargetPath '/remove:g' '*S-1-5-32-545' | Out-Null
            if ($LASTEXITCODE -ne 0) {
                throw "Could not remove the test Users:M rule; icacls exit=$LASTEXITCODE."
            }
        }
    if (-not (Test-Path -LiteralPath $callbackReceiptPath -PathType Leaf) -or
        [System.IO.File]::ReadAllText($callbackReceiptPath) -cne $targetPath) {
        throw 'The explicit ACL repair callback context was not executed intact.'
    }
} finally {
    Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host 'Open service exact ACL repair fixture diagnostics test passed.'
