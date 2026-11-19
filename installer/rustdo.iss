; Inno Setup script for the RustDo Windows installer.
; Build:  iscc /DAppVersion=0.1.0 installer\rustdo.iss
; Output: target\installer\RustDo-<version>-setup.exe
;
; Installs for the current user only (no administrator rights needed), so the
; in-app updater can run the next installer without a UAC prompt.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

[Setup]
; Never change AppId: Windows uses it to recognise upgrades of the same app.
AppId={{6E0B7C3E-4F0A-4E27-9C55-2B1D8A4F7D31}
AppName=RustDo
AppVersion={#AppVersion}
AppVerName=RustDo {#AppVersion}
AppPublisher=metman2013
AppPublisherURL=https://github.com/metman2013/RustDo
AppSupportURL=https://github.com/metman2013/RustDo/issues
AppUpdatesURL=https://github.com/metman2013/RustDo/releases
DefaultDirName={localappdata}\Programs\RustDo
DefaultGroupName=RustDo
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\target\installer
OutputBaseFilename=RustDo-{#AppVersion}-setup
SetupIconFile=..\assets\rustdo.ico
UninstallDisplayIcon={app}\rustdo.exe
UninstallDisplayName=RustDo
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Close a running RustDo before replacing it, and reopen it afterwards.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\rustdo.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\rustdo.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
; AppUserModelID must match notify.rs (AUMID): Windows ties reminder notifications to it.
Name: "{group}\RustDo"; Filename: "{app}\rustdo.exe"; IconFilename: "{app}\rustdo.ico"; AppUserModelID: "metman2013.RustDo"
Name: "{autodesktop}\RustDo"; Filename: "{app}\rustdo.exe"; IconFilename: "{app}\rustdo.ico"; Tasks: desktopicon

[Run]
; Normal install: offer "Launch RustDo" on the last page.
Filename: "{app}\rustdo.exe"; Description: "{cm:LaunchProgram,RustDo}"; Flags: nowait postinstall skipifsilent
; In-app update (runs this installer with /VERYSILENT): reopen RustDo when done.
Filename: "{app}\rustdo.exe"; Flags: nowait; Check: WizardSilent

[UninstallRun]
; Stop the background reminder helper and remove its sign-in entry.
Filename: "{sys}\taskkill.exe"; Parameters: "/IM rustdo.exe /F"; Flags: runhidden; RunOnceId: "StopRustDo"
Filename: "{sys}\reg.exe"; Parameters: "delete ""HKCU\Software\Microsoft\Windows\CurrentVersion\Run"" /v ""RustDo reminders"" /f"; Flags: runhidden; RunOnceId: "RemoveReminderAutostart"

; Your tasks live in Documents\RustDo and are deliberately NOT removed on uninstall.
