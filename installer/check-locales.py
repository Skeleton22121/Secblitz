"""Check installer locales and launch/shortcut source contracts without Inno Setup."""
import pathlib
import re


INSTALL_KEYS = ("Monitor", "Failed", "DesktopIcon", "LaunchSecblitz", "AutoUpdates", "TrayIcon")
# The keep-or-put-back question and its results, shown by the uninstaller.
REMOVE_KEYS = ("RemoveTitle", "RemoveQuestion", "KeepChoice", "KeepDetail", "PutBackChoice",
               "PutBackDetail", "RemoveNote", "PuttingBack", "LeftIntro", "PersonalLeft", "SettingsLeft")


def check(source):
    sections = {}
    current = None
    for line in source.splitlines():
        heading = re.fullmatch(r"\[([^]]+)\]", line.strip())
        if heading:
            current = heading[1]
        elif current:
            sections.setdefault(current, []).append(line)

    languages = {}
    for line in sections["Languages"]:
        entry = re.fullmatch(r'Name:\s*"([a-z]+)";\s*MessagesFile:\s*"([^"]+)"', line.strip())
        if entry:
            assert entry[1] not in languages, f"Duplicate language: {entry[1]}"
            languages[entry[1]] = entry[2]
    assert set(languages) == {"en", "es", "fr", "de", "pt", "it"}, languages
    assert languages["it"] == r"compiler:Languages\Italian.isl"

    messages = {}
    for line in sections["CustomMessages"]:
        entry = re.fullmatch(r"([a-z]+)\.([\w]+)=(.*)", line.strip())
        if entry:
            locale, key, value = entry.groups()
            assert locale in languages, f"Unknown custom-message language: {locale}"
            assert (locale, key) not in messages, f"Duplicate message: {locale}.{key}"
            assert value.strip(), f"Empty message: {locale}.{key}"
            messages[locale, key] = value
    keys = {key for _, key in messages} | set(re.findall(r"\{cm:([\w]+)\}", source))
    assert set(INSTALL_KEYS) | set(REMOVE_KEYS) == keys
    for locale in languages:
        for key in keys:
            assert (locale, key) in messages, f"Missing message: {locale}.{key}"
            if locale != "en":
                assert messages[locale, key] != messages["en", key], f"English placeholder: {locale}.{key}"
    def entries(section):
        return [{key: quoted if quoted else bare.strip()
                 for key, quoted, bare in re.findall(r'(\w+):\s*(?:"([^"]*)"|([^;]+))', line)}
                for line in sections.get(section, [])
                if line.strip() and not line.lstrip().startswith(';')]

    tasks = {entry['Name']: entry for entry in entries('Tasks')}
    assert [entry for entry in entries('Tasks') if entry['Name'] == 'desktopicon'] == [
        {'Name': 'desktopicon', 'Description': '{cm:DesktopIcon}', 'Check': 'DesktopDefault'},
        {'Name': 'desktopicon', 'Description': '{cm:DesktopIcon}', 'Flags': 'unchecked', 'Check': 'DesktopOptedOut'},
    ]
    assert tasks['monitor']['Flags'] == 'unchecked'
    updater = [entry for entry in entries('Tasks') if entry['Name'] == 'autoupdates']
    assert updater == [
        {'Name': 'autoupdates', 'Description': '{cm:AutoUpdates}', 'Check': 'AutoUpdatesDefault'},
        {'Name': 'autoupdates', 'Description': '{cm:AutoUpdates}', 'Flags': 'unchecked', 'Check': 'AutoUpdatesOptedOut'},
    ]
    desktop = [entry for entry in entries('Icons') if entry.get('Name', '').startswith('{commondesktop}')]
    assert desktop == [{'Name': r'{commondesktop}\Secblitz', 'Filename': r'{app}\secblitz.exe',
                        'WorkingDir': '{app}', 'Tasks': 'desktopicon'}]
    assert tasks['trayicon'] == {'Name': 'trayicon', 'Description': '{cm:TrayIcon}'}
    registry = [line for line in sections.get('Registry', []) if line.strip() and not line.lstrip().startswith(';')]
    assert registry == [
        'Root: HKLM; Subkey: "Software\\Microsoft\\Windows\\CurrentVersion\\Run"; ValueType: string; '
        'ValueName: "SecblitzTray"; ValueData: """{app}\\secblitz.exe"" tray"; '
        'Flags: uninsdeletevalue; Tasks: trayicon'], registry
    assert entries('Run') == [
        {'Filename': r'{app}\secblitz.exe',
         'WorkingDir': '{app}', 'Description': '{cm:LaunchSecblitz}',
         'Flags': 'nowait postinstall skipifsilent runasoriginaluser',
         'Check': 'CanLaunchSecblitz'},
        {'Filename': r'{app}\secblitz.exe', 'Parameters': 'tray', 'WorkingDir': '{app}',
         'Flags': 'nowait skipifsilent runasoriginaluser', 'Tasks': 'trayicon',
         'Check': 'CanLaunchSecblitz'}]
    code = '\n'.join(sections['Code'])
    initialize = re.search(r'procedure InitializeWizard;.*?\nend;', code, re.DOTALL)
    assert initialize, 'Preferences must be cached before uninstall data is rewritten'
    assert "PreviousDesktopSelected := GetPreviousData('DesktopSelected', '1') = '1';" in initialize[0]
    assert "PreviousHasUpdatePreference := RegQueryDWordValue(HKLM64, 'Software\\Secblitz', 'AutoUpdatesEnabled', Enabled);" in initialize[0]
    assert 'PreviousAutoUpdatesEnabled := True;' in initialize[0]
    assert 'if PreviousHasUpdatePreference then PreviousAutoUpdatesEnabled := Enabled <> 0;' in initialize[0]
    assert code.count('GetPreviousData(') == 1, 'Do not reread replaced uninstall data'
    for function, cached in (('DesktopDefault', 'PreviousDesktopSelected'),
                             ('AutoUpdatesDefault', 'PreviousAutoUpdatesEnabled'),
                             ('HasUpdatePreference', 'PreviousHasUpdatePreference')):
        assert re.search(rf'function {function}: Boolean;\s*begin\s*Result := {cached};\s*end;', code)
    register = re.search(r'procedure RegisterPreviousData\(PreviousDataKey: Integer\);.*?\nend;', code, re.DOTALL)
    assert register
    assert re.search(r"if ExpandConstant\('\{param:SECBLITZUPDATE\|0\}'\) = '1' then\s*"
                     r"DesktopSelected := PreviousDesktopSelected\s*else\s*"
                     r"DesktopSelected := WizardIsTaskSelected\('desktopicon'\);", register[0])
    assert re.search(r"if DesktopSelected then\s*"
                     r"SetPreviousData\(PreviousDataKey, 'DesktopSelected', '1'\)\s*else\s*"
                     r"SetPreviousData\(PreviousDataKey, 'DesktopSelected', '0'\);", register[0])
    assert re.search(r'function CanLaunchSecblitz:\s*Boolean;\s*begin\s*'
                     r'Result := not PostInstallFailed;\s*end;', code)
    assert 'if PostInstallFailed then Result := 20;' in code
    assert 'UsePreviousTasks=no' in sections['Setup']
    assert r'SetupMutex=Global\SecblitzSetup' in sections['Setup']
    assert "RegQueryDWordValue(HKLM64, 'Software\\Secblitz', 'AutoUpdatesEnabled', Enabled)" in code
    assert "ExpandConstant('{param:SECBLITZUPDATE|0}') = '1'" in code
    assert not entries('InstallDelete'), 'Installation must not delete unrelated files'
    assert entries('UninstallDelete') == [{'Type': 'filesandordirs', 'Name': r'{app}\Status'}], \
        'Uninstall removes only the fixed Status directory beyond its owned-file log'
    latch = code.index('PostInstallFailed := True;')
    clear = code.index('PostInstallFailed := False;')
    for action in ('Secure', 'InstallFilter', 'InstallMonitor', 'ResumeMonitor', 'ResumeFilter',
                   'PreserveUpdates', 'EnableUpdates', 'DisableUpdates'):
        assert latch < code.index(f"Maintain('{action}')", latch) < clear, action
    assert re.search(r'Result := \(PageID = wpFinished\) and PostInstallFailed;', code)
    uninstall_contract(code)
    return len(languages), len(keys)


