# Ivalice Companion

A local Windows save reader and editor for FINAL FANTASY TACTICS â€” The Ivalice
Chronicles (Steam Enhanced).

This is an unofficial fan project. Save processing is local. Save writes create
a recoverable backup and target only the loaded file and selected slot. Work on
a copy of your save.

## Build

Install Node 22.22.2, the Rust toolchain in `rust-toolchain.toml`, Visual Studio
2022 Build Tools with the Windows SDK, and WebView2. Then run `npm ci` and
`npm run build`. The executable is in `target/release/`.

The app needs `CompressDict.bin` in its `resources/` directory to read saves.
The release build can stage a locally supplied file from
`.local/research-inputs/resources/ticsaveeditor-07ea857/CompressDict.bin`. It
can also stage optional reader catalogue, job requirements and ability flags
from `.local/` if present. `npm run build:installer` requires all four resources
in the paths used by `scripts/stage-installer-resources.mjs`.

The source includes the app icon but no saves, generated game catalogue or other
extracted game artwork. The public installer includes the catalogue and the
other required runtime resources. A source build without a reader catalogue
cannot show named data or use some editor features. Creature additions are
experimental and have not been game verified. The current human addition path
still depends on a suitable saved unit.

## License and attribution

The application source is offered under GPL-3.0-only. See `LICENSE` and
`NOTICE.md`. The bundled Alegreya font has its own OFL notice in
`public/fonts/`.
