<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="gui/screenshots/logo-dark.svg">
  <img src="gui/screenshots/logo-light.svg" alt="Zapret2 GUI" width="72">
</picture>

# Zapret2 GUI

**YouTube, Discord, Telegram, ChatGPT and other blocked sites on Windows — in one click.**<br>
Install it, tick what doesn't open, and the app takes care of the rest.

[![Download for Windows](https://img.shields.io/badge/Download%20for%20Windows-0a0a0a?style=for-the-badge&logo=windows11&logoColor=white)](https://github.com/raiyaxkrash/zapret2-rust/releases/latest)

[![Release](https://img.shields.io/github/v/release/raiyaxkrash/zapret2-rust?style=flat-square&color=0a0a0a)](https://github.com/raiyaxkrash/zapret2-rust/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/raiyaxkrash/zapret2-rust/total?style=flat-square&color=0a0a0a)](https://github.com/raiyaxkrash/zapret2-rust/tree/stats)
![Windows 10/11](https://img.shields.io/badge/Windows-10%20%C2%B7%2011%20%C2%B7%20ARM64-555555?style=flat-square)
![Free](https://img.shields.io/badge/free-open%20source-555555?style=flat-square)

[Features](#features) · [Installation](#installation) · [Русский](README.md)

<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="gui/screenshots/main-en-dark.png">
  <img src="gui/screenshots/main-en-light.png" alt="Zapret2 GUI main screen" width="860">
</picture>

</div>

<table>
  <tr>
    <td width="33%" valign="top">
      <h3>One button</h3>
      Click the sphere and blocked services open. Click again to turn it off. Also available from the tray mini-window.
    </td>
    <td width="33%" valign="top">
      <h3>Sets itself up</h3>
      The wizard asks what doesn't open and finds a working strategy for your ISP in about a minute. If the ISP changes its blocking, the app notices and re-tunes itself.
    </td>
    <td width="33%" valign="top">
      <h3>Telegram, no setup</h3>
      A built-in proxy for Telegram Desktop: one click on “Connect in Telegram” and the app works even where it can't connect directly.
    </td>
  </tr>
</table>

## Features

- **Services:** YouTube (incl. QUIC), Discord with voice, Telegram Desktop, plus one-click packs — Telegram, Instagram & Facebook, X, torrent trackers, Roblox, Signal, LinkedIn, games & launchers (Steam, Epic, Riot, Ubisoft, EA, GOG, FACEIT), Twitch.
- **Foreign hosting & CDN:** ~16,000 subnets of Cloudflare, Hetzner, DigitalOcean, OVH, AWS, Google Cloud, Oracle and others for sites and games that hang or break mid-download.
- **Built-in WARP** for services that block Russia themselves: ChatGPT, Claude, Gemini and other AI tools, Spotify, Intel/AMD/Adobe/JetBrains downloads. Only the selected sites go through the free Cloudflare tunnel; everything else stays direct. Nothing extra to install.
- **Automatic strategy search** against real sites, with a separate strategy for YouTube.
- **Watchdog:** checks services every 15 minutes and re-picks a strategy when the ISP changes its blocking.
- **Diagnostics:** tells DNS spoofing, IP blocking, DPI resets and service-side geo-restrictions apart, with advice for each.
- **Live status:** service availability and a 10-minute traffic chart on the main screen.
- **Auto-updates** for the app, strategies, site lists and zapret2 itself; starts with Windows; dark and light themes; English and Russian interface; ARM64 support.

## Installation

1. **Download** `Zapret2.GUI_X.Y.Z_x64-setup.exe` from the [releases page](https://github.com/raiyaxkrash/zapret2-rust/releases/latest).
2. **Run** the installer and allow administrator rights (required to load the network driver).
3. **Tick** what doesn't open in the wizard and press “Set up”. Done.

The interface follows your Windows language; you can switch it in Settings → Appearance → Language.

On first launch the app downloads [zapret2](https://github.com/bol-van/zapret2) from GitHub and verifies it with checksums.

> [!NOTE]
> **Antivirus warnings?** The app uses the WinDivert packet driver — the same as GoodbyeDPI and zapret builds — which some engines flag as “potentially unwanted”. [VirusTotal scan](https://www.virustotal.com/gui/file/a7f73e997a1ea0374df15f16ac98f4400e6b81aabc377842ca2f31cc8728e76f): Microsoft Defender, ESET, Dr.Web, BitDefender and Malwarebytes report it clean. Installers are built by GitHub Actions from this repository's open source code — see the [Actions](https://github.com/raiyaxkrash/zapret2-rust/actions) tab.

<p align="center"><img src="gui/screenshots/warp-en.png" alt="WARP page" width="760"></p>

## Credits

- [**zapret2**](https://github.com/bol-van/zapret2) and [zapret-win-bundle](https://github.com/bol-van/zapret-win-bundle) by **bol-van** — the circumvention engine and ARM64 driver (MIT).
- [**tg-ws-proxy**](https://github.com/Flowseal/tg-ws-proxy) by **Flowseal** — the protocol behind the Telegram proxy (MIT).
- [WinDivert](https://reqrypt.org/windivert.html) — packet interception driver. [Tauri](https://tauri.app) — application framework.
