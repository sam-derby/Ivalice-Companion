# Ivalice Companion

An offline Windows save reader and editor for FINAL FANTASY TACTICS - The
Ivalice Chronicles (Steam Enhanced).

## Download

[Download the 0.1.5 Windows installer](https://github.com/sam-derby/Ivalice-Companion/releases/download/v0.1.5/Ivalice.Companion_0.1.5_x64-setup.exe)
or
[the portable ZIP](https://github.com/sam-derby/Ivalice-Companion/releases/download/v0.1.5/Ivalice.Companion_0.1.5_x64-portable.zip).
SHA-256 checksums are listed in the
[release notes](https://github.com/sam-derby/Ivalice-Companion/releases/tag/v0.1.5).

## Features

- **Units:** party roster with sprites, jobs, abilities, equipment and displayed
  stats. Edit levels, experience, base stats, job progress, learned and equipped
  abilities, and equipment; add recruits and creatures. Units away on an errand
  are marked in the roster.
- **Game:** saved chapter, objective and world-map area; edit gil, the in-game
  date, achievement unlocks and the story step, which rebuilds story members,
  guests, Ramza's form, the world map and side-quest progress for the chosen
  step.
- **Utilities:** copy, move, swap, delete or import whole save slots, or save
  one slot as a separate save file.

Version 0.1.5 adds the Game story view and editing and the Utilities tab, and
opens on the Game tab. Version 0.1.4 added level, experience and base-stat
editing with live previews.

For the portable ZIP, extract the whole folder and run `ivalice-companion.exe`.
Keep `resources/` and `licenses/` beside it. Windows 10/11 and WebView2 are
required; the ZIP does not install WebView2. Settings remain in LocalAppData.

Edits are staged until Save. Every write checks that the loaded file is
unchanged and creates a recoverable backup first. Try changes on a copy of your
save.

## Build from source

On Windows, install Node 22.22.2 (or a version allowed by `package.json`), the
Rust toolchain in [rust-toolchain.toml](rust-toolchain.toml), Visual Studio 2022
Build Tools with **Desktop development with C++** and the Windows SDK, and
WebView2. Run `npm ci`, then `npm run build`. The installer is written to
`target/release/bundle/nsis/`; required runtime resources are included in
source.

Run `npm run validate` for formatting, lint, TypeScript, frontend regressions,
Rust checks, Clippy, tests and the installer build. Optional real-save tests are
ignored or skipped without private inputs; no private inputs are needed for
validation.

## Licence

The application code is [GPL-3.0-only](LICENSE). Third-party software, the font
and game-derived assets have separate terms; see [NOTICE.md](NOTICE.md). This
project is not affiliated with Square Enix.
