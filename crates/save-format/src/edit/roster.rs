//! Story roster writes: Ramza's chapter form, permanent story members and the
//! four guest records, built from domain plans for one occupied manual slot.
use std::collections::{BTreeMap, BTreeSet};

use ivalice_domain::story_roster::{ramza_form, UnitRecordPlan, RAMZA_FORMS};

use super::{EditOperation, GilEditError};
use crate::manual::unit_record::{self, HP_BASE_OFFSET, LEVEL_OFFSET};
use crate::UnitRecord;

pub(crate) type Record = [u8; unit_record::SIZE];

const PARTY_POSITIONS: std::ops::Range<usize> = 1..50;
const GUEST_POSITIONS: std::ops::Range<u8> = 50..54;
const MAX_BASE: u32 = 0x00ff_ffff;
const INACTIVE: u8 = 0xff;
// TICSaveEditor UnitSaveDataLayout.cs/CombatSetLayout.cs at 07ea857.
const JOB: usize = 0x02;
const SECONDARY: usize = 0x07;
const NAME_KEY: usize = 0x230;
const COMBAT_SETS: usize = 0x126;
const COMBAT_SET_SIZE: usize = 88;
const SET_SKILLSETS: usize = 0x4c;
const SET_JOB: usize = 0x56;

/// New records for every changed position; guest positions are always included
/// when guests are given.
pub(crate) fn changes(
    operations: &[EditOperation],
    current: &[Record],
) -> Result<Vec<(usize, Record)>, GilEditError> {
    if current.len() != usize::from(GUEST_POSITIONS.end) {
        return Err(GilEditError::Encoding);
    }
    let mut working = current.to_vec();
    let mut forms = operations.iter().filter_map(|operation| match operation {
        EditOperation::RamzaForm { character } => Some(*character),
        _ => None,
    });
    if let Some(character) = forms.next() {
        if forms.next().is_some() {
            return Err(GilEditError::DuplicateField);
        }
        change_ramza(&mut working[0], character)?;
    }
    let mut members = BTreeSet::new();
    for operation in operations {
        if let EditOperation::StoryMember { plan, present } = operation {
            if !members.insert((plan.character, plan.name_key)) {
                return Err(GilEditError::DuplicateField);
            }
            place_member(&mut working, plan, *present)?;
        }
    }
    let guests = guest_plans(operations)?;
    if let Some(guests) = &guests {
        let mut assigned = active(&working[..usize::from(GUEST_POSITIONS.start)])?;
        for (position, guest) in guests {
            let record = &mut working[usize::from(*position)];
            *record = match guest {
                Some(plan) => {
                    if assigned.iter().any(|unit| unit.character == plan.character) {
                        return Err(GilEditError::InvalidField);
                    }
                    let mut bytes = new_record(plan, &assigned)?;
                    assigned.push(UnitRecord::parse(&bytes)?);
                    bytes[1] = *position;
                    bytes
                }
                // An empty game-created guest record is zero apart from UnitIndex 0xFF.
                None => {
                    let mut bytes = [0; unit_record::SIZE];
                    bytes[1] = INACTIVE;
                    bytes
                }
            };
        }
    }
    Ok(working
        .into_iter()
        .enumerate()
        .filter(|(position, record)| {
            *record != current[*position]
                || (guests.is_some() && *position >= usize::from(GUEST_POSITIONS.start))
        })
        .collect())
}

fn change_ramza(record: &mut Record, character: u8) -> Result<(), GilEditError> {
    let to = ramza_form(character).ok_or(GilEditError::InvalidField)?;
    if record[1] != 0 || ramza_form(record[0]).is_none() {
        return Err(GilEditError::InvalidField);
    }
    record[0] = to.character;
    if RAMZA_FORMS.iter().any(|form| form.job == record[JOB]) {
        record[JOB] = to.job;
    }
    if RAMZA_FORMS
        .iter()
        .any(|form| form.command == record[SECONDARY])
    {
        record[SECONDARY] = to.command;
    }
    for set in 0..3 {
        let base = COMBAT_SETS + set * COMBAT_SET_SIZE;
        if RAMZA_FORMS
            .iter()
            .any(|form| form.job == record[base + SET_JOB])
        {
            record[base + SET_JOB] = to.job;
        }
        for offset in [base + SET_SKILLSETS, base + SET_SKILLSETS + 2] {
            let value = u16::from_le_bytes([record[offset], record[offset + 1]]);
            if RAMZA_FORMS
                .iter()
                .any(|form| u16::from(form.command) == value)
            {
                record[offset..offset + 2].copy_from_slice(&u16::from(to.command).to_le_bytes());
            }
        }
    }
    Ok(())
}

