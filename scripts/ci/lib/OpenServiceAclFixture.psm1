Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not ('MacType.ControlCenter.Ci.BoundedProcessRunner' -as [type])) {
    Add-Type -Path @(
        (Join-Path $PSScriptRoot 'BoundedProcessRunner.cs'),
        (Join-Path $PSScriptRoot 'BoundedProcessIo.cs'),
        (Join-Path $PSScriptRoot 'WindowsProcessJob.cs')
    )
}

$script:UsersSid = 'S-1-5-32-545'
$script:MaximumDiagnosticValueCharacters = 4096
$script:WriteCapabilityFileRights = [Security.AccessControl.FileSystemRights]::WriteData -bor
    [Security.AccessControl.FileSystemRights]::AppendData -bor
    [Security.AccessControl.FileSystemRights]::WriteExtendedAttributes -bor
    [Security.AccessControl.FileSystemRights]::DeleteSubdirectoriesAndFiles -bor
    [Security.AccessControl.FileSystemRights]::WriteAttributes -bor
    [Security.AccessControl.FileSystemRights]::Delete -bor
    [Security.AccessControl.FileSystemRights]::ChangePermissions -bor
    [Security.AccessControl.FileSystemRights]::TakeOwnership

function Test-OpenServiceAclWriteCapability {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [Security.AccessControl.FileSystemRights] $Rights
    )

    return (($Rights -band $script:WriteCapabilityFileRights) -ne 0)
}

function ConvertTo-OpenServiceDiagnosticValue {
    param([AllowNull()] [object] $Value)

    if ($null -eq $Value) { return '<null>' }
    $text = ([string] $Value).Replace("`r", '\r').Replace("`n", '\n')
    if ($text.Length -le $script:MaximumDiagnosticValueCharacters) { return $text }
    return $text.Substring(0, $script:MaximumDiagnosticValueCharacters) + '<truncated>'
}

function Invoke-OpenServiceBoundedNativeCommand {
    param(
        [Parameter(Mandatory)] [string] $Executable,
        [Parameter(Mandatory)] [string[]] $Arguments
    )

    try {
        return [MacType.ControlCenter.Ci.BoundedProcessRunner]::RunArguments(
            $Executable,
            $Arguments,
            $null,
            15000,
            65536,
            5000
        )
    } catch {
        return [pscustomobject]@{
            ExitCode = '<runner-error>'
            StandardOutput = ''
            StandardError = $_.Exception.Message
        }
    }
}

function Test-OpenServiceExplicitUsersModify {
    param([Parameter(Mandatory)] [string] $Path)

    $acl = Get-Acl -LiteralPath $Path
    foreach ($rule in $acl.Access) {
        if ($rule.IsInherited -or
            $rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
            -not (Test-OpenServiceAclWriteCapability -Rights $rule.FileSystemRights)) {
            continue
        }
        try {
            $sid = $rule.IdentityReference.Translate(
                [Security.Principal.SecurityIdentifier]
            ).Value
        } catch {
            continue
        }
        if ($sid -ceq $script:UsersSid) { return $true }
    }
    return $false
}

function Add-OpenServiceUsersModifyFixture {
    param([Parameter(Mandatory)] [string] $Path)

    $icaclsPath = Join-Path $env:SystemRoot 'System32\icacls.exe'
    $result = Invoke-OpenServiceBoundedNativeCommand -Executable $icaclsPath `
        -Arguments @($Path, '/grant', "*$($script:UsersSid):(M)")
    if ($result.ExitCode -ne 0) {
        throw "icacls Users:M grant failed with Win32 exit $($result.ExitCode). " +
            "stdout=$(ConvertTo-OpenServiceDiagnosticValue $result.StandardOutput) " +
            "stderr=$(ConvertTo-OpenServiceDiagnosticValue $result.StandardError)"
    }
    if (-not (Test-OpenServiceExplicitUsersModify -Path $Path)) {
        throw 'icacls returned success without creating an explicit Users:M rule.'
    }
}

function Add-OpenServiceNativeDiagnostic {
    param(
        [Parameter(Mandatory)] [Collections.Generic.List[string]] $Lines,
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [string] $Executable,
        [Parameter(Mandatory)] [string[]] $Arguments
    )

    $result = Invoke-OpenServiceBoundedNativeCommand -Executable $Executable `
        -Arguments $Arguments
    $Lines.Add("${Name}ExitCode=$(ConvertTo-OpenServiceDiagnosticValue $result.ExitCode)")
    $Lines.Add("${Name}Stdout=$(ConvertTo-OpenServiceDiagnosticValue $result.StandardOutput)")
    $Lines.Add("${Name}Stderr=$(ConvertTo-OpenServiceDiagnosticValue $result.StandardError)")
}

