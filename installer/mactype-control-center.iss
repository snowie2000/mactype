#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef AppExe
  #error AppExe must point to MacType Control Center.exe
#endif
#ifndef PreviewExe
  #error PreviewExe must point to mactype-preview32.exe
#endif
#ifndef CoreRoot
  #error CoreRoot must point to the source-built core artifact directory
#endif
#ifndef ServiceRuntimeRoot
  #error ServiceRuntimeRoot must point to the fixed open-service runtime artifact directory
#endif
#ifndef SourceRoot
  #define SourceRoot ".."
#endif
#ifndef OutputRoot
  #define OutputRoot "..\artifacts\installer"
#endif
#define ControlCenterExeName "MacType Control Center.exe"
#define RootCleanupHostProvidesOwnerState

[Setup]
AppId={{AF6B9697-3DF2-46C4-B203-79194967AE7A}
AppName=MacType Control Center
AppVersion={#AppVersion}
AppPublisher=MacType contributors
AppPublisherURL=https://github.com/snowie2000/mactype
DefaultDirName={autopf}\MacType Control Center
DisableDirPage=yes
DefaultGroupName=MacType Control Center
PrivilegesRequired=admin
UsePreviousAppDir=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputRoot}
OutputBaseFilename=MacType-Control-Center-Installer
SetupIconFile={#SourceRoot}\assets\mactype.ico
LicenseFile={#SourceRoot}\LICENSE
UninstallDisplayIcon={app}\{#ControlCenterExeName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
CloseApplications=yes
RestartApplications=no
ChangesAssociations=no
VersionInfoDescription=Open MacType Control Center and source-built core

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"

[Registry]
Root: HKLM64; Subkey: "SOFTWARE\MacType\ControlCenter"; ValueType: string; ValueName: "InstallLocation"; ValueData: "{app}"; Flags: uninsdeletevalue uninsdeletekeyifempty

[CustomMessages]
english.VerifiedUpdateTitle=Update MacType Control Center?
english.VerifiedUpdateMessage=Your existing profiles and settings will be preserved while the new version is installed.%n%nThe fixed application folder will be cleaned and its prior contents overwritten. Cancel if you do not accept this operation.
english.VerifiedUpdateButton=Update
english.VerifiedReinstallTitle=Reinstall MacType Control Center?
english.VerifiedReinstallMessage=Setup will repair the current installation and reinstall the required files.%n%nYour existing profiles and settings will be preserved, but the fixed application folder will be cleaned and its prior contents overwritten. Cancel if you do not accept this operation.
english.VerifiedReinstallButton=Reinstall
english.ForeignContentsTitle=Clean the existing installation folder?
english.ForeignContentsMessage=The fixed application folder contains data that is not owned by a verified MacType Control Center installation.%n%nAll prior files and subfolders in that folder will be removed and overwritten with the new installation. Cancel if you do not accept this operation.
english.ForeignContentsButton=Continue
korean.VerifiedUpdateTitle=MacType Control Center를 업데이트하시겠습니까?
korean.VerifiedUpdateMessage=기존 프로필과 설정을 유지한 채 새 버전으로 업데이트합니다.%n%n고정 설치 폴더의 기존 내용은 정리한 뒤 새 파일로 덮어씁니다. 이 작업을 원하지 않으면 취소하십시오.
korean.VerifiedUpdateButton=업데이트
korean.VerifiedReinstallTitle=MacType Control Center를 다시 설치하시겠습니까?
korean.VerifiedReinstallMessage=현재 설치를 복구하고 필요한 파일을 다시 설치합니다.%n%n기존 프로필과 설정은 유지하지만, 고정 설치 폴더의 기존 내용은 정리한 뒤 새 파일로 덮어씁니다. 이 작업을 원하지 않으면 취소하십시오.
korean.VerifiedReinstallButton=다시 설치
korean.ForeignContentsTitle=설치 폴더의 기존 내용을 정리하시겠습니까?
korean.ForeignContentsMessage=고정 설치 폴더에 확인된 MacType Control Center 설치가 소유하지 않은 내용이 있습니다.%n%n이 폴더의 기존 파일과 하위 폴더를 모두 제거한 뒤 새 설치 파일로 덮어씁니다. 이 작업을 원하지 않으면 취소하십시오.
korean.ForeignContentsButton=계속

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: checkedonce

[InstallDelete]
Type: files; Name: "{app}\.setup-root-cleanup-trigger"; BeforeInstall: BootstrapAndPurgeApplicationRootBeforeInstall
Type: filesandordirs; Name: "{app}\service-runtime"

[UninstallDelete]
Type: dirifempty; Name: "{app}"

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; DestName: "{#ControlCenterExeName}"; Flags: ignoreversion
Source: "{#PreviewExe}"; DestDir: "{app}"; DestName: "mactype-preview32.exe"; Flags: ignoreversion
Source: "{#CoreRoot}\MacType.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#CoreRoot}\MacType64.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#CoreRoot}\MacType.Core.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#CoreRoot}\MacType64.Core.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#CoreRoot}\MacLoader.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#CoreRoot}\MacLoader64.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ServiceRuntimeRoot}\mactype-service-setup.exe"; DestDir: "{app}\service-runtime"; Flags: ignoreversion
Source: "{#ServiceRuntimeRoot}\payload\manifest.json"; DestDir: "{app}\service-runtime\payload"; Flags: ignoreversion
Source: "{#ServiceRuntimeRoot}\payload\files\mactype-service.exe"; DestDir: "{app}\service-runtime\payload\files"; Flags: ignoreversion
Source: "{#ServiceRuntimeRoot}\payload\files\mactype-injector32.exe"; DestDir: "{app}\service-runtime\payload\files"; Flags: ignoreversion
Source: "{#ServiceRuntimeRoot}\payload\files\mactype-injector64.exe"; DestDir: "{app}\service-runtime\payload\files"; Flags: ignoreversion
Source: "{#ServiceRuntimeRoot}\payload\files\MacType.dll"; DestDir: "{app}\service-runtime\payload\files"; Flags: ignoreversion
Source: "{#ServiceRuntimeRoot}\payload\files\MacType64.dll"; DestDir: "{app}\service-runtime\payload\files"; Flags: ignoreversion
Source: "{#SourceRoot}\distribution\MacType.ini"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\distribution\ini\*.ini"; DestDir: "{app}\ini"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#SourceRoot}\distribution\languages\*.json"; DestDir: "{app}\languages"; Flags: ignoreversion
Source: "{#SourceRoot}\distribution\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\MacType Control Center"; Filename: "{app}\{#ControlCenterExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\MacType Control Center"; Filename: "{app}\{#ControlCenterExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#ControlCenterExeName}"; Description: "{cm:LaunchProgram,MacType Control Center}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Code]
const
  FixedApplicationDirectory = '{autopf}\MacType Control Center';
  SetupBrokerRelativePath = 'service-runtime\mactype-service-setup.exe';
  SetupBrokerBackupRelativePath = 'service-runtime.setup-backup';
  MaximumBrokerDiagnosticCharacters = 4096;
  UninstallRegistryKey =
    'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{AF6B9697-3DF2-46C4-B203-79194967AE7A}_is1';
  ExistingInstallFresh = 0;
  ExistingInstallVerifiedUpdate = 1;
  ExistingInstallVerifiedReinstall = 2;
  ExistingInstallForeignContents = 3;

var
  BrokerApplied: Boolean;
  BrokerAllowedBlocked: Boolean;
  BrokerFatalLegacyTrayBlocked: Boolean;
  BrokerOutputError: Boolean;
  BrokerDiagnostic: String;
  DeferredRuntimeCleanup: Boolean;
  ExistingInstallState: Integer;
  ExistingInstallPage: TOutputMsgWizardPage;
  ExistingInstallNextCaption: String;
  ExistingInstallClassified: Boolean;
  RootCleanupPreservedUninstaller: String;

function CollectRootCleanupEntryNames(
  const Directory: String;
  const PreserveRootEntries: Boolean;
  const Names: TStringList
): String;
forward;

function ValidateApplicationRootCleanup: String;
forward;

function IsApplicationRootCleanupReparsePoint(const Path: String): Boolean;
forward;

function StageApplicationRootCleanup(
  const ApplicationRoot, RollbackRoot: String
): String;
forward;

function CommitStagedRootCleanup(
  const ApplicationRoot, RollbackRoot: String
): String;
forward;

function RestoreStagedRootCleanup(
  const ApplicationRoot, RollbackRoot: String
): String;
forward;

procedure FailApplicationRootCleanup(const MessageText: String);
forward;

function NormalizeApplicationDirectory(const Path: String): String;
begin
  Result := Path;
  while (Length(Result) > 3) and
        ((Result[Length(Result)] = '\') or (Result[Length(Result)] = '/')) do
    Delete(Result, Length(Result), 1);
end;

function ExtractRegisteredUninstaller(const UninstallString: String): String;
var
  ClosingQuote: Integer;
  Separator: Integer;
begin
  Result := Trim(UninstallString);
  if Result = '' then
    Exit;
  if Result[1] = '"' then
  begin
    ClosingQuote := Pos('"', Copy(Result, 2, MaxInt));
    if ClosingQuote = 0 then
    begin
      Result := '';
      Exit;
    end;
    Result := Copy(Result, 2, ClosingQuote - 1);
    Exit;
  end;
  Separator := Pos(' ', Result);
  if Separator > 0 then
    Result := Copy(Result, 1, Separator - 1);
end;

function IsRegularOwnedFile(const Path: String): Boolean;
var
  FindRec: TFindRec;
begin
  Result := False;
  if not FindFirst(Path, FindRec) then
    Exit;
  try
    Result :=
      (FindRec.Attributes and FILE_ATTRIBUTE_DIRECTORY = 0) and
      (FindRec.Attributes and FILE_ATTRIBUTE_REPARSE_POINT = 0);
  finally
    FindClose(FindRec);
  end;
end;

function ClassifyVerifiedExistingInstall: Boolean;
var
  InstallLocation: String;
  DisplayVersion: String;
  UninstallString: String;
  Uninstaller: String;
  ExpectedRoot: String;
begin
  Result := False;
  if not IsWin64 then
    Exit;
  if not RegQueryStringValue(HKLM64, UninstallRegistryKey, 'InstallLocation', InstallLocation) or
     not RegQueryStringValue(HKLM64, UninstallRegistryKey, 'DisplayVersion', DisplayVersion) or
     not RegQueryStringValue(HKLM64, UninstallRegistryKey, 'UninstallString', UninstallString) then
    Exit;

  ExpectedRoot := NormalizeApplicationDirectory(ExpandConstant(FixedApplicationDirectory));
  if CompareText(NormalizeApplicationDirectory(InstallLocation), ExpectedRoot) <> 0 then
    Exit;
  Uninstaller := ExtractRegisteredUninstaller(UninstallString);
  if (Uninstaller = '') or
     (CompareText(NormalizeApplicationDirectory(ExtractFileDir(Uninstaller)), ExpectedRoot) <> 0) or
     not IsRegularOwnedFile(Uninstaller) or
     not IsRegularOwnedFile(ChangeFileExt(Uninstaller, '.dat')) or
     not IsRegularOwnedFile(AddBackslash(ExpectedRoot) + '{#ControlCenterExeName}') then
    Exit;

  RootCleanupPreservedUninstaller := Uninstaller;
  if CompareText(DisplayVersion, '{#AppVersion}') = 0 then
    ExistingInstallState := ExistingInstallVerifiedReinstall
  else
    ExistingInstallState := ExistingInstallVerifiedUpdate;
  Result := True;
end;

function ApplicationRootHasForeignContents: Boolean;
var
  ApplicationRoot: String;
  Names: TStringList;
begin
  Result := False;
  ApplicationRoot := ExpandConstant('{app}');
  if not DirExists(ApplicationRoot) then
    Exit;
  Names := TStringList.Create;
  try
    if CollectRootCleanupEntryNames(ApplicationRoot, True, Names) <> '' then
    begin
      Result := True;
      Exit;
    end;
    Result := Names.Count > 0;
  finally
    Names.Free;
  end;
end;

procedure ClassifyExistingInstall;
begin
  ExistingInstallState := ExistingInstallFresh;
  RootCleanupPreservedUninstaller := '';
  if IsApplicationRootCleanupReparsePoint(ExpandConstant('{app}')) or
     (FileExists(ExpandConstant('{app}')) and not DirExists(ExpandConstant('{app}'))) then
  begin
    ExistingInstallState := ExistingInstallForeignContents;
    Exit;
  end;
  if ClassifyVerifiedExistingInstall then
    Exit;
  if ApplicationRootHasForeignContents then
    ExistingInstallState := ExistingInstallForeignContents;
end;

procedure EnsureExistingInstallClassified;
begin
  if ExistingInstallClassified then
    Exit;
  ClassifyExistingInstall;
  ExistingInstallClassified := True;
end;

function ExistingInstallPromptTitle: String;
begin
  case ExistingInstallState of
    ExistingInstallVerifiedUpdate:
      Result := CustomMessage('VerifiedUpdateTitle');
    ExistingInstallVerifiedReinstall:
      Result := CustomMessage('VerifiedReinstallTitle');
    ExistingInstallForeignContents:
      Result := CustomMessage('ForeignContentsTitle');
  else
    Result := '';
  end;
end;

function ExistingInstallPromptMessage: String;
begin
  case ExistingInstallState of
    ExistingInstallVerifiedUpdate:
      Result := CustomMessage('VerifiedUpdateMessage');
    ExistingInstallVerifiedReinstall:
      Result := CustomMessage('VerifiedReinstallMessage');
    ExistingInstallForeignContents:
      Result := CustomMessage('ForeignContentsMessage');
  else
    Result := '';
  end;
end;

function ExistingInstallPromptButton: String;
begin
  case ExistingInstallState of
    ExistingInstallVerifiedUpdate:
      Result := CustomMessage('VerifiedUpdateButton');
    ExistingInstallVerifiedReinstall:
      Result := CustomMessage('VerifiedReinstallButton');
    ExistingInstallForeignContents:
      Result := CustomMessage('ForeignContentsButton');
  else
    Result := WizardForm.NextButton.Caption;
  end;
end;

procedure ActivateExistingInstallPage(Sender: TWizardPage);
begin
  EnsureExistingInstallClassified;
  ExistingInstallNextCaption := WizardForm.NextButton.Caption;
  WizardForm.NextButton.Caption := ExistingInstallPromptButton;
end;

function LeaveExistingInstallPage(Sender: TWizardPage): Boolean;
begin
  WizardForm.NextButton.Caption := ExistingInstallNextCaption;
  Result := True;
end;

function SkipExistingInstallPage(Sender: TWizardPage): Boolean;
begin
  EnsureExistingInstallClassified;
  ExistingInstallPage.Caption := ExistingInstallPromptTitle;
  ExistingInstallPage.Description := '';
  ExistingInstallPage.MsgLabel.Caption := ExistingInstallPromptMessage;
  Result := WizardSilent or (ExistingInstallState = ExistingInstallFresh);
end;

procedure InitializeWizard;
begin
  ExistingInstallPage := CreateOutputMsgPage(
    wpReady,
    '',
    '',
    ''
  );
  ExistingInstallPage.OnActivate := @ActivateExistingInstallPage;
  ExistingInstallPage.OnNextButtonClick := @LeaveExistingInstallPage;
  ExistingInstallPage.OnBackButtonClick := @LeaveExistingInstallPage;
  ExistingInstallPage.OnShouldSkipPage := @SkipExistingInstallPage;
end;

procedure CaptureBrokerOutput(const S: String; const Error, FirstLine: Boolean);
var
  Remaining: Integer;
begin
  Log(S);
  if Error then
    BrokerOutputError := True;
  Remaining := MaximumBrokerDiagnosticCharacters - Length(BrokerDiagnostic);
  if Remaining > 0 then
  begin
    if BrokerDiagnostic <> '' then
    begin
      if Remaining >= 2 then
        BrokerDiagnostic := BrokerDiagnostic + #13#10
      else
        Remaining := 0;
    end;
    Remaining := MaximumBrokerDiagnosticCharacters - Length(BrokerDiagnostic);
    if Remaining > 0 then
      BrokerDiagnostic := BrokerDiagnostic + Copy(S, 1, Remaining);
  end;
  if (Pos('"ok":true', S) > 0) and (Pos('"outcome":"applied"', S) > 0) then
    BrokerApplied := True;
  if (Pos('"ok":true', S) > 0) and (Pos('"outcome":"skipped-blocked"', S) > 0) and
     (Pos('"reason":"legacy-tray-mode"', S) > 0) then
    BrokerFatalLegacyTrayBlocked := True;
  if (Pos('"ok":true', S) > 0) and (Pos('"outcome":"skipped-blocked"', S) > 0) and
     ((Pos('"reason":"legacy-service"', S) > 0) or
      (Pos('"reason":"appinit"', S) > 0) or
      (Pos('"reason":"foreign-open-service"', S) > 0)) then
    BrokerAllowedBlocked := True;
end;

function BrokerFailure(const MessageText: String): String;
begin
  Result := MessageText;
  if BrokerDiagnostic <> '' then
    Result := Result + #13#10#13#10 + BrokerDiagnostic;
end;

procedure ResetBrokerCapture;
begin
  BrokerApplied := False;
  BrokerAllowedBlocked := False;
  BrokerFatalLegacyTrayBlocked := False;
  BrokerOutputError := False;
  BrokerDiagnostic := '';
end;

procedure RunFixedBrokerOrFail(const Verb: String; const Operation: String);
var
  ResultCode: Integer;
  Broker: String;
begin
  ResetBrokerCapture;
  ResultCode := -1;
  Broker := AddBackslash(ExpandConstant('{app}')) + SetupBrokerRelativePath;
  if not ExecAndLogOutput(
    Broker,
    Verb,
    ExtractFileDir(Broker),
    SW_HIDE,
    ewWaitUntilTerminated,
    ResultCode,
    @CaptureBrokerOutput
  ) then
    RaiseException(BrokerFailure(Operation + ' could not start the protected setup broker.'));
  if ResultCode <> 0 then
    RaiseException(BrokerFailure(
      Operation + ' failed with setup broker exit code ' + IntToStr(ResultCode) + '.'
    ));
end;

function RunStagedBootstrap(const Broker: String): String;
var
  ResultCode: Integer;
begin
  ResetBrokerCapture;
  ResultCode := -1;
  if not ExecAndLogOutput(
    Broker,
    'bootstrap-install',
    ExtractFileDir(Broker),
    SW_HIDE,
    ewWaitUntilTerminated,
    ResultCode,
    @CaptureBrokerOutput
  ) then
  begin
    Result := BrokerFailure('Machine service bootstrap could not start the protected setup broker.');
    Exit;
  end;
  if ResultCode <> 0 then
  begin
    Result := BrokerFailure(
      'Machine service bootstrap failed with setup broker exit code ' + IntToStr(ResultCode) + '.'
    );
    Exit;
  end;
  if BrokerOutputError then
  begin
    Result := BrokerFailure('Machine service bootstrap diagnostics could not be read safely.');
    Exit;
  end;
  if BrokerFatalLegacyTrayBlocked then
  begin
    Result := BrokerFailure(
      'Machine service bootstrap is blocked because MacTray tray mode is still active or configured for startup.'
    );
    Exit;
  end;
  if BrokerApplied then
  begin
    Result := '';
    Exit;
  end;
  if BrokerAllowedBlocked then
  begin
    Log('Machine service bootstrap was safely skipped because an explicit legacy integration conflict is present.');
    Result := '';
    Exit;
  end;
  Result := BrokerFailure('Machine service bootstrap returned no accepted terminal outcome.');
end;

function ExtractedBrokerFile(const RelativePath: String): String;
begin
  Result := AddBackslash(ExpandConstant('{tmp}')) +
    '{app}\service-runtime\' + RelativePath;
end;

procedure ExtractBrokerPayload;
var
  ExtractedCount: Integer;
begin
  ExtractedCount := ExtractTemporaryFiles('{app}\service-runtime\*');
  if ExtractedCount <> 7 then
    RaiseException(
      'expected 7 fixed broker payload files, extracted ' + IntToStr(ExtractedCount)
    );
end;

function CopyBrokerFile(const RuntimeRoot, RelativePath: String): Boolean;
begin
  Result := FileCopy(
    ExtractedBrokerFile(RelativePath),
    AddBackslash(RuntimeRoot) + RelativePath,
    False
  );
end;

function PopulateStagedBroker(const RuntimeRoot: String): String;
begin
  Result := '';
  try
    ExtractBrokerPayload;
    if not ForceDirectories(AddBackslash(RuntimeRoot) + 'payload\files') then
      RaiseException('could not create the staged payload directory');
    if not CopyBrokerFile(RuntimeRoot, 'mactype-service-setup.exe') then
      RaiseException('could not stage the setup broker');
    if not CopyBrokerFile(RuntimeRoot, 'payload\manifest.json') then
      RaiseException('could not stage the runtime manifest');
    if not CopyBrokerFile(RuntimeRoot, 'payload\files\mactype-service.exe') then
      RaiseException('could not stage the service host');
    if not CopyBrokerFile(RuntimeRoot, 'payload\files\mactype-injector32.exe') then
      RaiseException('could not stage the x86 injector');
    if not CopyBrokerFile(RuntimeRoot, 'payload\files\mactype-injector64.exe') then
      RaiseException('could not stage the x64 injector');
    if not CopyBrokerFile(RuntimeRoot, 'payload\files\MacType.dll') then
      RaiseException('could not stage the x86 MacType core');
    if not CopyBrokerFile(RuntimeRoot, 'payload\files\MacType64.dll') then
      RaiseException('could not stage the x64 MacType core');
  except
    Result := 'Machine service bootstrap staging failed: ' + GetExceptionMessage;
  end;
end;

function RestoreApplicationBroker(
  const ApplicationRoot, RuntimeRoot, BackupRoot: String;
  const ApplicationRootExisted, HadRuntime: Boolean
): String;
begin
  Result := '';
  if DirExists(RuntimeRoot) and not DelTree(RuntimeRoot, True, True, True) then
  begin
    Result := 'could not remove the temporary app-side service broker';
    Exit;
  end;
  if HadRuntime then
  begin
    if not RenameFile(BackupRoot, RuntimeRoot) then
      Result := 'could not restore the previous app-side service broker';
  end
  else if DirExists(BackupRoot) then
    Result := 'an unexpected app-side service broker backup remains';
  if (Result = '') and (not ApplicationRootExisted) then
    RemoveDir(ApplicationRoot);
end;

function BootstrapBeforeFileInstall: String;
var
  ApplicationRoot: String;
  RuntimeRoot: String;
  BackupRoot: String;
  Broker: String;
  ApplicationRootExisted: Boolean;
  HadRuntime: Boolean;
  OperationError: String;
  RestoreError: String;
begin
  ApplicationRoot := ExpandConstant('{app}');
  ApplicationRootExisted := DirExists(ApplicationRoot);
  RuntimeRoot := AddBackslash(ApplicationRoot) + 'service-runtime';
  BackupRoot := AddBackslash(ApplicationRoot) + SetupBrokerBackupRelativePath;
  if FileExists(RuntimeRoot) or FileExists(BackupRoot) then
  begin
    Result := 'Machine service bootstrap found a non-directory app-side broker path.';
    Exit;
  end;
  if DirExists(BackupRoot) then
  begin
    if DirExists(RuntimeRoot) then
    begin
      Result := 'Machine service bootstrap found an unresolved app-side broker backup collision.';
      Exit;
    end;
    if not RenameFile(BackupRoot, RuntimeRoot) then
    begin
      Result := 'Machine service bootstrap could not recover the previous app-side broker backup.';
      Exit;
    end;
  end;

  HadRuntime := DirExists(RuntimeRoot);
  if HadRuntime and not RenameFile(RuntimeRoot, BackupRoot) then
  begin
    Result := 'Machine service bootstrap could not preserve the previous app-side broker.';
    Exit;
  end;

  OperationError := '';
  try
    OperationError := PopulateStagedBroker(RuntimeRoot);
    if OperationError = '' then
    begin
      Broker := AddBackslash(RuntimeRoot) + 'mactype-service-setup.exe';
      OperationError := RunStagedBootstrap(Broker);
    end;
  except
    OperationError := 'Machine service bootstrap failed unexpectedly: ' + GetExceptionMessage;
  finally
    RestoreError := RestoreApplicationBroker(
      ApplicationRoot,
      RuntimeRoot,
      BackupRoot,
      ApplicationRootExisted,
      HadRuntime
    );
  end;

  if RestoreError <> '' then
    Result := OperationError + #13#10 +
      'App-side broker restoration failed: ' + RestoreError
  else
    Result := OperationError;
end;

function RestoreLegacyTrayStartupAfterBootstrapFailure: String;
var
  ControlCenter: String;
  ResultCode: Integer;
  MachineError: String;
  UserError: String;
begin
  Result := '';
  MachineError := '';
  UserError := '';
  ControlCenter := AddBackslash(ExpandConstant('{app}')) + '{#ControlCenterExeName}';
  if not FileExists(ControlCenter) then
  begin
    Log('No existing Control Center is available for startup receipt restoration.');
    Exit;
  end;

  ResultCode := -1;
  if not Exec(
    ControlCenter,
    '--control-center-service-broker restore-legacy-tray-autostart',
    ExtractFileDir(ControlCenter),
    SW_HIDE,
    ewWaitUntilTerminated,
    ResultCode
  ) then
    MachineError := 'could not start local-machine MacTray startup restoration'
  else if ResultCode <> 0 then
    MachineError := 'local-machine MacTray startup restoration failed with exit code ' +
      IntToStr(ResultCode);

  ResultCode := -1;
  if not ExecAsOriginalUser(
    ControlCenter,
    '--restore-current-user-legacy-tray-autostart',
    ExtractFileDir(ControlCenter),
    SW_HIDE,
    ewWaitUntilTerminated,
    ResultCode
  ) then
    UserError := 'could not start current-user MacTray startup restoration'
  else if ResultCode <> 0 then
    UserError := 'current-user MacTray startup restoration failed with exit code ' +
      IntToStr(ResultCode);

  if MachineError <> '' then
    Result := MachineError;
  if UserError <> '' then
  begin
    if Result <> '' then
      Result := Result + #13#10;
    Result := Result + UserError;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  { Keep this phase mutation-free. CloseApplications must finish before the
    rollback-staged bootstrap and root cleanup begin in the first InstallDelete entry. }
  if CompareText(ExpandConstant('{app}'), ExpandConstant(FixedApplicationDirectory)) <> 0 then
  begin
    Result := 'MacType Control Center must be installed in its protected Program Files directory.';
    Exit;
  end;
  Result := ValidateApplicationRootCleanup;
end;

procedure BootstrapAndPurgeApplicationRootBeforeInstall;
var
  ApplicationRoot: String;
  RollbackRoot: String;
  OperationError: String;
  RestoreError: String;
  StartupRestoreError: String;
begin
  ApplicationRoot := ExpandConstant('{app}');
  RollbackRoot := AddBackslash(ApplicationRoot) + '.setup-root-rollback';
  try
    OperationError := StageApplicationRootCleanup(ApplicationRoot, RollbackRoot);
    if OperationError <> '' then
      FailApplicationRootCleanup(OperationError);

    OperationError := BootstrapBeforeFileInstall;
    if OperationError <> '' then
    begin
      Log('Fatal machine service bootstrap failure: ' + OperationError);
      RestoreError := RestoreStagedRootCleanup(ApplicationRoot, RollbackRoot);
      if RestoreError <> '' then
        OperationError := OperationError + #13#10 +
          'Application-root restoration failed: ' + RestoreError
      else
      begin
        StartupRestoreError := RestoreLegacyTrayStartupAfterBootstrapFailure;
        if StartupRestoreError <> '' then
          OperationError := OperationError + #13#10 +
            'MacTray startup receipt restoration failed: ' + StartupRestoreError;
      end;
      FailApplicationRootCleanup(OperationError);
    end;

    OperationError := CommitStagedRootCleanup(ApplicationRoot, RollbackRoot);
    if OperationError <> '' then
      FailApplicationRootCleanup(OperationError);
    Log('Application-root cleanup removed all non-protected prior contents.');
  except
    FailApplicationRootCleanup(
      'Protected bootstrap and application-root cleanup failed unexpectedly: ' +
      GetExceptionMessage
    );
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    RunFixedBrokerOrFail('uninstall-owned', 'Owned machine service removal');
    DeferredRuntimeCleanup := DirExists(ExpandConstant('{app}\Service'));
    if DeferredRuntimeCleanup then
      Log('Verified runtime files are still loaded; cleanup is scheduled for reboot.');
  end;
end;

function UninstallNeedRestart(): Boolean;
begin
  Result := DeferredRuntimeCleanup;
end;

#include "application-root-cleanup.iss"
