; Inno Setup script for Sheshnag. Built by packaging/windows/build-windows.ps1.
; Pass /DAppVersion=1.0.0 on the ISCC command line.
#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif

[Setup]
AppId={{6B1D7C0A-3E5F-4C58-9A34-5E2B9D1F7A41}
AppName=Sheshnag
AppVersion={#AppVersion}
AppPublisher=Sheshnag
DefaultDirName={autopf}\Sheshnag
DefaultGroupName=Sheshnag
DisableProgramGroupPage=yes
OutputDir=..\..\dist
OutputBaseFilename=Sheshnag-{#AppVersion}-windows-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\sheshnag.exe
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\..\target\release\sheshnag-ui.exe"; DestDir: "{app}"; DestName: "sheshnag.exe"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Sheshnag"; Filename: "{app}\sheshnag.exe"
Name: "{autodesktop}\Sheshnag"; Filename: "{app}\sheshnag.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\sheshnag.exe"; Description: "Launch Sheshnag"; Flags: nowait postinstall skipifsilent
