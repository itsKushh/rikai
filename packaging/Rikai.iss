[Setup]
AppId={{E2C87401-75B8-4209-8969-CCF5DE7B7855}
AppName=Rikai
VersionInfoProductName=Rikai
VersionInfoDescription=Rikai
VersionInfoVersion={#RikaiVersion}.0
UsePreviousAppDir=no
AppVersion={#RikaiVersion}
AppVerName=Rikai {#RikaiVersion}
UninstallDisplayName=Rikai
UsePreviousGroup=no
DefaultDirName={localappdata}\Programs\Rikai
DefaultGroupName=Rikai
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.20348
DisableProgramGroupPage=yes
WizardStyle=modern
SetupIconFile=..\assets\rikai.ico
UninstallDisplayIcon={app}\rikai.ico
OutputDir=..\dist\installer-engine
OutputBaseFilename=Rikai-Payload
Compression=lzma2
SolidCompression=yes
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Files]
Source: "..\target\release\rikai.exe"; DestDir: "{app}"; Flags: ignoreversion; BeforeInstall: ComponentStage(0)
Source: "..\target\release\rikai-audio.exe"; DestDir: "{app}"; Flags: ignoreversion; BeforeInstall: ComponentStage(0)
Source: "..\assets\rikai.ico"; DestDir: "{app}"; BeforeInstall: ComponentStage(0)
Source: "..\dist\rikai-worker\*"; DestDir: "{app}\worker"; Flags: ignoreversion recursesubdirs createallsubdirs; BeforeInstall: ComponentStage(1)
Source: "..\dist\notices\*"; DestDir: "{app}\licences"; Flags: recursesubdirs createallsubdirs; BeforeInstall: ComponentStage(1)

[InstallDelete]
Type: files; Name: "{app}\meetingnotes.exe"
Type: files; Name: "{app}\meetingnotes-audio.exe"
Type: files; Name: "{app}\meetingnotes.ico"
Type: files; Name: "{app}\worker\meetingnotes-worker.exe"
Type: files; Name: "{userprograms}\MeetingNotes\MeetingNotes.lnk"
Type: dirifempty; Name: "{userprograms}\MeetingNotes"

[Icons]
Name: "{group}\Rikai"; Filename: "{app}\rikai.exe"; IconFilename: "{app}\rikai.ico"; BeforeInstall: ComponentStage(2)
[Run]
Filename: "{app}\rikai.exe"; Description: "{cm:LaunchProgram,Rikai}"; Flags: postinstall nowait skipifsilent


[Code]
var
  Guided: Boolean;
  BridgeDir: String;
  Stage, Percent, WrittenStage, WrittenPercent: Integer;

procedure InitializeWizard;
begin
  Guided := ExpandConstant('{param:RIKAI_GUIDED|0}') = '1';
  BridgeDir := ExtractFileDir(ExpandConstant('{srcexe}'));
  WrittenStage := -1;
  WrittenPercent := -1;
end;

procedure WriteProgress;
begin
  if Guided and ((Stage <> WrittenStage) or (Percent <> WrittenPercent)) then
    if SaveStringToFile(BridgeDir + '\progress', IntToStr(Stage) + ' ' + IntToStr(Percent), False) then begin
      WrittenStage := Stage;
      WrittenPercent := Percent;
    end;
end;

procedure ComponentStage(Value: Integer);
begin
  Stage := Value;
  WriteProgress;
end;

function GetWindowLongW(Wnd: HWND; Index: Integer): Longint;
  external 'GetWindowLongW@user32.dll stdcall';
function SetWindowLongW(Wnd: HWND; Index: Integer; Value: Longint): Longint;
  external 'SetWindowLongW@user32.dll stdcall';
function SetLayeredWindowAttributes(Wnd: HWND; Key: Cardinal; Alpha: Byte; Flags: Cardinal): Boolean;
  external 'SetLayeredWindowAttributes@user32.dll stdcall';

procedure CurInstallProgressChanged(CurProgress, MaxProgress: Integer);
begin
  if not Guided then Exit;
  if MaxProgress > 0 then Percent := Round(100.0 * CurProgress / MaxProgress);
  WriteProgress;
  if FileExists(BridgeDir + '\cancel') then begin
    { Inno accepts cancellation only while its wizard is visible. Make the
      native form transparent before asking Inno to cancel and roll back. }
    if not WizardForm.Visible then begin
      SetWindowLongW(WizardForm.Handle, -20, GetWindowLongW(WizardForm.Handle, -20) or $80000 or $80);
      SetLayeredWindowAttributes(WizardForm.Handle, 0, 0, 2);
      WizardForm.Show;
    end;
    PostMessage(WizardForm.Handle, $0010, 0, 0);
  end;
end;

procedure CancelButtonClick(CurPageID: Integer; var Cancel, Confirm: Boolean);
begin
  if Guided and FileExists(BridgeDir + '\cancel') then begin
    Cancel := True;
    Confirm := False;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var OldDir, NewDir: String;
begin
  Result := '';
  OldDir := ExpandConstant('{localappdata}\Programs\MeetingNotes');
  NewDir := ExpandConstant('{app}');
  if FileExists(OldDir + '\meetingnotes.exe') and not DirExists(NewDir) then
    if not RenameFile(OldDir, NewDir) then
      Result := 'Rikai: ' + OldDir + ' → ' + NewDir + ' (' + SysErrorMessage(DLLGetLastError) + ')';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var ErrorCode: Integer;
begin
  if (CurStep = ssDone) and (ExpandConstant('{param:RIKAI_RESTART|0}') = '1') then
    Exec(ExpandConstant('{app}\rikai.exe'), '', '', SW_SHOWNORMAL, ewNoWait, ErrorCode);
end;
