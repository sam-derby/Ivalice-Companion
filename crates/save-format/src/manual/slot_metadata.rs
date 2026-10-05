//! TICSaveEditor.Core/Save/SaveWorkLayout.cs and Sections at 07ea857:
//! read-only public slot fields and the remaining opaque section properties.

use super::ManualParseError;

const CARD: usize = 0x0000;
const INFO: usize = 0x0100;
const WORLD: usize = 0x01b8;
const BATTLE: usize = 0x0518;
const USER: usize = 0x9460;
const FFTO_WORLD: usize = 0x94c4;
const FFTO_BATTLE: usize = 0x96cc;
const FFTO_ACHIEVEMENT: usize = 0x978e;
const FFTO_CONFIG: usize = 0x983a;
const FFTO_BRAVE_STORY: usize = 0x983b;
const UPSTREAM_END: usize = 0x9cdc;
const EVENT_OFFSET: usize = BATTLE + 54 * 600 + 0x105 + 0x105 + 0x80;
const EVENT_COUNT: usize = 256;
const EVENT_BYTES: usize = EVENT_COUNT * 4;
const BATTLE_SORT_OFFSET: usize = EVENT_OFFSET + EVENT_BYTES;
const BATTLE_END: usize = USER;
// Upstream's window starts two bytes early: the 4-aligned array puts variable 0x2C on gil.
const EVENT_VARIABLES_OFFSET: usize = EVENT_OFFSET + 2;
const STORY_TRACKS: usize = 12;
// Story replay (2026-10-05): ids 0x00-0x7F are i32; ids 0x80-0x3FF are bits from here.
pub(crate) const EVENT_FLAGS_OFFSET: usize = EVENT_VARIABLES_OFFSET + 0x1f0;
pub(crate) const EVENT_VARIABLE_COUNT: u16 = 0x80;
pub(crate) const EVENT_FLAG_LIMIT: u16 = 0x400;
pub(crate) const STORY_PROGRESS_OFFSETS: [usize; 2] = [USER, INFO + 0x20];
// TICSaveEditor.Core/Sections/UserSection.cs at 07ea857: GameProgressRaw then GameFlagRaw.
pub(crate) const GAME_FLAGS_OFFSET: usize = USER + 0x30;
pub(crate) fn story_track_offset(track: u8) -> usize {
    USER + 4 * usize::from(track)
}
pub(crate) fn event_variable_offset(id: u16) -> usize {
    EVENT_VARIABLES_OFFSET + 4 * usize::from(id)
}
// Owner game check of example slots 15, 17 and 28 (2026-10-05) matched this as the world-map area.
const CURRENT_AREA_VARIABLE: usize = 0x31;
// Example slots run from month 3 day 21 at step 40 to month 10 day 21 at 940.
const CALENDAR_VARIABLES: [usize; 2] = [0x2e, 0x2f];
pub(crate) const ACHIEVEMENTS_OFFSET: usize = FFTO_ACHIEVEMENT;
// Owner-controlled slot-32 before/after gil saves (2026-09-29); no upstream accessor.
pub(super) const GIL_OFFSET: usize = 0x86e4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawSlotField {
    pub section: &'static str,
    pub name: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotMetadata {
    pub card_magic: u16,
    pub title: String,
    pub title_raw: [u8; 64],
    pub saved_at_unix_seconds: i32,
    pub hero_name_raw: [u8; 17],
    pub next_event_id: i32,
    // Upstream parity name only: the independent analysis identifies scripted progress.
    pub playtime_minutes: i32,
    pub gil: u32,
    pub difficulty_level: u8,
    pub job_new_flags: Vec<u8>,
    pub job_disable_flags: Vec<u8>,
    pub event_work: Vec<i32>,
    pub event_variables: Vec<i32>,
    /// User GameProgressRaw as i32 tracks; track 0 is the main story.
    pub story_progress: [i32; STORY_TRACKS],
    pub raw_fields: Vec<RawSlotField>,
}

// TICSaveEditor.Core/Sections/{Card,Info,World,User,Ffto*}Section.cs at
// 07ea857: each tuple is (section, public raw property, section base, offset, size).
const RAW_FIELDS: &[(&str, &str, usize, usize, usize)] = &[
    ("Card", "IconRaw", CARD, 0x80, 128),
    ("Info", "InternalChecksumRaw", INFO, 0x64, 16),
    ("Info", "InfoTrailingRaw", INFO, 0x78, 64),
    ("World", "TreasureFindDayRaw", WORLD, 0x000, 53),
    ("World", "UnregFindDayRaw", WORLD, 0x035, 18),
    ("World", "MoukeFinishDayRaw", WORLD, 0x047, 108),
    ("World", "MoukeDelayRaw", WORLD, 0x0b3, 96),
    ("World", "SnplInfRaw", WORLD, 0x114, 200),
    ("World", "SnplPageFlagRaw", WORLD, 0x1dc, 160),
    ("World", "SnplStaticFlagRaw", WORLD, 0x27c, 8),
    ("World", "PersonYearRaw", WORLD, 0x284, 64),
    ("World", "MoukeEventRaw", WORLD, 0x2c5, 64),
    ("World", "WorldTrailingRaw", WORLD, 0x308, 88),
    (
        "Battle",
        "BattleSortRaw",
        BATTLE_SORT_OFFSET,
        0,
        BATTLE_END - BATTLE_SORT_OFFSET,
    ),
    ("User", "GameProgressRaw", USER, 0x00, 48),
    ("User", "GameFlagRaw", USER, 0x30, 32),
    ("User", "BonusItemsRaw", USER, 0x50, 20),
    ("FftoWorld", "RawBytes", FFTO_WORLD, 0, 0x208),
    ("FftoBattle", "GuideArrivalFlagsRaw", FFTO_BATTLE, 0x92, 48),
    ("FftoAchievement", "UnlockedRaw", FFTO_ACHIEVEMENT, 0x00, 50),
    ("FftoAchievement", "ProgressRaw", FFTO_ACHIEVEMENT, 0x32, 50),
    (
        "FftoAchievement",
        "PoachItemTypeRaw",
        FFTO_ACHIEVEMENT,
        0x64,
        26,
    ),
    (
        "FftoAchievement",
        "SummonTypeRaw",
        FFTO_ACHIEVEMENT,
        0x7e,
        16,
    ),
    (
        "FftoAchievement",
        "GeomancyTypeRaw",
        FFTO_ACHIEVEMENT,
        0x8e,
        12,
    ),
    ("FftoAchievement", "SongTypeRaw", FFTO_ACHIEVEMENT, 0x9a, 7),
    ("FftoAchievement", "IaidoTypeRaw", FFTO_ACHIEVEMENT, 0xa1, 1),
    (
        "FftoAchievement",
        "DanceTurnsRaw",
        FFTO_ACHIEVEMENT,
        0xa2,
        10,
    ),
    (
        "FftoBraveStory",
        "ZodiacStoneRaw",
        FFTO_BRAVE_STORY,
        0x000,
        52,
    ),
    ("FftoBraveStory", "BookRaw", FFTO_BRAVE_STORY, 0x034, 6),
    ("FftoBraveStory", "JournalRaw", FFTO_BRAVE_STORY, 0x03a, 520),
    (
        "FftoBraveStory",
        "GlossaryRaw",
        FFTO_BRAVE_STORY,
        0x242,
        440,
    ),
    (
        "FftoBraveStory",
        "WorldSituationRaw",
        FFTO_BRAVE_STORY,
        0x3fa,
        3,
    ),
    (
        "FftoBraveStory",
        "BraveStoryTrailingRaw",
        FFTO_BRAVE_STORY,
        0x3fd,
        152,
    ),
];

impl SlotMetadata {
    pub(crate) fn parse(slot: &[u8]) -> Result<Self, ManualParseError> {
        if slot.len() < UPSTREAM_END {
            return Err(ManualParseError::SlotBounds);
        }
        let raw_fields = RAW_FIELDS
            .iter()
            .map(|&(section, name, base, offset, len)| {
                Ok(RawSlotField {
                    section,
                    name,
                    bytes: field(slot, base, offset, len)?.to_vec(),
                })
            })
            .collect::<Result<Vec<_>, ManualParseError>>()?;
        let event_work = i32_array(field(slot, EVENT_OFFSET, 0, EVENT_BYTES)?);
        let event_variables = i32_array(field(slot, EVENT_VARIABLES_OFFSET, 0, EVENT_BYTES)?);
        let story_progress = i32_array(field(slot, USER, 0, STORY_TRACKS * 4)?)
            .try_into()
            .map_err(|_| ManualParseError::SlotBounds)?;
        // TICSaveEditor.Core/Sections/CardSection.cs at 07ea857.
        let card_magic = u16::from_le_bytes(
            field(slot, CARD, 0, 2)?
                .try_into()
                .map_err(|_| ManualParseError::SlotBounds)?,
        );
        let title_raw: [u8; 64] = field(slot, CARD, 4, 64)?
            .try_into()
            .map_err(|_| ManualParseError::SlotBounds)?;
        let title = ascii(&title_raw);
        let saved_at_unix_seconds = i32_le(field(slot, CARD, 0x44, 4)?)?;
        // TICSaveEditor.Core/Sections/InfoSection.cs at 07ea857.
        let hero_name_raw = field(slot, INFO, 1, 17)?
            .try_into()
            .map_err(|_| ManualParseError::SlotBounds)?;
        let next_event_id = i32_le(field(slot, INFO, 0x1c, 4)?)?;
        let playtime_minutes = i32_le(field(slot, INFO, 0x20, 4)?)?;
        let gil = u32::from_le_bytes(
            field(slot, GIL_OFFSET, 0, 4)?
                .try_into()
                .map_err(|_| ManualParseError::SlotBounds)?,
        );
        // TICSaveEditor.Core/Sections/FftoConfigSection.cs at 07ea857.
        let difficulty_level = field(slot, FFTO_CONFIG, 0, 1)?[0];
        // TICSaveEditor.Core/Sections/FftoBattleSection.cs at 07ea857.
        let job_new_flags = field(slot, FFTO_BATTLE, 0, 21)?.to_vec();
        let job_disable_flags = field(slot, FFTO_BATTLE, 0x15, 125)?.to_vec();
        Ok(Self {
            card_magic,
            title,
            title_raw,
            saved_at_unix_seconds,
            hero_name_raw,
            next_event_id,
            playtime_minutes,
            gil,
            difficulty_level,
            job_new_flags,
            job_disable_flags,
            event_work,
            event_variables,
            story_progress,
            raw_fields,
        })
    }

    #[must_use]
    pub fn hero_name(&self) -> String {
        ascii(&self.hero_name_raw)
    }

    #[must_use]
    pub fn readable_title(&self) -> Option<String> {
        readable_ascii(&self.title_raw)
    }

    #[must_use]
    pub fn readable_hero_name(&self) -> Option<String> {
        readable_ascii(&self.hero_name_raw)
    }

    #[must_use]
    pub fn unnamed_event_values(&self) -> usize {
        self.event_variables
            .iter()
            .filter(|value| **value != 0)
            .count()
    }

    #[must_use]
    pub fn current_area_index(&self) -> Option<u8> {
        self.event_variables
            .get(CURRENT_AREA_VARIABLE)
            .and_then(|value| u8::try_from(*value).ok())
    }

    /// Month and day from event variables 0x2E and 0x2F.
    #[must_use]
    pub fn calendar(&self) -> Option<(u8, u8)> {
        let value = |id: usize| {
            self.event_variables
                .get(id)
                .and_then(|value| u8::try_from(*value).ok())
        };
        Some((value(CALENDAR_VARIABLES[0])?, value(CALENDAR_VARIABLES[1])?))
    }

    /// FftoAchievement unlocked bytes and progress counters, by save index.
    #[must_use]
    pub fn achievements(&self) -> Option<(&[u8], &[u8])> {
        let raw = |name: &str| {
            self.raw_fields
                .iter()
                .find(|field| field.section == "FftoAchievement" && field.name == name)
                .map(|field| field.bytes.as_slice())
        };
        Some((raw("UnlockedRaw")?, raw("ProgressRaw")?))
    }

    /// Event flag 0x80-0x3FF, read from the bit array after the variables.
    #[must_use]
    pub fn event_flag(&self, id: u16) -> Option<bool> {
        if !(EVENT_VARIABLE_COUNT..EVENT_FLAG_LIMIT).contains(&id) {
            return None;
        }
        let byte = EVENT_FLAGS_OFFSET - EVENT_VARIABLES_OFFSET + usize::from(id / 8);
        let word = self.event_variables.get(byte / 4)?.to_le_bytes();
        Some(word[byte % 4] >> (id % 8) & 1 == 1)
    }
}

fn i32_array(bytes: &[u8]) -> Vec<i32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| i32::from_le_bytes(*bytes))
        .collect()
}