def uninstall_contract(code):
    """Removing Secblitz: the question, the silent default and the order of the steps."""
    initialize = re.search(r'function InitializeUninstall\(\): Boolean;.*?\nend;', code, re.DOTALL)
    assert initialize, 'InitializeUninstall is missing'
    body = initialize[0]
    # /SECBLITZDONE and any silent uninstall keep the changes and never ask.
    assert "HasSwitch('/SECBLITZDONE')" in body
    assert body.index("HasSwitch('/SECBLITZDONE')") < body.index('UninstallSilent') < body.index('AskRemoveChoice(UninstallPutBack)')
    assert re.search(r'else if not AskRemoveChoice\(UninstallPutBack\) then begin\s*Log\([^;]*\);\s*Result := False;\s*Exit;\s*end;', body), \
        'Cancel must stop the uninstall before anything is touched'
    # Cancel comes before the first thing that changes anything.
    assert body.index('AskRemoveChoice(UninstallPutBack)') < body.index("Maintain('RemoveMonitor')")
    put_back = re.search(r'procedure PutEverythingBack;.*?\nend;', code, re.DOTALL)
    assert put_back, 'PutEverythingBack is missing'
    # Personal part as the person, machine part elevated; neither the other way round.
    assert re.search(r"ExecAsOriginalUser\(SecblitzExe, 'uninstall-revert --user'", put_back[0])
    assert "uninstall-revert > " in put_back[0] and "uninstall-revert --user >" not in put_back[0]
    assert put_back[0].index("'uninstall-revert --user'") < put_back[0].index('uninstall-revert > ')
    steps = re.search(r'procedure CurUninstallStepChanged\(.*?\nend;', code, re.DOTALL)
    assert steps, 'CurUninstallStepChanged is missing'
    text = steps[0]
    assert text.index('usUninstall') < text.index('if UninstallPutBack then PutEverythingBack;') \
        < text.index("Maintain('RemoveFilter')") < text.index('CleanUserData') < text.index('usPostUninstall') \
        < text.index("Maintain('Purge')")
    assert "uninstall-cleanup --user" in code
    assert "if IsUninstaller and ((Action = 'RemoveMonitor') or (Action = 'RemoveFilter') or (Action = 'Purge')) then" in code


