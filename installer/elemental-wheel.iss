; Build: ISCC /DAppVersion=<version> elemental-wheel.iss  (tools\build-installer.ps1 does this)
#ifndef AppVersion
  #error AppVersion is not defined; pass /DAppVersion=x.y.z on the ISCC command line.
#endif

[Setup]
AppId={{BCD5ACAE-5043-45AA-A282-5B4E6612C03E}
AppName=Elemental Wheel
AppVersion={#AppVersion}
AppVerName=Elemental Wheel {#AppVersion}
AppPublisher=Elemental Wheel
AppPublisherURL=https://github.com/Koutasora/Elemental-Wheel
VersionInfoVersion={#AppVersion}
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\Elemental-Wheel
DisableProgramGroupPage=yes
DisableWelcomePage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=..\assets\elemental-wheel.ico
UninstallDisplayIcon={app}\wheel.exe
OutputDir=..\target\installer
OutputBaseFilename=Elemental-Wheel-{#AppVersion}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
LicenseFile=..\LICENSE
ShowLanguageDialog=auto
CloseApplications=yes

[Languages]
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
polish.AutostartTask=Uruchamiaj przy logowaniu
english.AutostartTask=Start at sign-in
polish.EditorShortcut=Elemental Wheel - edytor
english.EditorShortcut=Elemental Wheel Editor
polish.RunApp=Uruchom Elemental Wheel
english.RunApp=Launch Elemental Wheel

[Messages]
polish.FinishedLabel=Zainstalowano Elemental Wheel {#AppVersion}.
polish.FinishedLabelNoIcons=Zainstalowano Elemental Wheel {#AppVersion}.
english.FinishedLabel=Elemental Wheel {#AppVersion} has been installed.
english.FinishedLabelNoIcons=Elemental Wheel {#AppVersion} has been installed.

[Tasks]
Name: "autostart"; Description: "{cm:AutostartTask}"

[Files]
Source: "..\target\release\wheel.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\wheel-editor.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Elemental Wheel"; Filename: "{app}\wheel.exe"; Parameters: "--run"
Name: "{userprograms}\{cm:EditorShortcut}"; Filename: "{app}\wheel-editor.exe"

[Registry]
; An update without the task ticked removes a value left by an earlier install.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Elemental Wheel"; ValueData: """{app}\wheel.exe"" --run"; Flags: uninsdeletevalue; Tasks: autostart
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "Elemental Wheel"; Flags: deletevalue; Tasks: not autostart

[Run]
Filename: "{app}\wheel.exe"; Parameters: "--run"; Description: "{cm:RunApp}"; Flags: nowait postinstall

[UninstallRun]
Filename: "{app}\wheel.exe"; Parameters: "--quit"; RunOnceId: "QuitWheel"; Flags: runhidden waituntilterminated

[Code]
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Wheel: String;
  ResultCode: Integer;
begin
  Wheel := ExpandConstant('{app}\wheel.exe');
  if FileExists(Wheel) then
  begin
    Exec(Wheel, '--quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    Sleep(500);
  end;
  Result := '';
end;
