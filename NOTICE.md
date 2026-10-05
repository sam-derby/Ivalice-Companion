# Third-party notices

Ivalice Companion's application code is [GPL-3.0-only](LICENSE). The Rust
adaptation adds bounds checks, scoped save edits, slot operations, snapshots and
backup handling; last modified 2026-10-05. These terms do not relicense
third-party assets.

## Save code and data sources

- [Nelveska's TICSaveEditor](https://github.com/Nelveska/TICSaveEditor/tree/07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b),
  revision `07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b`, GPL-3.0: adapted save
  container, record and edit behavior. See the
  [file mapping](crates/save-format/NOTICE.md).
- [Nenkai's FF16Tools](https://github.com/Nenkai/FF16Tools/tree/dd91fb451d3b2e97bc637b5d43388e7118c145bf),
  revision `dd91fb451d3b2e97bc637b5d43388e7118c145bf`, MIT: TICSaveEditor
  credits FF16Tools for its UMIF implementation and the bundled 32 KiB
  dictionary. This Rust code ports TICSaveEditor; no FF16Tools source file is
  copied verbatim. Copyright (c) 2025 Nenkai;
  [MIT terms](LICENSES/Nenkai-MIT.txt).
- [Nenkai's FFTIVC utility mod loader](https://github.com/Nenkai/fftivc.utility.modloader/tree/d3123d2eaf4beabbe0edb21e76f9e745758fa539),
  revision `d3123d2eaf4beabbe0edb21e76f9e745758fa539`, MIT: equipment, job and
  ability table inputs. The loader itself is not included. Copyright (c) 2025
  Nenkai; [MIT terms](LICENSES/Nenkai-MIT.txt).
- [Nenkai's FFT Nex layouts](https://github.com/Nenkai/fftivc-nex-layouts/tree/335747ed6453b2386f7fada2db66f956ebd56c35),
  revision `335747ed6453b2386f7fada2db66f956ebd56c35`, MIT: GeneralJob column
  definitions used to extract job requirements. Copyright (c) 2025 Nenkai;
  [MIT terms](LICENSES/Nenkai-MIT.txt).

## Font and dependencies

Alegreya is Copyright 2011 The Alegreya Project Authors, under
[SIL Open Font License 1.1](public/fonts/Alegreya-OFL.txt). The unmodified
variable font comes from
[Google Fonts revision 4047817](https://github.com/google/fonts/tree/40478177239cbf3bac07908ef0738afee0f72be7/ofl/alegreya).

[Dependency notices](LICENSES/dependencies.txt) retain the locked Windows and
JavaScript dependency licences, copyright notices and source links. The
installer places these notices, GPL, Nenkai's MIT notice and the font licence in
`licenses/`.

## Game-derived material

The icon is a colour-reversed derivative of the game's executable icon. The
reader catalogue includes English game names and descriptions from
TICSaveEditor, plus mechanical table data. The job-requirements resource
contains extracted game table facts; the ability map combines upstream tables
and verified bit mappings. The story-progress, story-roster, achievements and
errands resources contain English labels and table, battle-entry and
event-script facts extracted from the game's NXD tables and PAC files with
FF16Tools, the Nex layouts above and the event disassembler from
[skeewirt's TIC research](https://github.com/skeewirt/TIC/tree/c2dae2a4be46f69e5a27008449d2299eff34ed0d);
none of those tools is included.

FINAL FANTASY TACTICS and its game content belong to their respective rights
holders. This unofficial project is not affiliated with Square Enix.
