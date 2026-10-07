# Building Ivalice Companion

Use Node 22.22.2 and the Rust version in `rust-toolchain.toml`. Install
[Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/) for your
platform, then run:

```sh
npm ci
npm run validate
```

This runs formatting, lint, TypeScript, frontend and Rust tests, Clippy and a
release build. Tests requiring private saves are optional.

For just a build, run `npm run build`. Packages appear under
`target/release/bundle/`: an NSIS installer on Windows, or a DEB and AppImage on
Linux. Windows packaging needs 7-Zip on PATH (`SEVEN_ZIP` can point to it).

To assemble the downloads and checksums:

```sh
node .github/scripts/release-package.mjs windows
# On Linux, use linux instead of windows.
```

Output goes to `target/release-assets/`. Remove the old package outputs before
rerunning packaging.

The build recovers reader artwork from the pinned v0.1.5 portable and checks
every image against its recorded hash. No game installation or personal save is
needed. The recovered images and extraction tool stay ignored.

See [deployment](DEPLOYMENT.md) for CI and releases.
