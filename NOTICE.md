# Third-party notices

Ivalice Companion adapts save container and record behavior from Nelveska's
TICSaveEditor, revision `07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b` (GPL-3.0).
The adapted implementation has been rewritten in Rust and adds bounded
validation, snapshot checks, scoped writes and backup handling. See
`crates/save-format/NOTICE.md` for the mapped files.

TICSaveEditor's UMIF implementation credits Nenkai's FF16Tools, revision
`dd91fb451d3b2e97bc637b5d43388e7118c145bf` (MIT), for the container algorithm
and 32 KiB preset dictionary. This Rust implementation uses the same XOR key and
block processing; the installer includes the dictionary. No FF16Tools source
file is copied verbatim into this repository. Copyright (c) 2025 Nenkai. See
`LICENSES/Nenkai-MIT.txt` for the accompanying MIT terms.

Some equipment and job facts come from Nenkai's `fftivc.utility.modloader`
tables at revision `d3123d2` (MIT). These are data inputs, not copied modloader
code. The app's runtime catalogue is included in
`src-tauri/installer-resources/`.

The app icon reverses the colours of the icon in the owner's installed FINAL
FANTASY TACTICS - The Ivalice Chronicles executable. This unofficial app is not
affiliated with Square Enix.

Alegreya is licensed under SIL Open Font License 1.1; see
`public/fonts/Alegreya-OFL.txt`.
