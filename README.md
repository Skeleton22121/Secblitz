<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/banner-dark.webp">
  <img src="docs/banner-light.webp" width="900" alt="Secblitz">
</picture>

Secblitz maximizes Windows security and privacy by fixing security holes and misconfigurations in one click. It also removes bloatware and blocks ads, trackers and malware sites on your whole PC. Every change can be undone.

[**Download for Windows**](https://secblitz.lol/downloads/secblitz-0.8.2-windows-x64-setup.exe) · [Website](https://secblitz.lol) · [Security](SECURITY.md) · [Contributing](CONTRIBUTING.md)

![Version 0.8.2](https://img.shields.io/badge/version-0.8.2-18181B) ![Windows 11, 64-bit](https://img.shields.io/badge/Windows%2011-64--bit-18181B) ![MIT license](https://img.shields.io/badge/license-MIT-18181B)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="website/assets/app-home-dark.webp">
  <img src="website/assets/app-home-light.webp" width="900" alt="The Secblitz home screen: how many protections are on, and what needs attention.">
</picture>

</div>

## How it works

1. **Scan.** Secblitz scans Windows for security holes and privacy problems. It takes about a minute and changes nothing.
2. **Review.** Each problem says what it protects against and what changes for you.
3. **Fix.** One click fixes everything you approved. Secblitz then confirms each fix worked.
4. **Undo.** Every change is recorded. History puts your old settings back.

## Features

- **One-click fixes** for more than 50 security and privacy settings: virus protection, firewall and network, sign-in, updates, startup, disk and privacy. Anything already secure is left alone.
- **Bloatware removal.** Removes unwanted apps that came with Windows. Secblitz keeps a copy of each, so you can restore it later, even offline.
- **Ad and malware blocker.** Blocks ads, trackers and malware sites in every browser and app.
- **Tools.** Microsoft Defender scans and updates, Windows repair, security updates and an optional Bitwarden install.
- **Background monitoring.** Tells you if your protection gets worse.
- **History** of every change and every removed app.
- **Six languages:** English, Spanish, French, German, Portuguese and Italian. Light and dark mode.

Secblitz is not an antivirus. It turns on and configures the protection built into Windows, and it leaves third-party antivirus and PCs managed by work or school alone.

## Install

Download the [installer](https://secblitz.lol/downloads/secblitz-0.8.2-windows-x64-setup.exe) (8.6 MB) and run it. Secblitz asks for administrator permission because it reads and changes system settings.

The installer is not code-signed yet, so Windows may show an unknown publisher warning. Choose **More info**, then **Run anyway**. To check the file, compare its SHA-256 with the one on the [download page](https://secblitz.lol/#download):

```powershell
Get-FileHash .\secblitz-0.8.2-windows-x64-setup.exe -Algorithm SHA256
```

Tested on Windows 11, 64-bit. Windows 10 is not tested.

## Privacy

No account, no ads, no telemetry. Secblitz goes online to update itself (if updates are on), for actions you start, and to download block lists if web protection is on. See the [privacy policy](https://secblitz.lol/privacy.html).

## Security and contributing

To report a security problem, see [SECURITY.md](SECURITY.md). To build Secblitz or send a change, see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE). Fonts, icons, libraries and block lists from others are credited in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
