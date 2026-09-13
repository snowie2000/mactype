const
  RootCleanupRollbackDirectoryName = '.setup-root-rollback';
  RootCleanupProtectedDirectoryName = 'Service';
  RootCleanupMaximumEntries = 32768;
  DeleteAccess = $00010000;
  FileShareRead = $00000001;
  FileShareWrite = $00000002;
  FileShareDelete = $00000004;
  OpenExisting = 3;
  FileFlagBackupSemantics = $02000000;
  FileFlagOpenReparsePoint = $00200000;

var
#ifndef RootCleanupHostProvidesOwnerState
  RootCleanupPreservedUninstaller: String;
#endif
  RootCleanupValidationError: String;
  RootCleanupRegisteredEntries: Integer;

function OpenFileForRootCleanup(
  FileName: String;
  DesiredAccess, ShareMode: LongWord;
  SecurityAttributes: Integer;
  CreationDisposition, FlagsAndAttributes: LongWord;
  TemplateFile: Integer
): THandle;
external 'CreateFileW@kernel32.dll stdcall setuponly';

function CloseRootCleanupHandle(Handle: THandle): Boolean;
external 'CloseHandle@kernel32.dll stdcall setuponly';

function SetRootCleanupFileAttributes(FileName: String; FileAttributes: LongWord): Boolean;
external 'SetFileAttributesW@kernel32.dll stdcall setuponly';

procedure ExitRootCleanupSetup(ExitCode: LongWord);
external 'ExitProcess@kernel32.dll stdcall setuponly';

procedure FailApplicationRootCleanup(const MessageText: String);
begin
  Log('Fatal application-root cleanup failure: ' + MessageText);
  SuppressibleMsgBox(MessageText, mbError, MB_OK, IDOK);
  ExitRootCleanupSetup(4);
  Abort;
end;

function IsRootCleanupDotEntry(const Name: String): Boolean;
begin
  Result := (Name = '.') or (Name = '..');
end;

function IsRootCleanupReparsePoint(const Attributes: LongWord): Boolean;
begin
  Result := (Attributes and FILE_ATTRIBUTE_REPARSE_POINT) <> 0;
end;

function IsRootCleanupDirectory(const Attributes: LongWord): Boolean;
begin
  Result := (Attributes and FILE_ATTRIBUTE_DIRECTORY) <> 0;
end;

function IsApplicationRootCleanupReparsePoint(const Path: String): Boolean;
var
  FindRec: TFindRec;
begin
  Result := False;
  if not FindFirst(Path, FindRec) then
    Exit;
  try
    Result := IsRootCleanupReparsePoint(FindRec.Attributes);
  finally
    FindClose(FindRec);
  end;
end;

function IsPreservedRootCleanupEntry(const Name: String): Boolean;
var
  PreservedName: String;
begin
  Result :=
    (CompareText(Name, RootCleanupProtectedDirectoryName) = 0) or
    (CompareText(Name, RootCleanupRollbackDirectoryName) = 0);
  if Result or (RootCleanupPreservedUninstaller = '') then
    Exit;

  PreservedName := ExtractFileName(RootCleanupPreservedUninstaller);
  Result :=
    (CompareText(Name, PreservedName) = 0) or
    (CompareText(Name, ChangeFileExt(PreservedName, '.dat')) = 0) or
    (CompareText(Name, ChangeFileExt(PreservedName, '.msg')) = 0);
end;

function CollectRootCleanupEntryNames(
  const Directory: String;
  const PreserveRootEntries: Boolean;
  const Names: TStringList
): String;
var
  FindRec: TFindRec;
begin
  Result := '';
  if not FindFirst(AddBackslash(Directory) + '*', FindRec) then
    Exit;
  try
    repeat
      if not IsRootCleanupDotEntry(FindRec.Name) and
         (not PreserveRootEntries or not IsPreservedRootCleanupEntry(FindRec.Name)) then
      begin
        if Names.Count >= RootCleanupMaximumEntries then
        begin
          Result := 'Application-root cleanup exceeds its bounded entry limit.';
          Exit;
        end;
        Names.Add(FindRec.Name);
      end;
    until not FindNext(FindRec);
  finally
    FindClose(FindRec);
  end;
