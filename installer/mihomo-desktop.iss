; Inno Setup 6 Script for Mihomo Desktop (Windows Service Edition)
; Build via: scripts\build-installer.ps1
; Or manually: iscc installer\mihomo-desktop.iss

#define MyAppName "Mihomo Desktop"
#ifndef MyAppVersion
  #define MyAppVersion "0.1.2"
#endif
#define MyAppPublisher "Mihomo Desktop"
#define MyAppExeName "mihomo-desktop.exe"
#define MyServiceName "mihomo"
#define StageDir "..\dist\stage"

[Setup]
AppId={{8D6E492A-91F3-4C5E-A28D-5F47B19D30C1}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\Mihomo
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=MihomoDesktop-Setup-{#MyAppVersion}-x64
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"

[Files]
; Desktop application (also acts as the native Windows Service host via --service)
Source: "{#StageDir}\mihomo-desktop.exe"; DestDir: "{app}"; Flags: ignoreversion
; Bundled Mihomo core binary (out-of-the-box ready without remote download)
Source: "{#StageDir}\mihomo.exe"; DestDir: "{app}"; Flags: ignoreversion
; Bundled GeoIP / GeoSite / MMDB / ASN databases for offline cold-start reliability
Source: "{#StageDir}\geo\*"; DestDir: "{app}\geo"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
; Install/upgrade Windows Service, configure non-admin DACL & ProgramData ACL, deploy bundled GEO files, generate minimal clash.yaml if missing, and start service
Filename: "{app}\{#MyAppExeName}"; Parameters: "--install-service --service-name {#MyServiceName} --core ""{app}\mihomo.exe"" --work-dir ""{commonappdata}\Mihomo"" --config ""{commonappdata}\Mihomo\config\clash.yaml"" --bundled-geo-dir ""{app}\geo"""; StatusMsg: "Installing and starting Mihomo Windows Service..."; Flags: runhidden waituntilterminated
; Optionally launch Desktop UI after setup finishes
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[UninstallRun]
; Cleanly stop and remove the Windows Service on uninstall (user configs in ProgramData\Mihomo are preserved)
Filename: "{app}\{#MyAppExeName}"; Parameters: "--uninstall-service --service-name {#MyServiceName}"; Flags: runhidden waituntilterminated; RunOnceId: "UninstallMihomoService"

[Code]
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Result := '';
  // Stop existing mihomo service & desktop UI before replacing binaries on upgrade
  Exec('sc.exe', 'stop {#MyServiceName}', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('taskkill.exe', '/F /IM mihomo-desktop.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('taskkill.exe', '/F /IM mihomo.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(800);
end;
