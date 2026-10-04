//! Explicit transactions for the currently loaded manual slot.

use super::*;
use ivalice_domain::equipment_rules::GearError;
use ivalice_domain::identity::{UnitSex, ZodiacSign};
use ivalice_domain::job_eligibility::level_from_total_jp;
use ivalice_domain::{game_data::SpoilerLevel, ValueState};
use ivalice_infrastructure::{
    replace_save_with_backup_if_unchanged, restore_save_from_backup_if_unchanged,
    AbilityFlagsLoader, JobRequirementsLoader, SaveEditError,
};
use ivalice_save_format::{
    edit_enhanced_png, edit_enhanced_png_with_abilities, edit_enhanced_png_with_jobs, BaseStatKind,
    EditOperation, EquippedSlot, GearSlot, GearSource, GilEditError,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SaveTransactionRequest {
    pub(super) snapshot_generation: u64,
    pub(super) manual_slot_id: u8,
    operations: Vec<RequestedOperation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum RequestedOperation {
    CreateCreature {
        form_key: String,
        unit_position: u8,
        name: String,
    },
    Gil {
        value: String,
    },
    InventoryQuantity {
        item_position: u16,
        quantity: String,
    },
    Gear {
        unit_position: u8,
        slot: GearSlot,
        item_key: String,
        source: Option<GearSource>,
    },
    CharacterLevel {
        unit_position: u8,
        value: String,
    },
    Experience {
        unit_position: u8,
        value: String,
    },
    BaseStat {
        unit_position: u8,
        stat: BaseStatKind,
        value: u32,
    },
    Bravery {
        unit_position: u8,
        value: String,
    },
    Faith {
        unit_position: u8,
        value: String,
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
        level: String,
        current_jp: String,
        total_jp: String,
    },
    LearnedAbility {
        unit_position: u8,
        job_slot: u8,
        ability_key: String,
        learned: bool,
    },
    EquippedSlot {
        unit_position: u8,
        combat_set: Option<u8>,
        slot: RequestedEquippedSlot,
        value_key: String,
    },
    CreateGenericFromSlot {
        source_slot: u8,
        source_position: u8,
        unit_position: u8,
        name: String,
    },
    AddStoryFromSlot {
        source_slot: u8,
        source_position: u8,
        unit_position: u8,
    },
    AddGuestFromSlot {
        source_slot: u8,
        source_position: u8,
        unit_position: u8,
    },
    AddNamedFromUnit {
        source_slot: u8,
        source_position: u8,
        unit_position: u8,
        character_key: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RequestedEquippedSlot {
    SecondaryCommand,
    Reaction,
    Support,
    Movement,
}

impl RequestedEquippedSlot {
    fn save_slot(self) -> EquippedSlot {
        match self {
            Self::SecondaryCommand => EquippedSlot::SecondaryCommand,
            Self::Reaction => EquippedSlot::Reaction,
            Self::Support => EquippedSlot::Support,
            Self::Movement => EquippedSlot::Movement,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveTransactionResponse {
    pub gil: u32,
    pub backup_created: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RestoreBackupRequest {
    snapshot_generation: u64,
    manual_slot_id: u8,
}

fn parse_gil(value: &str) -> Result<u32, IpcError> {
    if value.is_empty() || value.len() > 10 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_gil_request",
            false,
        ));
    }
    value
        .parse::<u32>()
        .map_err(|_| IpcError::simple(IpcErrorCategory::Input, "gil_out_of_range", false))
}

fn parse_item_id(digits: &str) -> Result<u16, IpcError> {
    if digits.is_empty()
        || digits.len() > 3
        || (digits.len() > 1 && digits.starts_with('0'))
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_gear_item",
            false,
        ));
    }
    let id = digits
        .parse::<u16>()
        .map_err(|_| IpcError::simple(IpcErrorCategory::Input, "invalid_gear_item", false))?;
    if !(1..=260).contains(&id) || matches!(id, 254 | 255) {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_gear_item",
            false,
        ));
    }
    Ok(id)
}

fn parse_unit_stat(value: &str, position: u8) -> Result<u8, IpcError> {
    // TICSaveEditor.Core/Records/UnitSaveData.cs at 07ea857 validates both
    // persistent StartBcp and StartFaith at 0..=100.
    if position >= 50
        || value.is_empty()
        || value.len() > 3
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_unit_stat",
            false,
        ));
    }
    let parsed = value
        .parse::<u8>()
        .map_err(|_| IpcError::simple(IpcErrorCategory::Input, "invalid_unit_stat", false))?;
    if parsed > 100 {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_unit_stat",
            false,
        ));
    }
    Ok(parsed)
}

fn parse_job_number<T: std::str::FromStr>(value: &str, max: usize) -> Result<T, IpcError> {
    if value.is_empty() || value.len() > max || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_job_progress",
            false,
        ));
    }
    value
        .parse::<T>()
        .map_err(|_| IpcError::simple(IpcErrorCategory::Input, "invalid_job_progress", false))
}

