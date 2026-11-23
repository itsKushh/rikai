[Setup]
AppId={{E2C87401-75B8-4209-8969-CCF5DE7B7855}
AppName=MeetingNotes
AppVersion=0.1.0
DefaultDirName={localappdata}\Programs\MeetingNotes
DefaultGroupName=MeetingNotes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.20348
DisableProgramGroupPage=yes
WizardStyle=modern
SetupIconFile=..\assets\meetingnotes.ico
UninstallDisplayIcon={app}\meetingnotes.ico
OutputDir=..\dist
OutputBaseFilename=MeetingNotes-Setup-x64
Compression=lzma2
SolidCompression=yes
CloseApplications=yes

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Files]
Source: "..\target\release\meetingnotes.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\meetingnotes-audio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\meetingnotes.ico"; DestDir: "{app}"
Source: "..\dist\meetingnotes-worker\*"; DestDir: "{app}\worker"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\dist\notices\*"; DestDir: "{app}\licences"; Flags: recursesubdirs createallsubdirs

[Icons]
Name: "{group}\MeetingNotes"; Filename: "{app}\meetingnotes.exe"; IconFilename: "{app}\meetingnotes.ico"
[Run]
Filename: "{app}\meetingnotes.exe"; Description: "Ouvrir MeetingNotes"; Flags: postinstall nowait skipifsilent

