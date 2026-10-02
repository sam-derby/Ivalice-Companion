# Third-party notices

Ivalice Companion adapts save container and record behavior from Nelveska's
TICSaveEditor, revision `07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b` (GPL-3.0).
The adapted implementation has been rewritten in Rust and adds bounded
validation, snapshot checks, scoped writes and backup handling. See
`crates/save-format/NOTICE.md` for the mapped files.

The container algorithm and 32 KiB preset dictionary originate with Nenkai's
FF16Tools, revision `dd91fb451d3b2e97bc637b5d43388e7118c145bf` (MIT). Copyright
(c) 2025 Nenkai. See `LICENSES/Nenkai-MIT.txt` for the MIT terms accompanying
the adapted code and any distributed dictionary.

Some equipment and job facts come from Nenkai's `fftivc.utility.modloader` data
at revision `d3123d2` (MIT). The generated game catalogue is not part of this
source snapshot.

The app icon reverses the colours of the icon in the owner's installed FINAL
FANTASY TACTICS â€” The Ivalice Chronicles executable. This unofficial app is
not affiliated with Square Enix.

Alegreya is licensed under SIL Open Font License 1.1; see
`public/fonts/Alegreya-OFL.txt`.