def regression_checks(source):
    mutations = []
    for locale in ('en', 'es', 'fr', 'de', 'pt', 'it'):
        for key in INSTALL_KEYS + REMOVE_KEYS:
            mutations.append(re.sub(rf'^{locale}\.{key}=.*$', '', source, flags=re.MULTILINE))
    for before, after in (
        ('Flags: nowait postinstall', 'Flags: unchecked nowait postinstall'),
        (' skipifsilent', ''), (' runasoriginaluser', ''),
        ('Parameters: "tray"', 'Parameters: "apply"'),
        ('Flags: uninsdeletevalue', ''),
        ('Tasks: trayicon', ''),
        ('Check: CanLaunchSecblitz', ''),
        ('Result := not PostInstallFailed;', 'Result := True;'),
        ('Description: "{cm:DesktopIcon}"', 'Description: "{cm:DesktopIcon}"; Flags: unchecked'),
        ('Tasks: desktopicon', ''),
        ('Flags: unchecked', 'Flags: checkedonce'),
        ('UsePreviousTasks=no', 'UsePreviousTasks=yes'),
        (r'SetupMutex=Global\SecblitzSetup', ''),
        ('Check: AutoUpdatesDefault', 'Check: AutoUpdatesOptedOut'),
        ('procedure InitializeWizard;', 'procedure TooLate;'),
        ('Result := PreviousDesktopSelected;', 'Result := True;'),
        ('Result := PreviousAutoUpdatesEnabled;', 'Result := True;'),
        ('Result := PreviousHasUpdatePreference;', 'Result := True;'),
        ('DesktopSelected := PreviousDesktopSelected', "DesktopSelected := GetPreviousData('DesktopSelected', '1') = '1'"),
        ("DesktopSelected := WizardIsTaskSelected('desktopicon');", 'DesktopSelected := PreviousDesktopSelected;'),
        ("'DesktopSelected', '0');", "'DesktopSelected', '1');"),
        ("{param:SECBLITZUPDATE|0}", "{param:SECBLITZUPDATE|1}"),
        ('PostInstallFailed := True;', 'PostInstallFailed := False;'),
        ('PostInstallFailed := False;', 'PostInstallFailed := True;'),
        ('(PageID = wpFinished) and PostInstallFailed', 'False'),
        ("HasSwitch('/SECBLITZDONE')", "HasSwitch('/SOMETHINGELSE')"),
        ("else if UninstallSilent then", "else if False then"),
        ('Result := False;\n    Exit;\n  end;\n  CloseTray;', 'Result := True;\n    Exit;\n  end;\n  CloseTray;'),
        ("ExecAsOriginalUser(SecblitzExe, 'uninstall-revert --user'", "Exec(SecblitzExe, 'uninstall-revert --user'"),
        ("if UninstallPutBack then PutEverythingBack;", "PutEverythingBack;"),
        ("Maintain('RemoveFilter')", "Maintain('RemoveMonitor')"),
        ("Maintain('Purge')", "Maintain('Secure')"),
        ("Maintain('InstallFilter')", "Maintain('Secure')"),
        ("(Action = 'Purge')", "(Action = 'Nothing')"),
    ):
        assert before in source, before
        mutations.append(source.replace(before, after))
    for section in ('InstallDelete', 'UninstallDelete'):
        mutations.append(source + f'\n[{section}]\nType: filesandordirs; Name: "{{app}}"\n')
    mutations.append(source.replace('Name: "{app}\\Status"', 'Name: "{app}"'))
    for mutated in mutations:
        try:
            check(mutated)
        except (AssertionError, ValueError):
            continue
        raise AssertionError('Installer regression was not detected')
    return len(mutations)


if __name__ == "__main__":
    source = pathlib.Path(__file__).with_name("setup.iss").read_text(encoding="utf-8-sig")
    languages, messages = check(source)
    regressions = regression_checks(source)
    print(f"PASS: {languages} installer languages, {messages} complete custom messages per language; "
          f"bundled Italian.isl; guided launch/shortcut contracts; {regressions} rejected regressions")
