# Save format attribution

The UMIF container implementation, manual slot mapping, unit record layout and
selected edit behavior were adapted from Nelveska's TICSaveEditor at revision
`07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b` (GPL-3.0), principally
`TICSaveEditor.Core/Save/UmifContainer.cs`, `PngEnvelope.cs`, `SaveSlot.cs`,
`SaveWork.cs`, `UnitSaveData.cs` and their layout types.

The UMIF XOR and preset-dictionary algorithm and `CompressDict.bin` originate
with Nenkai's FF16Tools at revision `dd91fb451d3b2e97bc637b5d43388e7118c145bf`
(MIT). See the root `NOTICE.md` and `LICENSES/Nenkai-MIT.txt`.

This Rust implementation adds strict bounds and integrity checks, immutable
decode, scoped replacement of the selected manual slot, and separate recoverable
file backup and restore. It does not include the upstream reference dump or
private save fixtures.