end;

function RegisterRootCleanupTree(const Path: String; const Attributes: LongWord): String;
var
  FindRec: TFindRec;
  ChildPath: String;
begin
  Result := '';
  if RootCleanupRegisteredEntries >= RootCleanupMaximumEntries then
  begin
    Result := 'Application-root cleanup exceeds its bounded Restart Manager resource limit.';
    Exit;
  end;
  RootCleanupRegisteredEntries := RootCleanupRegisteredEntries + 1;

  if IsRootCleanupReparsePoint(Attributes) or not IsRootCleanupDirectory(Attributes) then
  begin
#if VER >= EncodeVer(7,0,0)
    if not RegisterExtraCloseApplicationsResource(Path) then
#else
    if not RegisterExtraCloseApplicationsResource(False, Path) then
#endif
      Result := 'Application-root cleanup could not register an in-use resource: ' + Path;
    Exit;
  end;

  if not FindFirst(AddBackslash(Path) + '*', FindRec) then
    Exit;
  try
    repeat
      if not IsRootCleanupDotEntry(FindRec.Name) then
      begin
        ChildPath := AddBackslash(Path) + FindRec.Name;
        Result := RegisterRootCleanupTree(ChildPath, FindRec.Attributes);
        if Result <> '' then
          Exit;
      end;
    until not FindNext(FindRec);
  finally
    FindClose(FindRec);
  end;
end;

procedure RegisterExtraCloseApplicationsResources;
var
  ApplicationRoot: String;
  Names: TStringList;
  FindRec: TFindRec;
  EntryPath: String;
  I: Integer;
begin
  RootCleanupValidationError := '';
  RootCleanupRegisteredEntries := 0;
  ApplicationRoot := ExpandConstant('{app}');
  if not DirExists(ApplicationRoot) then
    Exit;
  if IsApplicationRootCleanupReparsePoint(ApplicationRoot) then
  begin
    RootCleanupValidationError :=
      'Application-root cleanup refuses a reparse-point application root.';
    Exit;
  end;
  if DirExists(AddBackslash(ApplicationRoot) + RootCleanupRollbackDirectoryName) or
     FileExists(AddBackslash(ApplicationRoot) + RootCleanupRollbackDirectoryName) then
  begin
    RootCleanupValidationError :=
      'Application-root cleanup found an unresolved rollback directory.';
    Exit;
  end;

  Names := TStringList.Create;
  try
    RootCleanupValidationError :=
      CollectRootCleanupEntryNames(ApplicationRoot, True, Names);
    if RootCleanupValidationError <> '' then
      Exit;
    for I := 0 to Names.Count - 1 do
    begin
      EntryPath := AddBackslash(ApplicationRoot) + Names[I];
      if FindFirst(EntryPath, FindRec) then
      begin
        try
          RootCleanupValidationError :=
            RegisterRootCleanupTree(EntryPath, FindRec.Attributes);
        finally
          FindClose(FindRec);
        end;
      end
      else
        RootCleanupValidationError :=
          'Application-root cleanup could not inspect a registered entry: ' + EntryPath;
      if RootCleanupValidationError <> '' then
        Exit;
    end;
  finally
    Names.Free;
  end;
end;

function ValidateApplicationRootCleanup: String;
var
  ApplicationRoot: String;
begin
  ApplicationRoot := ExpandConstant('{app}');
  if IsApplicationRootCleanupReparsePoint(ApplicationRoot) then
    Result := 'Application-root cleanup refuses a reparse-point application root.'
  else if FileExists(ApplicationRoot) and not DirExists(ApplicationRoot) then
    Result := 'Application-root cleanup refuses a non-directory application root.'
  else
    Result := RootCleanupValidationError;
end;

function PreflightRootCleanupTree(const Path: String; const Attributes: LongWord): String;
var
  Handle: THandle;
  FindRec: TFindRec;
  ChildPath: String;
  OpenFlags: LongWord;