function Get-OpenServiceAclFixtureDiagnostic {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $ServiceName,
        [Parameter(Mandatory)] [string] $Phase,
        [Parameter(Mandatory)] [string] $InnerError
    )

    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add('Open-service ACL regression fixture failed.')
    $lines.Add('fixture=exact-users-modify-repair')
    $lines.Add("phase=$(ConvertTo-OpenServiceDiagnosticValue $Phase)")
    $lines.Add("innerError=$(ConvertTo-OpenServiceDiagnosticValue $InnerError)")
    $lines.Add("target=$(ConvertTo-OpenServiceDiagnosticValue $Path)")
    $targetExists = Test-Path -LiteralPath $Path -PathType Leaf
    $lines.Add("targetExists=$targetExists")
    if ($targetExists) {
        try {
            $item = Get-Item -LiteralPath $Path -Force
            $lines.Add("targetLength=$($item.Length)")
            $lines.Add("targetAttributes=$(ConvertTo-OpenServiceDiagnosticValue $item.Attributes)")
        } catch {
            $lines.Add("targetMetadataError=$(ConvertTo-OpenServiceDiagnosticValue $_.Exception.Message)")
        }
        try {
            $acl = Get-Acl -LiteralPath $Path
            $lines.Add("targetAclSddl=$(ConvertTo-OpenServiceDiagnosticValue $acl.Sddl)")
            $lines.Add("explicitUsersModify=$(Test-OpenServiceExplicitUsersModify -Path $Path)")
        } catch {
            $lines.Add("targetAclSddl=<error:$(ConvertTo-OpenServiceDiagnosticValue $_.Exception.Message)>")
            $lines.Add('explicitUsersModify=<unknown>')
        }
    } else {
        $lines.Add('targetAclSddl=<missing>')
        $lines.Add('explicitUsersModify=<missing>')
    }

    $lines.Add("serviceName=$(ConvertTo-OpenServiceDiagnosticValue $ServiceName)")
    try {
        $service = Get-CimInstance Win32_Service -Filter "Name='$ServiceName'"
        if ($service) {
            $lines.Add("serviceState=$(ConvertTo-OpenServiceDiagnosticValue $service.State)")
            $lines.Add("serviceProcessId=$(ConvertTo-OpenServiceDiagnosticValue $service.ProcessId)")
            $lines.Add("serviceStartMode=$(ConvertTo-OpenServiceDiagnosticValue $service.StartMode)")
            $lines.Add("serviceStartName=$(ConvertTo-OpenServiceDiagnosticValue $service.StartName)")
            $lines.Add("servicePathName=$(ConvertTo-OpenServiceDiagnosticValue $service.PathName)")
        } else {
            $lines.Add('serviceState=<missing>')
        }
    } catch {
        $lines.Add("serviceSnapshotError=$(ConvertTo-OpenServiceDiagnosticValue $_.Exception.Message)")
    }

    $scPath = Join-Path $env:SystemRoot 'System32\sc.exe'
    Add-OpenServiceNativeDiagnostic -Lines $lines -Name 'scQueryex' `
        -Executable $scPath -Arguments @('queryex', $ServiceName)
    Add-OpenServiceNativeDiagnostic -Lines $lines -Name 'scQc' `
        -Executable $scPath -Arguments @('qc', $ServiceName)
    Add-OpenServiceNativeDiagnostic -Lines $lines -Name 'scQfailure' `
        -Executable $scPath -Arguments @('qfailure', $ServiceName)
    Add-OpenServiceNativeDiagnostic -Lines $lines -Name 'scQfailureflag' `
        -Executable $scPath -Arguments @('qfailureflag', $ServiceName)
    $icaclsPath = Join-Path $env:SystemRoot 'System32\icacls.exe'
    Add-OpenServiceNativeDiagnostic -Lines $lines -Name 'icacls' `
        -Executable $icaclsPath -Arguments @($Path)

    return $lines -join "`n"
}

function Invoke-OpenServiceAclRepairFixture {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $ServiceName,
        [Parameter(Mandatory)] [scriptblock] $RepairAction,
        [AllowNull()] [object] $RepairContext = $null
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "The exact ACL regression target is missing: $Path"
    }
    if (((Get-Item -LiteralPath $Path -Force).Attributes -band
            [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "The exact ACL regression target is a reparse point: $Path"
    }

    try {
        Add-OpenServiceUsersModifyFixture -Path $Path
    } catch {
        throw (Get-OpenServiceAclFixtureDiagnostic -Path $Path `
                -ServiceName $ServiceName -Phase 'poison' `
                -InnerError $_.Exception.Message)
    }

    try {
        & $RepairAction $RepairContext
    } catch {
        throw (Get-OpenServiceAclFixtureDiagnostic -Path $Path `
                -ServiceName $ServiceName -Phase 'repair' `
                -InnerError $_.Exception.Message)
    }

    try {
        $explicitUsersModifyRemains = Test-OpenServiceExplicitUsersModify -Path $Path
    } catch {
        throw (Get-OpenServiceAclFixtureDiagnostic -Path $Path `
                -ServiceName $ServiceName -Phase 'post-repair-verification' `
                -InnerError "could not read the repaired ACL: $($_.Exception.Message)")
    }
    if ($explicitUsersModifyRemains) {
        throw (Get-OpenServiceAclFixtureDiagnostic -Path $Path `
                -ServiceName $ServiceName -Phase 'post-repair-verification' `
                -InnerError 'repair returned success but the explicit Users:M rule remains')
    }
}

Export-ModuleMember -Function @(
    'Test-OpenServiceAclWriteCapability',
    'Get-OpenServiceAclFixtureDiagnostic',
    'Invoke-OpenServiceAclRepairFixture'
)
