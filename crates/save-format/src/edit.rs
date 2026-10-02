//! Staged save edit. Container repacking follows TICSaveEditor.Core/Save/
//! {UmifContainer,PngEnvelope,ManualSaveFile}.cs at 07ea857; only the
//! owner-verified fields are changed in the decoded payload.

use flate2::{Compress, Compression, FlushCompress, Status};
pub use ivalice_domain::ability_flags::LoadoutKind as EquippedSlot;
use ivalice_domain::ability_flags::ValidatedAbilityFlags;
use ivalice_domain::equipment_facts::EquipmentFacts;
use ivalice_domain::equipment_rules::{project_held_counts, GearError, GearPlan, GearUnit};
pub use ivalice_domain::equipment_rules::{GearSlot, GearSource};
use ivalice_domain::identity::{UnitSex, ZodiacSign};
use ivalice_domain::job_eligibility::{level_from_total_jp, ValidatedJobRequirements};

use crate::{
    manual, png, umif, ContainerError, ManualParseError, StoredAdlerStatus, MAX_CONTAINER_BYTES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GilEditError {
    Container(ContainerError),
    Manual(ManualParseError),
    StoredChecksum,
    Encoding,
    DuplicateField,
    InvalidField,
    EquippedAbility,
    Gear(GearError),
}

/// A bounded field operation within one selected occupied manual slot.
/// Further verified fields can be added without changing the container path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditOperation {
    /// Owner-authorized experimental construction without a saved donor.
    CreateCreature {
        form_id: u16,
        unit_position: u8,
        nickname: [u8; 16],
    },
    Gil {
        value: u32,
    },
    InventoryQuantity {
        item_position: u16,
        quantity: u8,
    },
    Gear {
        unit_position: u8,
        slot: GearSlot,
        item_id: Option<u16>,
        source: Option<GearSource>,
    },
    Bravery {
        unit_position: u8,
        value: u8,
    },
    Faith {
        unit_position: u8,
        value: u8,
    },
    Zodiac {
        unit_position: u8,
        sign: ZodiacSign,
    },
    Sex {
        unit_position: u8,
        sex: UnitSex,
    },
    JobProgress {
        unit_position: u8,
        job_slot: u8,
        level: u8,
        current_jp: u16,
        total_jp: u16,
    },
    LearnedAbility {
        unit_position: u8,
        job_slot: u8,
        ability_id: u16,
        learned: bool,
    },
    EquippedSlot {
        unit_position: u8,
        combat_set: Option<u8>,
        slot: EquippedSlot,
        value: u16,
    },
    /// Experimental same-position donor copy for disposable recruitment checks.
    /// It is deliberately unavailable through the desktop IPC.
    CopyUnitFromSlot {
        source_slot: u8,
        unit_position: u8,
    },
    /// Experimental donor relocation for a disposable game check. The desktop
    /// IPC must reject it until roster behavior has been verified in game.
    RebindUnitFromSlot {
        source_slot: u8,
        source_position: u8,
        unit_position: u8,
    },
    /// Creates a named generic from an intact game-created donor record.
    CreateGenericFromSlot {
        source_slot: u8,
        source_position: u8,
        unit_position: u8,
        nickname: [u8; 16],
    },
    /// Adds an intact permanent-party story donor, rebinding only UnitIndex.
    AddStoryFromSlot {
        source_slot: u8,
        source_position: u8,
        unit_position: u8,
    },
    /// Adds an intact active guest to the permanent party, rebinding UnitIndex.
    AddGuestFromSlot {
        source_slot: u8,
        source_position: u8,
        unit_position: u8,
    },
    /// Experimental named-identity construction from an intact saved unit.
    AddNamedFromUnit {
        source_slot: u8,
        source_position: u8,
        unit_position: u8,
        character: u8,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericCreationDonor {
    pub source_slot: u8,
    pub source_position: u8,
    pub unit_position: u8,
    pub record: crate::UnitRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoryAdditionDonor {
    pub source_slot: u8,
    pub source_position: u8,
    pub unit_position: u8,
    pub record: crate::UnitRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestAdditionDonor {
    pub source_slot: u8,
    pub source_position: u8,
    pub unit_position: u8,
    pub record: crate::UnitRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedCreationBase {
    pub source_slot: u8,
    pub source_position: u8,
    pub unit_position: u8,
    pub record: crate::UnitRecord,
}

impl crate::DecodedContainer {
    pub fn named_creation_bases(
        &self,
        target_slot: u8,
    ) -> Result<Vec<NamedCreationBase>, ManualParseError> {
        if self.stored_adler_status() != StoredAdlerStatus::Matched {
            return Ok(Vec::new());
        }
        let Some(targets) = self.unit_records(target_slot)? else {
            return Ok(Vec::new());
        };
        let Some(unit_position) = (1..50).find(|position| !targets[*position].is_active(*position))
        else {
            return Ok(Vec::new());
        };
        let mut bases = Vec::new();
        for source_slot in 0..50_u8 {
            let Some(records) = self.unit_records(source_slot)? else {
                continue;
            };
            for (source_position, record) in records.iter().enumerate().take(50) {
                if named_base_allowed(
                    &targets,
                    record,
                    source_position,
                    unit_position,
                    source_slot == target_slot,
                ) {
                    bases.push(NamedCreationBase {
                        source_slot,
                        source_position: u8::try_from(source_position)
                            .map_err(|_| ManualParseError::UnitBounds)?,
                        unit_position: u8::try_from(unit_position)
                            .map_err(|_| ManualParseError::UnitBounds)?,
                        record: record.clone(),
                    });
                }
            }
        }
        bases.sort_by_key(|base| {
            (
                !(base.source_slot == target_slot && base.source_position == 0),
                base.source_slot,
                base.source_position,
            )
        });
        Ok(bases)
    }

    pub fn guest_addition_donors(
        &self,
        target_slot: u8,
    ) -> Result<Vec<GuestAdditionDonor>, ManualParseError> {
        if self.stored_adler_status() != StoredAdlerStatus::Matched {
            return Ok(Vec::new());
        }
        let Some(targets) = self.unit_records(target_slot)? else {
            return Ok(Vec::new());
        };
        let Some(unit_position) = (1..50).find(|position| !targets[*position].is_active(*position))
        else {
            return Ok(Vec::new());
        };
        let mut donors = Vec::new();
        for source_slot in 0..50_u8 {
            if source_slot == target_slot {
                continue;
            }
            let Some(records) = self.unit_records(source_slot)? else {
                continue;
            };
            for (source_position, record) in records.iter().enumerate().skip(50) {
                if guest_addition_allowed(&targets, record, source_position, unit_position) {
                    donors.push(GuestAdditionDonor {
                        source_slot,
                        source_position: u8::try_from(source_position)
                            .map_err(|_| ManualParseError::UnitBounds)?,
                        unit_position: u8::try_from(unit_position)
                            .map_err(|_| ManualParseError::UnitBounds)?,
                        record: record.clone(),
                    });
                }
            }
        }
        Ok(donors)
    }

    pub fn story_addition_donors(
        &self,
        target_slot: u8,
    ) -> Result<Vec<StoryAdditionDonor>, ManualParseError> {
        if self.stored_adler_status() != StoredAdlerStatus::Matched {
            return Ok(Vec::new());
        }
        let Some(targets) = self.unit_records(target_slot)? else {
            return Ok(Vec::new());
        };
        let Some(unit_position) = (1..50).find(|position| !targets[*position].is_active(*position))
        else {
            return Ok(Vec::new());
        };
        let mut donors = Vec::new();
        for source_slot in 0..50_u8 {
            if source_slot == target_slot {
                continue;
            }
            let Some(records) = self.unit_records(source_slot)? else {
                continue;
            };
            for (source_position, record) in records.iter().enumerate().take(50).skip(1) {
                if story_candidate_allowed(&targets, record, source_position, unit_position) {
                    donors.push(StoryAdditionDonor {
                        source_slot,
                        source_position: u8::try_from(source_position)
                            .map_err(|_| ManualParseError::UnitBounds)?,
                        unit_position: u8::try_from(unit_position)
                            .map_err(|_| ManualParseError::UnitBounds)?,
                        record: record.clone(),
                    });
                }
            }
        }
        Ok(donors)
    }

    pub fn generic_creation_donors(
        &self,
        target_slot: u8,
    ) -> Result<Vec<GenericCreationDonor>, ManualParseError> {
        if self.stored_adler_status() != StoredAdlerStatus::Matched {
            return Ok(Vec::new());
        }
        let Some(targets) = self.unit_records(target_slot)? else {
            return Ok(Vec::new());
        };
        let Some(unit_position) = (1..50).find(|position| !targets[*position].is_active(*position))
        else {
            return Ok(Vec::new());
        };
        let mut donors = Vec::new();
        for source_slot in 0..50_u8 {
            if source_slot == target_slot {
                continue;
            }
            let Some(records) = self.unit_records(source_slot)? else {
                continue;
            };
            for (source_position, record) in records.iter().enumerate().take(50).skip(1) {
                if matches!(record.character, 0x80 | 0x81)
                    && copy_candidate_allowed(&targets, record, source_position, unit_position)
                {
                    donors.push(GenericCreationDonor {
                        source_slot,
                        source_position: u8::try_from(source_position)
                            .map_err(|_| ManualParseError::UnitBounds)?,
                        unit_position: u8::try_from(unit_position)
                            .map_err(|_| ManualParseError::UnitBounds)?,
                        record: record.clone(),
                    });
                }
            }
        }
        Ok(donors)
    }
}

impl std::fmt::Display for GilEditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for GilEditError {}

impl From<ContainerError> for GilEditError {
    fn from(error: ContainerError) -> Self {
        Self::Container(error)
    }
}

impl From<ManualParseError> for GilEditError {
    fn from(error: ManualParseError) -> Self {
        Self::Manual(error)
    }
}

pub fn edit_gil_enhanced_png(
    input: &[u8],
    dictionary: &[u8],
    slot: u8,
    gil: u32,
) -> Result<Vec<u8>, GilEditError> {
    edit_enhanced_png(
        input,
        dictionary,
        slot,
        &[EditOperation::Gil { value: gil }],
    )
}

pub fn edit_enhanced_png(
    input: &[u8],
    dictionary: &[u8],
    slot: u8,
    operations: &[EditOperation],
) -> Result<Vec<u8>, GilEditError> {
    edit_enhanced_png_inner(input, dictionary, slot, operations, None, None)
}

pub fn edit_enhanced_png_with_jobs(
    input: &[u8],
    dictionary: &[u8],
    slot: u8,
    operations: &[EditOperation],
    jobs: &ValidatedJobRequirements,
) -> Result<Vec<u8>, GilEditError> {
    edit_enhanced_png_inner(input, dictionary, slot, operations, Some(jobs), None)
}

pub fn edit_enhanced_png_with_abilities(
    input: &[u8],
    dictionary: &[u8],
    slot: u8,
    operations: &[EditOperation],
    jobs: &ValidatedJobRequirements,
    abilities: &ValidatedAbilityFlags,
) -> Result<Vec<u8>, GilEditError> {
    edit_enhanced_png_inner(
        input,
        dictionary,
        slot,
        operations,
        Some(jobs),
        Some(abilities),
    )
}

fn encode_zodiac(packed: u8, sign: ZodiacSign) -> Result<u8, GilEditError> {
    if packed >> 4 >= 12 {
        return Err(GilEditError::InvalidField);
    }
    let code = ZodiacSign::ALL
        .iter()
        .position(|choice| *choice == sign)
        .and_then(|index| u8::try_from(index).ok())
        .ok_or(GilEditError::InvalidField)?;
    Ok((code << 4) | (packed & 0x0f))
}

fn encode_generic_sex(
    record: &crate::UnitRecord,
    target: UnitSex,
) -> Result<(u8, u8), GilEditError> {
    if !record.can_change_generic_sex_to(target) {
        return Err(GilEditError::InvalidField);
    }
    // UnitSaveDataLayout.cs at 07ea857: Character and Sex are independent
    // bytes. Preserve Sex subflags seen in game-created generic records.
    let (character, high_bits) = match target {
        UnitSex::Male => (0x80, 0x80),
        UnitSex::Female => (0x81, 0x40),
    };
    Ok((character, (record.sex & 0x3f) | high_bits))
}

fn is_character_addition(operation: &EditOperation) -> bool {
    matches!(
        operation,
        EditOperation::CreateGenericFromSlot { .. }
            | EditOperation::AddNamedFromUnit { .. }
            | EditOperation::CreateCreature { .. }
    )
}

fn edit_enhanced_png_inner(
    input: &[u8],
    dictionary: &[u8],
    slot: u8,
    operations: &[EditOperation],
    jobs: Option<&ValidatedJobRequirements>,
    abilities: Option<&ValidatedAbilityFlags>,
) -> Result<Vec<u8>, GilEditError> {
    // All initialization precedes field edits; the filesystem caller replaces once.
    // Donors must be active in the immutable input, never another pending addition.
    if operations.len() > 1 && operations.iter().any(is_character_addition) {
        let original = crate::decode_enhanced_png(input, Some(dictionary))?;
        let mut positions = std::collections::BTreeSet::new();
        let mut initialized = input.to_vec();
        for operation in operations
            .iter()
            .filter(|operation| is_character_addition(operation))
        {
            if let EditOperation::CreateCreature { unit_position, .. } = operation {
                if !positions.insert(*unit_position) {
                    return Err(GilEditError::DuplicateField);
                }
                initialized = edit_enhanced_png_inner(
                    &initialized,
                    dictionary,
                    slot,
                    std::slice::from_ref(operation),
                    jobs,
                    abilities,
                )?;
                continue;
            }
            let (source_slot, source_position, unit_position) = match operation {
                EditOperation::CreateGenericFromSlot {
                    source_slot,
                    source_position,
                    unit_position,
                    ..
                }
                | EditOperation::AddNamedFromUnit {
                    source_slot,
                    source_position,
                    unit_position,
                    ..
                } => (*source_slot, *source_position, *unit_position),
                _ => return Err(GilEditError::InvalidField),
            };
            if !positions.insert(unit_position) {
                return Err(GilEditError::DuplicateField);
            }
            let donor = original
                .unit_record_image(source_slot, source_position)
                .map_err(|_| GilEditError::InvalidField)?;
            if !donor.record().is_active(usize::from(source_position)) {
                return Err(GilEditError::InvalidField);
            }
            initialized = edit_enhanced_png_inner(
                &initialized,
                dictionary,
                slot,
                std::slice::from_ref(operation),
                jobs,
                abilities,
            )?;
        }
        let fields = operations
            .iter()
            .filter(|operation| !is_character_addition(operation))
            .copied()
            .collect::<Vec<_>>();
        if fields.iter().any(|operation| {
            matches!(
                operation,
                EditOperation::CopyUnitFromSlot { .. }
                    | EditOperation::RebindUnitFromSlot { .. }
                    | EditOperation::AddStoryFromSlot { .. }
                    | EditOperation::AddGuestFromSlot { .. }
            )
        }) {
            return Err(GilEditError::InvalidField);
        }
        return edit_enhanced_png_inner(&initialized, dictionary, slot, &fields, jobs, abilities);
    }
    let decoded = crate::decode_enhanced_png(input, Some(dictionary))?;
    if decoded.stored_adler_status() != StoredAdlerStatus::Matched {
        return Err(GilEditError::StoredChecksum);
    }
    let original_records = decoded
        .unit_records(slot)?
        .ok_or(GilEditError::InvalidField)?;
    let mut projected_records = original_records.clone();
    for operation in operations {
        if let EditOperation::JobProgress {
            unit_position,
            job_slot,
            level,
            total_jp,
            ..
        } = operation
        {
            let graph = jobs.ok_or(GilEditError::InvalidField)?;
            let record = projected_records
                .get_mut(usize::from(*unit_position))
                .ok_or(GilEditError::InvalidField)?;
            if usize::from(*job_slot) >= graph.requirements().len()
                || *level > 8
                || level_from_total_jp(*total_jp) != *level
            {
                return Err(GilEditError::InvalidField);
            }
            record.job_levels[usize::from(*job_slot)] = *level;
        }
    }
    if let Some(graph) = jobs {
        for (before, after) in original_records.iter().zip(&projected_records) {
            if !graph
                .invalidated_paths(&before.job_levels, &after.job_levels)
                .is_empty()
            {
                return Err(GilEditError::InvalidField);
            }
        }
    }
    let mut payload = decoded.payload().to_vec();
    let gil_start = manual::gil_payload_offset(&payload, slot)?;
    let gil_end = gil_start.checked_add(4).ok_or(GilEditError::Encoding)?;
    let mut expected_gil = None;
    let mut staged_counts = [None; manual::inventory::PARTY_CAPACITY];
    let mut staged_stats = Vec::new();
    let mut staged_zodiacs = Vec::new();
    let mut staged_sexes = Vec::new();
    let mut staged_jobs = Vec::new();
    let mut staged_job_bytes = Vec::new();
    let mut staged_ability_bits = Vec::new();
    let mut staged_ability_masks = Vec::<(usize, u8)>::new();
    let mut staged_loadout = Vec::<(usize, usize, u8, Option<u8>, EquippedSlot, u16)>::new();
    let mut staged_gear = Vec::<(u8, GearSlot, Option<u16>, Option<GearSource>)>::new();
    let mut staged_gear_bytes = Vec::<(usize, u16)>::new();
    let mut staged_gear_expected = Vec::<(u8, [Option<u16>; 7])>::new();
    let mut staged_unit_copy = None;
    for operation in operations {
        match operation {
            EditOperation::Gil { value } => {
                if expected_gil.replace(*value).is_some() {
                    return Err(GilEditError::DuplicateField);
                }
                payload
                    .get_mut(gil_start..gil_end)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&value.to_le_bytes());
            }
            EditOperation::InventoryQuantity {
                item_position,
                quantity,
            } => {
                if *quantity > 99 {
                    return Err(GilEditError::InvalidField);
                }
                let offset = manual::party_count_payload_offset(&payload, slot, *item_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                let staged = staged_counts
                    .get_mut(usize::from(*item_position))
                    .ok_or(GilEditError::InvalidField)?;
                if staged.replace((*quantity, offset)).is_some() {
                    return Err(GilEditError::DuplicateField);
                }
                *payload.get_mut(offset).ok_or(GilEditError::Encoding)? = *quantity;
            }
            EditOperation::Gear {
                unit_position,
                slot: gear_slot,
                item_id,
                source,
            } => {
                if staged_gear
                    .iter()
                    .any(|(position, slot, _, _)| position == unit_position && slot == gear_slot)
                {
                    return Err(GilEditError::DuplicateField);
                }
                staged_gear.push((*unit_position, *gear_slot, *item_id, *source));
            }
            EditOperation::Zodiac {
                unit_position,
                sign,
            } => {
                let base = manual::identity_unit_payload_offset(&payload, slot, *unit_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                let record = original_records
                    .get(usize::from(*unit_position))
                    .ok_or(GilEditError::InvalidField)?;
                if record.zodiac().is_none() {
                    return Err(GilEditError::InvalidField);
                }
                let offset = base + 6;
                if staged_zodiacs.iter().any(|(prior, _)| *prior == offset) {
                    return Err(GilEditError::DuplicateField);
                }
                // UnitSaveData.cs ZodiacSign at 07ea857: low nibble and Birthday are independent.
                let value = encode_zodiac(record.zodiac_sign, *sign)?;
                *payload.get_mut(offset).ok_or(GilEditError::Encoding)? = value;
                staged_zodiacs.push((offset, value));
            }
            EditOperation::Sex { unit_position, sex } => {
                if *unit_position >= 50 {
                    return Err(GilEditError::InvalidField);
                }
                let base = manual::identity_unit_payload_offset(&payload, slot, *unit_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                let record = original_records
                    .get(usize::from(*unit_position))
                    .ok_or(GilEditError::InvalidField)?;
                if staged_sexes.iter().any(|(prior, _, _)| *prior == base) {
                    return Err(GilEditError::DuplicateField);
                }
                let (character, sex_byte) = encode_generic_sex(record, *sex)?;
                *payload.get_mut(base).ok_or(GilEditError::Encoding)? = character;
                *payload.get_mut(base + 4).ok_or(GilEditError::Encoding)? = sex_byte;
                staged_sexes.push((base, character, sex_byte));
            }
            EditOperation::Bravery {
                unit_position,
                value,
            }
            | EditOperation::Faith {
                unit_position,
                value,
            } => {
                if *value > 100 {
                    return Err(GilEditError::InvalidField);
                }
                let field = if matches!(operation, EditOperation::Bravery { .. }) {
                    manual::UnitStatField::Bravery
                } else {
                    manual::UnitStatField::Faith
                };
                let offset =
                    manual::unit_stat_payload_offset(&payload, slot, *unit_position, field)
                        .map_err(|_| GilEditError::InvalidField)?;
                if staged_stats
                    .iter()
                    .any(|(previous, _, _, _)| *previous == offset)
                {
                    return Err(GilEditError::DuplicateField);
                }
                staged_stats.push((offset, *unit_position, *value, field));
                *payload.get_mut(offset).ok_or(GilEditError::Encoding)? = *value;
            }
            EditOperation::JobProgress {
                unit_position,
                job_slot,
                level,
                current_jp,
                total_jp,
            } => {
                let jobs = jobs.ok_or(GilEditError::InvalidField)?;
                let index = usize::from(*job_slot);
                // TICSaveEditor JobPointEntry.cs and TotalJobPointEntry.cs at 07ea857 store independent counters.
                if index >= jobs.requirements().len()
                    || *level > 8
                    || level_from_total_jp(*total_jp) != *level
                    || staged_jobs
                        .iter()
                        .any(|(_, _, prior, _, _, _)| prior == job_slot)
                {
                    return Err(GilEditError::InvalidField);
                }
                let records = &projected_records;
                let record = records
                    .get(usize::from(*unit_position))
                    .ok_or(GilEditError::InvalidField)?;
                let paths = jobs.reachable_paths(&record.job_levels);
                let unlocks = jobs
                    .saved_unlocks(record.unlocked_jobs)
                    .ok_or(GilEditError::InvalidField)?;
                if !paths[index] && !unlocks[index] {
                    return Err(GilEditError::InvalidField);
                }
                let base = manual::editable_unit_payload_offset(&payload, slot, *unit_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                let level_offset = base + 0x74 + index / 2;
                let level_byte = payload
                    .get_mut(level_offset)
                    .ok_or(GilEditError::Encoding)?;
                *level_byte = if index % 2 == 0 {
                    (*level_byte & 0x0f) | (*level << 4)
                } else {
                    (*level_byte & 0xf0) | *level
                };
                let jp_offset = base + 0x80 + index * 2;
                let total_offset = base + 0xae + index * 2;
                payload
                    .get_mut(jp_offset..jp_offset + 2)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&current_jp.to_le_bytes());
                payload
                    .get_mut(total_offset..total_offset + 2)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&total_jp.to_le_bytes());
                staged_job_bytes.extend([
                    level_offset,
                    jp_offset,
                    jp_offset + 1,
                    total_offset,
                    total_offset + 1,
                ]);
                staged_jobs.push((
                    base,
                    *unit_position,
                    *job_slot,
                    *level,
                    *current_jp,
                    *total_jp,
                ));
            }
            EditOperation::LearnedAbility {
                unit_position,
                job_slot,
                ability_id,
                learned,
            } => {
                let graph = jobs.ok_or(GilEditError::InvalidField)?;
                let map = abilities.ok_or(GilEditError::InvalidField)?;
                let records = &projected_records;
                let record = records
                    .get(usize::from(*unit_position))
                    .ok_or(GilEditError::InvalidField)?;
                let member = map
                    .member_by_id_for_save_slot(
                        *job_slot,
                        record.first_progress_job_id(),
                        *ability_id,
                    )
                    .ok_or(GilEditError::InvalidField)?;
                if staged_ability_bits
                    .iter()
                    .any(|(prior_unit, prior_slot, prior_bit, _)| {
                        prior_unit == unit_position
                            && prior_slot == job_slot
                            && *prior_bit == member.bit
                    })
                {
                    return Err(GilEditError::DuplicateField);
                }
                if !record.is_active(usize::from(*unit_position))
                    || (0x5e..=0x8d).contains(&record.job)
                    || (!graph.reachable_paths(&record.job_levels)[usize::from(*job_slot)]
                        && !graph
                            .saved_unlocks(record.unlocked_jobs)
                            .ok_or(GilEditError::InvalidField)?[usize::from(*job_slot)])
                {
                    return Err(GilEditError::InvalidField);
                }
                let base = manual::editable_unit_payload_offset(&payload, slot, *unit_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                // TICSaveEditor UnitSaveDataLayout.cs at 07ea857 stores 3 bytes per job at 0x32.
                let offset = base + 0x32 + usize::from(*job_slot) * 3 + usize::from(member.bit / 8);
                let byte = payload.get_mut(offset).ok_or(GilEditError::Encoding)?;
                let mask = 1_u8 << (member.bit % 8);
                *byte = if *learned {
                    *byte | mask
                } else {
                    *byte & !mask
                };
                staged_ability_bits.push((*unit_position, *job_slot, member.bit, *learned));
                if let Some((_, allowed)) = staged_ability_masks
                    .iter_mut()
                    .find(|(prior_offset, _)| *prior_offset == offset)
                {
                    *allowed |= mask;
                } else {
                    staged_ability_masks.push((offset, mask));
                }
            }
            EditOperation::EquippedSlot {
                unit_position,
                combat_set,
                slot: equipped_slot,
                value,
            } => {
                if abilities.is_none()
                    || jobs.is_none()
                    || *value > 511
                    || (*equipped_slot == EquippedSlot::SecondaryCommand && *value > 255)
                {
                    return Err(GilEditError::InvalidField);
                }
                let records = &projected_records;
                let record = records
                    .get(usize::from(*unit_position))
                    .ok_or(GilEditError::InvalidField)?;
                if !record.is_active(usize::from(*unit_position))
                    || (0x5e..=0x8d).contains(&record.job)
                {
                    return Err(GilEditError::InvalidField);
                }
                let base = manual::editable_unit_payload_offset(&payload, slot, *unit_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                let (offset, width) = equipped_offset(base, *combat_set, *equipped_slot)?;
                if staged_loadout
                    .iter()
                    .any(|(prior, _, _, _, _, _)| *prior == offset)
                {
                    return Err(GilEditError::DuplicateField);
                }
                let bytes = value.to_le_bytes();
                payload
                    .get_mut(offset..offset + width)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&bytes[..width]);
                staged_loadout.push((
                    offset,
                    width,
                    *unit_position,
                    *combat_set,
                    *equipped_slot,
                    *value,
                ));
            }
            EditOperation::CopyUnitFromSlot {
                source_slot,
                unit_position,
            } => {
                if operations.len() != 1 || *source_slot == slot || !(1..50).contains(unit_position)
                {
                    return Err(GilEditError::InvalidField);
                }
                let position = usize::from(*unit_position);
                let donor = decoded
                    .unit_record_image(*source_slot, *unit_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                if !same_position_copy_allowed(&original_records, donor.record(), position) {
                    return Err(GilEditError::InvalidField);
                }
                let base =
                    manual::inactive_party_unit_payload_offset(&payload, slot, *unit_position)
                        .map_err(|_| GilEditError::InvalidField)?;
                let end = base + manual::unit_record::SIZE;
                // TICSaveEditor.Core/Operations/SlotOperations.cs CopyOrDuplicate at 07ea857 copies the full record.
                payload
                    .get_mut(base..end)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(donor.bytes());
                staged_unit_copy = Some((base, end, *donor.bytes()));
            }
            EditOperation::RebindUnitFromSlot {
                source_slot,
                source_position,
                unit_position,
            } => {
                if operations.len() != 1
                    || *source_slot == slot
                    || !(1..50).contains(source_position)
                    || !(1..50).contains(unit_position)
                    || source_position == unit_position
                {
                    return Err(GilEditError::InvalidField);
                }
                let donor = decoded
                    .unit_record_image(*source_slot, *source_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                if !copy_candidate_allowed(
                    &original_records,
                    donor.record(),
                    usize::from(*source_position),
                    usize::from(*unit_position),
                ) {
                    return Err(GilEditError::InvalidField);
                }
                let base =
                    manual::inactive_party_unit_payload_offset(&payload, slot, *unit_position)
                        .map_err(|_| GilEditError::InvalidField)?;
                let end = base + manual::unit_record::SIZE;
                let mut rebound = *donor.bytes();
                // TICSaveEditor.Core/Records/UnitSaveData.cs UnitIndex at 07ea857.
                rebound[1] = *unit_position;
                payload
                    .get_mut(base..end)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&rebound);
                staged_unit_copy = Some((base, end, rebound));
            }
            EditOperation::CreateGenericFromSlot {
                source_slot,
                source_position,
                unit_position,
                nickname,
            } => {
                if operations.len() != 1
                    || *source_slot == slot
                    || !(1..50).contains(source_position)
                    || !(1..50).contains(unit_position)
                    || !valid_generic_nickname(nickname)
                {
                    return Err(GilEditError::InvalidField);
                }
                let donor = decoded
                    .unit_record_image(*source_slot, *source_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                if !matches!(donor.record().character, 0x80 | 0x81)
                    || !generic_reservation_allowed(
                        &original_records,
                        donor.record(),
                        usize::from(*source_position),
                        usize::from(*unit_position),
                    )
                {
                    return Err(GilEditError::InvalidField);
                }
                let base =
                    manual::inactive_party_unit_payload_offset(&payload, slot, *unit_position)
                        .map_err(|_| GilEditError::InvalidField)?;
                let end = base + manual::unit_record::SIZE;
                let mut created = *donor.bytes();
                // TICSaveEditor.Core/Records/UnitSaveData.cs UnitIndex and UnitNicknameRaw at 07ea857.
                created[1] = *unit_position;
                created[0xdc..0xec].copy_from_slice(nickname);
                payload
                    .get_mut(base..end)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&created);
                staged_unit_copy = Some((base, end, created));
            }
            EditOperation::AddStoryFromSlot {
                source_slot,
                source_position,
                unit_position,
            } => {
                if operations.len() != 1 || *source_slot == slot {
                    return Err(GilEditError::InvalidField);
                }
                let donor = decoded
                    .unit_record_image(*source_slot, *source_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                if !story_candidate_allowed(
                    &original_records,
                    donor.record(),
                    usize::from(*source_position),
                    usize::from(*unit_position),
                ) {
                    return Err(GilEditError::InvalidField);
                }
                let base =
                    manual::inactive_party_unit_payload_offset(&payload, slot, *unit_position)
                        .map_err(|_| GilEditError::InvalidField)?;
                let end = base + manual::unit_record::SIZE;
                let mut added = *donor.bytes();
                // TICSaveEditor.Core/Records/UnitSaveData.cs UnitIndex at 07ea857.
                added[1] = *unit_position;
                payload
                    .get_mut(base..end)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&added);
                staged_unit_copy = Some((base, end, added));
            }
            EditOperation::AddGuestFromSlot {
                source_slot,
                source_position,
                unit_position,
            } => {
                if operations.len() != 1 || *source_slot == slot {
                    return Err(GilEditError::InvalidField);
                }
                let donor = decoded
                    .unit_record_image(*source_slot, *source_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                if !guest_addition_allowed(
                    &original_records,
                    donor.record(),
                    usize::from(*source_position),
                    usize::from(*unit_position),
                ) {
                    return Err(GilEditError::InvalidField);
                }
                let base =
                    manual::inactive_party_unit_payload_offset(&payload, slot, *unit_position)
                        .map_err(|_| GilEditError::InvalidField)?;
                let end = base + manual::unit_record::SIZE;
                let mut added = *donor.bytes();
                // TICSaveEditor.Core/Records/UnitSaveData.cs UnitIndex at 07ea857.
                added[1] = *unit_position;
                payload
                    .get_mut(base..end)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&added);
                staged_unit_copy = Some((base, end, added));
            }
            EditOperation::CreateCreature {
                form_id,
                unit_position,
                nickname,
            } => {
                if operations.len() != 1 {
                    return Err(GilEditError::InvalidField);
                }
                let added = crate::creature::construct(
                    *form_id,
                    *unit_position,
                    *nickname,
                    &original_records,
                )?;
                let base =
                    manual::inactive_party_unit_payload_offset(&payload, slot, *unit_position)
                        .map_err(|_| GilEditError::InvalidField)?;
                let end = base + added.len();
                payload
                    .get_mut(base..end)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&added);
                staged_unit_copy = Some((base, end, added));
            }
            EditOperation::AddNamedFromUnit {
                source_slot,
                source_position,
                unit_position,
                character,
            } => {
                if operations.len() != 1
                    || *source_slot >= 50
                    || *source_position >= 50
                    || !(1..50).contains(unit_position)
                    || !named_human_initialization_supported(*character)
                {
                    return Err(GilEditError::InvalidField);
                }
                let donor = decoded
                    .unit_record_image(*source_slot, *source_position)
                    .map_err(|_| GilEditError::InvalidField)?;
                if !named_base_allowed(
                    &original_records,
                    donor.record(),
                    usize::from(*source_position),
                    usize::from(*unit_position),
                    *source_slot == slot,
                ) || named_character_sex(*character).is_none_or(|sex| {
                    let donor_sex = if donor.record().sex & 0x80 != 0 {
                        UnitSex::Male
                    } else {
                        UnitSex::Female
                    };
                    sex != donor_sex
                }) || original_records.iter().enumerate().any(|(index, record)| {
                    record.is_active(index)
                        && (record.character == *character
                            || record.chara_name_key == u16::from(*character))
                }) {
                    return Err(GilEditError::InvalidField);
                }
                let base =
                    manual::inactive_party_unit_payload_offset(&payload, slot, *unit_position)
                        .map_err(|_| GilEditError::InvalidField)?;
                let end = base + manual::unit_record::SIZE;
                let mut added = *donor.bytes();
                // TICSaveEditor.Core/Records/{Layouts/UnitSaveDataLayout,UnitSaveData}.cs at 07ea857:
                // Character, UnitIndex, UnitNickname, NameNo and CharaNameKey.
                added[0] = *character;
                added[1] = *unit_position;
                added[0xdc..0xec].fill(0);
                added[0x11c..0x11e].fill(0);
                added[0x230..0x232].copy_from_slice(&u16::from(*character).to_le_bytes());
                // TICSaveEditor.Core/Records/Layouts/UnitSaveDataLayout.cs at 07ea857:
                // primary job, learned first-job flags and combat-set loadouts.
                // The installed Job and CharaName tables at d3123d2 give the
                // matching personal job for the IDs accepted below. Other named
                // identities start in the ordinary Squire job (74).
                let job = personal_job_for_named(*character).unwrap_or(74);
                added[2] = job;
                added[7..0x0e].fill(0);
                for equipment in added[0x0e..0x1c].as_chunks_mut::<2>().0 {
                    equipment.copy_from_slice(&255_u16.to_le_bytes());
                }
                added[0x2f..0xdc].fill(0);
                added[0x74] = 0x10;
                for set in 0..3 {
                    let base = 0x126 + set * 88;
                    for equipment in added[base + 0x42..base + 0x4c].as_chunks_mut::<2>().0 {
                        equipment.copy_from_slice(&255_u16.to_le_bytes());
                    }
                    added[base + 0x4c..base + 0x56].fill(0);
                    added[base + 0x56] = job;
                }
                payload
                    .get_mut(base..end)
                    .ok_or(GilEditError::Encoding)?
                    .copy_from_slice(&added);
                staged_unit_copy = Some((base, end, added));
            }
        }
    }
    if !staged_gear.is_empty() {
        apply_gear_operations(
            &decoded,
            &mut payload,
            slot,
            &staged_gear,
            &mut staged_counts,
            GearStaging {
                bytes: &mut staged_gear_bytes,
                expected: &mut staged_gear_expected,
            },
            operations,
        )?;
    }
    validate_final_loadout(&decoded, &payload, slot, operations, jobs, abilities)?;
    if payload == decoded.payload() {
        return Ok(input.to_vec());
    }
    let checksum = png::crc32(&payload[0x10..]);
    payload[4..8].copy_from_slice(&checksum.to_le_bytes());

    let ffto_range = png::single_ffto_range(input)?;
    let original_umif = input
        .get(ffto_range.clone())
        .ok_or(GilEditError::Encoding)?;
    let umif = pack_umif(original_umif, &payload, dictionary)?;
    let chunk_start = ffto_range
        .start
        .checked_sub(8)
        .ok_or(GilEditError::Encoding)?;
    let chunk_end = ffto_range
        .end
        .checked_add(4)
        .ok_or(GilEditError::Encoding)?;
    let mut output = Vec::with_capacity(
        input
            .len()
            .saturating_sub(original_umif.len())
            .saturating_add(umif.len()),
    );
    output.extend_from_slice(input.get(..chunk_start).ok_or(GilEditError::Encoding)?);
    output.extend_from_slice(
        &u32::try_from(umif.len())
            .map_err(|_| GilEditError::Encoding)?
            .to_be_bytes(),
    );
    output.extend_from_slice(b"ffTo");
    output.extend_from_slice(&umif);
    output.extend_from_slice(&png::crc32_parts(b"ffTo", &umif).to_be_bytes());
    output.extend_from_slice(input.get(chunk_end..).ok_or(GilEditError::Encoding)?);
    if output.len() > MAX_CONTAINER_BYTES {
        return Err(GilEditError::Encoding);
    }
    let check = crate::decode_enhanced_png(&output, Some(dictionary))?;
    if check.stored_adler_status() != StoredAdlerStatus::Matched || check.payload() != payload {
        return Err(GilEditError::Encoding);
    }
    if expected_gil.is_some_and(|gil| {
        check
            .slot_metadata(slot)
            .ok()
            .flatten()
            .is_none_or(|metadata| metadata.gil != gil)
    }) {
        return Err(GilEditError::Encoding);
    }
    if staged_counts.iter().enumerate().any(|(position, staged)| {
        staged.is_some_and(|(quantity, _)| {
            check
                .battle_stores(slot)
                .ok()
                .flatten()
                .is_none_or(|stores| stores.party[position] != quantity)
        })
    }) {
        return Err(GilEditError::Encoding);
    }
    if staged_zodiacs
        .iter()
        .any(|(offset, value)| check.payload().get(*offset) != Some(value))
    {
        return Err(GilEditError::Encoding);
    }
    if staged_sexes.iter().any(|(base, character, sex)| {
        check.payload().get(*base) != Some(character) || check.payload().get(*base + 4) != Some(sex)
    }) {
        return Err(GilEditError::Encoding);
    }
    if !staged_stats.is_empty() {
        let records = check.unit_records(slot)?.ok_or(GilEditError::Encoding)?;
        for (_, position, value, field) in &staged_stats {
            let record = records
                .get(usize::from(*position))
                .ok_or(GilEditError::Encoding)?;
            let actual = match field {
                manual::UnitStatField::Bravery => record.start_bcp,
                manual::UnitStatField::Faith => record.start_faith,
            };
            if actual != *value {
                return Err(GilEditError::Encoding);
            }
        }
    }
    if !staged_jobs.is_empty() {
        let records = check.unit_records(slot)?.ok_or(GilEditError::Encoding)?;
        for (_, position, job_slot, level, current_jp, total_jp) in &staged_jobs {
            let record = records
                .get(usize::from(*position))
                .ok_or(GilEditError::Encoding)?;
            let index = usize::from(*job_slot);
            if record.job_levels[index] != *level
                || record.job_points[index] != *current_jp
                || record.total_job_points[index] != *total_jp
            {
                return Err(GilEditError::Encoding);
            }
            let graph = jobs.ok_or(GilEditError::InvalidField)?;
            let original = decoded
                .unit_records(slot)?
                .ok_or(GilEditError::InvalidField)?;
            let original = original
                .get(usize::from(*position))
                .ok_or(GilEditError::InvalidField)?;
            let original_unlocks = graph
                .saved_unlocks(original.unlocked_jobs)
                .ok_or(GilEditError::InvalidField)?;
            if !original_unlocks[index] && !graph.reachable_paths(&record.job_levels)[index] {
                return Err(GilEditError::InvalidField);
            }
        }
    }
    if !staged_ability_bits.is_empty() {
        let records = check.unit_records(slot)?.ok_or(GilEditError::Encoding)?;
        for (position, job_slot, bit, learned) in &staged_ability_bits {
            let actual = records
                .get(usize::from(*position))
                .and_then(|record| record.ability_is_learned(*job_slot, *bit));
            if actual != Some(*learned) {
                return Err(GilEditError::Encoding);
            }
        }
    }
    if !staged_loadout.is_empty() {
        let records = check.unit_records(slot)?.ok_or(GilEditError::Encoding)?;
        for (_, _, position, combat_set, equipped_slot, value) in &staged_loadout {
            let record = records
                .get(usize::from(*position))
                .ok_or(GilEditError::Encoding)?;
            if equipped_value(record, *combat_set, *equipped_slot) != Some(*value) {
                return Err(GilEditError::Encoding);
            }
        }
    }
    if !staged_gear_bytes.is_empty() {
        let records = check.unit_records(slot)?.ok_or(GilEditError::Encoding)?;
        for (position, expected) in &staged_gear_expected {
            let record = records
                .get(usize::from(*position))
                .ok_or(GilEditError::Encoding)?;
            if normalized_gear(&record.equip_items) != *expected {
                return Err(GilEditError::Encoding);
            }
        }
        for (offset, value) in &staged_gear_bytes {
            if check.payload().get(*offset..*offset + 2) != Some(value.to_le_bytes().as_slice()) {
                return Err(GilEditError::Encoding);
            }
        }
    }
    if let Some((base, end, expected)) = &staged_unit_copy {
        if check.payload().get(*base..*end) != Some(expected.as_slice()) {
            return Err(GilEditError::Encoding);
        }
    }
    // The payload checksum word and explicitly staged field are the only
    // allowed decoded changes. All other slots remain byte-for-byte identical.
    if decoded
        .payload()
        .iter()
        .zip(check.payload())
        .enumerate()
        .any(|(index, (before, after))| {
            before != after
                && !(4..8).contains(&index)
                && !(expected_gil.is_some() && (gil_start..gil_end).contains(&index))
                && !staged_counts
                    .iter()
                    .flatten()
                    .any(|(_, offset)| *offset == index)
                && !staged_zodiacs
                    .iter()
                    .any(|(offset, _)| *offset == index && (*before ^ *after) & 0x0f == 0)
                && !staged_sexes.iter().any(|(base, _, _)| {
                    index == *base || (index == *base + 4 && (*before ^ *after) & 0x3f == 0)
                })
                && !staged_stats
                    .iter()
                    .any(|(offset, _, _, _)| *offset == index)
                && !staged_job_bytes.contains(&index)
                && !staged_ability_masks
                    .iter()
                    .any(|(offset, mask)| *offset == index && (*before ^ *after) & !mask == 0)
                && !staged_loadout
                    .iter()
                    .any(|(offset, width, _, _, _, _)| (*offset..*offset + *width).contains(&index))
                && !staged_gear_bytes
                    .iter()
                    .any(|(offset, _)| (*offset..*offset + 2).contains(&index))
                && !staged_unit_copy
                    .as_ref()
                    .is_some_and(|(start, end, _)| (*start..*end).contains(&index))
        })
    {
        return Err(GilEditError::Encoding);
    }
    Ok(output)
}

fn normalized_gear(equipment: &[u16; 7]) -> [Option<u16>; 7] {
    equipment.map(|item| (!matches!(item, 0 | 255)).then_some(item))
}

struct GearStaging<'a> {
    bytes: &'a mut Vec<(usize, u16)>,
    expected: &'a mut Vec<(u8, [Option<u16>; 7])>,
}

fn apply_gear_operations(
    decoded: &crate::DecodedContainer,
    payload: &mut [u8],
    slot: u8,
    gear: &[(u8, GearSlot, Option<u16>, Option<GearSource>)],
    staged_counts: &mut [Option<(u8, usize)>; manual::inventory::PARTY_CAPACITY],
    staging: GearStaging<'_>,
    operations: &[EditOperation],
) -> Result<(), GilEditError> {
    let facts = EquipmentFacts::bundled().map_err(|_| GilEditError::InvalidField)?;
    let records = decoded
        .unit_records(slot)?
        .ok_or(GilEditError::InvalidField)?;
    let stores = decoded
        .battle_stores(slot)?
        .ok_or(GilEditError::InvalidField)?;
    let mut projected = std::collections::BTreeMap::<u8, GearUnit>::new();
    let mut plans = Vec::<GearPlan>::new();
    // Remove conflicting gear before applying additions, independent of UI
    // order. Every logical slot may still occur only once in the transaction.
    let mut ordered = gear.to_vec();
    ordered.sort_by_key(|(position, _, item, _)| (*position, item.is_some()));
    for (position, gear_slot, item_id, source) in ordered {
        if position >= 50 {
            return Err(GilEditError::InvalidField);
        }
        let record = records
            .get(usize::from(position))
            .ok_or(GilEditError::InvalidField)?;
        if !record.is_active(usize::from(position)) || (0x5e..=0x8d).contains(&record.job) {
            return Err(GilEditError::InvalidField);
        }
        let support = operations
            .iter()
            .find_map(|operation| match operation {
                EditOperation::EquippedSlot {
                    unit_position,
                    combat_set: None,
                    slot: EquippedSlot::Support,
                    value,
                } if *unit_position == position => Some(*value),
                _ => None,
            })
            .unwrap_or(record.support_ability);
        let unit = projected.entry(position).or_insert_with(|| GearUnit {
            job_id: u16::from(record.job),
            support_ability: support,
            equipment: normalized_gear(&record.equip_items),
        });
        let plan = unit
            .plan(facts, gear_slot, item_id, source)
            .map_err(GilEditError::Gear)?;
        unit.equipment = plan.equipment;
        plans.push(plan);
    }
    for (position, unit) in projected {
        let record = records
            .get(usize::from(position))
            .ok_or(GilEditError::InvalidField)?;
        let base = manual::editable_unit_payload_offset(payload, slot, position)
            .map_err(|_| GilEditError::InvalidField)?;
        for (index, item) in unit.equipment.iter().enumerate() {
            let value = item.unwrap_or(255);
            if normalized_gear(&record.equip_items)[index] == *item {
                continue;
            }
            // UnitSaveDataLayout.cs at 07ea857: seven active u16 slots at
            // unit+0x0e; EquipmentLoadoutHelpers.cs demuxes both hands.
            let offset = base + 0x0e + index * 2;
            payload
                .get_mut(offset..offset + 2)
                .ok_or(GilEditError::Encoding)?
                .copy_from_slice(&value.to_le_bytes());
            staging.bytes.push((offset, value));
        }
        staging.expected.push((position, unit.equipment));
    }
    let manual = staged_counts
        .iter()
        .enumerate()
        .filter_map(|(index, staged)| {
            staged.and_then(|(quantity, _)| u16::try_from(index).ok().map(|id| (id, quantity)))
        })
        .collect::<Vec<_>>();
    let final_counts =
        project_held_counts(&stores.party, &manual, &plans).map_err(GilEditError::Gear)?;
    for (index, quantity) in final_counts.into_iter().enumerate() {
        if quantity == stores.party[index] && staged_counts[index].is_none() {
            continue;
        }
        let item_position = u16::try_from(index).map_err(|_| GilEditError::Encoding)?;
        let offset = manual::party_count_payload_offset(payload, slot, item_position)
            .map_err(|_| GilEditError::InvalidField)?;
        *payload.get_mut(offset).ok_or(GilEditError::Encoding)? = quantity;
        staged_counts[index] = Some((quantity, offset));
    }
    Ok(())
}

fn same_position_copy_allowed(
    targets: &[crate::UnitRecord],
    donor: &crate::UnitRecord,
    position: usize,
) -> bool {
    copy_candidate_allowed(targets, donor, position, position)
}

/// Named identities present in the pinned CharaName table and installed Chara
/// rows. The empty/generic ranges and unnamed rows are not selectable.
fn named_character_id(character: u8) -> bool {
    matches!(character, 4..=52 | 60 | 62 | 64..=65 | 67 | 69 | 72..=76 | 115..=127)
}

/// Creature identities cannot use the human donor initializer: it would inherit
/// human metadata and fall back to Squire. CharaName/Job tables at d3123d2;
/// independent creature initialization is implemented in `creature.rs`.
pub fn named_human_initialization_supported(character: u8) -> bool {
    matches!(character, 4..=52 | 74..=76 | 120..=127)
}

/// The installed CharaName table at d3123d2 identifies these human characters;
/// game-created records in the owner's 1.03 and current saves corroborate the
/// available Sex values. Ambiguous Ajora and PSP-only names remain unavailable.
pub fn named_character_sex(character: u8) -> Option<UnitSex> {
    match character {
        12 | 15 | 20 | 25 | 28 | 30 | 33 | 41 | 42 | 44 | 45 | 46 | 47 | 48 | 52 | 75 | 120
        | 121 => Some(UnitSex::Female),
        4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 13 | 14 | 16 | 17 | 18 | 19 | 21 | 22 | 23 | 24 | 26
        | 27 | 29 | 31 | 32 | 34 | 35 | 36 | 37 | 38 | 39 | 40 | 43 | 50 | 51 | 74 | 76 | 127 => {
            Some(UnitSex::Male)
        }
        _ => None,
    }
}

fn personal_job_for_named(character: u8) -> Option<u8> {
    // Installed Job and CharaName tables at d3123d2: these named human rows
    // have a nonblank job at the same key. Other keys are NPCs or refer to a
    // different species/job and must not be treated as personal classes.
    matches!(
        character,
        4 | 5
            | 7
            | 8
            | 9
            | 12
            | 13
            | 15
            | 16
            | 17
            | 18
            | 20
            | 21
            | 22
            | 23
            | 25
            | 26
            | 27
            | 30
            | 31
            | 32
            | 34
            | 36
            | 37
            | 38
            | 39
            | 40
            | 41
            | 42
            | 43
            | 44
            | 45
            | 46
            | 47
            | 48
            | 50
            | 51
            | 52
            | 72..=76
    )
    .then_some(character)
}

fn named_base_allowed(
    targets: &[crate::UnitRecord],
    source: &crate::UnitRecord,
    source_position: usize,
    position: usize,
    same_slot: bool,
) -> bool {
    (1..50).contains(&position)
        && source_position < 50
        && source.is_active(source_position)
        && (matches!(source.character, 0x80 | 0x81)
            || (same_slot && source_position == 0 && (1..=3).contains(&source.character)))
        && targets.first().is_some_and(|record| record.is_active(0))
        && targets
            .get(position)
            .is_some_and(|record| !record.is_active(position))
}

/// Selectable saved Character values with nonblank names in the pinned
/// CharaName catalogue and corresponding installed Chara rows.
pub fn named_character_ids() -> impl Iterator<Item = u8> {
    (4..=127).filter(|character| named_character_id(*character))
}

fn generic_reservation_allowed(
    targets: &[crate::UnitRecord],
    donor: &crate::UnitRecord,
    source_position: usize,
    position: usize,
) -> bool {
    (1..50).contains(&position)
        && (1..50).contains(&source_position)
        && donor.is_active(source_position)
        && donor.character != 1
        && targets.first().is_some_and(|record| record.is_active(0))
        && targets
            .get(position)
            .is_some_and(|record| !record.is_active(position))
        && (donor.chara_name_key == 0
            || targets.iter().enumerate().take(50).all(|(index, record)| {
                !record.is_active(index) || record.chara_name_key != donor.chara_name_key
            }))
}

fn copy_candidate_allowed(
    targets: &[crate::UnitRecord],
    donor: &crate::UnitRecord,
    source_position: usize,
    position: usize,
) -> bool {
    (1..50).contains(&position)
        && (1..50).contains(&source_position)
        && donor.is_active(source_position)
        && donor.character != 1
        && targets.first().is_some_and(|record| record.is_active(0))
        && targets
            .get(position)
            .is_some_and(|record| !record.is_active(position))
        && targets
            .iter()
            .enumerate()
            .skip(1)
            .take(position - 1)
            .all(|(index, record)| record.is_active(index))
        && (donor.chara_name_key == 0
            || targets.iter().enumerate().take(50).all(|(index, record)| {
                !record.is_active(index) || record.chara_name_key != donor.chara_name_key
            }))
}

fn story_candidate_allowed(
    targets: &[crate::UnitRecord],
    donor: &crate::UnitRecord,
    source_position: usize,
    position: usize,
) -> bool {
    !matches!(donor.character, 0 | 1 | 0x80 | 0x81)
        && !(0x5e..=0x8d).contains(&donor.job)
        && copy_candidate_allowed(targets, donor, source_position, position)
        && targets
            .iter()
            .enumerate()
            .take(50)
            .all(|(index, record)| !record.is_active(index) || record.character != donor.character)
}

fn guest_addition_allowed(
    targets: &[crate::UnitRecord],
    donor: &crate::UnitRecord,
    source_position: usize,
    position: usize,
) -> bool {
    (50..54).contains(&source_position)
        && (1..50).contains(&position)
        && donor.is_active(source_position)
        && !matches!(donor.character, 0 | 1 | 0x80 | 0x81)
        && !(0x5e..=0x8d).contains(&donor.job)
        && targets.first().is_some_and(|record| record.is_active(0))
        && targets
            .get(position)
            .is_some_and(|record| !record.is_active(position))
        && targets
            .iter()
            .enumerate()
            .skip(1)
            .take(position - 1)
            .all(|(index, record)| record.is_active(index))
        && targets.iter().enumerate().all(|(index, record)| {
            !record.is_active(index)
                || (record.character != donor.character
                    && (donor.chara_name_key == 0 || record.chara_name_key != donor.chara_name_key))
        })
}

pub(crate) fn valid_generic_nickname(bytes: &[u8; 16]) -> bool {
    let Some(end) = bytes.iter().position(|byte| *byte == 0) else {
        return false;
    };
    (1..=15).contains(&end)
        && bytes[..end].iter().all(|byte| (0x20..=0x7e).contains(byte))
        && bytes[..end].iter().any(|byte| *byte != b' ')
        && bytes[end..].iter().all(|byte| *byte == 0)
}

fn equipped_offset(
    base: usize,
    combat_set: Option<u8>,
    slot: EquippedSlot,
) -> Result<(usize, usize), GilEditError> {
    // TICSaveEditor.Core/Records/Layouts/{UnitSaveDataLayout,CombatSetLayout}.cs at 07ea857.
    match combat_set {
        None => Ok((
            base + match slot {
                EquippedSlot::SecondaryCommand => 0x07,
                EquippedSlot::Reaction => 0x08,
                EquippedSlot::Support => 0x0a,
                EquippedSlot::Movement => 0x0c,
            },
            if slot == EquippedSlot::SecondaryCommand {
                1
            } else {
                2
            },
        )),
        Some(index) if index < 3 => Ok((
            base + 0x126
                + usize::from(index) * 88
                + match slot {
                    EquippedSlot::SecondaryCommand => 0x4e,
                    EquippedSlot::Reaction => 0x50,
                    EquippedSlot::Support => 0x52,
                    EquippedSlot::Movement => 0x54,
                },
            2,
        )),
        Some(_) => Err(GilEditError::InvalidField),
    }
}

fn equipped_value(
    record: &crate::UnitRecord,
    combat_set: Option<u8>,
    slot: EquippedSlot,
) -> Option<u16> {
    if let Some(index) = combat_set {
        let set = record.combat_sets.get(usize::from(index))?;
        return match slot {
            EquippedSlot::SecondaryCommand => u16::try_from(set.skillsets[1]).ok(),
            EquippedSlot::Reaction => Some(set.abilities[0]),
            EquippedSlot::Support => Some(set.abilities[1]),
            EquippedSlot::Movement => Some(set.abilities[2]),
        };
    }
    Some(match slot {
        EquippedSlot::SecondaryCommand => u16::from(record.secondary_action),
        EquippedSlot::Reaction => record.reaction_ability,
        EquippedSlot::Support => record.support_ability,
        EquippedSlot::Movement => record.movement_ability,
    })
}

fn validate_final_loadout(
    decoded: &crate::DecodedContainer,
    payload: &[u8],
    slot: u8,
    operations: &[EditOperation],
    jobs: Option<&ValidatedJobRequirements>,
    abilities: Option<&ValidatedAbilityFlags>,
) -> Result<(), GilEditError> {
    if !operations.iter().any(|operation| {
        matches!(
            operation,
            EditOperation::EquippedSlot { .. }
                | EditOperation::LearnedAbility { learned: false, .. }
        )
    }) {
        return Ok(());
    }
    let graph = jobs.ok_or(GilEditError::InvalidField)?;
    let map = abilities.ok_or(GilEditError::InvalidField)?;
    let originals = decoded
        .unit_records(slot)?
        .ok_or(GilEditError::InvalidField)?;
    for operation in operations {
        let (position, equipped) = match operation {
            EditOperation::EquippedSlot {
                unit_position,
                combat_set,
                slot,
                value,
            } => (*unit_position, Some((*combat_set, *slot, *value))),
            EditOperation::LearnedAbility {
                unit_position,
                learned: false,
                ..
            } => (*unit_position, None),
            _ => continue,
        };
        let original = originals
            .get(usize::from(position))
            .ok_or(GilEditError::InvalidField)?;
        let base = manual::editable_unit_payload_offset(payload, slot, position)
            .map_err(|_| GilEditError::InvalidField)?;
        let revised = crate::UnitRecord::parse(
            payload
                .get(base..base + manual::unit_record::SIZE)
                .ok_or(GilEditError::Encoding)?,
        )?;
        if let Some((combat_set, equipped_slot, value)) = equipped {
            if !original.is_active(usize::from(position))
                || equipped_value(&revised, combat_set, equipped_slot) != Some(value)
                || !equipped_choice_allowed(&revised, graph, map, equipped_slot, value)
            {
                return Err(GilEditError::InvalidField);
            }
        } else if let EditOperation::LearnedAbility { ability_id, .. } = operation {
            if revised.has_equipped_ability(*ability_id) {
                return Err(GilEditError::EquippedAbility);
            }
        }
    }
    Ok(())
}

fn equipped_choice_allowed(
    record: &crate::UnitRecord,
    graph: &ValidatedJobRequirements,
    map: &ValidatedAbilityFlags,
    slot: EquippedSlot,
    value: u16,
) -> bool {
    let paths = graph.reachable_paths(&record.job_levels);
    let Some(unlocks) = graph.saved_unlocks(record.unlocked_jobs) else {
        return false;
    };
    let eligible = (0..20)
        .filter(|index| paths[*index] || unlocks[*index])
        .filter_map(|index| u8::try_from(index).ok())
        .collect::<Vec<_>>();
    map.is_compatible_loadout_choice(
        record.first_progress_job_id(),
        &eligible,
        |job_slot, bit| record.ability_is_learned(job_slot, bit) == Some(true),
        slot,
        value,
    )
}

fn pack_umif(original: &[u8], payload: &[u8], dictionary: &[u8]) -> Result<Vec<u8>, GilEditError> {
    // The source has already passed exact single-file UMIF validation.
    const DATA_OFFSET: usize = 0x3c;
    let mut compressor = Compress::new(Compression::best(), true);
    compressor
        .set_dictionary(dictionary)
        .map_err(|_| GilEditError::Encoding)?;
    let mut compressed = vec![
        0_u8;
        payload
            .len()
            .checked_add(65_536)
            .ok_or(GilEditError::Encoding)?
    ];
    let status = compressor
        .compress(payload, &mut compressed, FlushCompress::Finish)
        .map_err(|_| GilEditError::Encoding)?;
    if status != Status::StreamEnd || compressor.total_in() != payload.len() as u64 {
        return Err(GilEditError::Encoding);
    }
    compressed
        .truncate(usize::try_from(compressor.total_out()).map_err(|_| GilEditError::Encoding)?);
    let stored = compressed.get(2..).ok_or(GilEditError::Encoding)?;
    let data_end = DATA_OFFSET
        .checked_add(stored.len())
        .ok_or(GilEditError::Encoding)?;
    let total = data_end.checked_add(3).ok_or(GilEditError::Encoding)? & !3;
    if total > MAX_CONTAINER_BYTES {
        return Err(GilEditError::Encoding);
    }
    let mut result = vec![0_u8; total];
    result
        .get_mut(..DATA_OFFSET)
        .ok_or(GilEditError::Encoding)?
        .copy_from_slice(original.get(..DATA_OFFSET).ok_or(GilEditError::Encoding)?);
    result[0x14..0x18].copy_from_slice(
        &u32::try_from(stored.len())
            .map_err(|_| GilEditError::Encoding)?
            .to_le_bytes(),
    );
    result[DATA_OFFSET..data_end].copy_from_slice(stored);
    umif::crypt(&mut result[DATA_OFFSET..data_end]);
    Ok(result)
}

#[cfg(test)]
mod copy_tests {
    use super::*;

    fn record(character: u8, index: u8, name_key: u16) -> crate::UnitRecord {
        let mut bytes = [0_u8; manual::unit_record::SIZE];
        bytes[0] = character;
        bytes[1] = index;
        bytes[0x230..0x232].copy_from_slice(&name_key.to_le_bytes());
        crate::UnitRecord::parse(&bytes).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn first_inactive_candidate_accepts_an_intact_donor_and_rejects_collisions() {
        let mut targets = vec![record(0, 0xff, 0); 54];
        targets[0] = record(1, 0, 1);
        for (index, target) in targets.iter_mut().enumerate().take(12).skip(1) {
            *target = record(0x80, u8::try_from(index).unwrap_or(0), 0);
        }
        let donor = record(0x80, 12, 77);
        assert!(same_position_copy_allowed(&targets, &donor, 12));
        assert!(!same_position_copy_allowed(&targets, &donor, 0));
        assert!(!same_position_copy_allowed(&targets, &donor, 50));
        assert!(!same_position_copy_allowed(&targets, &record(1, 12, 1), 12));
        assert!(!same_position_copy_allowed(
            &targets,
            &record(0x80, 13, 77),
            12
        ));
        targets[3] = record(0x80, 0xff, 0);
        assert!(!same_position_copy_allowed(&targets, &donor, 12));
        targets[3] = record(0x80, 3, 77);
        assert!(!same_position_copy_allowed(&targets, &donor, 12));
        targets[3] = record(0x80, 3, 0);
        assert!(copy_candidate_allowed(
            &targets,
            &record(0x80, 8, 78),
            8,
            12
        ));
        assert!(!copy_candidate_allowed(
            &targets,
            &record(0x80, 9, 78),
            8,
            12
        ));
        assert!(!copy_candidate_allowed(&targets, &record(1, 8, 1), 8, 12));
        targets[12] = record(0x80, 12, 0);
        assert!(!same_position_copy_allowed(&targets, &donor, 12));
    }

    #[test]
    fn generic_nickname_is_printable_terminated_and_bounded() {
        let mut valid = [0_u8; 16];
        valid[..7].copy_from_slice(b"Mira 42");
        assert!(valid_generic_nickname(&valid));
        valid[7] = b'X';
        valid[8..].fill(b'X');
        assert!(!valid_generic_nickname(&valid));
        valid = [0; 16];
        assert!(!valid_generic_nickname(&valid));
        valid[0] = b' ';
        assert!(!valid_generic_nickname(&valid));
        valid[0] = 0x1f;
        assert!(!valid_generic_nickname(&valid));
        valid[0] = b'A';
        valid[1] = 0;
        valid[2] = b'B';
        assert!(!valid_generic_nickname(&valid));
    }

    #[test]
    fn every_zodiac_edit_preserves_subflags_and_rejects_corrupt_signs() {
        for low in 0..16 {
            for (index, sign) in ZodiacSign::ALL.into_iter().enumerate() {
                let code = u8::try_from(index).unwrap_or(u8::MAX);
                assert_eq!(encode_zodiac(0xb0 | low, sign), Ok((code << 4) | low));
            }
        }
        for invalid in 12..16 {
            assert_eq!(
                encode_zodiac(invalid << 4, ZodiacSign::Aries),
                Err(GilEditError::InvalidField)
            );
        }
    }

    #[test]
    fn generic_sex_swap_preserves_subflags_and_rejects_locked_loadouts() {
        for subflags in [0, 0x10, 0x2f] {
            let mut male = record(0x80, 1, 0);
            male.sex = 0x80 | subflags;
            assert_eq!(male.generic_sex(), Some(UnitSex::Male));
            assert_eq!(
                encode_generic_sex(&male, UnitSex::Female),
                Ok((0x81, 0x40 | subflags))
            );
            let mut female = record(0x81, 1, 0);
            female.sex = 0x40 | subflags;
            assert_eq!(female.generic_sex(), Some(UnitSex::Female));
            assert_eq!(
                encode_generic_sex(&female, UnitSex::Male),
                Ok((0x80, 0x80 | subflags))
            );
        }
        let mut male = record(0x80, 1, 0);
        male.sex = 0x80;
        male.job = 91;
        assert_eq!(
            encode_generic_sex(&male, UnitSex::Female),
            Err(GilEditError::InvalidField)
        );
        male.job = 74;
        male.secondary_action = 22;
        assert_eq!(
            encode_generic_sex(&male, UnitSex::Female),
            Err(GilEditError::InvalidField)
        );
        male.secondary_action = 0;
        male.combat_sets[0].job = 91;
        assert_eq!(
            encode_generic_sex(&male, UnitSex::Female),
            Err(GilEditError::InvalidField)
        );
        male.combat_sets[0].job = 74;
        male.combat_sets[0].skillsets[1] = 22;
        assert_eq!(
            encode_generic_sex(&male, UnitSex::Female),
            Err(GilEditError::InvalidField)
        );
        male.combat_sets[0].skillsets[1] = 0;
        male.character = 0x81;
        assert_eq!(
            encode_generic_sex(&male, UnitSex::Female),
            Err(GilEditError::InvalidField)
        );
    }

    #[test]
    fn creature_identities_never_use_the_human_squire_fallback() {
        for character in [60, 62, 64, 65, 67, 69, 72, 73, 115, 116, 117, 118, 119] {
            assert!(named_character_id(character));
            assert!(!named_human_initialization_supported(character));
        }
        for character in [4, 30, 32, 50, 74, 75, 76, 120, 127] {
            assert!(named_human_initialization_supported(character));
        }
    }

    #[test]
    fn named_job_selection_distinguishes_personal_classes_from_npcs() {
        assert_eq!(personal_job_for_named(32), Some(32)); // Wiegraf, White Knight
        assert_eq!(personal_job_for_named(30), Some(30)); // Agrias, Holy Knight
        assert_eq!(personal_job_for_named(4), Some(4)); // Delita, Squire
        assert_eq!(personal_job_for_named(10), None); // Larg, no named job
        assert_eq!(personal_job_for_named(115), None); // Construct 7, mismatched species row
    }

    #[test]
    fn named_human_sex_covers_known_identities_and_rejects_unverified_ones() {
        let supported = (4..=52)
            .chain(74..=76)
            .chain(120..=127)
            .filter(|character| named_character_sex(*character).is_some())
            .count();
        assert_eq!(supported, 54);
        assert_eq!(named_character_sex(32), Some(UnitSex::Male)); // Wiegraf
        assert_eq!(named_character_sex(40), Some(UnitSex::Male)); // Wiegraf variant
        assert_eq!(named_character_sex(30), Some(UnitSex::Female)); // Agrias
        assert_eq!(named_character_sex(52), Some(UnitSex::Female)); // Agrias variant
        for character in [49, 122, 123, 124, 125, 126, 0x80] {
            assert_eq!(named_character_sex(character), None);
        }
    }

    #[test]
    fn story_donor_requires_permanent_human_and_unique_character() {
        let mut targets = vec![record(0, 0xff, 0); 54];
        targets[0] = record(1, 0, 1);
        targets[1] = record(0x80, 1, 0);
        let story = record(0x25, 5, 37);
        assert!(story_candidate_allowed(&targets, &story, 5, 2));
        assert!(!story_candidate_allowed(&targets, &story, 50, 2));
        assert!(!story_candidate_allowed(
            &targets,
            &record(0x80, 5, 37),
            5,
            2
        ));
        let mut monster = story.clone();
        monster.job = 0x5e;
        assert!(!story_candidate_allowed(&targets, &monster, 5, 2));
        targets[1] = record(0x25, 1, 0);
        assert!(!story_candidate_allowed(&targets, &story, 5, 2));
    }

    #[test]
    fn guest_addition_preserves_partition_and_rejects_duplicates() {
        let mut targets = vec![record(0, 0xff, 0); 54];
        targets[0] = record(1, 0, 1);
        targets[1] = record(0x80, 1, 0);
        let guest = record(0x25, 50, 37);
        assert!(guest_addition_allowed(&targets, &guest, 50, 2));
        assert!(!guest_addition_allowed(&targets, &guest, 49, 2));
        targets[50] = guest.clone();
        assert!(!guest_addition_allowed(&targets, &guest, 50, 2));
        targets[50] = record(0, 0xff, 0);
        targets[1] = record(0x25, 1, 0);
        assert!(!guest_addition_allowed(&targets, &guest, 50, 2));
    }
}