begin
  Result := '';
  OpenFlags := FileFlagOpenReparsePoint;
  if IsRootCleanupDirectory(Attributes) then
    OpenFlags := OpenFlags or FileFlagBackupSemantics;
  Handle := OpenFileForRootCleanup(
    Path,
    DeleteAccess,
    FileShareRead or FileShareWrite or FileShareDelete,
    0,
    OpenExisting,
    OpenFlags,
    0
  );
  if Handle = THandle(-1) then
  begin
    Result := 'Application-root cleanup cannot remove an in-use or protected path: ' + Path;
    Exit;
  end;
  CloseRootCleanupHandle(Handle);

  if IsRootCleanupReparsePoint(Attributes) or not IsRootCleanupDirectory(Attributes) then
    Exit;
  if not FindFirst(AddBackslash(Path) + '*', FindRec) then
    Exit;
  try
    repeat
      if not IsRootCleanupDotEntry(FindRec.Name) then
      begin
        ChildPath := AddBackslash(Path) + FindRec.Name;
        Result := PreflightRootCleanupTree(ChildPath, FindRec.Attributes);
        if Result <> '' then
          Exit;
      end;
    until not FindNext(FindRec);
  finally
    FindClose(FindRec);
  end;
end;

function DeleteRootCleanupTree(const Path: String; const Attributes: LongWord): Boolean;
var
  FindRec: TFindRec;
  ChildPath: String;
begin
  Result := False;
  if IsRootCleanupReparsePoint(Attributes) then
  begin
    SetRootCleanupFileAttributes(Path, FILE_ATTRIBUTE_NORMAL);
    if IsRootCleanupDirectory(Attributes) then
      Result := RemoveDir(Path)
    else
      Result := DeleteFile(Path);
    Exit;
  end;
  if not IsRootCleanupDirectory(Attributes) then
  begin
    SetRootCleanupFileAttributes(Path, FILE_ATTRIBUTE_NORMAL);
    Result := DeleteFile(Path);
    Exit;
  end;

  if FindFirst(AddBackslash(Path) + '*', FindRec) then
  begin
    try
      repeat
        if not IsRootCleanupDotEntry(FindRec.Name) then
        begin
          ChildPath := AddBackslash(Path) + FindRec.Name;
          if not DeleteRootCleanupTree(ChildPath, FindRec.Attributes) then
            Exit;
        end;
      until not FindNext(FindRec);
    finally
      FindClose(FindRec);
    end;
  end;
  SetRootCleanupFileAttributes(Path, FILE_ATTRIBUTE_NORMAL);
  Result := RemoveDir(Path);
end;

function RestoreStagedRootCleanup(
  const ApplicationRoot, RollbackRoot: String
): String;
var
  Names: TStringList;
  I: Integer;
  SourcePath: String;
  DestinationPath: String;
begin
  Result := '';
  if not DirExists(RollbackRoot) then
    Exit;
  Names := TStringList.Create;
  try
    Result := CollectRootCleanupEntryNames(RollbackRoot, False, Names);
    if Result <> '' then
      Exit;
    for I := 0 to Names.Count - 1 do
    begin
      SourcePath := AddBackslash(RollbackRoot) + Names[I];
      DestinationPath := AddBackslash(ApplicationRoot) + Names[I];
      if FileExists(DestinationPath) or DirExists(DestinationPath) or
         not RenameFile(SourcePath, DestinationPath) then
      begin
        Result := 'Application-root cleanup could not restore: ' + DestinationPath;
        Exit;
      end;
    end;
    if not RemoveDir(RollbackRoot) then
      Result := 'Application-root cleanup could not remove its empty rollback directory.';
  finally
    Names.Free;
  end;
end;

function StageApplicationRootCleanup(
  const ApplicationRoot, RollbackRoot: String
): String;
var
  Names: TStringList;
  FindRec: TFindRec;
  EntryPath: String;
  RollbackPath: String;
  RestoreError: String;
  I: Integer;
