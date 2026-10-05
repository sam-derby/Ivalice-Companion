# Save format attribution

Adapted in Rust from
[Nelveska's TICSaveEditor](https://github.com/Nelveska/TICSaveEditor/tree/07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b)
revision `07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b` (GPL-3.0):

| Rust implementation                                          | Upstream files in `TICSaveEditor.Core`                                            |
| ------------------------------------------------------------ | --------------------------------------------------------------------------------- |
| `src/umif.rs`, `src/png.rs`                                  | `Save/UmifContainer.cs`, `Save/PngEnvelope.cs`                                    |
| `src/manual.rs`, `src/manual/slot_metadata.rs`               | `Save/SaveSlot.cs`, `Save/SaveWork.cs`, `Save/SaveWorkLayout.cs`, `Sections/*.cs` |
| `src/manual/unit_record.rs`, `src/manual/upstream_reader.rs` | `Records/UnitSaveData.cs`, `Records/CombatSet.cs`, `Records/Layouts/*.cs`         |
| `src/manual/inventory.rs`                                    | `Records/PartyInventory.cs`, `Sections/BattleSection.cs`                          |
| `src/edit.rs`, `src/manual/unit_image.rs`                    | `Operations/SlotOperations.cs`, `Records/Layouts/*.cs`                            |

TICSaveEditor credits Nenkai's MIT-licensed FF16Tools
`FF16Tools.Files/Save/FaithSaveFile.cs` and `CompressDict.cs` at revision
`dd91fb451d3b2e97bc637b5d43388e7118c145bf` for the UMIF implementation and
dictionary. Matching the XOR key and byte processing does not by itself show
that the Rust code copied FF16Tools source. See the
[root notices](../../NOTICE.md) and [MIT terms](../../LICENSES/Nenkai-MIT.txt).

The port adds bounds and integrity checks and scoped manual-slot edits. File
snapshots, backups and restoration are implemented separately in infrastructure.

The v0.1.4 progression and base-stat edits in `src/edit/progression.rs` adapt
`Records/UnitSaveData.cs` and `Records/Layouts/UnitSaveDataLayout.cs` at the
TICSaveEditor revision above, with synthetic bounds and selected-field tests.
Companion stat projection and validation were last modified 2026-10-04.

The v0.1.5 story, roster and whole-slot edits in `src/edit/story.rs`,
`src/edit/roster.rs` and `src/slots.rs` use the slot, section and unit layouts
from `Save/SaveWorkLayout.cs`, `Sections/*.cs`,
`Records/Layouts/UnitSaveDataLayout.cs` and `CombatSetLayout.cs` at the same
revision. Event-work, game-flag and achievement writes, story-roster rebuilding
and slot copy, move, swap, delete, import and export are companion additions,
last modified 2026-10-05.
