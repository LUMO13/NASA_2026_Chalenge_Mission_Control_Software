; Inno Setup recipe for Windows. Requires the binaries produced by scripts/04_build_release.ps1.
; This script is configuration only; the ZIP does NOT include a pre-built installer.
#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
[Setup]
AppId={{6C097360-8CFB-4E5B-9416-75D388419F83}
AppName=FPECS
AppVersion={#AppVersion}
SetupIconFile=..\assets\fpecs.ico
AppPublisher=FPECS NASA Space Apps Team
DefaultDirName={localappdata}\Programs\FPECS
DefaultGroupName=FPECS
PrivilegesRequired=lowest
OutputDir=..\..\dist\windows-x86_64
OutputBaseFilename=FPECS_Setup_{#AppVersion}_x64
Compression=lzma
SolidCompression=yes
ArchitecturesAllowed=x64compatible
UninstallDisplayIcon={app}\desktop-gui.exe

[Files]
Source: "..\..\dist\windows-x86_64\desktop-gui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\windows-x86_64\sensor-simulator.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\windows-x86_64\telemetry-receiver.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\windows-x86_64\telemetry-distributor.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "FPECS_DB_Setup.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\fpecs.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\migrations\001_initial_schema.sql"; DestDir: "{app}\migrations"; Flags: ignoreversion
Source: "..\..\migrations\002_runtime_snapshot.sql"; DestDir: "{app}\migrations"; Flags: ignoreversion

[Icons]
Name: "{group}\FPECS"; Filename: "{app}\desktop-gui.exe"; IconFilename: "{app}\fpecs.ico"

[Run]
Filename: "{app}\desktop-gui.exe"; Description: "Launch FPECS"; Flags: nowait postinstall skipifsilent
