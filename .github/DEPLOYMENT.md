# Desktop builds and releases

This directory is synced into the curated public repository. The application
version stays at 0.1.5 until the owner requests a version change. Existing
published releases are never changed by these workflows.

## Validation

`Validate` runs on pull requests, branch pushes, and manual dispatch. Windows
2022 and Ubuntu 22.04 run the public `npm run validate`: formatting, lint,
TypeScript, frontend and packaging controls, Rust check, Clippy with warnings
denied, tests, and the platform's installer build. Node and Rust are pinned by
the repository; actions use exact commit revisions. WIP features stay hidden.
Successful validation also uploads both platforms' installer and portable as
test-package workflow artifacts, without creating a release.

Linux is x86_64 GNU/Linux. Ubuntu 22.04 is the packaging baseline to limit the
minimum glibc requirement. The Linux filesystem implementation accepts local
ext-family, Btrfs, XFS, tmpfs and overlay filesystems; squashfs permits AppImage
resource reads. Network filesystems, symlinks in selected paths and hard-linked
write targets are rejected. If Steam's shortcut directory is a symlink, select
the real save directory instead. Saves remain where the user selects them; Linux
settings use the current user's XDG data directory.

## Build four downloads without publishing

Run `Build release packages` manually on the desired branch. Leave `tag` empty
and `create_draft` disabled. Download the `packages-windows` and
`packages-linux` workflow artifacts. They expire after 30 days.

| Platform     | Installer                               | Portable                                   |
| ------------ | --------------------------------------- | ------------------------------------------ |
| Windows x64  | `Ivalice.Companion_X.Y.Z_x64-setup.exe` | `Ivalice.Companion_X.Y.Z_x64-portable.zip` |
| Linux x86_64 | `Ivalice.Companion_X.Y.Z_amd64.deb`     | `Ivalice.Companion_X.Y.Z_x86_64.AppImage`  |

Each package includes the eight runtime resources and five licence notices.
Linux builds extend the existing dependency notice with the selected locked Rust
runtime tree and the linked distribution libraries' copyright files before
bundling. Missing licence texts fail the build; upstream workspace licences are
retrieved only at the exact revision recorded in the package archive. Windows
portable files come from the extracted installer and are verified against the
build; no WebView2 or installation behavior changes. Linux packages are
extracted and their resources/notices checked before upload. Package scripts
refuse stale bundles, collisions and missing files, and create SHA-256
manifests.

Install the Linux `.deb` using the distribution's package manager, for example
`sudo apt install ./Ivalice.Companion_X.Y.Z_amd64.deb`. Steam Deck users should
use the AppImage in Desktop Mode. Make it executable with `chmod +x`, then open
it. If FUSE is unavailable, try the AppImage's `--appimage-extract-and-run`
option. The AppImage needs no installation; settings still live in the XDG data
directory.

## Prepare a release

1. Obtain the owner's authorization for a release and version change. Update
   npm, Tauri and Cargo application versions and lockfiles together.
2. Review and commit the curated source. Push a new matching `vX.Y.Z` tag.
   Alternatively, dispatch the package workflow with that existing tag and
   enable `create_draft`. Never move an old tag to redeploy.
3. Both platforms must validate and produce their installer and portable. Only
   then does the write-enabled job create a **draft** GitHub release with all
   four packages and `SHA256SUMS.txt`. It verifies uploaded asset digests.
   Reruns may update drafts; published releases are refused.
4. Review the draft, checksums and actual package smoke tests. Verify Windows
   install/upgrade and portable behavior. On Linux/Steam Deck, use disposable
   save copies to test read, edit, backup, save, game loading, restore and game
   loading again. Check the app header matches the package version.
5. Publish the draft on GitHub when ready. Linux packages initially remain Linux
   / Steam Deck test builds until the owner accepts runtime results.

No workflow automatically increments versions, changes game installations,
uploads personal saves, publishes to Nexus, or pushes source changes.

## Local commands in the public checkout

Run `npm ci`, then `npm run validate`. `npm run build` selects NSIS on Windows
and `.deb` plus AppImage on Linux. To verify and assemble downloads, run
`node .github/scripts/release-package.mjs windows` or the equivalent `linux`.
The output is `target/release-assets/<platform>/`. On Windows, 7-Zip must be on
PATH, or set `SEVEN_ZIP` to its executable. Packaging outputs must be absent
before rerunning. These commands need only the curated checkout's committed
resources and artwork, never private development inputs.
