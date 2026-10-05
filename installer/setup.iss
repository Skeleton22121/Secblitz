; Compile through scripts/build-release.ps1 (Inno Setup 6.4 or later).
#ifndef AppVersion
  #error AppVersion must be supplied by build-release.ps1
#endif
#ifndef SourceExe
  #error SourceExe must be supplied by build-release.ps1
#endif
#ifndef OutputPath
  #define OutputPath "..\dist"
#endif

[Setup]
AppId={{30C8385C-D114-44DD-868B-45C469D690BC}
AppName=Secblitz
AppVersion={#AppVersion}
AppPublisher=Secblitz
DefaultDirName={autopf64}\Secblitz
DisableDirPage=yes
UsePreviousAppDir=no
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern
SetupIconFile=..\assets\secblitz.ico
UninstallDisplayIcon={app}\secblitz.exe
OutputDir={#OutputPath}
OutputBaseFilename=secblitz-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
CloseApplications=no
RestartApplications=no
ChangesAssociations=no
UsePreviousTasks=no
SetupMutex=Global\SecblitzSetup
Uninstallable=yes
#ifdef SignToolName
SignTool={#SignToolName}
SignedUninstaller=yes
#endif

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "pt"; MessagesFile: "compiler:Languages\Portuguese.isl"
Name: "it"; MessagesFile: "compiler:Languages\Italian.isl"

[CustomMessages]
en.AutoUpdates=Keep Secblitz up to date automatically (checks every hour)
es.AutoUpdates=Mantener Secblitz actualizado automáticamente (comprueba cada hora)
fr.AutoUpdates=Maintenir Secblitz à jour automatiquement (vérification toutes les heures)
de.AutoUpdates=Secblitz automatisch aktuell halten (prüft jede Stunde)
pt.AutoUpdates=Manter o Secblitz atualizado automaticamente (verifica a cada hora)
it.AutoUpdates=Mantieni Secblitz aggiornato automaticamente (controlla ogni ora)
en.DesktopIcon=Keep Secblitz handy - create a desktop shortcut
es.DesktopIcon=Ten Secblitz a mano: crea un acceso directo en el escritorio
fr.DesktopIcon=Gardez Secblitz à portée de main : créez un raccourci sur le bureau
de.DesktopIcon=Secblitz griffbereit halten – Desktop-Verknüpfung erstellen
pt.DesktopIcon=Tenha o Secblitz à mão - criar um atalho no ambiente de trabalho
it.DesktopIcon=Secblitz a portata di mano: crea un collegamento sul desktop
en.LaunchSecblitz=Open Secblitz - start the guided setup
es.LaunchSecblitz=Abrir Secblitz: iniciar la configuración guiada
fr.LaunchSecblitz=Ouvrir Secblitz : démarrer la configuration guidée
de.LaunchSecblitz=Secblitz öffnen – geführte Einrichtung starten
pt.LaunchSecblitz=Abrir o Secblitz - iniciar a configuração guiada
it.LaunchSecblitz=Apri Secblitz: avvia la configurazione guidata
en.Monitor=Install the optional read-only monitor (starts at next Windows boot)
es.Monitor=Instalar el monitor opcional de solo lectura (se inicia al reiniciar Windows)
fr.Monitor=Installer le moniteur facultatif en lecture seule (au prochain démarrage de Windows)
de.Monitor=Optionalen schreibgeschützten Monitor installieren (ab nächstem Windows-Start)
pt.Monitor=Instalar o monitor opcional apenas de leitura (no próximo arranque do Windows)
it.Monitor=Installa il monitor facoltativo in sola lettura (si avvia al prossimo avvio di Windows)
en.Failed=Secblitz maintenance failed. See the setup log. No security settings were applied.
es.Failed=Error de mantenimiento de Secblitz. Consulte el registro. No se aplicaron ajustes de seguridad.
fr.Failed=Échec de la maintenance Secblitz. Consultez le journal. Aucun réglage de sécurité appliqué.
de.Failed=Secblitz-Wartung fehlgeschlagen. Siehe Protokoll. Keine Sicherheitseinstellungen angewendet.
pt.Failed=A manutenção do Secblitz falhou. Consulte o registo. Nenhuma definição de segurança foi aplicada.
it.Failed=Manutenzione di Secblitz non riuscita. Consulta il registro di installazione. Nessuna impostazione di sicurezza è stata applicata.

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; Check: DesktopDefault
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; Flags: unchecked; Check: DesktopOptedOut
Name: "monitor"; Description: "{cm:Monitor}"; Flags: unchecked
Name: "autoupdates"; Description: "{cm:AutoUpdates}"; Check: AutoUpdatesDefault
Name: "autoupdates"; Description: "{cm:AutoUpdates}"; Flags: unchecked; Check: AutoUpdatesOptedOut

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "secblitz.exe"; Flags: ignoreversion

[Icons]
; Ordinary launch: the application's asInvoker manifest and UAC flow own elevation.
Name: "{commonprograms}\Secblitz"; Filename: "{app}\secblitz.exe"; WorkingDir: "{app}"
Name: "{commondesktop}\Secblitz"; Filename: "{app}\secblitz.exe"; WorkingDir: "{app}"; Tasks: desktopicon

; Inno creates/removes the owned .lnk. No wildcard [UninstallDelete].
[Run]
; Start Setup normally so Inno retains the original unelevated user context.
; runasoriginaluser cannot de-elevate an already-elevated Setup invocation.
Filename: "{app}\secblitz.exe"; Parameters: "guide"; WorkingDir: "{app}"; Description: "{cm:LaunchSecblitz}"; Flags: nowait postinstall skipifsilent runasoriginaluser; Check: CanLaunchSecblitz

[Code]
{ Embed the maintenance source in both Setup and Uninstall. Never execute a
  pre-existing script from the installation directory before validating it. }
const MaintenanceSource =
#define MaintenanceHandle
#define MaintenanceLine
#sub EmitMaintenanceLine
  #define MaintenanceLine = FileRead(MaintenanceHandle)
  #emit "'" + StringChange(MaintenanceLine, "'", "''") + "' + #13#10 +"
#endsub
#for {MaintenanceHandle = FileOpen(AddBackslash(SourcePath) + "maintenance.ps1"); MaintenanceHandle && !FileEof(MaintenanceHandle); ""} EmitMaintenanceLine
#if MaintenanceHandle
  #expr FileClose(MaintenanceHandle)
#else
  #error Cannot read maintenance.ps1
#endif
'';

var ResumeAfterUpgrade, PostInstallFailed: Boolean;
    PreviousDesktopSelected, PreviousAutoUpdatesEnabled, PreviousHasUpdatePreference: Boolean;

procedure InitializeWizard;
var Enabled: Cardinal;
begin
  { Snapshot while the previous uninstall data is still available. Inno replaces
    that data before RegisterPreviousData, so that callback must not reread it. }
  PreviousDesktopSelected := GetPreviousData('DesktopSelected', '1') = '1';
  PreviousHasUpdatePreference := RegQueryDWordValue(HKLM64, 'Software\Secblitz', 'AutoUpdatesEnabled', Enabled);
  PreviousAutoUpdatesEnabled := True;
  if PreviousHasUpdatePreference then PreviousAutoUpdatesEnabled := Enabled <> 0;
end;

function DesktopDefault: Boolean;
begin
  Result := PreviousDesktopSelected;
end;

function DesktopOptedOut: Boolean;
begin
  Result := not DesktopDefault;
end;

procedure RegisterPreviousData(PreviousDataKey: Integer);
var DesktopSelected: Boolean;
begin
  { Automatic upgrades suppress optional tasks, not the saved desktop choice. }
  if ExpandConstant('{param:SECBLITZUPDATE|0}') = '1' then
    DesktopSelected := PreviousDesktopSelected
  else
    DesktopSelected := WizardIsTaskSelected('desktopicon');
  if DesktopSelected then
    SetPreviousData(PreviousDataKey, 'DesktopSelected', '1')
  else
    SetPreviousData(PreviousDataKey, 'DesktopSelected', '0');
end;

function AutoUpdatesDefault: Boolean;
begin
  Result := PreviousAutoUpdatesEnabled;
end;

function AutoUpdatesOptedOut: Boolean;
begin
  Result := not AutoUpdatesDefault;
end;

function HasUpdatePreference: Boolean;
begin
  Result := PreviousHasUpdatePreference;
end;

{ Setup's default x86 process uses 32-bit pointers, including in 64-bit install mode. }
function EnvironmentBlock: LongWord;
  external 'GetEnvironmentStringsW@kernel32.dll stdcall';
function FreeEnvironmentBlock(Block: LongWord): Boolean;
  external 'FreeEnvironmentStringsW@kernel32.dll stdcall';
function EnvironmentLength(Value: LongWord): Integer;
  external 'lstrlenW@kernel32.dll stdcall';
function EnvironmentCopy(Dest: String; Source: LongWord; Count: Integer): LongWord;
  external 'lstrcpynW@kernel32.dll stdcall';
function SetEnvironment(Name, Value: String): Boolean;
  external 'SetEnvironmentVariableW@kernel32.dll stdcall';
function DeleteEnvironment(Name: String; Value: LongWord): Boolean;
  external 'SetEnvironmentVariableW@kernel32.dll stdcall';

procedure PutEnvironment(Name, Value: String);
begin
  if not SetEnvironment(Name, Value) then RaiseException('Cannot set maintenance environment.');
end;

procedure CleanEnvironment(Saved: TStringList);
var Block, Entry: LongWord; N, Split: Integer; Value: String;
begin
  Block := EnvironmentBlock;
  if Block = 0 then RaiseException('Cannot read maintenance environment.');
  try
    Entry := Block;
    N := EnvironmentLength(Entry);
    while N > 0 do begin
      SetLength(Value, N + 1);
      EnvironmentCopy(Value, Entry, N + 1);
      SetLength(Value, N);
      Split := Pos('=', Value);
      if Split > 1 then begin
        Saved.Add(Value);
        if not DeleteEnvironment(Copy(Value, 1, Split - 1), 0) then
          RaiseException('Cannot clear maintenance environment.');
      end;
      Entry := Entry + (N + 1) * 2;
      N := EnvironmentLength(Entry);
    end;
  finally
    FreeEnvironmentBlock(Block);
  end;
  { An allowlist also removes CLR profiler/startup-hook and module-path injection. }
  PutEnvironment('SystemRoot', ExpandConstant('{win}'));
  PutEnvironment('windir', ExpandConstant('{win}'));
  PutEnvironment('SystemDrive', ExtractFileDrive(ExpandConstant('{win}')));
  PutEnvironment('PATH', ExpandConstant('{sys}'));
  PutEnvironment('ComSpec', ExpandConstant('{sys}\cmd.exe'));
  PutEnvironment('PATHEXT', '.COM;.EXE;.BAT;.CMD');
  PutEnvironment('TEMP', ExpandConstant('{tmp}'));
  PutEnvironment('TMP', ExpandConstant('{tmp}'));
  PutEnvironment('PSModulePath', ExpandConstant('{sys}\WindowsPowerShell\v1.0\Modules'));
end;

procedure RestoreEnvironment(Saved: TStringList);
var I, Split: Integer; Value: String;
begin
  DeleteEnvironment('SystemRoot', 0);
  DeleteEnvironment('windir', 0);
  DeleteEnvironment('SystemDrive', 0);
  DeleteEnvironment('PATH', 0);
  DeleteEnvironment('ComSpec', 0);
  DeleteEnvironment('PATHEXT', 0);
  DeleteEnvironment('TEMP', 0);
  DeleteEnvironment('TMP', 0);
  DeleteEnvironment('PSModulePath', 0);
  for I := 0 to Saved.Count - 1 do begin
    Value := Saved[I];
    Split := Pos('=', Value);
    SetEnvironment(Copy(Value, 1, Split - 1), Copy(Value, Split + 1, Length(Value)));
  end;
end;

function Maintain(Action: String): Boolean;
var Code: Integer; Saved: TStringList; Script, Arguments: String;
begin
  Script := ExpandConstant('{tmp}\secblitz-maintenance.ps1');
  if not SaveStringToFile(Script, UTF8Encode(MaintenanceSource), False) then
    RaiseException('Cannot extract maintenance script.');
  Saved := TStringList.Create;
  Code := -1;
  try
    CleanEnvironment(Saved);
    Arguments := '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' +
      Script + '" -Action ' + Action;
    if IsUninstaller and (Action = 'RemoveMonitor') then
      Arguments := Arguments + ' -UninstallerDataPath "' +
        ChangeFileExt(ExpandConstant('{uninstallexe}'), '.dat') + '"';
    Result := Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
      Arguments, ExpandConstant('{sys}'), SW_HIDE, ewWaitUntilTerminated, Code);
  finally
    RestoreEnvironment(Saved);
    Saved.Free;
    DeleteFile(Script);
  end;
  if Result and (Action = 'Prepare') and (Code = 10) then begin
    ResumeAfterUpgrade := True;
    Code := 0;
  end;
  if Result then Result := Code = 0;
  if not Result then Log('Secblitz maintenance failed: ' + Action + ', code ' + IntToStr(Code));
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  { Reject /DIR overrides too, not just directory-page edits. }
  if CompareText(ExpandConstant('{app}'), ExpandConstant('{autopf64}\Secblitz')) <> 0 then
  begin
    Result := 'Secblitz requires the fixed Program Files\Secblitz directory.';
    Exit;
  end;
  if not Maintain('Prepare') then
    Result := ExpandConstant('{cm:Failed}');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    { Inno may display an event exception yet finish with exit 0. Keep failure
      latched across every exception/early return until ALL requested work succeeds. }
    PostInstallFailed := True;
    if not Maintain('Secure') then
      RaiseException(ExpandConstant('{cm:Failed}'));
    if WizardIsTaskSelected('monitor') then
      if not Maintain('InstallMonitor') then
        RaiseException(ExpandConstant('{cm:Failed}'));
    if ResumeAfterUpgrade then begin
      if not Maintain('ResumeMonitor') then RaiseException(ExpandConstant('{cm:Failed}'));
      ResumeAfterUpgrade := False;
    end;
    { The marker is a preference-preservation hint, not caller authentication.
      Setup already requires elevation; it grants no additional authority. }
    if ExpandConstant('{param:SECBLITZUPDATE|0}') = '1' then begin
      if not Maintain('PreserveUpdates') then RaiseException(ExpandConstant('{cm:Failed}'));
    end else if WizardIsTaskSelected('autoupdates') then begin
      if not Maintain('EnableUpdates') then RaiseException(ExpandConstant('{cm:Failed}'));
    end else if WizardSilent and HasUpdatePreference then begin
      { Older verified workers use /TASKS="" without the marker. Only an explicit
        interactive opt-out removes an existing enabled task on upgrade. }
      if not Maintain('PreserveUpdates') then RaiseException(ExpandConstant('{cm:Failed}'));
    end else begin
      if not Maintain('DisableUpdates') then RaiseException(ExpandConstant('{cm:Failed}'));
    end;
    PostInstallFailed := False;
  end;
end;

function GetCustomSetupExitCode: Integer;
begin
  Result := 0;
  if PostInstallFailed then Result := 20;
end;

function CanLaunchSecblitz: Boolean;
begin
  Result := not PostInstallFailed;
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  { The error has already been shown; do not follow it with a success page. }
  Result := (PageID = wpFinished) and PostInstallFailed;
end;

procedure DeinitializeSetup;
begin
  { Also restore a formerly running monitor after cancellation/rollback. }
  if ResumeAfterUpgrade then
    if not Maintain('ResumeMonitor') then Log('Could not resume monitor after failed upgrade.');
end;

function InitializeUninstall(): Boolean;
begin
  Result := True;
  { Inno may call this once before its elevation relaunch. }
  if not IsAdmin then Exit;
  try
    Result := Maintain('RemoveMonitor');
  except
    Log('Secblitz uninstall maintenance exception: ' + GetExceptionMessage);
    Result := False;
  end;
  if not Result then begin
    Log(ExpandConstant('{cm:Failed}'));
    if not UninstallSilent then
      SuppressibleMsgBox(ExpandConstant('{cm:Failed}'), mbError, MB_OK, IDOK);
  end;
end;
