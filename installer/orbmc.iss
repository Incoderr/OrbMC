; OrbMC installer (Inno Setup 6). Per-user install, no administrator rights.
;
; Build: iscc /DAppVersion=0.2.2 /DSourceExe=..\target\release\orbmc.exe installer\orbmc.iss
; The release workflow builds it as dist\orbmc-<version>-windows-x64-setup.exe:
; the launcher finds its update by that name (platform::self_update).
;
; The launcher updates itself by running this same installer with
; /VERYSILENT /SUPPRESSMSGBOXES /NORESTART: it waits for the running
; launcher to quit, replaces the files and starts the new version.
; Launcher data (%LOCALAPPDATA%\OrbMC) is never touched, uninstall included.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\orbmc.exe"
#endif
#ifndef OutputName
  #define OutputName "orbmc-setup"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
; Never change AppId: Windows identifies the installed launcher by it.
AppId={{6F1C2B8E-4D3A-4B7F-9E21-0A5C8D7B3F14}
AppName=OrbMC
AppVersion={#AppVersion}
AppVerName=OrbMC {#AppVersion}
AppPublisher=OrbMC
AppPublisherURL=https://github.com/Incoderr/OrbMC
AppSupportURL=https://github.com/Incoderr/OrbMC/issues
AppUpdatesURL=https://github.com/Incoderr/OrbMC/releases
VersionInfoVersion={#AppVersion}
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\OrbMC
DisableDirPage=auto
DefaultGroupName=OrbMC
DisableProgramGroupPage=yes
UninstallDisplayName=OrbMC
UninstallDisplayIcon={app}\orbmc.exe
SetupIconFile=..\assets\logo\orbmc.ico
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
Compression=lzma2/max
SolidCompression=yes
; The launcher is closed by the update itself (see InitializeSetup); Restart
; Manager would only ask about it again.
CloseApplications=no
RestartApplications=no

[Languages]
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "orbmc.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\OrbMC"; Filename: "{app}\orbmc.exe"
Name: "{autodesktop}\OrbMC"; Filename: "{app}\orbmc.exe"; Tasks: desktopicon

[Run]
; Runs in silent updates too (no skipifsilent): the updated launcher starts
; on its own, unless the update ran because the user quit (/NOLAUNCH).
Filename: "{app}\orbmc.exe"; Description: "{cm:LaunchProgram,OrbMC}"; Flags: nowait postinstall; Check: ShouldLaunch

[InstallDelete]
; Leftovers of the old self-replacing updater.
Type: files; Name: "{app}\orbmc.old.exe"
Type: files; Name: "{app}\orbmc.update.exe"

[Code]
const
  // Held by the running launcher (platform::single_instance).
  LauncherMutex = 'Local\OrbMC.SingleInstance';

function ShouldLaunch(): Boolean;
begin
  Result := Pos('/NOLAUNCH', Uppercase(GetCmdTail())) = 0;
end;

// Waits for a running launcher to quit: during an update it is closing right
// after starting this installer.
function InitializeSetup(): Boolean;
var
  Waited: Integer;
begin
  Waited := 0;
  while CheckForMutexes(LauncherMutex) and (Waited < 30000) do
  begin
    Sleep(250);
    Waited := Waited + 250;
  end;
  Result := True;
  if CheckForMutexes(LauncherMutex) then
  begin
    if WizardSilent() then
      Result := False
    else
      Result := MsgBox(CustomMessage('CloseLauncher'), mbError, MB_RETRYCANCEL) = IDRETRY;
    if Result then
      Result := InitializeSetup();
  end;
end;

[CustomMessages]
ru.CloseLauncher=OrbMC сейчас запущен. Закрой лаунчер (и в трее тоже) и нажми «Повтор».
en.CloseLauncher=OrbMC is running. Close the launcher (the tray one too) and press Retry.