pub(super) fn parse_transaction_request(
    value: serde_json::Value,
) -> Result<(SaveTransactionRequest, Vec<EditOperation>), IpcError> {
    let request: SaveTransactionRequest = serde_json::from_value(value)
        .map_err(|_| IpcError::simple(IpcErrorCategory::Input, "invalid_edit_request", false))?;
    if !(1..=MAX_SAFE_GENERATION).contains(&request.snapshot_generation)
        || request.manual_slot_id >= 50
        || request.operations.len() > 1024
    {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_edit_request",
            false,
        ));
    }
    let operations = request
        .operations
        .iter()
        .map(|operation| match operation {
            RequestedOperation::Gil { value } => {
                parse_gil(value).map(|value| EditOperation::Gil { value })
            }
            RequestedOperation::InventoryQuantity {
                item_position,
                quantity,
            } => {
                if *item_position >= 261 || matches!(item_position, 0 | 254 | 255) {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_item_position",
                        false,
                    ));
                }
                if quantity.is_empty()
                    || quantity.len() > 2
                    || !quantity.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_item_quantity",
                        false,
                    ));
                }
                let value = quantity.parse::<u8>().map_err(|_| {
                    IpcError::simple(IpcErrorCategory::Input, "invalid_item_quantity", false)
                })?;
                if value > 99 {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_item_quantity",
                        false,
                    ));
                }
                Ok(EditOperation::InventoryQuantity {
                    item_position: *item_position,
                    quantity: value,
                })
            }
            RequestedOperation::Gear {
                unit_position,
                slot,
                item_key,
                source,
            } => {
                if *unit_position >= 50 || (item_key.is_empty() != source.is_none()) {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_gear_request",
                        false,
                    ));
                }
                let item_id = if item_key.is_empty() {
                    None
                } else {
                    let digits = item_key.strip_prefix("item:").ok_or_else(|| {
                        IpcError::simple(IpcErrorCategory::Input, "invalid_gear_item", false)
                    })?;
                    let id = parse_item_id(digits)?;
                    Some(id)
                };
                Ok(EditOperation::Gear {
                    unit_position: *unit_position,
                    slot: *slot,
                    item_id,
                    source: *source,
                })
            }
            RequestedOperation::CharacterLevel {
                unit_position,
                value,
            }
            | RequestedOperation::Experience {
                unit_position,
                value,
            } => {
                let level = matches!(operation, RequestedOperation::CharacterLevel { .. });
                let parsed = parse_job_number::<u8>(value, 2)?;
                if *unit_position >= 50 || parsed > 99 || (level && parsed == 0) {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_character_progress",
                        false,
                    ));
                }
                Ok(if level {
                    EditOperation::CharacterLevel {
                        unit_position: *unit_position,
                        value: parsed,
                    }
                } else {
                    EditOperation::Experience {
                        unit_position: *unit_position,
                        value: parsed,
                    }
                })
            }
            RequestedOperation::BaseStat {
                unit_position,
                stat,
                value,
            } => {
                if *unit_position >= 50 || *value > ivalice_domain::reader::stat_edit::MAX_BASE {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_base_stat",
                        false,
                    ));
                }
                Ok(EditOperation::BaseStat {
                    unit_position: *unit_position,
                    stat: *stat,
                    value: *value,
                })
            }
            RequestedOperation::Bravery {
                unit_position,
                value,
            } => parse_unit_stat(value, *unit_position).map(|value| EditOperation::Bravery {
                unit_position: *unit_position,
                value,
            }),
            RequestedOperation::Faith {
                unit_position,
                value,
            } => parse_unit_stat(value, *unit_position).map(|value| EditOperation::Faith {
                unit_position: *unit_position,
                value,
            }),
            RequestedOperation::Zodiac {
                unit_position,
                sign,
            } => {
                if *unit_position >= 54 {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_zodiac_unit",
                        false,
                    ));
                }
                Ok(EditOperation::Zodiac {
                    unit_position: *unit_position,
                    sign: *sign,
                })
            }
            RequestedOperation::Sex { unit_position, sex } => {
                if *unit_position >= 50 {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_sex_unit",
                        false,
                    ));
                }
                Ok(EditOperation::Sex {
                    unit_position: *unit_position,
                    sex: *sex,
                })
            }
            RequestedOperation::JobProgress {
                unit_position,
                job_slot,
                level,
                current_jp,
                total_jp,
            } => {
                let level: u8 = parse_job_number(level, 1)?;
                let current_jp: u16 = parse_job_number(current_jp, 5)?;
                let total_jp: u16 = parse_job_number(total_jp, 5)?;
                if *unit_position >= 50
                    || *job_slot >= 20
                    || level > 8
                    || level_from_total_jp(total_jp) != level
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_job_progress",
                        false,
                    ));
                }
                Ok(EditOperation::JobProgress {
                    unit_position: *unit_position,
                    job_slot: *job_slot,
                    level,
                    current_jp,
                    total_jp,
                })
            }
            RequestedOperation::LearnedAbility {
                unit_position,
                job_slot,
                ability_key,
                learned,
            } => {
                let Some(ability_id) = ability_key
                    .strip_prefix("ability:")
                    .and_then(|digits| digits.parse::<u16>().ok())
                else {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_learned_ability",
                        false,
                    ));
                };
                if *unit_position >= 50
                    || *job_slot >= 20
                    || !(1..=511).contains(&ability_id)
                    || ability_key != &format!("ability:{ability_id}")
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_learned_ability",
                        false,
                    ));
                }
                Ok(EditOperation::LearnedAbility {
                    unit_position: *unit_position,
                    job_slot: *job_slot,
                    ability_id,
                    learned: *learned,
                })
            }
            RequestedOperation::EquippedSlot {
                unit_position,
                combat_set,
                slot,
                value_key,
            } => {
                let prefix = if matches!(slot, RequestedEquippedSlot::SecondaryCommand) {
                    "command:"
                } else {
                    "ability:"
                };
                let value = if value_key.is_empty() {
                    0
                } else {
                    let id = value_key
                        .strip_prefix(prefix)
                        .and_then(|digits| digits.parse::<u16>().ok())
                        .ok_or_else(|| {
                            IpcError::simple(
                                IpcErrorCategory::Input,
                                "invalid_equipped_slot",
                                false,
                            )
                        })?;
                    if value_key != &format!("{prefix}{id}") {
                        return Err(IpcError::simple(
                            IpcErrorCategory::Input,
                            "invalid_equipped_slot",
                            false,
                        ));
                    }
                    id
                };
                if *unit_position >= 50
                    || combat_set.is_some_and(|index| index >= 3)
                    || value > 511
                    || (matches!(slot, RequestedEquippedSlot::SecondaryCommand) && value > 175)
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_equipped_slot",
                        false,
                    ));
                }
                Ok(EditOperation::EquippedSlot {
                    unit_position: *unit_position,
                    combat_set: *combat_set,
                    slot: slot.save_slot(),
                    value,
                })
            }
            RequestedOperation::CreateCreature {
                form_key,
                unit_position,
                name,
            } => {
                let form_id = form_key
                    .strip_prefix("creature:")
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|id| {
                        form_key == &format!("creature:{id}")
                            && ivalice_save_format::creature_form(*id).is_some()
                    })
                    .ok_or_else(|| {
                        IpcError::simple(IpcErrorCategory::Input, "invalid_creature", false)
                    })?;
                if !(1..50).contains(unit_position)
                    || name.len() > 15
                    || !name.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
                    || (!name.is_empty() && name.trim().is_empty())
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_creature",
                        false,
                    ));
                }
                let mut nickname = [0; 16];
                nickname[..name.len()].copy_from_slice(name.as_bytes());
                Ok(EditOperation::CreateCreature {
                    form_id,
                    unit_position: *unit_position,
                    nickname,
                })
            }
            RequestedOperation::CreateGenericFromSlot {
                source_slot,
                source_position,
                unit_position,
                name,
            } => {
                if *source_slot >= 50
                    || *source_slot == request.manual_slot_id
                    || !(1..50).contains(source_position)
                    || !(1..50).contains(unit_position)
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_generic_source",
                        false,
                    ));
                }
                let bytes = name.as_bytes();
                if bytes.is_empty()
                    || bytes.len() > 15
                    || !bytes.iter().all(|byte| (0x20..=0x7e).contains(byte))
                    || !bytes.iter().any(|byte| *byte != b' ')
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_generic_name",
                        false,
                    ));
                }
                let mut nickname = [0_u8; 16];
                nickname[..bytes.len()].copy_from_slice(bytes);
                Ok(EditOperation::CreateGenericFromSlot {
                    source_slot: *source_slot,
                    source_position: *source_position,
                    unit_position: *unit_position,
                    nickname,
                })
            }
            RequestedOperation::AddStoryFromSlot {
                source_slot,
                source_position,
                unit_position,
            } => {
                if *source_slot >= 50
                    || *source_slot == request.manual_slot_id
                    || !(1..50).contains(source_position)
                    || !(1..50).contains(unit_position)
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_story_source",
                        false,
                    ));
                }
                Ok(EditOperation::AddStoryFromSlot {
                    source_slot: *source_slot,
                    source_position: *source_position,
                    unit_position: *unit_position,
                })
            }
            RequestedOperation::AddGuestFromSlot {
                source_slot,
                source_position,
                unit_position,
            } => {
                if *source_slot >= 50
                    || *source_slot == request.manual_slot_id
                    || !(50..54).contains(source_position)
                    || !(1..50).contains(unit_position)
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_guest_source",
                        false,
                    ));
                }
                Ok(EditOperation::AddGuestFromSlot {
                    source_slot: *source_slot,
                    source_position: *source_position,
                    unit_position: *unit_position,
                })
            }
            RequestedOperation::AddNamedFromUnit {
                source_slot,
                source_position,
                unit_position,
                character_key,
            } => {
                let character = character_key
                    .strip_prefix("character_name:")
                    .and_then(|value| value.parse::<u8>().ok())
                    .filter(|value| {
                        ivalice_save_format::named_human_initialization_supported(*value)
                            && character_key == &format!("character_name:{value}")
                    })
                    .ok_or_else(|| {
                        IpcError::simple(IpcErrorCategory::Input, "invalid_named_identity", false)
                    })?;
                if *source_slot >= 50 || *source_position >= 50 || !(1..50).contains(unit_position)
                {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_named_source",
                        false,
                    ));
                }
                Ok(EditOperation::AddNamedFromUnit {
                    source_slot: *source_slot,
                    source_position: *source_position,
                    unit_position: *unit_position,
                    character,
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut fields = std::collections::BTreeSet::new();
    for operation in &operations {
        let field = match operation {
            EditOperation::CopyUnitFromSlot { .. } | EditOperation::RebindUnitFromSlot { .. } => {
                return Err(IpcError::simple(
                    IpcErrorCategory::Input,
                    "invalid_edit_request",
                    false,
                ));
            }
            EditOperation::CreateCreature { unit_position, .. }
            | EditOperation::CreateGenericFromSlot { unit_position, .. } => {
                700_000 + u32::from(*unit_position)
            }
            EditOperation::AddStoryFromSlot { unit_position, .. } => {
                if operations.len() != 1 {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_edit_request",
                        false,
                    ));
                }
                710_000 + u32::from(*unit_position)
            }
            EditOperation::AddGuestFromSlot { unit_position, .. } => {
                if operations.len() != 1 {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "invalid_edit_request",
                        false,
                    ));
                }
                720_000 + u32::from(*unit_position)
            }
            EditOperation::AddNamedFromUnit { unit_position, .. } => {
                700_000 + u32::from(*unit_position)
            }
            EditOperation::Gil { .. } => 1000,
            EditOperation::InventoryQuantity { item_position, .. } => u32::from(*item_position),
            EditOperation::Gear {
                unit_position,
                slot,
                ..
            } => {
                800_000
                    + u32::from(*unit_position) * 5
                    + match slot {
                        GearSlot::RightHand => 0,
                        GearSlot::LeftHand => 1,
                        GearSlot::Head => 2,
                        GearSlot::Body => 3,
                        GearSlot::Accessory => 4,
                    }
            }
            EditOperation::CharacterLevel { unit_position, .. } => {
                900_000 + u32::from(*unit_position) * 7
            }
            EditOperation::Experience { unit_position, .. } => {
                900_001 + u32::from(*unit_position) * 7
            }
            EditOperation::BaseStat {
                unit_position,
                stat,
                ..
            } => {
                900_002
                    + u32::from(*unit_position) * 7
                    + match stat {
                        BaseStatKind::Hp => 0,
                        BaseStatKind::Mp => 1,
                        BaseStatKind::Speed => 2,
                        BaseStatKind::PhysicalAttack => 3,
                        BaseStatKind::MagicalAttack => 4,
                    }
            }
            EditOperation::Bravery { unit_position, .. } => 261 + u32::from(*unit_position) * 2,
            EditOperation::Faith { unit_position, .. } => 262 + u32::from(*unit_position) * 2,
            EditOperation::Zodiac { unit_position, .. } => 750_000 + u32::from(*unit_position),
            EditOperation::Sex { unit_position, .. } => 760_000 + u32::from(*unit_position),
            EditOperation::JobProgress {
                unit_position,
                job_slot,
                ..
            } => 1001 + u32::from(*unit_position) * 20 + u32::from(*job_slot),
            EditOperation::LearnedAbility {
                unit_position,
                job_slot,
                ability_id,
                ..
            } => {
                2001 + u32::from(*unit_position) * 20 * 512
                    + u32::from(*job_slot) * 512
                    + u32::from(*ability_id)
            }
            EditOperation::EquippedSlot {
                unit_position,
                combat_set,
                slot,
                ..
            } => {
                600_000
                    + u32::from(*unit_position) * 16
                    + u32::from(combat_set.unwrap_or(3)) * 4
                    + match slot {
                        EquippedSlot::SecondaryCommand => 0,
                        EquippedSlot::Reaction => 1,
                        EquippedSlot::Support => 2,
                        EquippedSlot::Movement => 3,
                    }
            }
        };
        if !fields.insert(field) {
            return Err(IpcError::simple(
                IpcErrorCategory::Input,
                "duplicate_edit_field",
                false,
            ));
        }
    }
    Ok((request, operations))
}

