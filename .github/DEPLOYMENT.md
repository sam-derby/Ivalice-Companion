# Desktop builds and releases

## Test builds

`Validate` runs on pull requests, branch pushes and manual dispatch. It checks
formatting, lint, TypeScript, frontend tests, Rust, Clippy and packaging on
Windows 2022 and Ubuntu 22.04. Both jobs upload test packages; they don't create
a release.

To build a particular branch, run `Build release packages` with an empty `tag`
and `create_draft` disabled. Download the `packages-windows` and
`packages-linux` artifacts within 30 days.

| Platform     | Installer                               | Portable                                   |
| ------------ | --------------------------------------- | ------------------------------------------ |
| Windows x64  | `Ivalice.Companion_X.Y.Z_x64-setup.exe` | `Ivalice.Companion_X.Y.Z_x64-portable.zip` |
| Linux x86_64 | `Ivalice.Companion_X.Y.Z_amd64.deb`     | `Ivalice.Companion_X.Y.Z_x86_64.AppImage`  |

Each package includes the runtime resources and licence notices, with a
`SHA256SUMS.txt` alongside the downloads. Packaging checks the extracted files
before uploading them. The Windows ZIP comes from the installer, so both ship
the same executable and resources. WebView2 and installation behavior are
unchanged.

## Linux notes

Ubuntu 22.04 is the build baseline. Install the `.deb` with the package manager,
for example `sudo apt install ./Ivalice.Companion_X.Y.Z_amd64.deb`.

For Steam Deck, use the AppImage in Desktop Mode. Make it executable with
`chmod +x`, then open it. If FUSE is unavailable, try
`--appimage-extract-and-run`. Settings live in the user's XDG data directory.

Saves can be on local ext-family, Btrfs, XFS, tmpfs or overlay filesystems.
Network filesystems, symlinks in the selected path and hard-linked write targets
are rejected. If a Steam shortcut is a symlink, choose the real save directory.
AppImage resources can be read through FUSE; saves can't.

Before replacing a save, the Linux writer flushes an independent backup. Copying
matters here: the game may still have the old file open. A failed replacement
leaves the original in place or reports that recovery is needed. It won't
overwrite a file that appeared at the same name during the write.

Linux packages are test builds until real Linux/Steam Deck testing confirms read
→ edit → backup → save → game load, plus restore and another game load. Use
disposable save copies. CI covers filesystem behavior, including a real FUSE
mount, but doesn't test the GUI or Proton.

## Build from the public checkout

Run `npm ci`, then `npm run validate`. For just a build, `npm run build`
produces NSIS on Windows or a `.deb` and AppImage on Linux. Assemble the
downloads with `node .github/scripts/release-package.mjs windows` (or `linux`).
Output goes to `target/release-assets/<platform>/`; clear old packaging outputs
before rerunning. Windows needs 7-Zip on PATH, or `SEVEN_ZIP` set to its
executable.

Fresh builds recover 481 reader images and their manifest from the pinned v0.1.5
portable using Tauri Dumper at a fixed revision. The archive, recovered tree and
built artwork are checked against SHA-256 hashes. The tool and raw artwork stay
in ignored build directories.

Linux builds also collect notices for locked Rust dependencies and linked
distribution libraries. Missing licence texts fail the build; any fetched
workspace licence comes from the crate's recorded revision.

## Release

Keep the app at 0.1.5 until a version change is requested.

1. Update npm, Tauri and Cargo versions and lockfiles together for an approved
   release. Commit the curated source and push a new matching `vX.Y.Z` tag.
   Don't move old tags.
2. The workflow builds all four packages and creates a **draft** release with
   checksums. You can also run it manually with an existing matching tag and
   `create_draft` enabled. Reruns can update drafts; published releases are left
   alone.
3. Check the downloads, Windows installation/upgrade and portable behavior.
   Confirm the app header matches the package version and review Linux runtime
   results.
4. Publish the draft when ready. Nexus uploads are separate.

The workflows don't bump versions or publish drafts automatically.