/// Reactivates or builds a present member; deactivates an absent one.
fn place_member(
    working: &mut [Record],
    plan: &UnitRecordPlan,
    present: bool,
) -> Result<(), GilEditError> {
    let matches: Vec<usize> = PARTY_POSITIONS
        .filter(|position| {
            let record = &working[*position];
            record[0] == plan.character
                && u16::from_le_bytes([record[NAME_KEY], record[NAME_KEY + 1]]) == plan.name_key
        })
        .collect();
    let is_active = |record: &Record, position: usize| usize::from(record[1]) == position;
    if !present {
        for position in matches {
            if is_active(&working[position], position) {
                working[position][1] = INACTIVE;
            }
        }
        return Ok(());
    }
    if matches
        .iter()
        .any(|position| is_active(&working[*position], *position))
    {
        return Ok(());
    }
    let (position, record) = match matches.first() {
        Some(position) => (*position, working[*position]),
        None => {
            let position = PARTY_POSITIONS
                .clone()
                .find(|position| working[*position][0] == 0)
                .ok_or(GilEditError::InvalidField)?;
            let assigned = active(&working[..PARTY_POSITIONS.end])?;
            (position, new_record(plan, &assigned)?)
        }
    };
    working[position] = record;
    working[position][1] = u8::try_from(position).map_err(|_| GilEditError::Encoding)?;
    Ok(())
}

fn guest_plans(
    operations: &[EditOperation],
) -> Result<Option<BTreeMap<u8, Option<UnitRecordPlan>>>, GilEditError> {
    let mut planned = BTreeMap::new();
    for operation in operations {
        if let EditOperation::StoryGuest {
            unit_position,
            guest,
        } = operation
        {
            if !GUEST_POSITIONS.contains(unit_position) {
                return Err(GilEditError::InvalidField);
            }
            if planned.insert(*unit_position, *guest).is_some() {
                return Err(GilEditError::DuplicateField);
            }
        }
    }
    match planned.len() {
        0 => Ok(None),
        count if count == GUEST_POSITIONS.len() => Ok(Some(planned)),
        _ => Err(GilEditError::InvalidField),
    }
}

fn active(records: &[Record]) -> Result<Vec<UnitRecord>, GilEditError> {
    let mut units = Vec::new();
    for (position, bytes) in records.iter().enumerate() {
        let unit = UnitRecord::parse(bytes)?;
        if unit.is_active(position) {
            units.push(unit);
        }
    }
    Ok(units)
}

fn free_id(assigned: &[UnitRecord], field: impl Fn(&UnitRecord) -> u8) -> Result<u8, GilEditError> {
    (0..=254)
        .find(|value| assigned.iter().all(|unit| field(unit) != *value))
        .ok_or(GilEditError::InvalidField)
}

