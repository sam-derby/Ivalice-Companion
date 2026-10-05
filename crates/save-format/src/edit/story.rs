//! Story step and replayed event-work writes for one occupied manual slot.
use std::collections::{BTreeMap, BTreeSet};

use super::{EditOperation, GilEditError};
use crate::manual::{
    self,
    slot_metadata::{
        event_variable_offset, story_track_offset, ACHIEVEMENTS_OFFSET, EVENT_FLAGS_OFFSET,
        EVENT_FLAG_LIMIT, EVENT_VARIABLE_COUNT, GAME_FLAGS_OFFSET, STORY_PROGRESS_OFFSETS,
    },
};
use ivalice_domain::achievements::ACHIEVEMENT_COUNT;
use ivalice_domain::calendar::{day_of_year, DAY_VARIABLE, MONTH_VARIABLE};

// Event variable 0x2C is gil; only the Gil operation may change it.
const GIL_VARIABLE: u16 = 0x2c;
const STORY_FIELD: u32 = u32::MAX;
const GAME_FLAG_FIELD: u32 = 0x1_0000;
const SIDE_TRACK_FIELD: u32 = 0x2_0000;
const ACHIEVEMENT_FIELD: u32 = 0x3_0000;
const GAME_FLAG_COUNT: u8 = 32;

#[derive(Default)]
pub(crate) struct StoryStaging {
    fields: BTreeSet<u32>,
    masks: BTreeMap<usize, (u8, u8)>,
}

impl StoryStaging {
    pub(crate) fn is_empty(&self) -> bool {
        self.masks.is_empty()
    }

    pub(crate) fn verify(&self, payload: &[u8]) -> bool {
        self.masks.iter().all(|(offset, (mask, value))| {
            payload
                .get(*offset)
                .is_some_and(|byte| byte & mask == value & mask)
        })
    }

    pub(crate) fn allows(&self, index: usize, before: u8, after: u8) -> bool {
        self.masks
            .get(&index)
            .is_some_and(|(mask, _)| (before ^ after) & !mask == 0)
    }

    fn stage(
        &mut self,
        payload: &mut [u8],
        offset: usize,
        mask: u8,
        value: u8,
    ) -> Result<(), GilEditError> {
        let byte = payload.get_mut(offset).ok_or(GilEditError::Encoding)?;
        *byte = (*byte & !mask) | (value & mask);
        let entry = self.masks.entry(offset).or_insert((0, 0));
        entry.0 |= mask;
        entry.1 = (entry.1 & !mask) | (value & mask);
        Ok(())
    }

    fn stage_i32(
        &mut self,
        payload: &mut [u8],
        offset: usize,
        value: i32,
    ) -> Result<(), GilEditError> {
        for (index, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.stage(payload, offset + index, 0xff, byte)?;
        }
        Ok(())
    }
}

