# CI and releases

Pull requests run formatting, lint, TypeScript, frontend tests, Rust checks,
Clippy and filesystem tests on Windows and Ubuntu. They don't build installers.

Pushes to main run those checks, then build and verify all four downloads. The
`packages-windows` and `packages-linux` artifacts are kept for 30 days. You can
run `Validate` manually on main to refresh them.

Rust caches are shared by build profile. Reader artwork is cached separately and
hash-checked before every build.

## Publish a version

1. Update npm, Tauri and Cargo versions and lockfiles together.
2. Push the reviewed source to main and wait for `Validate` to pass.
3. Push the matching `vX.Y.Z` tag. `Release` takes the verified packages from
   that exact main commit and creates a draft with all four downloads and
   checksums. It doesn't compile the app again.
4. Review the draft and publish it. Published releases aren't overwritten.

To collect packages without a release, run `Release` on main with an empty `tag`
and `create_draft` disabled. A tag is required to create a draft.

## Linux testing

Ubuntu 22.04 is the build baseline. CI covers filesystem behavior, including an
actual FUSE mount, but not the GUI or Proton. Linux remains a test build until
testers confirm editing, backup, saving, game loading and restoration.

Save access supports local ext-family, Btrfs, XFS, tmpfs and overlay
filesystems. Symlinks in selected paths, hard-linked write targets and network
filesystems are rejected. Choose the real save directory if a Steam shortcut is
a symlink. Read-only AppImage resources can use FUSE; saves can't.

For AppImages without FUSE, try `--appimage-extract-and-run`. Settings live in
the user's XDG data directory.

See [building](BUILDING.md) for local commands.
