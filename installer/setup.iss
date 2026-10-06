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
; The tray agent answers WM_QUERYENDSESSION/WM_ENDSESSION/WM_CLOSE by exiting, so
; Restart Manager can close it. Updates also signal it through the quiesce event.
CloseApplications=yes
CloseApplicationsFilter=secblitz.exe
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
Name: "pt"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
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
pt.DesktopIcon=Tenha o Secblitz à mão - criar um atalho na área de trabalho
it.DesktopIcon=Secblitz a portata di mano: crea un collegamento sul desktop
en.LaunchSecblitz=Open Secblitz
es.LaunchSecblitz=Abrir Secblitz
fr.LaunchSecblitz=Ouvrir Secblitz
de.LaunchSecblitz=Secblitz öffnen
pt.LaunchSecblitz=Abrir o Secblitz
it.LaunchSecblitz=Apri Secblitz
en.TrayIcon=Show the Secblitz shield in the taskbar corner
es.TrayIcon=Mostrar el escudo de Secblitz en la esquina de la barra de tareas
fr.TrayIcon=Afficher le bouclier Secblitz dans le coin de la barre des tâches
de.TrayIcon=Secblitz-Schild in der Ecke der Taskleiste anzeigen
pt.TrayIcon=Mostrar o escudo do Secblitz no canto da barra de tarefas
it.TrayIcon=Mostra lo scudo di Secblitz nell'angolo della barra delle applicazioni
en.Monitor=Check my PC automatically in the background (from the next restart)
es.Monitor=Comprobar mi PC automáticamente en segundo plano (desde el próximo reinicio)
fr.Monitor=Vérifier mon PC automatiquement en arrière-plan (dès le prochain redémarrage)
de.Monitor=Meinen PC automatisch im Hintergrund prüfen (ab dem nächsten Neustart)
pt.Monitor=Verificar meu PC automaticamente em segundo plano (a partir da próxima reinicialização)
it.Monitor=Controlla il mio PC automaticamente in background (dal prossimo riavvio)
en.Failed=Secblitz maintenance failed. See the setup log. No security settings were applied.
es.Failed=Error de mantenimiento de Secblitz. Consulte el registro. No se aplicaron ajustes de seguridad.
fr.Failed=Échec de la maintenance Secblitz. Consultez le journal. Aucun réglage de sécurité appliqué.
de.Failed=Secblitz-Wartung fehlgeschlagen. Siehe Protokoll. Keine Sicherheitseinstellungen angewendet.
pt.Failed=A manutenção do Secblitz falhou. Consulte o log de instalação. Nenhuma configuração de segurança foi aplicada.
it.Failed=Manutenzione di Secblitz non riuscita. Consulta il registro di installazione. Nessuna impostazione di sicurezza è stata applicata.
en.RemoveTitle=Remove Secblitz
es.RemoveTitle=Quitar Secblitz
fr.RemoveTitle=Supprimer Secblitz
de.RemoveTitle=Secblitz entfernen
pt.RemoveTitle=Remover o Secblitz
it.RemoveTitle=Rimuovi Secblitz
en.RemoveQuestion=What should happen to the changes Secblitz made?
es.RemoveQuestion=¿Qué debe pasar con los cambios que hizo Secblitz?
fr.RemoveQuestion=Que faire des modifications effectuées par Secblitz ?
de.RemoveQuestion=Was soll mit den Änderungen geschehen, die Secblitz vorgenommen hat?
pt.RemoveQuestion=O que deve acontecer com as alterações feitas pelo Secblitz?
it.RemoveQuestion=Cosa deve succedere alle modifiche fatte da Secblitz?
en.KeepChoice=Keep my PC as it is now
es.KeepChoice=Dejar mi PC como está ahora
fr.KeepChoice=Laisser mon PC tel qu'il est maintenant
de.KeepChoice=Meinen PC so lassen, wie er jetzt ist
pt.KeepChoice=Manter meu PC como está agora
it.KeepChoice=Lascia il mio PC com'è adesso
en.KeepDetail=Your protection stays on. Apps you removed stay removed; you can reinstall them from the Microsoft Store.
es.KeepDetail=Tu protección sigue activa. Las apps que quitaste siguen quitadas; puedes volver a instalarlas desde Microsoft Store.
fr.KeepDetail=Votre protection reste active. Les applications que vous avez supprimées restent supprimées ; vous pouvez les réinstaller depuis le Microsoft Store.
de.KeepDetail=Dein Schutz bleibt eingeschaltet. Apps, die Du entfernt hast, bleiben entfernt; Du kannst sie aus dem Microsoft Store neu installieren.
pt.KeepDetail=Sua proteção continua ativada. Os apps que você removeu continuam removidos; você pode reinstalá-los pela Microsoft Store.
it.KeepDetail=La tua protezione resta attiva. Le app che hai rimosso restano rimosse; puoi reinstallarle dal Microsoft Store.
en.PutBackChoice=Put everything back the way it was
es.PutBackChoice=Devolver todo a como estaba
fr.PutBackChoice=Tout remettre comme avant
de.PutBackChoice=Alles wieder so machen, wie es war
pt.PutBackChoice=Voltar tudo como estava
it.PutBackChoice=Rimetti tutto com'era
en.PutBackDetail=Secblitz undoes its changes and brings back the apps you removed first. This can take a few minutes.
es.PutBackDetail=Primero Secblitz deshace sus cambios y recupera las apps que quitaste. Esto puede tardar unos minutos.
fr.PutBackDetail=Secblitz commence par annuler ses modifications et récupérer les applications que vous avez supprimées. Cela peut prendre quelques minutes.
de.PutBackDetail=Secblitz macht zuerst seine Änderungen rückgängig und holt die Apps zurück, die Du entfernt hast. Das kann einige Minuten dauern.
pt.PutBackDetail=O Secblitz primeiro desfaz suas alterações e traz de volta os apps que você removeu. Isso pode levar alguns minutos.
it.PutBackDetail=Prima Secblitz annulla le sue modifiche e recupera le app che hai rimosso. Può richiedere alcuni minuti.
en.RemoveNote=Windows updates, virus scans and apps you installed with Secblitz stay.
es.RemoveNote=Las actualizaciones de Windows, los análisis de virus y las apps que instalaste con Secblitz se quedan.
fr.RemoveNote=Les mises à jour Windows, les analyses antivirus et les applications installées avec Secblitz sont conservées.
de.RemoveNote=Windows-Updates, Virenscans und mit Secblitz installierte Apps bleiben erhalten.
pt.RemoveNote=As atualizações do Windows, as verificações de vírus e os apps que você instalou com o Secblitz permanecem.
it.RemoveNote=Gli aggiornamenti di Windows, le scansioni antivirus e le app installate con Secblitz restano.
en.WebStops=Web protection stops too, because it is part of Secblitz.
es.WebStops=La protección web también se detiene, porque forma parte de Secblitz.
fr.WebStops=La protection web s’arrête aussi, car elle fait partie de Secblitz.
de.WebStops=Der Webschutz endet ebenfalls, weil er Teil von Secblitz ist.
pt.WebStops=A proteção da web também para, porque faz parte do Secblitz.
it.WebStops=Anche la protezione web si ferma, perché fa parte di Secblitz.
en.PuttingBack=Putting your settings back
es.PuttingBack=Restaurando tus ajustes
fr.PuttingBack=Rétablissement de vos paramètres
de.PuttingBack=Deine Einstellungen werden zurückgesetzt
pt.PuttingBack=Restaurando suas configurações
it.PuttingBack=Ripristino delle tue impostazioni
en.LeftIntro=Some things could not be put back:
es.LeftIntro=Algunas cosas no se pudieron restaurar:
fr.LeftIntro=Certaines choses n'ont pas pu être rétablies :
de.LeftIntro=Einiges ließ sich nicht zurücksetzen:
pt.LeftIntro=Algumas coisas não puderam ser restauradas:
it.LeftIntro=Alcune cose non è stato possibile ripristinarle:
en.PersonalLeft=Some of your personal settings could not be put back.
es.PersonalLeft=Algunos de tus ajustes personales no se pudieron restaurar.
fr.PersonalLeft=Certains de vos paramètres personnels n'ont pas pu être rétablis.
de.PersonalLeft=Einige Deiner persönlichen Einstellungen ließen sich nicht zurücksetzen.
pt.PersonalLeft=Algumas das suas configurações pessoais não puderam ser restauradas.
it.PersonalLeft=Alcune delle tue impostazioni personali non è stato possibile ripristinarle.
en.SettingsLeft=Some settings could not be put back.
es.SettingsLeft=Algunos ajustes no se pudieron restaurar.
fr.SettingsLeft=Certains paramètres n'ont pas pu être rétablis.
de.SettingsLeft=Einige Einstellungen ließen sich nicht zurücksetzen.
pt.SettingsLeft=Algumas configurações não puderam ser restauradas.
it.SettingsLeft=Alcune impostazioni non è stato possibile ripristinarle.