#[tauri::command]
pub async fn save_transaction(
    request: serde_json::Value,
    state: State<'_, DesktopState>,
) -> Result<SaveTransactionResponse, IpcError> {
    let (request, operations) = parse_transaction_request(request)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_selected_operations(
            &state,
            request.snapshot_generation,
            request.manual_slot_id,
            &operations,
        )
    })
    .await
    .map_err(|_| worker_error())?
}

pub(super) fn save_selected_operations(
    state: &DesktopState,
    generation: u64,
    slot: u8,
    operations: &[EditOperation],
) -> Result<SaveTransactionResponse, IpcError> {
    if operations.iter().any(|operation| {
        matches!(
            operation,
            EditOperation::CopyUnitFromSlot { .. } | EditOperation::RebindUnitFromSlot { .. }
        )
    }) {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_edit_request",
            false,
        ));
    }
    let _serial = lock_recover(&state.load_serial);
    ensure_current(state, generation)?;
    let loaded = lock_recover(&state.loaded_edit)
        .clone()
        .ok_or_else(IpcError::stale)?;
    if loaded.generation != generation || loaded.slot != slot {
        return Err(IpcError::stale());
    }
    let settings = state
        .settings()?
        .load()
        .map_err(map_settings_error)?
        .ok_or_else(no_selection)?;
    if selected_candidate(state, &settings)? != loaded.path {
        return Err(IpcError::stale());
    }
    let snapshot = state
        .reader
        .acquire(&loaded.path, &CancellationToken::default())
        .map_err(map_snapshot_error)?;
    if snapshot.sha256() != loaded.sha256 {
        return Err(IpcError::simple(
            IpcErrorCategory::Snapshot,
            "save_changed_since_load",
            false,
        ));
    }
    ensure_current(state, generation)?;
    let gil = operations
        .iter()
        .find_map(|operation| match operation {
            EditOperation::Gil { value } => Some(*value),
            EditOperation::InventoryQuantity { .. }
            | EditOperation::Gear { .. }
            | EditOperation::CharacterLevel { .. }
            | EditOperation::Experience { .. }
            | EditOperation::BaseStat { .. }
            | EditOperation::Bravery { .. }
            | EditOperation::Faith { .. }
            | EditOperation::Zodiac { .. }
            | EditOperation::Sex { .. }
            | EditOperation::JobProgress { .. }
            | EditOperation::LearnedAbility { .. }
            | EditOperation::EquippedSlot { .. }
            | EditOperation::CopyUnitFromSlot { .. }
            | EditOperation::RebindUnitFromSlot { .. }
            | EditOperation::CreateGenericFromSlot { .. }
            | EditOperation::AddStoryFromSlot { .. }
            | EditOperation::AddGuestFromSlot { .. } => None,
            EditOperation::AddNamedFromUnit { .. } | EditOperation::CreateCreature { .. } => None,
        })
        .unwrap_or(loaded.gil);
    if operations.is_empty() {
        return Ok(SaveTransactionResponse {
            gil,
            backup_created: false,
        });
    }
    let replacement = prepare_replacement(state, &loaded, snapshot.bytes(), operations)?;
    if replacement == snapshot.bytes() {
        return Ok(SaveTransactionResponse {
            gil,
            backup_created: false,
        });
    }
    ensure_current(state, generation)?;
    let backup = replace_save_with_backup_if_unchanged(
        &loaded.path,
        loaded.sha256,
        snapshot.bytes(),
        &replacement,
    )
    .map_err(map_save_edit_error)?;
    *lock_recover(&state.last_backup) = Some(BackupReceipt {
        path: loaded.path,
        slot,
        edited_sha256: backup.edited_sha256(),
        backup,
    });
    state.cancel_load();
    Ok(SaveTransactionResponse {
        gil,
        backup_created: true,
    })
}

