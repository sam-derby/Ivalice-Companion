# Ivalice Companion

An offline save reader and editor for FINAL FANTASY TACTICS - The Ivalice
Chronicles (Steam Enhanced).

## Download

[Windows installer](https://github.com/sam-derby/Ivalice-Companion/releases/download/v0.1.5/Ivalice.Companion_0.1.5_x64-setup.exe)
or
[portable ZIP](https://github.com/sam-derby/Ivalice-Companion/releases/download/v0.1.5/Ivalice.Companion_0.1.5_x64-portable.zip),
version 0.1.5. Checksums are in the
[release notes](https://github.com/sam-derby/Ivalice-Companion/releases/tag/v0.1.5).

For the ZIP, extract the whole folder and run `ivalice-companion.exe`. Keep
`resources/` and `licenses/` beside it. Windows 10/11 and WebView2 are required;
the ZIP doesn't install WebView2. Settings stay in LocalAppData.

Linux / Steam Deck test builds come from the
[package workflow](https://github.com/sam-derby/Ivalice-Companion/actions/workflows/release.yml):
a `.deb` installer for Debian/Ubuntu and an AppImage for x86_64 Linux. See
[desktop builds](.github/DEPLOYMENT.md) for artifacts and installation. Linux
GUI and Proton testing is still needed.

## Features

- **Units:** party roster with sprites, jobs, abilities, equipment and displayed
  stats. Edit levels, experience, base stats, job progress, learned and equipped
  abilities, and equipment; add recruits and creatures. Units away on an errand
  are marked in the roster.
- **Game:** saved chapter, objective and world-map area; edit gil, the in-game
  date, achievements and the story step. Changing the story step rebuilds story
  members, guests, Ramza's form, the world map and side-quest progress.
- **Utilities:** copy, move, swap, delete or import whole save slots, or export
  one slot as a separate save file.

Edits are staged until Save. Each write checks that the loaded file is unchanged
and makes a recoverable backup first. Try changes on a copy of your save.

## Build from source

Use Node 22.22.2 (or a version allowed by `package.json`) and the Rust toolchain
in [rust-toolchain.toml](rust-toolchain.toml).

On Windows, you'll also need Visual Studio 2022 Build Tools with **Desktop
development with C++**, the Windows SDK and WebView2. On Linux, install
[Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/#linux). CI
builds on Ubuntu 22.04.

Run `npm ci`, then `npm run build`. Packages go under `target/release/bundle/`.
Reader artwork is recovered from the pinned v0.1.5 portable and hash-checked
during the build; no private save or game installation is needed.

`npm run validate` runs formatting, lint, TypeScript, frontend tests, Rust
checks, Clippy, tests and the package build. Tests that need private saves are
optional.

Pull requests and branch pushes run these checks on Windows and Ubuntu and
upload test packages. A new matching version tag prepares a draft release with
both installers, both portables and checksums. See
[desktop builds](.github/DEPLOYMENT.md) for the release steps.

## Licence

The application code is [GPL-3.0-only](LICENSE). Third-party software, the font
and game-derived assets have separate terms; see [NOTICE.md](NOTICE.md). This
project is not affiliated with Square Enix.
