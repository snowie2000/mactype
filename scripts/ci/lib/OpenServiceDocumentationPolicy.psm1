Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Test-OpenServiceDocumentationPolicy {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string] $Root)

    $failures = [System.Collections.Generic.List[string]]::new()
    $documentationContracts = @(
        @{ Path = 'CONTEXT.md'; Tokens = @('MachineIntegration', '신식 서비스', '레거시 서비스', 'generation', 'InjectionOrchestrator', 'fixed helper', 'ExecutionViewModel') },
        @{ Path = 'docs\open-service-contract.md'; Tokens = @('신식 서비스', '레거시 서비스', 'M01', 'M22', 'IMPLEMENTED', 'UNKNOWN') },
        @{ Path = 'docs\service-maintenance.md'; Tokens = @('신식 서비스', '레거시 서비스', 'open-service-disposable-vm.yml', 'UNKNOWN') },
        @{ Path = 'docs\control-center-architecture.md'; Tokens = @('MachineIntegration', '신식 서비스', '레거시 서비스') },
        @{ Path = 'docs\independent-distribution.md'; Tokens = @('신식 서비스', 'mactype-service-setup.exe') },
        @{ Path = 'docs\control-center-ci.md'; Tokens = @('service-injector', 'open-service-disposable-vm.yml', 'UNKNOWN') },
        @{ Path = 'HOWTOBUILD.md'; Tokens = @('Tauri', 'Build-ServiceRuntime.ps1', 'workflow_dispatch') },
        @{ Path = 'service-runtime\README.md'; Tokens = @('신식 서비스', '레거시 서비스', 'docs/open-service-contract.md') },
        @{ Path = 'docs\mactray-service-characterization.md'; Tokens = @('신식 서비스', '레거시 서비스') },
        @{ Path = 'evidence\mactray-service\README.md'; Tokens = @('신식 서비스', '레거시 서비스', 'UNKNOWN') }
    )
    foreach ($contract in $documentationContracts) {
        $path = Join-Path $Root $contract.Path
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            $failures.Add("$($contract.Path) is missing.")
            continue
        }
        $text = Get-Content -LiteralPath $path -Raw
        foreach ($token in $contract.Tokens) {
            if (-not $text.Contains($token)) {
                $failures.Add("$($contract.Path) is missing documentation contract '$token'.")
            }
        }
    }
    return $failures.ToArray()
}

Export-ModuleMember -Function 'Test-OpenServiceDocumentationPolicy'