fn field(slot: &[u8], base: usize, offset: usize, len: usize) -> Result<&[u8], ManualParseError> {
    let start = base
        .checked_add(offset)
        .ok_or(ManualParseError::SlotBounds)?;
    let end = start.checked_add(len).ok_or(ManualParseError::SlotBounds)?;
    slot.get(start..end).ok_or(ManualParseError::SlotBounds)
}

fn i32_le(bytes: &[u8]) -> Result<i32, ManualParseError> {
    Ok(i32::from_le_bytes(
        bytes.try_into().map_err(|_| ManualParseError::SlotBounds)?,
    ))
}

fn ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| {
            if byte.is_ascii() {
                char::from(*byte)
            } else {
                '?'
            }
        })
        .collect()
}

fn readable_ascii(bytes: &[u8]) -> Option<String> {
    let raw: Vec<_> = bytes
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .collect();
    if raw.is_empty()
        || raw
            .iter()
            .any(|byte| !byte.is_ascii_graphic() && *byte != b' ')
    {
        return None;
    }
    let text = ascii(&raw);
    (!text.trim().is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_last_and_neighbor_sections_are_independent() {
        let mut slot = vec![0; UPSTREAM_END];
        slot[0..2].copy_from_slice(&1_u16.to_le_bytes());
        slot[4..9].copy_from_slice(b"Title");
        slot[0x44..0x48].copy_from_slice(&1_234_567_i32.to_le_bytes());
        slot[INFO + 1..INFO + 5].copy_from_slice(b"Hero");
        slot[INFO + 0x20..INFO + 0x24].copy_from_slice(&123_i32.to_le_bytes());
        slot[EVENT_OFFSET..EVENT_OFFSET + 4].copy_from_slice(&(-7_i32).to_le_bytes());
        slot[EVENT_OFFSET + EVENT_BYTES - 4..EVENT_OFFSET + EVENT_BYTES]
            .copy_from_slice(&9_i32.to_le_bytes());
        slot[FFTO_CONFIG] = 3;
        slot[FFTO_BRAVE_STORY + 0x494] = 255;
        let before = slot.clone();
        let parsed = SlotMetadata::parse(&slot).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(parsed.title, "Title");
        assert_eq!(parsed.readable_title().as_deref(), Some("Title"));
        assert_eq!(parsed.hero_name(), "Hero");
        assert_eq!(parsed.readable_hero_name().as_deref(), Some("Hero"));
        assert_eq!(parsed.playtime_minutes, 123);
        assert_eq!(parsed.event_work.len(), 256);
        assert_eq!((parsed.event_work[0], parsed.event_work[255]), (-7, 9));
        // Both misaligned upstream values straddle aligned variables 0 and 254.
        assert_eq!(parsed.unnamed_event_values(), 2);
        assert_eq!(parsed.difficulty_level, 3);
        assert_eq!(
            parsed
                .raw_fields
                .iter()
                .find(|field| field.name == "BraveStoryTrailingRaw")
                .map(|field| field.bytes[151]),
            Some(255)
        );
        assert_eq!(slot, before);
        assert_eq!(
            SlotMetadata::parse(&slot[..UPSTREAM_END - 1]),
            Err(ManualParseError::SlotBounds)
        );
        slot[4] = 0x81;
        let invalid = SlotMetadata::parse(&slot).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(invalid.readable_title(), None);
    }

    #[test]
    fn event_flags_are_bits_after_the_variables() {
        let mut slot = vec![0; UPSTREAM_END];
        slot[EVENT_FLAGS_OFFSET + 0xa9 / 8] = 1 << (0xa9 % 8);
        slot[EVENT_FLAGS_OFFSET + 0x3ee / 8] = 1 << (0x3ee % 8);
        let parsed = SlotMetadata::parse(&slot).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(parsed.event_flag(0xa9), Some(true));
        assert_eq!(parsed.event_flag(0xa8), Some(false));
        assert_eq!(parsed.event_flag(0xaa), Some(false));
        assert_eq!(parsed.event_flag(0x3ee), Some(true));
        assert_eq!(parsed.event_flag(0x7f), None);
        assert_eq!(parsed.event_flag(0x400), None);
    }

    #[test]
    fn aligned_event_variables_and_story_tracks_are_independent() {
        assert_eq!(EVENT_VARIABLES_OFFSET + 0x2c * 4, GIL_OFFSET);
        let mut slot = vec![0; UPSTREAM_END];
        slot[EVENT_VARIABLES_OFFSET - 1] = 0x5a;
        slot[EVENT_VARIABLES_OFFSET..EVENT_VARIABLES_OFFSET + 4]
            .copy_from_slice(&(-3_i32).to_le_bytes());
        slot[EVENT_VARIABLES_OFFSET + EVENT_BYTES - 4..EVENT_VARIABLES_OFFSET + EVENT_BYTES]
            .copy_from_slice(&i32::MAX.to_le_bytes());
        slot[EVENT_VARIABLES_OFFSET + EVENT_BYTES] = 0xa5;
        let area = EVENT_VARIABLES_OFFSET + CURRENT_AREA_VARIABLE * 4;
        slot[area..area + 4].copy_from_slice(&24_i32.to_le_bytes());
        slot[USER - 1] = 0x5a;
        slot[USER..USER + 4].copy_from_slice(&940_i32.to_le_bytes());
        slot[USER + 44..USER + 48].copy_from_slice(&(-1_i32).to_le_bytes());
        slot[USER + 48] = 0xa5;
        let parsed = SlotMetadata::parse(&slot).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(parsed.event_variables.len(), 256);
        assert_eq!(
            (parsed.event_variables[0], parsed.event_variables[255]),
            (-3, i32::MAX)
        );
        assert_eq!(parsed.unnamed_event_values(), 3);
        assert_eq!(parsed.current_area_index(), Some(24));
        assert_eq!(parsed.story_progress[0], 940);
        assert_eq!(parsed.story_progress[11], -1);
        assert!(parsed.story_progress[1..11].iter().all(|value| *value == 0));
        for outside in [-1_i32, 256] {
            slot[area..area + 4].copy_from_slice(&outside.to_le_bytes());
            let parsed = SlotMetadata::parse(&slot).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(parsed.current_area_index(), None);
        }
    }

    #[test]
    fn gil_reads_zero_and_full_width_without_neighbor_bytes() {
        let mut slot = vec![0; UPSTREAM_END];
        slot[GIL_OFFSET - 1] = 0x5a;
        slot[GIL_OFFSET + 4] = 0xa5;
        assert_eq!(
            SlotMetadata::parse(&slot)
                .unwrap_or_else(|error| panic!("{error}"))
                .gil,
            0
        );
        slot[GIL_OFFSET..GIL_OFFSET + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            SlotMetadata::parse(&slot)
                .unwrap_or_else(|error| panic!("{error}"))
                .gil,
            u32::MAX
        );
        assert_eq!((slot[GIL_OFFSET - 1], slot[GIL_OFFSET + 4]), (0x5a, 0xa5));
    }
}
