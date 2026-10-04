//! TICSaveEditor.Core/Records/Layouts/UnitSaveDataLayout.cs and CombatSetLayout.cs
//! at 07ea857: the complete 600-byte unit record, including opaque fields.

use super::ManualParseError;
use ivalice_domain::reader::{SavedCombatSet, SavedUnitRecord};

pub const SIZE: usize = 600;
pub(crate) const EXP_OFFSET: usize = 0x1c;
pub(crate) const LEVEL_OFFSET: usize = 0x1d;
pub(crate) const HP_BASE_OFFSET: usize = 0x20;
pub(crate) const MP_BASE_OFFSET: usize = 0x23;
pub(crate) const SPEED_BASE_OFFSET: usize = 0x26;
pub(crate) const PA_BASE_OFFSET: usize = 0x29;
pub(crate) const MA_BASE_OFFSET: usize = 0x2c;
const COMBAT_SET_OFFSET: usize = 0x126;
const COMBAT_SET_SIZE: usize = 88;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnitRecord {
    pub character: u8,
    pub unit_index: u8,
    pub job: u8,
    pub union: u8,
    pub sex: u8,
    pub birthday: u8,
    pub zodiac_sign: u8,
    pub secondary_action: u8,
    pub reaction_ability: u16,
    pub support_ability: u16,
    pub movement_ability: u16,
    pub equip_items: [u16; 7],
    pub exp: u8,
    pub level: u8,
    pub start_bcp: u8,
    pub start_faith: u8,
    pub hp_max_base: u32,
    pub mp_max_base: u32,
    pub wt_base: u32,
    pub at_base: u32,
    pub mat_base: u32,
    pub unlocked_jobs: u32,
    pub ability_flags: [[u8; 3]; 22],
    pub job_levels_raw: [u8; 12],
    pub job_levels: [u8; 24],
    pub job_points: [u16; 23],
    pub total_job_points: [u16; 23],
    pub nickname_raw: [u8; 16],
    pub custom_job_name_raw: [u8; 16],
    pub unit_name_trailing: [u8; 32],
    pub name_no: u16,
    pub in_trip: u8,
    pub parasite: u8,
    pub egg_color: u8,
    pub psp_killed_num: u8,
    pub unit_order_id: u8,
    pub unit_starting_team: u8,
    pub unit_join_id: u8,
    pub current_combat_set: u8,
    pub combat_sets: [CombatSetRecord; 3],
    pub pad: u16,
    pub chara_name_key: u16,
    pub pad2: [u8; 38],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombatSetRecord {
    pub name_raw: [u8; 16],
    pub name_padding: [u8; 50],
    pub equipment: [u16; 5],
    pub skillsets: [i16; 2],
    pub abilities: [u16; 3],
    pub job: u8,
    pub is_double_hand: bool,
}

impl UnitRecord {
    /// TICSaveEditor UnitSaveData.cs at 07ea857 uses 0x00ff for an empty
    /// active equipment field. Older empty records can also contain zero.
    #[must_use]
    pub fn active_equipment(&self) -> [Option<u16>; 7] {
        self.equip_items
            .map(|item| (!matches!(item, 0 | 255)).then_some(item))
    }

    /// TICSaveEditor UnitSaveDataLayout.cs at 07ea857 locates both fields;
    /// game-created generic records in the owner's copies pair 0x80/0x81
    /// Character with the high Sex bits shown here.
    #[must_use]
    pub fn generic_sex(&self) -> Option<ivalice_domain::identity::UnitSex> {
        use ivalice_domain::identity::UnitSex;
        match (self.character, self.sex & 0xc0) {
            (0x80, 0x80) => Some(UnitSex::Male),
            (0x81, 0x40) => Some(UnitSex::Female),
            _ => None,
        }
    }

    #[must_use]
    pub fn can_change_generic_sex_to(&self, target: ivalice_domain::identity::UnitSex) -> bool {
        if self.generic_sex().is_none() {
            return false;
        }
        // Job.json at 07ea857: Bard 91/Sing 22, Dancer 92/Dance 23.
        let (locked_job, locked_command) = match target {
            ivalice_domain::identity::UnitSex::Male => (92, 23),
            ivalice_domain::identity::UnitSex::Female => (91, 22),
        };
        self.job != locked_job
            && self.secondary_action != locked_command
            && self.combat_sets.iter().all(|set| {
                set.job != locked_job && !set.skillsets.contains(&i16::from(locked_command))
            })
    }

    /// TICSaveEditor.Core/Records/UnitSaveData.cs at 07ea857 validates the high nibble;
    /// FFT Game Shark Handbook v4.1 supplies the Aries-through-Pisces order.
    #[must_use]
    pub fn zodiac(&self) -> Option<ivalice_domain::identity::ZodiacSign> {
        ivalice_domain::identity::ZodiacSign::ALL
            .get(usize::from(self.zodiac_sign >> 4))
            .copied()
    }

    pub fn ability_is_learned(&self, job_slot: u8, bit: u8) -> Option<bool> {
        let byte = self
            .ability_flags
            .get(usize::from(job_slot))?
            .get(usize::from(bit / 8))?;
        Some(byte & (1 << (bit % 8)) != 0)
    }

    pub fn has_equipped_ability(&self, ability_id: u16) -> bool {
        [
            self.reaction_ability,
            self.support_ability,
            self.movement_ability,
        ]
        .contains(&ability_id)
            || self
                .combat_sets
                .iter()
                .any(|set| set.abilities.contains(&ability_id))
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ManualParseError> {
        if bytes.len() != SIZE {
            return Err(ManualParseError::UnitBounds);
        }
        let ability_flags = std::array::from_fn(|index| {
            [
                bytes[0x32 + index * 3],
                bytes[0x33 + index * 3],
                bytes[0x34 + index * 3],
            ]
        });
        // TICSaveEditor.Core/Records/UnitSaveData.cs at 07ea857 reads the high
        // nibble first for even job-level indices, low nibble for odd indices.
        let job_levels = std::array::from_fn(|index| {
            let packed = bytes[0x74 + index / 2];
            if index % 2 == 0 {
                packed >> 4
            } else {
                packed & 0x0f
            }
        });
        let job_points = std::array::from_fn(|index| u16_le(bytes, 0x80 + index * 2));
        let total_job_points = std::array::from_fn(|index| u16_le(bytes, 0xae + index * 2));
        let combat_sets = std::array::from_fn(|index| {
            CombatSetRecord::parse(
                &bytes[COMBAT_SET_OFFSET + index * COMBAT_SET_SIZE
                    ..COMBAT_SET_OFFSET + (index + 1) * COMBAT_SET_SIZE],
            )
        });
        Ok(Self {
            character: bytes[0x00],
            unit_index: bytes[0x01],
            job: bytes[0x02],
            union: bytes[0x03],
            sex: bytes[0x04],
            birthday: bytes[0x05],
            zodiac_sign: bytes[0x06],
            secondary_action: bytes[0x07],
            reaction_ability: u16_le(bytes, 0x08),
            support_ability: u16_le(bytes, 0x0a),
            movement_ability: u16_le(bytes, 0x0c),
            equip_items: std::array::from_fn(|index| u16_le(bytes, 0x0e + index * 2)),
            exp: bytes[EXP_OFFSET],
            level: bytes[LEVEL_OFFSET],
            start_bcp: bytes[0x1e],
            start_faith: bytes[0x1f],
            hp_max_base: u24_le(bytes, HP_BASE_OFFSET),
            mp_max_base: u24_le(bytes, MP_BASE_OFFSET),
            wt_base: u24_le(bytes, SPEED_BASE_OFFSET),
            at_base: u24_le(bytes, PA_BASE_OFFSET),
            mat_base: u24_le(bytes, MA_BASE_OFFSET),
            unlocked_jobs: u24_le(bytes, 0x2f),
            ability_flags,
            job_levels_raw: copy(bytes, 0x74),
            job_levels,
            job_points,
            total_job_points,
            nickname_raw: copy(bytes, 0xdc),
            custom_job_name_raw: copy(bytes, 0xec),
            unit_name_trailing: copy(bytes, 0xfc),
            name_no: u16_le(bytes, 0x11c),
            in_trip: bytes[0x11e],
            parasite: bytes[0x11f],
            egg_color: bytes[0x120],
            psp_killed_num: bytes[0x121],
            unit_order_id: bytes[0x122],
            unit_starting_team: bytes[0x123],
            unit_join_id: bytes[0x124],
            current_combat_set: bytes[0x125],
            combat_sets,
            pad: u16_le(bytes, 0x22e),
            chara_name_key: u16_le(bytes, 0x230),
            pad2: copy(bytes, 0x232),
        })
    }

    pub fn is_active(&self, position: usize) -> bool {
        self.character != 0 && usize::from(self.unit_index) == position
    }

    pub fn is_empty(&self) -> bool {
        self.character == 0
    }

    pub fn first_progress_job_id(&self) -> u16 {
        // TICSaveEditor.Core/Records/UnitSaveData.cs BuildJobSlotTable at 07ea857:
        // personal classes share progress and ability slot zero with Squire.
        // Character is the stable template when the current class is generic.
        if personal_job_uses_slot_zero(self.job) {
            u16::from(self.job)
        } else if personal_job_uses_slot_zero(self.character) {
            u16::from(self.character)
        } else {
            0x4a
        }
    }

    pub fn to_saved(&self) -> SavedUnitRecord {
        SavedUnitRecord {
            character: self.character,
            unit_index: self.unit_index,
            job: self.job,
            union: self.union,
            sex: self.sex,
            birthday: self.birthday,
            zodiac_sign: self.zodiac_sign,
            secondary_action: self.secondary_action,
            reaction_ability: self.reaction_ability,
            support_ability: self.support_ability,
            movement_ability: self.movement_ability,
            equip_items: self.equip_items,
            exp: self.exp,
            level: self.level,
            start_bcp: self.start_bcp,
            start_faith: self.start_faith,
            hp_max_base: self.hp_max_base,
            mp_max_base: self.mp_max_base,
            wt_base: self.wt_base,
            at_base: self.at_base,
            mat_base: self.mat_base,
            unlocked_jobs: self.unlocked_jobs,
            ability_flags: self.ability_flags,
            job_levels_raw: self.job_levels_raw,
            job_levels: self.job_levels,
            job_points: self.job_points,
            total_job_points: self.total_job_points,
            nickname_raw: self.nickname_raw,
            custom_job_name_raw: self.custom_job_name_raw,
            unit_name_trailing: self.unit_name_trailing,
            name_no: self.name_no,
            in_trip: self.in_trip,
            parasite: self.parasite,
            egg_color: self.egg_color,
            psp_killed_num: self.psp_killed_num,
            unit_order_id: self.unit_order_id,
            unit_starting_team: self.unit_starting_team,
            unit_join_id: self.unit_join_id,
            current_combat_set: self.current_combat_set,
            combat_sets: self.combat_sets.clone().map(CombatSetRecord::into_saved),
            pad: self.pad,
            chara_name_key: self.chara_name_key,
            pad2: self.pad2.to_vec(),
        }
    }
}

fn personal_job_uses_slot_zero(job: u8) -> bool {
    matches!(
        job,
        0x01..=0x34
            | 0x3c
            | 0x3e
            | 0x40
            | 0x41
            | 0x43
            | 0x45
            | 0x48
            | 0x49
            | 0x90
            | 0x91
            | 0x96..=0x9a
            | 0xa2
            | 0xa3
            | 0xa5..=0xa8
    )
}

impl CombatSetRecord {
    fn into_saved(self) -> SavedCombatSet {
        SavedCombatSet {
            name_raw: self.name_raw,
            name_padding: self.name_padding.to_vec(),
            equipment: self.equipment,
            skillsets: self.skillsets,
            abilities: self.abilities,
            job: self.job,
            is_double_hand: self.is_double_hand,
        }
    }

    fn parse(bytes: &[u8]) -> Self {
        Self {
            name_raw: copy(bytes, 0x00),
            name_padding: copy(bytes, 0x10),
            equipment: std::array::from_fn(|index| u16_le(bytes, 0x42 + index * 2)),
            skillsets: std::array::from_fn(|index| {
                i16::from_le_bytes([bytes[0x4c + index * 2], bytes[0x4d + index * 2]])
            }),
            abilities: std::array::from_fn(|index| u16_le(bytes, 0x50 + index * 2)),
            job: bytes[0x56],
            is_double_hand: bytes[0x57] != 0,
        }
    }
}

fn copy<const N: usize>(bytes: &[u8], start: usize) -> [u8; N] {
    let mut result = [0; N];
    result.copy_from_slice(&bytes[start..start + N]);
    result
}

fn u16_le(bytes: &[u8], start: usize) -> u16 {
    u16::from_le_bytes([bytes[start], bytes[start + 1]])
}

fn u24_le(bytes: &[u8], start: usize) -> u32 {
    u32::from(bytes[start])
        | (u32::from(bytes[start + 1]) << 8)
        | (u32::from(bytes[start + 2]) << 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_boundary_and_neighbor_is_independent() {
        assert_eq!(
            UnitRecord::parse(&[0; SIZE - 1]),
            Err(ManualParseError::UnitBounds)
        );
        let mut bytes = [0; SIZE];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = index.to_le_bytes()[0];
        }
        let before = bytes;
        let record = UnitRecord::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(record.character, 0);
        assert_eq!(record.unit_index, 1);
        assert_eq!(record.movement_ability, u16::from_le_bytes([12, 13]));
        assert_eq!(record.equip_items[6], u16::from_le_bytes([26, 27]));
        assert_eq!(record.hp_max_base, 0x22_21_20);
        assert_eq!(record.unlocked_jobs, 0x31_30_2f);
        assert_eq!(record.ability_flags[21], [0x71, 0x72, 0x73]);
        assert_eq!(record.job_levels[0], 0x07);
        assert_eq!(record.job_levels[1], 0x04);
        assert_eq!(record.job_levels_raw[0], 0x74);
        assert_eq!(record.job_points[22], u16::from_le_bytes([0xac, 0xad]));
        assert_eq!(
            record.total_job_points[22],
            u16::from_le_bytes([0xda, 0xdb])
        );
        assert_eq!(record.nickname_raw[0], 0xdc);
        assert_eq!(record.name_no, u16::from_le_bytes([0x1c, 0x1d]));
        assert_eq!(
            record.combat_sets[2].equipment[4],
            u16::from_le_bytes([0x20, 0x21])
        );
        assert_eq!(
            record.combat_sets[2].skillsets[1],
            i16::from_le_bytes([0x24, 0x25])
        );
        assert_eq!(record.pad2[37], 0x57);
        assert_eq!(bytes, before);
    }

    #[test]
    fn active_flag_uses_own_position_even_for_guests() {
        let mut bytes = [0; SIZE];
        bytes[0] = 2;
        bytes[1] = 53;
        let record = UnitRecord::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        assert!(!record.is_empty());
        assert!(record.is_active(53));
        assert!(!record.is_active(52));
    }
}