[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; Check: DesktopDefault
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; Flags: unchecked; Check: DesktopOptedOut
Name: "trayicon"; Description: "{cm:TrayIcon}"
Name: "monitor"; Description: "{cm:Monitor}"; Flags: unchecked
Name: "autoupdates"; Description: "{cm:AutoUpdates}"; Check: AutoUpdatesDefault
Name: "autoupdates"; Description: "{cm:AutoUpdates}"; Flags: unchecked; Check: AutoUpdatesOptedOut

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "secblitz.exe"; Flags: ignoreversion

[Registry]
; Per-machine logon entry for the unelevated tray agent; removed on uninstall.
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "SecblitzTray"; ValueData: """{app}\secblitz.exe"" tray"; Flags: uninsdeletevalue; Tasks: trayicon

[Icons]
; Ordinary launch: the application's asInvoker manifest and UAC flow own elevation.
Name: "{commonprograms}\Secblitz"; Filename: "{app}\secblitz.exe"; WorkingDir: "{app}"
Name: "{commondesktop}\Secblitz"; Filename: "{app}\secblitz.exe"; WorkingDir: "{app}"; Tasks: desktopicon

; Inno creates/removes the owned .lnk. No wildcard; only the fixed Status
; directory (tray status file written by the monitor/app) is removed.
[UninstallDelete]
Type: filesandordirs; Name: "{app}\Status"

[Run]
; Start Setup normally so Inno retains the original unelevated user context.
; runasoriginaluser cannot de-elevate an already-elevated Setup invocation.
Filename: "{app}\secblitz.exe"; WorkingDir: "{app}"; Description: "{cm:LaunchSecblitz}"; Flags: nowait postinstall skipifsilent runasoriginaluser; Check: CanLaunchSecblitz
; Start the tray right away for the person who ran Setup (not for silent updates,
; where the update worker restarts it in each session that had one).
Filename: "{app}\secblitz.exe"; Parameters: "tray"; WorkingDir: "{app}"; Flags: nowait skipifsilent runasoriginaluser; Tasks: trayicon; Check: CanLaunchSecblitz

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

var ResumeAfterUpgrade, ResumeFilterAfterUpgrade, PostInstallFailed: Boolean;
    UninstallPutBack: Boolean;
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

{ Politely ask a running tray agent (this session) to exit; never kill it. }
procedure CloseTray;
var Window: HWND; Tries: Integer;
begin
  Tries := 0;
  Window := FindWindowByClassName('SecblitzTrayWindow');
  while (Window <> 0) and (Tries < 20) do begin
    PostMessage(Window, 16, 0, 0); { WM_CLOSE }
    Sleep(250);
    Tries := Tries + 1;
    Window := FindWindowByClassName('SecblitzTrayWindow');
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
    if IsUninstaller and ((Action = 'RemoveMonitor') or (Action = 'RemoveFilter') or (Action = 'Purge')) then
      Arguments := Arguments + ' -UninstallerDataPath "' +
        ChangeFileExt(ExpandConstant('{uninstallexe}'), '.dat') + '"';
    Result := Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
      Arguments, ExpandConstant('{sys}'), SW_HIDE, ewWaitUntilTerminated, Code);
  finally
    RestoreEnvironment(Saved);
    Saved.Free;
    DeleteFile(Script);
  end;
  { Prepare stopped what was running: 10 the monitor, 11 web protection, 12 both. }
  if Result and (Action = 'Prepare') and (Code >= 10) and (Code <= 12) then begin
    if (Code = 10) or (Code = 12) then ResumeAfterUpgrade := True;
    if (Code = 11) or (Code = 12) then ResumeFilterAfterUpgrade := True;
    Code := 0;
  end;
  if Result then Result := Code = 0;
  if not Result then Log('Secblitz maintenance failed: ' + Action + ', code ' + IntToStr(Code));
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  CloseTray;
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
    { Web protection is registered turned off; it starts only when the person
      switches something on. An upgrade starts it again if it was running. }
    if not Maintain('InstallFilter') then
      RaiseException(ExpandConstant('{cm:Failed}'));
    if WizardIsTaskSelected('monitor') then
      if not Maintain('InstallMonitor') then
        RaiseException(ExpandConstant('{cm:Failed}'));
    if ResumeAfterUpgrade then begin
      if not Maintain('ResumeMonitor') then RaiseException(ExpandConstant('{cm:Failed}'));
      ResumeAfterUpgrade := False;
    end;
    if ResumeFilterAfterUpgrade then begin
      if not Maintain('ResumeFilter') then RaiseException(ExpandConstant('{cm:Failed}'));
      ResumeFilterAfterUpgrade := False;
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
  if ResumeFilterAfterUpgrade then
    if not Maintain('ResumeFilter') then Log('Could not resume web protection after failed upgrade.');
end;

{ ---- Removing Secblitz ----------------------------------------------------------
  Interactive uninstall asks one question: keep the PC as it is, or put
  everything back. A silent uninstall, and one the app started after it already
  put things back (/SECBLITZDONE), keeps the changes and never asks. }

function SecblitzExe: String;
begin
  Result := ExpandConstant('{app}\secblitz.exe');
end;

function HasSwitch(Switch: String): Boolean;
var I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), Switch) = 0 then Result := True;
end;

procedure AddLine(var Lines: TArrayOfString; Line: String);
var N: Integer;
begin
  Line := Trim(Line);
  if Line = '' then Exit;
  N := GetArrayLength(Lines);
  SetArrayLength(Lines, N + 1);
  Lines[N] := Line;
end;

procedure AddLines(var Lines: TArrayOfString; Text: String);
var I: Integer; Line: String;
begin
  Line := '';
  for I := 1 to Length(Text) do begin
    if Text[I] = #10 then begin
      AddLine(Lines, Line);
      Line := '';
    end else
      Line := Line + Text[I];
  end;
  AddLine(Lines, Line);
end;

function AddText(Form: TSetupForm; Top, Indent: Integer; Text: String; Bold: Boolean): TNewStaticText;
begin
  Result := TNewStaticText.Create(Form);
  Result.Parent := Form;
  Result.Left := ScaleX(16) + Indent;
  Result.Top := Top;
  Result.Width := Form.ClientWidth - ScaleX(32) - Indent;
  Result.WordWrap := True;
  Result.Caption := Text;
  if Bold then Result.Font.Style := [fsBold];
  Result.AutoSize := True;
end;

function AddChoice(Form: TSetupForm; Top: Integer; Text: String): TNewRadioButton;
begin
  Result := TNewRadioButton.Create(Form);
  Result.Parent := Form;
  Result.Left := ScaleX(16);
  Result.Top := Top;
  Result.Width := Form.ClientWidth - ScaleX(32);
  Result.Height := ScaleY(20);
  Result.Caption := Text;
end;

{ A web protection switch is on. Only true/false values hold "true". }
function WebProtectionOn: Boolean;
var
  Config: AnsiString;
begin
  Result := LoadStringFromFile(ExpandConstant('{commonappdata}\Secblitz\Filter\config.json'), Config)
    and (Pos('true', String(Config)) > 0);
end;

{ The one question. False means Cancel: nothing has been touched yet. }
function AskRemoveChoice(var PutBack: Boolean): Boolean;
var
  Form: TSetupForm;
  Question, KeepDetail, PutDetail, Note, WebNote: TNewStaticText;
  KeepChoice, PutChoice: TNewRadioButton;
  RemoveButton, CancelButton: TNewButton;
  Top: Integer;
begin
  { Inno 6.3+: client size, then whether to keep it when the dialog font scales. }
  Form := CreateCustomForm(ScaleX(470), ScaleY(300), False, False);
  try
    Form.Caption := CustomMessage('RemoveTitle');
    Top := ScaleY(16);
    Question := AddText(Form, Top, 0, CustomMessage('RemoveQuestion'), True);
    Top := Question.Top + Question.Height + ScaleY(14);
    KeepChoice := AddChoice(Form, Top, CustomMessage('KeepChoice'));
    Top := KeepChoice.Top + KeepChoice.Height + ScaleY(2);
    KeepDetail := AddText(Form, Top, ScaleX(20), CustomMessage('KeepDetail'), False);
    Top := KeepDetail.Top + KeepDetail.Height + ScaleY(12);
    PutChoice := AddChoice(Form, Top, CustomMessage('PutBackChoice'));
    Top := PutChoice.Top + PutChoice.Height + ScaleY(2);
    PutDetail := AddText(Form, Top, ScaleX(20), CustomMessage('PutBackDetail'), False);
    Top := PutDetail.Top + PutDetail.Height + ScaleY(16);
    Note := AddText(Form, Top, 0, CustomMessage('RemoveNote'), False);
    Top := Note.Top + Note.Height + ScaleY(18);
    if WebProtectionOn then begin
      WebNote := AddText(Form, Top - ScaleY(12), 0, CustomMessage('WebStops'), False);
      Top := WebNote.Top + WebNote.Height + ScaleY(18);
    end;

    CancelButton := TNewButton.Create(Form);
    CancelButton.Parent := Form;
    CancelButton.Width := ScaleX(90);
    CancelButton.Height := ScaleY(23);
    CancelButton.Left := Form.ClientWidth - ScaleX(16) - CancelButton.Width;
    CancelButton.Top := Top;
    CancelButton.Caption := SetupMessage(msgButtonCancel);
    CancelButton.ModalResult := mrCancel;
    CancelButton.Cancel := True;

    RemoveButton := TNewButton.Create(Form);
    RemoveButton.Parent := Form;
    RemoveButton.Width := ScaleX(130);
    RemoveButton.Height := ScaleY(23);
    RemoveButton.Left := CancelButton.Left - ScaleX(8) - RemoveButton.Width;
    RemoveButton.Top := Top;
    RemoveButton.Caption := CustomMessage('RemoveTitle');
    RemoveButton.ModalResult := mrOk;
    RemoveButton.Default := True;

    Form.ClientHeight := Top + RemoveButton.Height + ScaleY(16);
    KeepChoice.Checked := True;
    Form.ActiveControl := KeepChoice;
    Result := Form.ShowModal = mrOk;
    PutBack := Result and PutChoice.Checked;
  finally
    Form.Free;
  end;
end;

{ Personal settings first, as the person (the uninstaller itself is elevated),
  then the machine part elevated. The machine part writes one plain line per
  thing it left to a file in the uninstaller's private temp folder; the lines
  are UTF-8. Failures never stop the removal: the person already chose it. }
procedure PutEverythingBack;
var
  Lines: TArrayOfString;
  Raw: AnsiString;
  Output, Report: String;
  Code, I, Shown: Integer;
  PersonalLeft: Boolean;
begin
  UninstallProgressForm.StatusLabel.Caption := CustomMessage('PuttingBack');
  SetArrayLength(Lines, 0);

  PersonalLeft := True;
  if ExecAsOriginalUser(SecblitzExe, 'uninstall-revert --user', ExpandConstant('{app}'),
      SW_HIDE, ewWaitUntilTerminated, Code) then
    PersonalLeft := Code <> 0;
  Log('Secblitz put back (personal): left=' + IntToStr(Code));
  if PersonalLeft then AddLine(Lines, CustomMessage('PersonalLeft'));

  Output := ExpandConstant('{tmp}\secblitz-put-back.txt');
  DeleteFile(Output);
  if Exec(ExpandConstant('{sys}\cmd.exe'),
      '/D /S /C ""' + SecblitzExe + '" uninstall-revert > "' + Output + '" 2>NUL"',
      ExpandConstant('{sys}'), SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0) then begin
    if LoadStringFromFile(Output, Raw) then AddLines(Lines, UTF8Decode(Raw));
  end else begin
    Log('Secblitz put back (machine) failed, code ' + IntToStr(Code));
    AddLine(Lines, CustomMessage('SettingsLeft'));
  end;
  DeleteFile(Output);

  if GetArrayLength(Lines) = 0 then Exit;
  Report := CustomMessage('LeftIntro');
  Shown := GetArrayLength(Lines);
  if Shown > 12 then Shown := 12;
  for I := 0 to Shown - 1 do
    Report := Report + #13#10 + '- ' + Lines[I];
  if not UninstallSilent then
    SuppressibleMsgBox(Report, mbInformation, MB_OK, IDOK);
end;

{ Both choices: the current person's own Secblitz folder goes too. }
procedure CleanUserData;
var Code: Integer;
begin
  if not ExecAsOriginalUser(SecblitzExe, 'uninstall-cleanup --user', ExpandConstant('{app}'),
      SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0) then
    Log('Secblitz could not remove the per-user data folder, code ' + IntToStr(Code));
end;

function InitializeUninstall(): Boolean;
begin
  Result := True;
  UninstallPutBack := False;
  { Inno may call this once before its elevation relaunch. }
  if not IsAdmin then Exit;
  if HasSwitch('/SECBLITZDONE') then
    Log('Secblitz uninstall: the app already answered the question; keeping the PC as it is.')
  else if UninstallSilent then
    Log('Secblitz uninstall: a silent uninstall keeps the changes.')
  else if not AskRemoveChoice(UninstallPutBack) then begin
    Log('Secblitz uninstall: cancelled before anything was touched.');
    Result := False;
    Exit;
  end;
  CloseTray;
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

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then begin
    { The program is still in place here. Whatever fails below is logged and the
      removal carries on. }
    try
      if UninstallPutBack then PutEverythingBack;
      if not Maintain('RemoveFilter') then
        Log('Secblitz could not fully turn off web protection.');
      CleanUserData;
    except
      Log('Secblitz uninstall cleanup exception: ' + GetExceptionMessage);
    end;
  end else if CurUninstallStep = usPostUninstall then begin
    try
      if not Maintain('Purge') then
        Log('Secblitz could not remove all of its data.');
    except
      Log('Secblitz uninstall data cleanup exception: ' + GetExceptionMessage);
    end;
  end;
end;