/// The same catalogue and operation validation is used by previews and save writes.
pub(super) fn prepare_replacement(
    state: &DesktopState,
    loaded: &LoadedEdit,
    input: &[u8],
    operations: &[EditOperation],
) -> Result<Vec<u8>, IpcError> {
    with_edit_resources(
        state,
        loaded,
        operations,
        |dictionary, jobs, abilities| match (jobs, abilities) {
            (Some(jobs), Some(abilities)) => edit_enhanced_png_with_abilities(
                input,
                dictionary,
                loaded.slot,
                operations,
                jobs,
                abilities,
            ),
            (Some(jobs), None) => {
                edit_enhanced_png_with_jobs(input, dictionary, loaded.slot, operations, jobs)
            }
            (None, None) => edit_enhanced_png(input, dictionary, loaded.slot, operations),
            (None, Some(_)) => Err(ivalice_save_format::GilEditError::InvalidField),
        },
    )
}

pub(super) fn prepare_preview(
    state: &DesktopState,
    loaded: &LoadedEdit,
    input: &[u8],
    operations: &[EditOperation],
) -> Result<ivalice_save_format::DecodedContainer, IpcError> {
    with_edit_resources(state, loaded, operations, |dictionary, jobs, abilities| {
        ivalice_save_format::preview_enhanced_png(
            input,
            dictionary,
            loaded.slot,
            operations,
            jobs,
            abilities,
        )
    })
}

fn with_edit_resources<T>(
    state: &DesktopState,
    loaded: &LoadedEdit,
    operations: &[EditOperation],
    action: impl FnOnce(
        &[u8],
        Option<&ivalice_domain::job_eligibility::ValidatedJobRequirements>,
        Option<&ivalice_domain::ability_flags::ValidatedAbilityFlags>,
    ) -> Result<T, ivalice_save_format::GilEditError>,
) -> Result<T, IpcError> {
    if operations.iter().any(|operation| {
        matches!(
            operation,
            EditOperation::InventoryQuantity { .. }
                | EditOperation::Gear { .. }
                | EditOperation::BaseStat { .. }
        )
    }) {
        let token = loaded
            .catalogue_token
            .as_ref()
            .ok_or_else(IpcError::stale)?;
        let loader = state
            .catalogue
            .as_ref()
            .map_err(|error| reader::map_catalogue_error(*error))?;
        let resource = loader
            .load(&CancellationToken::default())
            .map_err(reader::map_catalogue_error)?;
        let actual_token: String = resource
            .sha256()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if &actual_token != token {
            return Err(IpcError::stale());
        }
        for operation in operations {
            let item_position = match operation {
                EditOperation::InventoryQuantity { item_position, .. } => Some(*item_position),
                EditOperation::Gear { item_id, .. } => *item_id,
                _ => None,
            };
            if let Some(item_position) = item_position {
                let known = resource
                    .lookup(&format!("item:{item_position}"), SpoilerLevel::Full)
                    .is_some_and(|item| matches!(item.label, ValueState::Known(_)));
                if !known {
                    return Err(IpcError::simple(
                        IpcErrorCategory::Input,
                        "unnamed_item",
                        false,
                    ));
                }
            }
        }
        loader
            .load_matching(resource.sha256(), &CancellationToken::default())
            .map_err(|_| IpcError::stale())?;
    }
    let dictionary = read_dictionary(&state.dictionary_path())?;
    let has_abilities = operations.iter().any(|operation| {
        matches!(
            operation,
            EditOperation::LearnedAbility { .. } | EditOperation::EquippedSlot { .. }
        )
    });
    let graph = if has_abilities
        || operations
            .iter()
            .any(|operation| matches!(operation, EditOperation::JobProgress { .. }))
    {
        Some(
            JobRequirementsLoader::load(&state.resource_root).map_err(|_| {
                IpcError::simple(
                    IpcErrorCategory::Resource,
                    "job_requirements_unavailable",
                    false,
                )
            })?,
        )
    } else {
        None
    };
    let abilities = if has_abilities {
        Some(AbilityFlagsLoader::load(&state.resource_root).map_err(|_| {
            IpcError::simple(
                IpcErrorCategory::Resource,
                "ability_flags_unavailable",
                false,
            )
        })?)
    } else {
        None
    };
    action(&dictionary, graph.as_ref(), abilities.as_ref()).map_err(map_gil_edit_error)
}

