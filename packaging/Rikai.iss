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
OutputDir=..\dist
OutputBaseFilename=Rikai-Setup-x64
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
Source: "..\target\release\rikai.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\rikai-audio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\rikai.ico"; DestDir: "{app}"
Source: "..\dist\rikai-worker\*"; DestDir: "{app}\worker"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\dist\notices\*"; DestDir: "{app}\licences"; Flags: recursesubdirs createallsubdirs

[InstallDelete]
Type: files; Name: "{app}\meetingnotes.exe"
Type: files; Name: "{app}\meetingnotes-audio.exe"
Type: files; Name: "{app}\meetingnotes.ico"
Type: files; Name: "{app}\worker\meetingnotes-worker.exe"
Type: files; Name: "{userprograms}\MeetingNotes\MeetingNotes.lnk"
Type: dirifempty; Name: "{userprograms}\MeetingNotes"

[Icons]
Name: "{group}\Rikai"; Filename: "{app}\rikai.exe"; IconFilename: "{app}\rikai.ico"
[Run]
Filename: "{app}\rikai.exe"; Description: "{cm:LaunchProgram,Rikai}"; Flags: postinstall nowait skipifsilent


[Code]
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
