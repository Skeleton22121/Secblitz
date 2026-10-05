"""Check installer locales and launch/shortcut source contracts without Inno Setup."""
import pathlib
import re


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
    assert {"Monitor", "Failed", "DesktopIcon", "LaunchSecblitz", "AutoUpdates"} == keys
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
    assert entries('Run') == [{'Filename': r'{app}\secblitz.exe', 'Parameters': 'guide',
                               'WorkingDir': '{app}', 'Description': '{cm:LaunchSecblitz}',
                               'Flags': 'nowait postinstall skipifsilent runasoriginaluser',
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
    assert not entries('UninstallDelete'), 'Uninstall must rely on its owned-file log'
    latch = code.index('PostInstallFailed := True;')
    clear = code.index('PostInstallFailed := False;')
    for action in ('Secure', 'InstallMonitor', 'ResumeMonitor', 'PreserveUpdates', 'EnableUpdates', 'DisableUpdates'):
        assert latch < code.index(f"Maintain('{action}')", latch) < clear, action
    assert re.search(r'Result := \(PageID = wpFinished\) and PostInstallFailed;', code)
    return len(languages), len(keys)


def regression_checks(source):
    mutations = []
    for locale in ('en', 'es', 'fr', 'de', 'pt', 'it'):
        for key in ('Monitor', 'Failed', 'DesktopIcon', 'LaunchSecblitz', 'AutoUpdates'):
            mutations.append(re.sub(rf'^{locale}\.{key}=.*$', '', source, flags=re.MULTILINE))
    for before, after in (
        ('Flags: nowait postinstall', 'Flags: unchecked nowait postinstall'),
        (' skipifsilent', ''), (' runasoriginaluser', ''),
        ('Parameters: "guide"', 'Parameters: "apply"'),
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
    ):
        assert before in source, before
        mutations.append(source.replace(before, after))
    for section in ('InstallDelete', 'UninstallDelete'):
        mutations.append(source + f'\n[{section}]\nType: filesandordirs; Name: "{{app}}"\n')
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