#[tauri::command]
pub async fn restore_last_backup(
    request: serde_json::Value,
    state: State<'_, DesktopState>,
) -> Result<(), IpcError> {
    let request: RestoreBackupRequest = serde_json::from_value(request)
        .map_err(|_| IpcError::simple(IpcErrorCategory::Input, "invalid_restore_request", false))?;
    if !(1..=MAX_SAFE_GENERATION).contains(&request.snapshot_generation)
        || request.manual_slot_id >= 50
    {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_restore_request",
            false,
        ));
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || restore_selected(&state, request))
        .await
        .map_err(|_| worker_error())?
}

fn restore_selected(state: &DesktopState, request: RestoreBackupRequest) -> Result<(), IpcError> {
    let _serial = lock_recover(&state.load_serial);
    ensure_current(state, request.snapshot_generation)?;
    let loaded = lock_recover(&state.loaded_edit)
        .clone()
        .ok_or_else(IpcError::stale)?;
    let receipt = lock_recover(&state.last_backup)
        .clone()
        .ok_or_else(IpcError::stale)?;
    if loaded.generation != request.snapshot_generation
        || loaded.slot != request.manual_slot_id
        || loaded.path != receipt.path
        || loaded.slot != receipt.slot
        || loaded.sha256 != receipt.edited_sha256
    {
        return Err(IpcError::stale());
    }
    let settings = state
        .settings()?
        .load()
        .map_err(map_settings_error)?
        .ok_or_else(no_selection)?;
    if selected_candidate(state, &settings)? != loaded.path {
        return Err(IpcError::stale());
    }
    restore_save_from_backup_if_unchanged(&loaded.path, loaded.sha256, &receipt.backup)
        .map_err(map_save_edit_error)?;
    *lock_recover(&state.last_backup) = None;
    state.cancel_load();
    Ok(())
}

pub(super) fn map_gil_edit_error(error: GilEditError) -> IpcError {
    match error {
        GilEditError::Container(error) => map_container_error(error),
        GilEditError::Manual(error) => map_manual_error(error),
        GilEditError::StoredChecksum => {
            IpcError::simple(IpcErrorCategory::Corrupt, "stored_checksum", false)
        }
        GilEditError::Encoding => {
            IpcError::simple(IpcErrorCategory::Internal, "gil_encode_failed", false)
        }
        GilEditError::DuplicateField => {
            IpcError::simple(IpcErrorCategory::Input, "duplicate_edit_field", false)
        }
        GilEditError::InvalidField => {
            IpcError::simple(IpcErrorCategory::Input, "invalid_edit_field", false)
        }
        GilEditError::EquippedAbility => {
            IpcError::simple(IpcErrorCategory::Input, "equipped_ability", false)
        }
        GilEditError::Gear(error) => {
            let code = match error {
                GearError::StockUnderflow => "equipment_stock_exhausted",
                GearError::StockOverflow => "equipment_stock_full",
                GearError::JobCannotEquip => "equipment_job_restricted",
                GearError::WrongSlot => "equipment_wrong_slot",
                GearError::HandConflict => "equipment_hand_conflict",
                GearError::UnverifiedRestriction => "equipment_rule_unavailable",
                GearError::UnknownItem | GearError::UnknownJob => "equipment_data_unavailable",
                GearError::DuplicateField => "duplicate_edit_field",
                GearError::NoChange | GearError::InvalidSource => "invalid_gear_request",
            };
            IpcError::simple(IpcErrorCategory::Input, code, false)
        }
    }
}

