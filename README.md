# Ivalice Companion

A local Windows save reader and editor for FINAL FANTASY TACTICS - The Ivalice
Chronicles (Steam Enhanced).

## Download

[Download the 0.1.3 Windows installer](https://github.com/sam-derby/Ivalice-Companion/releases/download/v0.1.3/Ivalice.Companion_0.1.3_x64-setup.exe).
SHA-256: `0E9FC99D2DBC2A114987E3F6E13937A041A4B4A393D78D64BD7E770875E220C7`.

## Build from source

Install Node 22.22.2, the Rust toolchain in `rust-toolchain.toml`, Visual Studio
2022 Build Tools with the Windows SDK, and WebView2. Run `npm ci` and
`npm run build`. The Windows installer is in `target/release/bundle/nsis/`.

## License

GPL-3.0-only. See `LICENSE` and `NOTICE.md` for attribution and bundled font
terms.
