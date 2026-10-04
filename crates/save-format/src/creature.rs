//! Experimental independent creature initialization (T074/T078).
//! Layout: TICSaveEditor UnitSaveDataLayout.cs at 07ea857. Relations: pinned
//! Job/CharaName tables and mod-loader JobData/JobCommandData.xml at ba92f91.
//! Defaults are an owner-authorized hypothesis, not a game-verified constructor.

use crate::{GilEditError, UnitRecord};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreatureForm {
    pub id: u16,
    pub character: u8,
    pub job: u8,
    pub name_key: u16,
    pub sex: u8,
    pub secondary_command: u8,
    pub unique: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreatureCategory {
    Monster,
    Enemy,
}

/// IDs are stable editor keys, not arithmetic character/job joins.
pub fn creature_forms() -> impl Iterator<Item = CreatureForm> {
    (94..=141)
        .chain(169..=173)
        .map(|job| CreatureForm {
            id: u16::from(job),
            character: 0x82,
            job,
            name_key: 0,
            sex: 0x20,
            secondary_command: 0,
            unique: false,
        })
        .chain(SPECIAL_FORMS.iter().copied())
}

pub fn creature_form(id: u16) -> Option<CreatureForm> {
    creature_forms().find(|form| form.id == id)
}

// Explicit candidate identities. Classic BMG §8 supports sprite selection by
// Character for Lucavi, by Job for demons. Enhanced form/control is experimental.
const SPECIAL_FORMS: [CreatureForm; 17] = [
    special(60, 60, 60, 60, 0x80, 104),    // Belias
    special(62, 62, 62, 62, 0x80, 108),    // Zalera
    special(64, 64, 64, 64, 0x80, 112),    // Hashmal
    special(65, 65, 65, 65, 0x40, 123),    // High Seraph: empty primary; trial Ultima secondary
    special(67, 67, 67, 67, 0x80, 116),    // Cuchulainn
    special(69, 69, 69, 69, 0x80, 120),    // Adrammelech
    special(72, 72, 72, 72, 0x20, 0),      // dragon Reis
    special(73, 73, 73, 73, 0x80, 126),    // Arch Seraph
    special(256, 0x82, 145, 115, 0x20, 0), // Construct 7
    special(257, 0x82, 150, 116, 0x20, 0), // Syneugh
    special(258, 0x82, 145, 117, 0x20, 0), // Construct 8
    special(259, 0x82, 94, 118, 0x20, 0),  // Boco
    special(260, 119, 151, 119, 0x80, 0),  // Serpentarius
    special(144, 0x82, 144, 0, 0x20, 0),   // Byblos
    repeatable(150, 150),                  // Reaver
    repeatable(153, 153),                  // Archaeodaemon
    repeatable(154, 154),                  // Ultima Demon
];

const fn special(
    id: u16,
    character: u8,
    job: u8,
    name_key: u16,
    sex: u8,
    secondary_command: u8,
) -> CreatureForm {
    CreatureForm {
        id,
        character,
        job,
        name_key,
        sex,
        secondary_command,
        unique: true,
    }
}

const fn repeatable(id: u16, job: u8) -> CreatureForm {
    CreatureForm {
        id,
        character: 0x82,
        job,
        name_key: 0,
        sex: 0x20,
        secondary_command: 0,
        unique: false,
    }
}

impl CreatureForm {
    fn uses_saved_combat_sets(self) -> bool {
        !matches!(self.job, 94..=141 | 169..=173)
    }

    pub fn can_create(self) -> bool {
        // Owner game trial, 2026-10-02: the zero-built Treant crashes the
        // game's roster. Keep its source identity visible but reject writes.
        self.id != 125
    }

    pub fn category(self) -> CreatureCategory {
        // Pinned Job.json at d3123d2: jobs 94–141 and 169–173 share
        // recruitable monster egg families. Four special forms join via story.
        if self.name_key == 0 && matches!(self.job, 94..=141 | 169..=173)
            || matches!(self.id, 72 | 144 | 258 | 259)
        {
            CreatureCategory::Monster
        } else {
            CreatureCategory::Enemy
        }
    }

    // Pinned Job.json / JobData.xml. Ordinary command IDs are contiguous in
    // both sources; the differential source test guards the complete relation.
    pub fn primary_command(self) -> u8 {
        match self.job {
            94..=141 => 176 + (self.job - 94),
            60 => 103,
            62 => 107,
            64 => 111,
            65 => 89,
            67 => 115,
            69 => 119,
            72 => 44,
            73 => 125,
            144 => 170,
            145 => 171,
            150 => 172,
            151 => 173,
            153 => 174,
            154 => 175,
            169 => 185,
            170 => 179,
            171 => 180,
            172 => 182,
            173 => 176,
            _ => 0,
        }
    }

    pub fn collides_with(self, unit: &UnitRecord) -> bool {
        self.unique
            && if self.name_key != 0 {
                unit.chara_name_key == self.name_key
                    || (self.character != 0x82 && unit.character == self.character)
            } else {
                unit.character == self.character && unit.job == self.job
            }
    }
}

pub(crate) fn construct(
    id: u16,
    position: u8,
    nickname: [u8; 16],
    targets: &[UnitRecord],
) -> Result<[u8; 600], GilEditError> {
    let form = creature_form(id).ok_or(GilEditError::InvalidField)?;
    if !form.can_create()
        || !(1..50).contains(&position)
        || targets
            .get(usize::from(position))
            .is_none_or(|unit| unit.is_active(usize::from(position)))
        || targets
            .iter()
            .enumerate()
            .any(|(index, unit)| unit.is_active(index) && form.collides_with(unit))
        || (!nickname.iter().all(|byte| *byte == 0)
            && !crate::edit::valid_generic_nickname(&nickname))
    {
        return Err(GilEditError::InvalidField);
    }
    // Deliberate defaults: zero opaque/favourite, egg/parasite, errand and
    // padding bytes; no inherited progress. Union/team zero is a trial party
    // assignment. Order/join use a free byte, independently of the destination.
    let order = free_metadata(targets, |unit| unit.unit_order_id)?;
    let join = free_metadata(targets, |unit| unit.unit_join_id)?;
    let mut bytes = [0_u8; 600];
    bytes[0] = form.character;
    bytes[1] = position;
    bytes[2] = form.job;
    bytes[4] = form.sex;
    bytes[7] = form.secondary_command;
    bytes[0x1d] = 1;
    bytes[0x1e] = 70;
    bytes[0x1f] = 70;
    // SpawnData profile 3: zero-variance level 1; MP × .75, PA/MA minus 1.
    // 16384 is the inverse of the reader's raw × multiplier / 1,638,400.
    // This policy supplies deterministic starting bases, not canonical boss stats.
    for (offset, base) in [0x20, 0x23, 0x26, 0x29, 0x2c]
        .into_iter()
        .zip([35_u32, 6, 5, 4, 4])
    {
        bytes[offset..offset + 3].copy_from_slice(&(base * 16_384).to_le_bytes()[..3]);
    }
    if form.primary_command() < 176 {
        // Personal boss command flags share slot zero in upstream. Enable the
        // 16 action bits only, not human R/S/M learning or other job progress.
        bytes[0x32..0x34].fill(0xff);
    }
    bytes[0xdc..0xec].copy_from_slice(&nickname);
    bytes[0x122] = order;
    bytes[0x124] = join;
    bytes[0x230..0x232].copy_from_slice(&form.name_key.to_le_bytes());
    for equipment in bytes[0x0e..0x1c].as_chunks_mut::<2>().0 {
        equipment.copy_from_slice(&255_u16.to_le_bytes());
    }
    if form.uses_saved_combat_sets() {
        for set in 0..3 {
            let offset = 0x126 + set * 88;
            for equipment in bytes[offset + 0x42..offset + 0x4c].as_chunks_mut::<2>().0 {
                equipment.copy_from_slice(&255_u16.to_le_bytes());
            }
            bytes[offset + 0x4c..offset + 0x4e]
                .copy_from_slice(&u16::from(form.primary_command()).to_le_bytes());
            bytes[offset + 0x4e..offset + 0x50]
                .copy_from_slice(&u16::from(form.secondary_command).to_le_bytes());
            bytes[offset + 0x56] = form.job;
        }
    } else {
        // TICSaveEditor UnitSaveDataLayout.cs at 07ea857: current 1.50
        // game-saved ordinary monsters leave all combat sets unassigned.
        bytes[0x125] = 255;
    }
    Ok(bytes)
}

fn free_metadata(
    targets: &[UnitRecord],
    field: impl Fn(&UnitRecord) -> u8,
) -> Result<u8, GilEditError> {
    (0..=254)
        .find(|value| {
            targets
                .iter()
                .enumerate()
                .all(|(index, unit)| !unit.is_active(index) || field(unit) != *value)
        })
        .ok_or(GilEditError::InvalidField)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_partition_into_joinable_monsters_and_enemy_only_forms() {
        let monsters = creature_forms()
            .filter(|form| form.category() == CreatureCategory::Monster)
            .map(|form| form.id)
            .collect::<Vec<_>>();
        let enemies = creature_forms()
            .filter(|form| form.category() == CreatureCategory::Enemy)
            .map(|form| form.id)
            .collect::<Vec<_>>();
        assert_eq!(monsters.len(), 57);
        assert_eq!(enemies.len(), 13);
        assert!([72, 144, 258, 259].iter().all(|id| monsters.contains(id)));
        assert!([169, 173].iter().all(|id| monsters.contains(id)));
        assert!([60, 65, 73].iter().all(|id| enemies.contains(id)));
    }

    fn empty_party() -> Result<Vec<UnitRecord>, crate::ManualParseError> {
        Ok(vec![UnitRecord::parse(&[0; 600])?; 54])
    }

    #[test]
    fn every_form_is_independent_active_and_has_no_gear_or_human_progress(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let targets = empty_party()?;
        let mut keys = std::collections::BTreeSet::new();
        for form in creature_forms() {
            assert!(keys.insert(form.id));
            if !form.can_create() {
                assert!(construct(form.id, 1, [0; 16], &targets).is_err());
                continue;
            }
            let bytes = construct(form.id, 1, [0; 16], &targets)?;
            let record = UnitRecord::parse(&bytes)?;
            assert!(record.is_active(1));
            assert_eq!(record.level, 1);
            assert_eq!(record.job, form.job);
            assert_eq!(record.equip_items, [255; 7]);
            assert_eq!(record.job_points, [0; 23]);
            assert_eq!(record.total_job_points, [0; 23]);
            assert_eq!(record.ability_flags[1..], [[0; 3]; 21]);
            assert!(record.hp_max_base > 0 && record.at_base > 0);
            assert_eq!(record.pad2, [0; 38]);
            assert_eq!(
                (record.parasite, record.egg_color, record.in_trip),
                (0, 0, 0)
            );
            if form.uses_saved_combat_sets() {
                assert!(record.combat_sets.iter().all(|set| set.job == form.job
                    && set.equipment == [255; 5]
                    && set.abilities == [0; 3]));
            } else {
                assert_eq!(record.current_combat_set, 255);
                assert!(record
                    .combat_sets
                    .iter()
                    .all(|set| set.job == 0 && set.equipment == [0; 5] && set.skillsets == [0; 2]));
            }
        }
        assert_eq!(keys.len(), 70);
        Ok(())
    }

    #[test]
    fn protects_roster_and_unique_identities_but_allows_repeated_species(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut targets = empty_party()?;
        targets[1] = UnitRecord::parse(&construct(65, 1, [0; 16], &targets)?)?;
        assert!(construct(65, 2, [0; 16], &targets).is_err());
        assert!(construct(73, 2, [0; 16], &targets).is_ok());
        for position in [0, 1, 50, 255] {
            assert!(construct(94, position, [0; 16], &targets).is_err());
        }
        assert!(construct(74, 2, [0; 16], &targets).is_err());
        assert!(construct(94, 2, [b'A'; 16], &targets).is_err());
        targets[2] = UnitRecord::parse(&construct(94, 2, [0; 16], &targets)?)?;
        let next = UnitRecord::parse(&construct(94, 3, [0; 16], &targets)?)?;
        assert_ne!(next.unit_join_id, targets[2].unit_join_id);
        assert_ne!(next.unit_order_id, targets[2].unit_order_id);
        Ok(())
    }
}
