Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Test-RequiredTokens {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [System.Collections.Generic.List[string]] $Failures,
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $MissingMessage,
        [Parameter(Mandatory)] [string] $TokenMessage,
        [Parameter(Mandatory)] [string[]] $Tokens
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        $Failures.Add($MissingMessage)
        return $null
    }
    $text = Get-Content -LiteralPath $Path -Raw
    foreach ($token in $Tokens) {
        if (-not $text.Contains($token)) {
            $Failures.Add(($TokenMessage -f $token))
        }
    }
    return $text
}

function Test-OpenServiceWorkflowPolicy {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string] $Root)

    $failures = [System.Collections.Generic.List[string]]::new()
    $markerVerifierPath = Join-Path $Root 'scripts\ci\Test-OpenServiceMarkersWindows.ps1'
    $null = Test-RequiredTokens -Failures $failures -Path $markerVerifierPath `
        -MissingMessage 'scripts/ci/Test-OpenServiceMarkersWindows.ps1 is missing.' `
        -TokenMessage "hosted marker verification is missing generation-binding token '{0}'." `
        -Tokens @('ExpectedRuntimeRoot', 'resolvedModuleRoot', 'OrdinalIgnoreCase', 'pid = [uint32]', 'sessionId = [uint32]')

    $buildWorkflowPath = Join-Path $Root '.github\workflows\build.yml'
    $null = Test-RequiredTokens -Failures $failures -Path $buildWorkflowPath `
        -MissingMessage '.github/workflows/build.yml is missing.' `
        -TokenMessage "build.yml is missing required open-service CI token '{0}'." `
        -Tokens @(
            'open-core:', 'mactype-open-core', 'artifacts/open-core',
            'open-service-windows:', 'Test-OpenServiceWindows.ps1',
            'Build-ServiceRuntime.ps1', 'ServiceRuntimeRoot', 'hook x86/x64 markers'
        )

    $hostedLifecyclePath = Join-Path $Root 'scripts\ci\Test-OpenServiceWindows.ps1'
    $hostedLifecycle = Test-RequiredTokens -Failures $failures -Path $hostedLifecyclePath `
        -MissingMessage 'scripts/ci/Test-OpenServiceWindows.ps1 is missing.' `
        -TokenMessage "hosted lifecycle verification is missing required contract token '{0}'." `
        -Tokens @(
            'Assert-GenerationBoundMarkerTelemetry', 'runtimeGenerationId',
            'profileDigest', '$MarkerResults', 'successCount', 'lastSuccess',
            'x86 and x64 marker telemetry is not bound to the same runtime generation',
            'OpenServiceAclFixture.psm1', 'Invoke-OpenServiceAclRepairFixture',
            '-RepairContext $stagedSetup', 'param($setupExecutable)',
            "-Verb 'publish-profile' -InputBytes `$profileA",
            "Assert-ActiveRuntimeProfile -ExpectedBytes `$profileA"
        )
    if ($hostedLifecycle) {
        $profilePublishToken = "-Verb 'publish-profile' -InputBytes `$profileA"
        $profilePublishIndex = $hostedLifecycle.IndexOf($profilePublishToken)
        $profileVerificationIndex = $hostedLifecycle.IndexOf(
            "Assert-ActiveRuntimeProfile -ExpectedBytes `$profileA"
        )
        $aclRepairIndex = $hostedLifecycle.IndexOf('Invoke-OpenServiceAclRepairFixture')
        if ([regex]::Matches(
                $hostedLifecycle,
                [regex]::Escape($profilePublishToken)
            ).Count -ne 1) {
            $failures.Add('hosted lifecycle must publish profile A exactly once.')
        }
        if ($profilePublishIndex -gt $aclRepairIndex -or
            $profileVerificationIndex -gt $aclRepairIndex) {
            $failures.Add(
                'hosted lifecycle must publish and verify profile A before the exact ACL repair fixture.'
            )
        }
    }

    $aclFixtureModulePath = Join-Path $Root 'scripts\ci\lib\OpenServiceAclFixture.psm1'
    $null = Test-RequiredTokens -Failures $failures -Path $aclFixtureModulePath `
        -MissingMessage 'scripts/ci/lib/OpenServiceAclFixture.psm1 is missing.' `
        -TokenMessage "exact ACL repair diagnostics are missing required token '{0}'." `
        -Tokens @(
            'S-1-5-32-545', 'exact-users-modify-repair',
            'post-repair-verification', 'targetAclSddl', 'innerError',
            'scQueryex', 'scQfailure', "-Name 'icacls'", 'RepairContext'
        )

    $supportTestPath = Join-Path $Root 'scripts\ci\Test-OpenServiceTestSupport.ps1'
    $null = Test-RequiredTokens -Failures $failures -Path $supportTestPath `
        -MissingMessage 'scripts/ci/Test-OpenServiceTestSupport.ps1 is missing.' `
        -TokenMessage "open-service CI support tests do not execute required test '{0}'." `
        -Tokens @('Test-OpenServiceAclFixture.ps1')

    $lintWorkflowPath = Join-Path $Root '.github\workflows\lint.yml'
    $null = Test-RequiredTokens -Failures $failures -Path $lintWorkflowPath `
        -MissingMessage '.github/workflows/lint.yml is missing.' `
        -TokenMessage "lint.yml does not enforce the service-injector contract '{0}'." `
        -Tokens @(
            'service-injector:', 'service-injector-x86', 'service-injector-x64',
            'ctest --test-dir', '-DCMAKE_CXX_FLAGS=/analyze',
            'mactype-injector32', 'mactype-injector64',
            'Test-OpenServiceTestSupport.ps1', 'Test-OpenServicePolicyModules.ps1'
        )

    $codeqlWorkflowPath = Join-Path $Root '.github\workflows\codeql.yml'
    $null = Test-RequiredTokens -Failures $failures -Path $codeqlWorkflowPath `
        -MissingMessage '.github/workflows/codeql.yml is missing.' `
        -TokenMessage "codeql.yml does not compile the service-injector analysis target '{0}'." `
        -Tokens @(
            'service-injector-codeql-x86', 'service-injector-codeql-x64',
            'mactype-injector32', 'mactype-injector64'
        )

    $disposableWorkflowPath = Join-Path $Root '.github\workflows\open-service-disposable-vm.yml'
    $disposableScriptPath = Join-Path $Root 'scripts\ci\Test-OpenServiceDisposableVm.ps1'
    if (-not (Test-Path -LiteralPath $disposableWorkflowPath -PathType Leaf)) {
        $failures.Add('.github/workflows/open-service-disposable-vm.yml is missing.')
    } else {
        $disposableWorkflow = Get-Content -LiteralPath $disposableWorkflowPath -Raw
        if ($disposableWorkflow -match '(?m)^\s{2}(?:push|pull_request|schedule|workflow_call):') {
            $failures.Add('open-service-disposable-vm.yml must be workflow_dispatch-only.')
        }
        foreach ($requiredToken in @(
            'workflow_dispatch:', 'mactype-disposable-vm',
            'I_UNDERSTAND_DISPOSABLE_VM', 'Test-OpenServiceDisposableVm.ps1'
        )) {
            if (-not $disposableWorkflow.Contains($requiredToken)) {
                $failures.Add("open-service-disposable-vm.yml is missing '$requiredToken'.")
            }
        }
        if ($disposableWorkflow -match '(?m)^\s{4}if:\s*inputs\.confirmation\s*==') {
            $failures.Add('open-service-disposable-vm.yml must not skip the verification job on an invalid confirmation.')
        }
        $confirmationGuardIndex = $disposableWorkflow.IndexOf('Reject invalid confirmation')
        $checkoutIndex = $disposableWorkflow.IndexOf('actions/checkout@')
        if ($confirmationGuardIndex -lt 0 -or $checkoutIndex -lt 0 -or
            $confirmationGuardIndex -gt $checkoutIndex) {
            $failures.Add('open-service-disposable-vm.yml must reject an invalid confirmation in the first step before checkout or build work.')
        }
        foreach ($token in @(
            "-cne 'I_UNDERSTAND_DISPOSABLE_VM'",
            "throw 'Disposable VM confirmation must exactly match I_UNDERSTAND_DISPOSABLE_VM.'"
        )) {
            if (-not $disposableWorkflow.Contains($token)) {
                $failures.Add("open-service-disposable-vm.yml is missing strict confirmation guard '$token'.")
            }
        }
    }

    $null = Test-RequiredTokens -Failures $failures -Path $disposableScriptPath `
        -MissingMessage 'scripts/ci/Test-OpenServiceDisposableVm.ps1 is missing.' `
        -TokenMessage "disposable VM verifier is missing scenario contract '{0}'." `
        -Tokens @(
            'lifecycle', 'prepare-reboot', 'verify-after-reboot',
            'verify-migration', 'verify-multi-session', 'AppInit_DLLs',
            'MacTypeControlCenterTest'
        )

    return $failures.ToArray()
}

Export-ModuleMember -Function 'Test-OpenServiceWorkflowPolicy'