begin
  Result := '';
  if not DirExists(ApplicationRoot) and not ForceDirectories(ApplicationRoot) then
  begin
    Result := 'Application-root cleanup could not create the fixed application root.';
    Exit;
  end;
  if IsApplicationRootCleanupReparsePoint(ApplicationRoot) then
  begin
    Result := 'Application-root cleanup refuses a reparse-point application root.';
    Exit;
  end;
  if FileExists(RollbackRoot) or DirExists(RollbackRoot) then
  begin
    Result := 'Application-root cleanup found an unresolved rollback directory.';
    Exit;
  end;
  if not ForceDirectories(RollbackRoot) then
  begin
    Result := 'Application-root cleanup could not create its rollback directory.';
    Exit;
  end;

  Names := TStringList.Create;
  try
    Result := CollectRootCleanupEntryNames(ApplicationRoot, True, Names);
    if Result = '' then
    begin
      for I := 0 to Names.Count - 1 do
      begin
        EntryPath := AddBackslash(ApplicationRoot) + Names[I];
        RollbackPath := AddBackslash(RollbackRoot) + Names[I];
        if not RenameFile(EntryPath, RollbackPath) then
        begin
          Result := 'Application-root cleanup could not stage: ' + EntryPath;
          Break;
        end;
      end;
    end;

    if Result = '' then
    begin
      for I := 0 to Names.Count - 1 do
      begin
        RollbackPath := AddBackslash(RollbackRoot) + Names[I];
        if FindFirst(RollbackPath, FindRec) then
        begin
          try
            Result := PreflightRootCleanupTree(RollbackPath, FindRec.Attributes);
          finally
            FindClose(FindRec);
          end;
        end
        else
          Result := 'Application-root cleanup lost a staged path: ' + RollbackPath;
        if Result <> '' then
          Break;
      end;
    end;

    if Result <> '' then
    begin
      RestoreError := RestoreStagedRootCleanup(ApplicationRoot, RollbackRoot);
      if RestoreError <> '' then
        Result := Result + #13#10 + RestoreError;
    end;
  finally
    Names.Free;
  end;
end;

function CommitStagedRootCleanup(
  const ApplicationRoot, RollbackRoot: String
): String;
var
  Names: TStringList;
  FindRec: TFindRec;
  RollbackPath: String;
  RestoreError: String;
  I: Integer;
begin
  Result := '';
  if not DirExists(RollbackRoot) then
    Exit;
  Names := TStringList.Create;
  try
    Result := CollectRootCleanupEntryNames(RollbackRoot, False, Names);
    if Result <> '' then
      Exit;
    for I := 0 to Names.Count - 1 do
    begin
      RollbackPath := AddBackslash(RollbackRoot) + Names[I];
      if FindFirst(RollbackPath, FindRec) then
      begin
        try
          if not DeleteRootCleanupTree(RollbackPath, FindRec.Attributes) then
          begin
            Result := 'Application-root cleanup could not purge: ' + RollbackPath;
            RestoreError := RestoreStagedRootCleanup(ApplicationRoot, RollbackRoot);
            if RestoreError <> '' then
              Result := Result + #13#10 + RestoreError;
            Exit;
          end;
        finally
          FindClose(FindRec);
        end;
      end;
    end;
    if not RemoveDir(RollbackRoot) then
      Result := 'Application-root cleanup could not remove its rollback directory.';
  finally
    Names.Free;
  end;
end;

procedure PurgeApplicationRootBeforeInstall;
var
  ApplicationRoot: String;
  RollbackRoot: String;
  OperationError: String;
begin
  ApplicationRoot := ExpandConstant('{app}');
  RollbackRoot := AddBackslash(ApplicationRoot) + RootCleanupRollbackDirectoryName;
  try
    OperationError := StageApplicationRootCleanup(ApplicationRoot, RollbackRoot);
    if OperationError = '' then
      OperationError := CommitStagedRootCleanup(ApplicationRoot, RollbackRoot);
    if OperationError <> '' then
      FailApplicationRootCleanup(OperationError);
    Log('Application-root cleanup removed all non-protected prior contents.');
  except
    FailApplicationRootCleanup(
      'Application-root cleanup failed unexpectedly: ' + GetExceptionMessage
    );
  end;
end;
