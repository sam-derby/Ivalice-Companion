# Ivalice Companion

An offline Windows save reader and editor for FINAL FANTASY TACTICS - The
Ivalice Chronicles (Steam Enhanced).

## Download

[Download the 0.1.3 Windows installer](https://github.com/sam-derby/Ivalice-Companion/releases/download/v0.1.3/Ivalice.Companion_0.1.3_x64-setup.exe).
SHA-256: `0E9FC99D2DBC2A114987E3F6E13937A041A4B4A393D78D64BD7E770875E220C7`.

Edits are staged until Save. Saving checks that the loaded file is unchanged and
creates a recoverable backup. Creature additions are experimental; their in-game
behavior is unverified.

## Build from source

On Windows, install Node 22.22.2 (or a version allowed by `package.json`), the
Rust toolchain in [rust-toolchain.toml](rust-toolchain.toml), Visual Studio 2022
Build Tools with **Desktop development with C++** and the Windows SDK, and
WebView2. Run `npm ci`, then `npm run build`. The installer is written to
`target/release/bundle/nsis/`; required runtime resources are included in
source.

Run `npm run validate` for TypeScript, Rust checks, Clippy, tests and the
installer build. The optional real-save parity test is ignored by default; no
private inputs are needed for validation.

## Licence

The application code is [GPL-3.0-only](LICENSE). Third-party software, the font
and game-derived assets have separate terms; see [NOTICE.md](NOTICE.md). This
project is not affiliated with Square Enix.