fn map_save_edit_error(error: SaveEditError) -> IpcError {
    match error {
        SaveEditError::UnsafePath => {
            IpcError::simple(IpcErrorCategory::Selection, "unsafe_save_path", false)
        }
        SaveEditError::Changed => {
            IpcError::simple(IpcErrorCategory::Snapshot, "save_changed_since_load", false)
        }
        SaveEditError::TooLarge => {
            IpcError::simple(IpcErrorCategory::Limit, "save_too_large", false)
        }
        SaveEditError::BackupFailed => {
            IpcError::simple(IpcErrorCategory::Snapshot, "save_backup_failed", false)
        }
        SaveEditError::RecoveryRequired => {
            IpcError::simple(IpcErrorCategory::Snapshot, "save_recovery_required", false)
        }
        SaveEditError::Io => {
            IpcError::simple(IpcErrorCategory::Snapshot, "save_write_failed", true)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn progression_operations_are_independent_strict_and_duplicate_checked() {
        use serde_json::json;
        let request =
            |operations| json!({"snapshotGeneration":1,"manualSlotId":0,"operations":operations});
        let operations = json!([
            {"kind":"character_level","unitPosition":0,"value":"10"},
            {"kind":"experience","unitPosition":0,"value":"73"},
            {"kind":"base_stat","unitPosition":0,"stat":"hp","value":9830400}
        ]);
        let (_, parsed) = parse_transaction_request(request(operations))
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(
            parsed,
            vec![
                EditOperation::CharacterLevel {
                    unit_position: 0,
                    value: 10
                },
                EditOperation::Experience {
                    unit_position: 0,
                    value: 73
                },
                EditOperation::BaseStat {
                    unit_position: 0,
                    stat: super::BaseStatKind::Hp,
                    value: 9_830_400
                }
            ]
        );
        for invalid in [
            json!({"kind":"character_level","unitPosition":0,"value":"0"}),
            json!({"kind":"experience","unitPosition":0,"value":"100"}),
            json!({"kind":"base_stat","unitPosition":0,"stat":"hp","value":16777216}),
            json!({"kind":"base_stat","unitPosition":0,"stat":"hp","value":-1}),
            json!({"kind":"base_stat","unitPosition":0,"stat":"hp","value":1.5}),
            json!({"kind":"character_level","unitPosition":50,"value":"10"}),
            json!({"kind":"character_level","unitPosition":0,"value":"10","exp":"0"}),
        ] {
            assert!(parse_transaction_request(request(json!([invalid]))).is_err());
        }
        let base = json!({"kind":"base_stat","unitPosition":0,"stat":"hp","value":1});
        assert_eq!(
            parse_transaction_request(request(json!([base.clone(), base])))
                .err()
                .map(|error| error.code),
            Some("duplicate_edit_field")
        );
    }

    #[test]
    fn creature_requests_are_explicit_bounded_and_share_addition_capacity() {
        let valid =
            json!({"kind":"create_creature","formKey":"creature:65","unitPosition":13,"name":""});
        let request =
            |operations| json!({"snapshotGeneration":1,"manualSlotId":31,"operations":operations});
        assert!(parse_transaction_request(request(json!([valid.clone()]))).is_ok());
        assert!(parse_transaction_request(request(json!([valid.clone(), valid.clone()]))).is_err());
        for invalid in [
            json!({"kind":"create_creature","formKey":"creature:074","unitPosition":13,"name":""}),
            json!({"kind":"create_creature","formKey":"creature:74","unitPosition":13,"name":""}),
            json!({"kind":"create_creature","formKey":"creature:65","unitPosition":0,"name":""}),
            json!({"kind":"create_creature","formKey":"creature:65","unitPosition":50,"name":""}),
            json!({"kind":"create_creature","formKey":"creature:65","unitPosition":13,"name":"0123456789012345"}),
            json!({"kind":"create_creature","formKey":"creature:65","unitPosition":13,"name":"  "}),
        ] {
            assert!(parse_transaction_request(request(json!([invalid]))).is_err());
        }
        assert!(parse_transaction_request(request(json!([valid,
            {"kind":"add_named_from_unit","sourceSlot":31,"sourcePosition":0,"unitPosition":13,"characterKey":"character_name:50"}
        ]))).is_err());
    }

    use super::{parse_transaction_request, EditOperation, UnitSex, ZodiacSign};

    #[test]
    fn additions_and_existing_units_share_one_request_but_not_targets() {
        use serde_json::json;
        let addition = json!({"kind":"create_generic_from_slot", "sourceSlot":37, "sourcePosition":12, "unitPosition":13, "name":"Mira"});
        let named = json!({"kind":"add_named_from_unit", "sourceSlot":31, "sourcePosition":0, "unitPosition":14, "characterKey":"character_name:50"});
        assert!(
            parse_transaction_request(json!({"snapshotGeneration":1, "manualSlotId":31,
            "operations":[addition.clone(), named.clone(),
                {"kind":"bravery", "unitPosition":13, "value":"75"},
                {"kind":"faith", "unitPosition":14, "value":"25"},
                {"kind":"faith", "unitPosition":0, "value":"70"},
                {"kind":"gil", "value":"200"}]}))
            .is_ok()
        );
        for operations in [
            json!([addition.clone(), addition.clone()]),
            json!([addition, {"kind":"add_named_from_unit", "sourceSlot":31, "sourcePosition":0, "unitPosition":13, "characterKey":"character_name:50"}]),
        ] {
            assert!(parse_transaction_request(
                json!({"snapshotGeneration":1, "manualSlotId":31, "operations":operations})
            )
            .is_err());
        }
    }
    use serde_json::json;

    #[test]
    fn mixed_fields_parse_to_exact_selected_slot_operations() -> Result<(), super::IpcError> {
        let (request, operations) = parse_transaction_request(json!({
            "snapshotGeneration":1, "manualSlotId":31,
            "operations":[
                {"kind":"add_named_from_unit","sourceSlot":37,"sourcePosition":12,"unitPosition":13,"characterKey":"character_name:32"},
                {"kind":"gil","value":"1"},
                {"kind":"bravery","unitPosition":0,"value":"1"},
                {"kind":"faith","unitPosition":1,"value":"2"},
                {"kind":"job_progress","unitPosition":0,"jobSlot":12,"level":"2","currentJp":"0","totalJp":"200"}
            ]
        }))?;
        assert_eq!(request.manual_slot_id, 31);
        assert_eq!(
            operations,
            vec![
                EditOperation::AddNamedFromUnit {
                    source_slot: 37,
                    source_position: 12,
                    unit_position: 13,
                    character: 32
                },
                EditOperation::Gil { value: 1 },
                EditOperation::Bravery {
                    unit_position: 0,
                    value: 1
                },
                EditOperation::Faith {
                    unit_position: 1,
                    value: 2
                },
                EditOperation::JobProgress {
                    unit_position: 0,
                    job_slot: 12,
                    level: 2,
                    current_jp: 0,
                    total_jp: 200
                }
            ]
        );
        Ok(())
    }

    #[test]
    fn zodiac_wire_values_and_destinations_are_explicit() {
        for sign in ZodiacSign::ALL {
            for position in [0, 49, 50, 53] {
                assert_eq!(
                    parse_transaction_request(json!({"snapshotGeneration":1,"manualSlotId":31,
                    "operations":[{"kind":"zodiac","unitPosition":position,"sign":sign}]}))
                    .map(|(_, operations)| operations),
                    Ok(vec![EditOperation::Zodiac {
                        unit_position: position,
                        sign
                    }])
                );
            }
        }
        for operation in [
            json!({"kind":"zodiac","unitPosition":54,"sign":"aries"}),
            json!({"kind":"zodiac","unitPosition":0,"sign":"Aries"}),
            json!({"kind":"zodiac","unitPosition":0,"sign":"unknown"}),
            json!({"kind":"zodiac","unitPosition":0,"sign":"aries","birthday":1}),
        ] {
            assert!(parse_transaction_request(
                json!({"snapshotGeneration":1,"manualSlotId":31,"operations":[operation]})
            )
            .is_err());
        }
        assert!(parse_transaction_request(json!({"snapshotGeneration":1,"manualSlotId":31,"operations":[
            {"kind":"zodiac","unitPosition":0,"sign":"aries"},{"kind":"zodiac","unitPosition":0,"sign":"pisces"}]})).is_err());
    }

    #[test]
    fn generic_sex_wire_values_and_destinations_are_explicit() {
        for sex in UnitSex::ALL {
            assert_eq!(
                parse_transaction_request(json!({"snapshotGeneration":1,"manualSlotId":31,
                    "operations":[{"kind":"sex","unitPosition":13,"sex":sex}]}))
                .map(|(_, operations)| operations),
                Ok(vec![EditOperation::Sex {
                    unit_position: 13,
                    sex
                }])
            );
        }
        for operation in [
            json!({"kind":"sex","unitPosition":50,"sex":"female"}),
            json!({"kind":"sex","unitPosition":13,"sex":"unknown"}),
            json!({"kind":"sex","unitPosition":13,"sex":"Female"}),
            json!({"kind":"sex","unitPosition":13,"sex":"male","character":128}),
        ] {
            assert!(
                parse_transaction_request(json!({"snapshotGeneration":1,"manualSlotId":31,
                "operations":[operation]}))
                .is_err()
            );
        }
        assert!(
            parse_transaction_request(json!({"snapshotGeneration":1,"manualSlotId":31,
            "operations":[{"kind":"sex","unitPosition":13,"sex":"male"},
                          {"kind":"sex","unitPosition":13,"sex":"female"}]}))
            .is_err()
        );
    }

    #[test]
    fn creature_overlay_requests_fail_before_any_file_write() {
        for character in [60, 62, 64, 65, 67, 69, 72, 73, 115, 116, 117, 118, 119] {
            assert!(parse_transaction_request(json!({"snapshotGeneration":1,"manualSlotId":31,
                "operations":[{"kind":"add_named_from_unit","sourceSlot":31,"sourcePosition":0,"unitPosition":13,"characterKey":format!("character_name:{character}")}] })).is_err());
        }
    }

    #[test]
    fn transaction_gil_request_boundaries() {
        for value in ["0", "4294967295"] {
            assert!(parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 49,
                "operations": [{"kind": "gil", "value": value}]
            }))
            .is_ok());
        }
        for value in ["", "-1", "1.5", "4294967296", " 12", "12 ", "1e3"] {
            assert!(parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 0,
                "operations": [{"kind": "gil", "value": value}]
            }))
            .is_err());
        }
        for invalid in [
            json!({"snapshotGeneration": 0, "manualSlotId": 0, "operations": [{"kind": "gil", "value": "1"}]}),
            json!({"snapshotGeneration": 1, "manualSlotId": 50, "operations": [{"kind": "gil", "value": "1"}]}),
            json!({"snapshotGeneration": 1, "manualSlotId": 0, "operations": [{"kind": "gil", "value": "1"}], "path": "x"}),
        ] {
            assert!(parse_transaction_request(invalid).is_err());
        }
    }

    #[test]
    fn equipment_request_is_explicit_and_bounded() {
        for source in ["held", "create_and_equip"] {
            let request = json!({
                "snapshotGeneration": 1,
                "manualSlotId": 0,
                "operations": [{"kind":"gear","unitPosition":0,"slot":"right_hand","itemKey":"item:256","source":source}]
            });
            assert!(parse_transaction_request(request).is_ok());
        }
        for operation in [
            json!({"kind":"gear","unitPosition":0,"slot":"right_hand","itemKey":"","source":null}),
            json!({"kind":"gear","unitPosition":0,"slot":"head","itemKey":"item:0","source":"held"}),
            json!({"kind":"gear","unitPosition":0,"slot":"head","itemKey":"item:261","source":"held"}),
            json!({"kind":"gear","unitPosition":0,"slot":"head","itemKey":"item:0256","source":"held"}),
            json!({"kind":"gear","unitPosition":0,"slot":"head","itemKey":"item:256","source":null}),
            json!({"kind":"gear","unitPosition":50,"slot":"head","itemKey":"item:256","source":"held"}),
            json!({"kind":"gear","unitPosition":0,"slot":"preset","itemKey":"item:256","source":"held"}),
        ] {
            let valid_empty = operation["itemKey"] == "";
            assert_eq!(
                parse_transaction_request(json!({
                    "snapshotGeneration": 1,
                    "manualSlotId": 0,
                    "operations": [operation],
                }))
                .is_ok(),
                valid_empty,
            );
        }
    }

    #[test]
    fn transaction_rejects_unbounded_or_unverified_operations() {
        assert_eq!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 49,
                "operations": [{"kind": "gil", "value": "4294967295"}]
            }))
            .map(|(_, operations)| operations.len()),
            Ok(1)
        );
        assert_eq!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 0,
                "operations": [{"kind": "inventory_quantity", "itemPosition": 1, "quantity": "99"}]
            }))
            .map(|(_, operations)| operations.len()),
            Ok(1)
        );
        assert_eq!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 0,
                "operations": [{"kind": "bravery", "unitPosition": 0, "value": "0"}, {"kind": "faith", "unitPosition": 0, "value": "100"}]
            })).map(|(_, operations)| operations.len()),
            Ok(2)
        );
        assert_eq!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 9,
                "operations": [{"kind": "job_progress", "unitPosition": 0, "jobSlot": 12, "level": "2", "currentJp": "178", "totalJp": "200"}]
            })).map(|(_, operations)| operations.len()),
            Ok(1)
        );
        assert_eq!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 9,
                "operations": [{"kind": "job_progress", "unitPosition": 0, "jobSlot": 12, "level": "8", "currentJp": "4000", "totalJp": "3000"}]
            })).map(|(_, operations)| operations.len()),
            Ok(1)
        );
        assert_eq!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 33,
                "operations": [{"kind": "learned_ability", "unitPosition": 0,
                    "jobSlot": 13, "abilityKey": "ability:394", "learned": true}]
            }))
            .map(|(_, operations)| operations.len()),
            Ok(1)
        );
        assert_eq!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 33,
                "operations": [{"kind": "equipped_slot", "unitPosition": 0,
                    "combatSet": null, "slot": "reaction", "valueKey": "ability:427"}]
            }))
            .map(|(_, operations)| operations.len()),
            Ok(1)
        );
        assert!(matches!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 31,
                "operations": [{"kind": "create_generic_from_slot", "sourceSlot": 37,
                    "sourcePosition": 12, "unitPosition": 13, "name": "Mira"}]
            })),
            Ok((_, operations)) if matches!(&operations[..],
                [EditOperation::CreateGenericFromSlot { nickname, .. }]
                if &nickname[..5] == b"Mira\0")
        ));
        assert!(matches!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 31,
                "operations": [{"kind": "add_story_from_slot", "sourceSlot": 37,
                    "sourcePosition": 12, "unitPosition": 13}]
            })),
            Ok((_, operations)) if matches!(&operations[..],
                [EditOperation::AddStoryFromSlot { source_slot: 37, source_position: 12, unit_position: 13 }])
        ));
        assert!(matches!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 31,
                "operations": [{"kind": "add_guest_from_slot", "sourceSlot": 37,
                    "sourcePosition": 50, "unitPosition": 13}]
            })),
            Ok((_, operations)) if matches!(&operations[..],
                [EditOperation::AddGuestFromSlot { source_slot: 37, source_position: 50, unit_position: 13 }])
        ));
        assert!(matches!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 31,
                "operations": [{"kind": "add_named_from_unit", "sourceSlot": 37,
                    "sourcePosition": 12, "unitPosition": 13,
                    "characterKey": "character_name:32"}]
            })),
            Ok((_, operations)) if matches!(&operations[..],
                [EditOperation::AddNamedFromUnit { source_slot: 37, source_position: 12, unit_position: 13, character: 32 }])
        ));
        assert!(matches!(
            parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 31,
                "operations": [{"kind": "add_named_from_unit", "sourceSlot": 31,
                    "sourcePosition": 0, "unitPosition": 13,
                    "characterKey": "character_name:32"}]
            })),
            Ok((_, operations)) if matches!(&operations[..],
                [EditOperation::AddNamedFromUnit { source_slot: 31, source_position: 0, unit_position: 13, character: 32 }])
        ));
        for operations in [
            json!([{"kind":"add_named_from_unit","sourceSlot":37,"sourcePosition":12,"unitPosition":13,"characterKey":"character_name:53"}]),
            json!([{"kind":"add_named_from_unit","sourceSlot":37,"sourcePosition":12,"unitPosition":13,"characterKey":"character_name:032"}]),
            json!([{"kind":"add_named_from_unit","sourceSlot":50,"sourcePosition":12,"unitPosition":13,"characterKey":"character_name:32"}]),
            json!([{"kind":"add_named_from_unit","sourceSlot":37,"sourcePosition":50,"unitPosition":13,"characterKey":"character_name:32"}]),
            json!([{"kind":"add_guest_from_slot","sourceSlot":0,"sourcePosition":50,"unitPosition":13}]),
            json!([{"kind":"add_guest_from_slot","sourceSlot":37,"sourcePosition":49,"unitPosition":13}]),
            json!([{"kind":"add_guest_from_slot","sourceSlot":37,"sourcePosition":54,"unitPosition":13}]),
            json!([{"kind":"add_guest_from_slot","sourceSlot":37,"sourcePosition":50,"unitPosition":50}]),
            json!([{"kind":"add_guest_from_slot","sourceSlot":37,"sourcePosition":50,"unitPosition":13}, {"kind":"gil","value":"1"}]),
            json!([{"kind":"add_story_from_slot","sourceSlot":0,"sourcePosition":12,"unitPosition":13}]),
            json!([{"kind":"add_story_from_slot","sourceSlot":37,"sourcePosition":50,"unitPosition":13}]),
            json!([{"kind":"add_story_from_slot","sourceSlot":37,"sourcePosition":12,"unitPosition":13}, {"kind":"gil","value":"1"}]),
            json!([{"kind":"create_generic_from_slot","sourceSlot":0,"sourcePosition":12,"unitPosition":13,"name":"Mira"}]),
            json!([{"kind":"create_generic_from_slot","sourceSlot":50,"sourcePosition":12,"unitPosition":13,"name":"Mira"}]),
            json!([{"kind":"create_generic_from_slot","sourceSlot":37,"sourcePosition":0,"unitPosition":13,"name":"Mira"}]),
            json!([{"kind":"create_generic_from_slot","sourceSlot":37,"sourcePosition":12,"unitPosition":13,"name":""}]),
            json!([{"kind":"create_generic_from_slot","sourceSlot":37,"sourcePosition":12,"unitPosition":13,"name":"Åsa"}]),
            json!([{"kind":"create_generic_from_slot","sourceSlot":37,"sourcePosition":12,"unitPosition":13,"name":"1234567890123456"}]),
            json!([{"kind":"equipped_slot","unitPosition":0,"combatSet":3,"slot":"reaction","valueKey":"ability:427"}]),
            json!([{"kind":"equipped_slot","unitPosition":0,"combatSet":null,"slot":"reaction","valueKey":"command:21"}]),
            json!([{"kind":"equipped_slot","unitPosition":0,"combatSet":null,"slot":"reaction","valueKey":"ability:0427"}]),
            json!([{"kind":"equipped_slot","unitPosition":0,"combatSet":null,"slot":"secondary_command","valueKey":"command:176"}]),
            json!([{"kind":"equipped_slot","unitPosition":0,"combatSet":null,"slot":"reaction","valueKey":"ability:427"},{"kind":"equipped_slot","unitPosition":0,"combatSet":null,"slot":"reaction","valueKey":""}]),
            json!([{"kind":"learned_ability","unitPosition":0,"jobSlot":13,"abilityKey":"ability:0394","learned":true}]),
            json!([{"kind":"learned_ability","unitPosition":0,"jobSlot":20,"abilityKey":"ability:394","learned":true}]),
            json!([{"kind":"learned_ability","unitPosition":0,"jobSlot":13,"abilityKey":"ability:394","learned":true},
                {"kind":"learned_ability","unitPosition":0,"jobSlot":13,"abilityKey":"ability:394","learned":false}]),
            json!([{"kind": "gil", "value": "4294967296"}]),
            json!([{"kind": "gil", "value": "1", "offset": 0}]),
            json!([{"kind": "unknown", "value": "1"}]),
            json!([{"kind": "gil", "value": "1"}, {"kind": "gil", "value": "2"}]),
            json!([{"kind": "inventory_quantity", "itemPosition": 0, "quantity": "1"}]),
            json!([{"kind": "inventory_quantity", "itemPosition": 254, "quantity": "1"}]),
            json!([{"kind": "inventory_quantity", "itemPosition": 255, "quantity": "1"}]),
            json!([{"kind": "inventory_quantity", "itemPosition": 261, "quantity": "1"}]),
            json!([{"kind": "inventory_quantity", "itemPosition": 1, "quantity": "100"}]),
            json!([{"kind": "inventory_quantity", "itemPosition": 1, "quantity": "-1"}]),
            json!([{"kind": "inventory_quantity", "itemPosition": 1, "quantity": "1"}, {"kind": "inventory_quantity", "itemPosition": 1, "quantity": "2"}]),
            json!([{"kind": "bravery", "unitPosition": 50, "value": "1"}]),
            json!([{"kind": "faith", "unitPosition": 0, "value": "101"}]),
            json!([{"kind": "bravery", "unitPosition": 0, "value": "-1"}]),
            json!([{"kind": "bravery", "unitPosition": 0, "value": "1"}, {"kind": "bravery", "unitPosition": 0, "value": "2"}]),
            json!([{"kind": "job_progress", "unitPosition": 50, "jobSlot": 12, "level": "2", "currentJp": "0", "totalJp": "200"}]),
            json!([{"kind": "job_progress", "unitPosition": 0, "jobSlot": 20, "level": "2", "currentJp": "0", "totalJp": "200"}]),
            json!([{"kind": "job_progress", "unitPosition": 0, "jobSlot": 12, "level": "9", "currentJp": "0", "totalJp": "3000"}]),
            json!([{"kind": "job_progress", "unitPosition": 0, "jobSlot": 12, "level": "2", "currentJp": "65536", "totalJp": "200"}]),
            json!([{"kind": "job_progress", "unitPosition": 0, "jobSlot": 12, "level": "2", "currentJp": "0", "totalJp": "100"}]),
            json!([{"kind": "job_progress", "unitPosition": 0, "jobSlot": 12, "level": "2", "currentJp": "0", "totalJp": "200"}, {"kind": "job_progress", "unitPosition": 0, "jobSlot": 12, "level": "2", "currentJp": "0", "totalJp": "200"}]),
        ] {
            assert!(parse_transaction_request(json!({
                "snapshotGeneration": 1,
                "manualSlotId": 0,
                "operations": operations
            }))
            .is_err());
        }
    }
}