/// TICSaveEditor UnitSaveDataLayout.cs at 07ea857 offsets; fields the plan does
/// not name keep the example save's game-created values (zero, sets 255). Order
/// and join ids stay unique among the active units given.
fn new_record(plan: &UnitRecordPlan, assigned: &[UnitRecord]) -> Result<Record, GilEditError> {
    let mut bytes = [0; unit_record::SIZE];
    bytes[0] = plan.character;
    bytes[JOB] = plan.job;
    bytes[4] = plan.sex;
    bytes[5] = plan.birthday;
    bytes[6] = plan.zodiac;
    for (offset, ability) in [
        (0x08, plan.reaction),
        (0x0a, plan.support),
        (0x0c, plan.movement),
    ] {
        bytes[offset..offset + 2].copy_from_slice(&ability.to_le_bytes());
    }
    for (index, item) in plan.equipment.iter().enumerate() {
        bytes[0x0e + index * 2..0x10 + index * 2].copy_from_slice(&item.to_le_bytes());
    }
    bytes[LEVEL_OFFSET] = plan.level;
    bytes[0x1e] = plan.brave;
    bytes[0x1f] = plan.faith;
    for (index, base) in plan.bases.iter().enumerate() {
        if *base > MAX_BASE {
            return Err(GilEditError::InvalidField);
        }
        let offset = HP_BASE_OFFSET + index * 3;
        bytes[offset..offset + 3].copy_from_slice(&base.to_le_bytes()[..3]);
    }
    if plan.unlocked_jobs > MAX_BASE {
        return Err(GilEditError::InvalidField);
    }
    bytes[0x2f..0x32].copy_from_slice(&plan.unlocked_jobs.to_le_bytes()[..3]);
    for (index, level) in plan.job_levels.iter().enumerate() {
        if *level > 8 {
            return Err(GilEditError::InvalidField);
        }
        // UnitSaveData.cs at 07ea857: even job slots use the high nibble.
        bytes[0x74 + index / 2] |= if index % 2 == 0 { level << 4 } else { *level };
    }
    for (index, total) in plan.total_job_points().iter().enumerate() {
        bytes[0xae + index * 2..0xb0 + index * 2].copy_from_slice(&total.to_le_bytes());
    }
    bytes[0x122] = free_id(assigned, |unit| unit.unit_order_id)?;
    bytes[0x124] = free_id(assigned, |unit| unit.unit_join_id)?;
    bytes[0x125] = 0xff;
    bytes[NAME_KEY..NAME_KEY + 2].copy_from_slice(&plan.name_key.to_le_bytes());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(character: u8, name_key: u16) -> UnitRecordPlan {
        UnitRecordPlan {
            character,
            name_key,
            sex: 0x91,
            job: character,
            birthday: 73,
            zodiac: 0x81,
            level: 10,
            brave: 61,
            faith: 67,
            reaction: 0,
            support: 0,
            movement: 0x1e0,
            equipment: [150, 177, 215, 22, 255, 255, 133],
            bases: [860_160, 401_402, 107_061, 96_374, 77_101],
            unlocked_jobs: 0xc2,
            job_levels: [1; 22],
        }
    }

    fn guests(plans: [Option<UnitRecordPlan>; 4]) -> Vec<EditOperation> {
        (50..54)
            .zip(plans)
            .map(|(unit_position, guest)| EditOperation::StoryGuest {
                unit_position,
                guest,
            })
            .collect()
    }

    fn party() -> Vec<Record> {
        let mut records = vec![[0; unit_record::SIZE]; 54];
        // Ramza (form 2) in the Squire job, with Mettle as secondary and in set 1.
        records[0][0] = 2;
        records[0][JOB] = 2;
        records[0][SECONDARY] = 26;
        records[0][COMBAT_SETS + COMBAT_SET_SIZE + SET_JOB] = 2;
        records[0][COMBAT_SETS + COMBAT_SET_SIZE + SET_SKILLSETS + 2] = 26;
        records
    }

    fn get(changes: &[(usize, Record)], position: usize) -> Option<UnitRecord> {
        changes
            .iter()
            .find(|(changed, _)| *changed == position)
            .and_then(|(_, bytes)| UnitRecord::parse(bytes).ok())
    }

    #[test]
    fn guests_are_built_and_empty_positions_cleared_with_free_ids(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let changes = changes(
            &guests([Some(plan(23, 23)), None, Some(plan(52, 52)), None]),
            &party(),
        )?;
        assert_eq!(changes.len(), 4);
        let first = get(&changes, 50).ok_or("guest missing")?;
        let third = get(&changes, 52).ok_or("guest missing")?;
        assert!(first.is_active(50) && third.is_active(52));
        assert_eq!((first.unit_order_id, first.unit_join_id), (1, 1));
        assert_eq!((third.unit_order_id, third.unit_join_id), (2, 2));
        assert_eq!(
            (first.character, first.job, first.level, first.birthday),
            (23, 23, 10, 73)
        );
        assert_eq!(first.equip_items, [150, 177, 215, 22, 255, 255, 133]);
        assert_eq!(
            (first.hp_max_base, first.mat_base, first.unlocked_jobs),
            (860_160, 77_101, 0xc2)
        );
        assert_eq!((first.movement_ability, first.chara_name_key), (0x1e0, 23));
        assert_eq!((first.job_levels[0], first.job_levels[22]), (1, 0));
        assert_eq!((first.total_job_points[0], first.job_points[0]), (100, 0));
        assert_eq!(first.current_combat_set, 255);
        for position in [51, 53] {
            let empty = get(&changes, position).ok_or("guest missing")?;
            assert!(empty.is_empty() && empty.unit_index == 0xff);
        }
        Ok(())
    }

    #[test]
    fn members_are_built_reactivated_or_deactivated() -> Result<(), Box<dyn std::error::Error>> {
        let mut records = party();
        // An inactive Mustadio keeps his record; an active Agrias is in position 3.
        records[2][0] = 22;
        records[2][1] = INACTIVE;
        records[2][NAME_KEY] = 22;
        records[3][0] = 30;
        records[3][1] = 3;
        records[3][NAME_KEY] = 30;
        let operations = [
            EditOperation::StoryMember {
                plan: plan(22, 22),
                present: true,
            },
            EditOperation::StoryMember {
                plan: plan(30, 30),
                present: false,
            },
            EditOperation::StoryMember {
                plan: plan(130, 118),
                present: true,
            },
            EditOperation::StoryMember {
                plan: plan(41, 41),
                present: false,
            },
        ];
        let changes = changes(&operations, &records)?;
        assert_eq!(
            changes
                .iter()
                .map(|(position, _)| *position)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        let boco = get(&changes, 1).ok_or("member missing")?;
        assert!(boco.is_active(1));
        assert_eq!((boco.character, boco.chara_name_key), (130, 118));
        assert!(get(&changes, 2).ok_or("member missing")?.is_active(2));
        let agrias = get(&changes, 3).ok_or("member missing")?;
        assert_eq!((agrias.character, agrias.unit_index), (30, 0xff));
        Ok(())
    }

    #[test]
    fn ramza_form_follows_personal_job_and_command() -> Result<(), Box<dyn std::error::Error>> {
        let form_one = changes(&[EditOperation::RamzaForm { character: 1 }], &party())?;
        let ramza = get(&form_one, 0).ok_or("Ramza missing")?;
        assert_eq!(
            (ramza.character, ramza.job, ramza.secondary_action),
            (1, 1, 25)
        );
        assert_eq!(ramza.combat_sets[1].job, 1);
        assert_eq!(ramza.combat_sets[1].skillsets, [0, 25]);
        let mut generic_job = party();
        generic_job[0][JOB] = 89;
        generic_job[0][SECONDARY] = 5;
        let form_three = changes(&[EditOperation::RamzaForm { character: 3 }], &generic_job)?;
        let ramza = get(&form_three, 0).ok_or("Ramza missing")?;
        assert_eq!(
            (ramza.character, ramza.job, ramza.secondary_action),
            (3, 89, 5)
        );
        let mut not_ramza = party();
        not_ramza[0][0] = 4;
        assert_eq!(
            changes(&[EditOperation::RamzaForm { character: 2 }], &not_ramza),
            Err(GilEditError::InvalidField)
        );
        Ok(())
    }

    #[test]
    fn invalid_roster_operations_fail() {
        let records = party();
        let mut missing = guests([None; 4]);
        missing.pop();
        assert_eq!(changes(&missing, &records), Err(GilEditError::InvalidField));
        let mut duplicate = guests([None; 4]);
        duplicate.push(duplicate[0]);
        assert_eq!(
            changes(&duplicate, &records),
            Err(GilEditError::DuplicateField)
        );
        assert_eq!(
            changes(
                &guests([Some(plan(52, 52)), Some(plan(52, 52)), None, None]),
                &records
            ),
            Err(GilEditError::InvalidField)
        );
        let mut oversized = plan(52, 52);
        oversized.bases[0] = 0x0100_0000;
        assert_eq!(
            changes(&guests([Some(oversized), None, None, None]), &records),
            Err(GilEditError::InvalidField)
        );
        let mut party_member = records.clone();
        party_member[3][0] = 23;
        party_member[3][1] = 3;
        assert_eq!(
            changes(
                &guests([Some(plan(23, 23)), None, None, None]),
                &party_member
            ),
            Err(GilEditError::InvalidField)
        );
        let member = EditOperation::StoryMember {
            plan: plan(22, 22),
            present: true,
        };
        assert_eq!(
            changes(&[member, member], &records),
            Err(GilEditError::DuplicateField)
        );
        let mut full = records.clone();
        for (position, record) in full.iter_mut().enumerate().skip(1).take(49) {
            record[0] = 128;
            record[1] = u8::try_from(position).unwrap_or(INACTIVE);
        }
        assert_eq!(changes(&[member], &full), Err(GilEditError::InvalidField));
        assert_eq!(
            changes(
                &[
                    EditOperation::RamzaForm { character: 1 },
                    EditOperation::RamzaForm { character: 2 }
                ],
                &records
            ),
            Err(GilEditError::DuplicateField)
        );
    }
}