pub(crate) fn apply(
    payload: &mut [u8],
    slot: u8,
    operation: &EditOperation,
    staging: &mut StoryStaging,
) -> Result<(), GilEditError> {
    let base = manual::occupied_slot_payload_base(payload, slot)?;
    match *operation {
        EditOperation::StoryProgress { value } => {
            if value < 0 || !staging.fields.insert(STORY_FIELD) {
                return Err(if value < 0 {
                    GilEditError::InvalidField
                } else {
                    GilEditError::DuplicateField
                });
            }
            for offset in STORY_PROGRESS_OFFSETS {
                staging.stage_i32(payload, base + offset, value)?;
            }
        }
        EditOperation::EventVariable { id, value } => {
            if id == GIL_VARIABLE || id >= EVENT_FLAG_LIMIT {
                return Err(GilEditError::InvalidField);
            }
            if !staging.fields.insert(u32::from(id)) {
                return Err(GilEditError::DuplicateField);
            }
            if id < EVENT_VARIABLE_COUNT {
                staging.stage_i32(payload, base + event_variable_offset(id), value)?;
            } else {
                let bit = match value {
                    0 => 0,
                    1 => 1,
                    _ => return Err(GilEditError::InvalidField),
                };
                let mask = 1 << (id % 8);
                let offset = base + EVENT_FLAGS_OFFSET + usize::from(id / 8);
                staging.stage(payload, offset, mask, bit << (id % 8))?;
            }
        }
        EditOperation::GameFlag { id, value } => {
            if id >= GAME_FLAG_COUNT {
                return Err(GilEditError::InvalidField);
            }
            if !staging.fields.insert(GAME_FLAG_FIELD + u32::from(id)) {
                return Err(GilEditError::DuplicateField);
            }
            let offset = base + GAME_FLAGS_OFFSET + usize::from(id);
            staging.stage(payload, offset, 0xff, u8::from(value))?;
        }
        EditOperation::SideProgress { track, value } => {
            if !(1..12).contains(&track) || value < 0 {
                return Err(GilEditError::InvalidField);
            }
            if !staging.fields.insert(SIDE_TRACK_FIELD + u32::from(track)) {
                return Err(GilEditError::DuplicateField);
            }
            staging.stage_i32(payload, base + story_track_offset(track), value)?;
        }
        EditOperation::CalendarDate { month, day } => {
            if day_of_year(month, day).is_none() {
                return Err(GilEditError::InvalidField);
            }
            for (id, value) in [(MONTH_VARIABLE, month), (DAY_VARIABLE, day)] {
                if !staging.fields.insert(u32::from(id)) {
                    return Err(GilEditError::DuplicateField);
                }
                staging.stage_i32(payload, base + event_variable_offset(id), i32::from(value))?;
            }
        }
        EditOperation::Achievement { index, unlocked } => {
            if index >= ACHIEVEMENT_COUNT {
                return Err(GilEditError::InvalidField);
            }
            if !staging.fields.insert(ACHIEVEMENT_FIELD + u32::from(index)) {
                return Err(GilEditError::DuplicateField);
            }
            let offset = base + ACHIEVEMENTS_OFFSET + usize::from(index);
            staging.stage(payload, offset, 0xff, u8::from(unlocked))?;
        }
        _ => return Err(GilEditError::InvalidField),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SUPPORTED_FORMAT_DISCRIMINATOR, SUPPORTED_PAYLOAD_LENGTH, SUPPORTED_PAYLOAD_VERSION,
    };

    const SLOT: u8 = 3;
    const BASE: usize = 0x10 + 3 * 0x9ce4;

    fn payload() -> Vec<u8> {
        let mut payload = vec![0x5a; SUPPORTED_PAYLOAD_LENGTH];
        payload[0..4].copy_from_slice(&SUPPORTED_PAYLOAD_VERSION.to_le_bytes());
        payload[8..16].copy_from_slice(&SUPPORTED_FORMAT_DISCRIMINATOR.to_le_bytes());
        for slot in 0..50 {
            payload[0x10 + slot * 0x9ce4..0x10 + slot * 0x9ce4 + 2].fill(0);
        }
        payload[BASE..BASE + 2].copy_from_slice(&1_u16.to_le_bytes());
        payload
    }

    #[test]
    fn story_step_writes_both_copies_and_flags_only_their_bits() {
        let before = payload();
        let mut after = before.clone();
        let mut staging = StoryStaging::default();
        for operation in [
            EditOperation::StoryProgress { value: 940 },
            EditOperation::EventVariable {
                id: 0x6e,
                value: -2,
            },
            EditOperation::EventVariable {
                id: 0x1a4,
                value: 1,
            },
            EditOperation::EventVariable {
                id: 0x1a5,
                value: 0,
            },
        ] {
            apply(&mut after, SLOT, &operation, &mut staging)
                .unwrap_or_else(|error| panic!("{error:?}"));
        }
        assert_eq!(&after[BASE + 0x9460..BASE + 0x9464], &940_i32.to_le_bytes());
        assert_eq!(&after[BASE + 0x120..BASE + 0x124], &940_i32.to_le_bytes());
        assert_eq!(
            &after[BASE + 0x8634 + 0x1b8..BASE + 0x8634 + 0x1bc],
            &(-2_i32).to_le_bytes()
        );
        let flags = BASE + 0x8634 + 0x1f0 + 0x1a4 / 8;
        assert_eq!(after[flags], (0x5a | 0b0001_0000) & !0b0010_0000);
        assert!(staging.verify(&after));
        assert!(!staging.verify(&before));
        let changed: Vec<_> = (0..before.len())
            .filter(|index| before[*index] != after[*index])
            .collect();
        assert!(changed
            .iter()
            .all(|index| staging.allows(*index, before[*index], after[*index])));
        assert!(!staging.allows(flags, 0x5a, 0x5a ^ 0b1000_0000));
        assert!(!staging.allows(BASE + 0x9464, 0, 1));
    }

    #[test]
    fn invalid_duplicate_and_gil_writes_fail() {
        let mut bytes = payload();
        let mut staging = StoryStaging::default();
        for (operation, error) in [
            (
                EditOperation::StoryProgress { value: -1 },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::EventVariable { id: 0x2c, value: 1 },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::EventVariable {
                    id: 0x400,
                    value: 1,
                },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::EventVariable { id: 0x80, value: 2 },
                GilEditError::InvalidField,
            ),
            (EditOperation::Gil { value: 1 }, GilEditError::InvalidField),
        ] {
            assert_eq!(
                apply(&mut bytes, SLOT, &operation, &mut staging),
                Err(error)
            );
        }
        let progress = EditOperation::StoryProgress { value: 10 };
        assert_eq!(apply(&mut bytes, SLOT, &progress, &mut staging), Ok(()));
        assert_eq!(
            apply(&mut bytes, SLOT, &progress, &mut staging),
            Err(GilEditError::DuplicateField)
        );
        assert_eq!(
            apply(&mut bytes, 4, &progress, &mut StoryStaging::default()),
            Err(GilEditError::Manual(crate::ManualParseError::SlotEmpty))
        );
    }

    #[test]
    fn game_flags_and_side_tracks_write_their_user_fields() {
        let before = payload();
        let mut after = before.clone();
        let mut staging = StoryStaging::default();
        for operation in [
            EditOperation::GameFlag { id: 9, value: true },
            EditOperation::GameFlag {
                id: 20,
                value: false,
            },
            EditOperation::SideProgress { track: 2, value: 0 },
        ] {
            apply(&mut after, SLOT, &operation, &mut staging)
                .unwrap_or_else(|error| panic!("{error:?}"));
        }
        assert_eq!(after[BASE + 0x9490 + 9], 1);
        assert_eq!(after[BASE + 0x9490 + 20], 0);
        assert_eq!(&after[BASE + 0x9468..BASE + 0x946c], &[0; 4]);
        assert!(staging.verify(&after));
        for (operation, error) in [
            (
                EditOperation::GameFlag {
                    id: 32,
                    value: true,
                },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::SideProgress { track: 0, value: 0 },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::SideProgress {
                    track: 12,
                    value: 0,
                },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::SideProgress {
                    track: 3,
                    value: -1,
                },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::GameFlag {
                    id: 9,
                    value: false,
                },
                GilEditError::DuplicateField,
            ),
            (
                EditOperation::SideProgress { track: 2, value: 5 },
                GilEditError::DuplicateField,
            ),
        ] {
            assert_eq!(
                apply(&mut after, SLOT, &operation, &mut staging),
                Err(error)
            );
        }
    }

    #[test]
    fn calendar_and_achievements_write_their_bytes() {
        let before = payload();
        let mut after = before.clone();
        let mut staging = StoryStaging::default();
        for operation in [
            EditOperation::CalendarDate { month: 2, day: 28 },
            EditOperation::Achievement {
                index: 0,
                unlocked: true,
            },
            EditOperation::Achievement {
                index: 49,
                unlocked: false,
            },
        ] {
            apply(&mut after, SLOT, &operation, &mut staging)
                .unwrap_or_else(|error| panic!("{error:?}"));
        }
        let variables = BASE + 0x8634;
        assert_eq!(
            &after[variables + 4 * 0x2e..variables + 4 * 0x2e + 4],
            &2_i32.to_le_bytes()
        );
        assert_eq!(
            &after[variables + 4 * 0x2f..variables + 4 * 0x2f + 4],
            &28_i32.to_le_bytes()
        );
        assert_eq!(after[BASE + 0x978e], 1);
        assert_eq!(after[BASE + 0x978e + 49], 0);
        assert_eq!(after[BASE + 0x978e + 0x32], before[BASE + 0x978e + 0x32]);
        assert!(staging.verify(&after));
        for (operation, error) in [
            (
                EditOperation::CalendarDate { month: 2, day: 29 },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::CalendarDate { month: 13, day: 1 },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::Achievement {
                    index: 50,
                    unlocked: true,
                },
                GilEditError::InvalidField,
            ),
            (
                EditOperation::Achievement {
                    index: 0,
                    unlocked: false,
                },
                GilEditError::DuplicateField,
            ),
            (
                EditOperation::EventVariable { id: 0x2f, value: 1 },
                GilEditError::DuplicateField,
            ),
        ] {
            assert_eq!(
                apply(&mut after, SLOT, &operation, &mut staging),
                Err(error)
            );
        }
    }
}
