//! Native reader projection from one immutable save decode and a bound offline catalogue.

use super::*;
use ivalice_domain::{
    ability_flags::{AbilityKind, LoadoutChoice, LoadoutKind, ValidatedAbilityFlags},
    equipment_facts::EquipmentFacts,
    equipment_rules::{GearSlot, GearSource, GearUnit},
    game_data::reader_catalogue::ReaderCatalogueError,
    game_data::SpoilerLevel,
    job_eligibility::JOB_LEVEL_TOTAL_JP,
    reader::{ReaderDocument, ReaderIdentity, ValidatedReader},
    story_roster::RosterUnit,
    ValueState,
};
use ivalice_infrastructure::{
    AbilityFlagsLoader, AchievementsLoader, ErrandsLoader, JobRequirementsLoader,
    ReaderCatalogueLoadError, ReaderCatalogueLoadErrorCode, ReaderCatalogueResource,
    StoryProgressLoader, StoryRosterLoader,
};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadReaderRequest {
    pub request_id: u64,
    pub manual_slot_id: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadReaderResponse {
    pub request_id: u64,
    pub occupied_manual_slots: Vec<ManualSlotId>,
    pub slot_summaries: Vec<SlotSummary>,
    pub save: Option<NormalizedSave>,
    pub reader: Option<ReaderDocument>,
    pub reader_error: Option<IpcError>,
    pub edit_context: Option<EditContext>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditContext {
    pub ability_descriptions: std::collections::BTreeMap<String, String>,
    pub snapshot_generation: u64,
    pub manual_slot_id: u8,
    pub gil: u32,
    pub story_step: i32,
    pub story_choices: Vec<StoryStepChoice>,
    pub calendar: Option<CalendarDate>,
    pub achievements: Vec<AchievementState>,
    pub side_quests: Vec<SideQuestState>,
    pub errands: Vec<ErrandInfo>,
    pub collection: Vec<CollectionInfo>,
    /// Active unit positions whose saved errand byte (InTrip) is set.
    pub units_on_errands: Vec<u8>,
    pub backup_available: bool,
    pub job_options: Vec<EditJobOptions>,
    pub zodiac_options: Vec<EditZodiacOptions>,
    pub sex_options: Vec<EditSexOptions>,
    pub ability_options: Vec<EditAbilityOptions>,
    pub loadout_options: Vec<EditLoadoutOptions>,
    pub gear_options: Vec<EditGearOptions>,
    pub job_level_total_jp: [u16; 9],
    pub generic_creation: GenericCreationContext,
    pub story_addition: GenericCreationContext,
    pub guest_addition: GenericCreationContext,
    pub named_addition: NamedAdditionContext,
    pub creature_addition: CreatureAdditionContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryStepChoice {
    pub progress: i32,
    pub chapter: Option<String>,
    pub objective: Option<String>,
    /// Guests the step edit rebuilds, in guest-position order.
    pub guests: Vec<String>,
    /// Story members the step edit adds to or removes from the saved party.
    pub joins: Vec<String>,
    pub leaves: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalendarDate {
    pub month: u8,
    pub day: u8,
    /// Days in each month, January first, for choosing a valid day.
    pub month_lengths: [u8; 12],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrandInfo {
    pub index: u8,
    pub title: String,
    pub client: String,
    pub posting: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionInfo {
    pub index: u8,
    pub kind: ivalice_domain::errands::CollectionKind,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SideQuestState {
    pub name: String,
    pub scenes: Vec<SideQuestSceneState>,
    pub counter: Option<SideQuestCounterState>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SideQuestCounterState {
    pub label: String,
    pub value: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SideQuestSceneState {
    pub label: String,
    pub seen: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AchievementState {
    pub index: u8,
    pub description: String,
    pub unlocked: bool,
    /// Raw FftoAchievement progress counter; its unit depends on the award.
    pub progress: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditZodiacOptions {
    pub unit_position: u8,
    pub current: ivalice_domain::identity::ZodiacSign,
    pub choices: Vec<ZodiacChoice>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ZodiacChoice {
    pub key: ivalice_domain::identity::ZodiacSign,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditSexOptions {
    pub unit_position: u8,
    pub current: ivalice_domain::identity::UnitSex,
    pub choices: Vec<SexChoice>,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SexChoice {
    pub key: ivalice_domain::identity::UnitSex,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatureAdditionContext {
    pub target_position: Option<u8>,
    pub monsters: Vec<NamedCharacterChoice>,
    pub enemies: Vec<NamedCharacterChoice>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NamedAdditionContext {
    pub target_position: Option<u8>,
    pub donors: Vec<GenericDonorChoice>,
    pub characters: Vec<NamedCharacterChoice>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NamedCharacterChoice {
    pub key: String,
    pub label: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenericCreationContext {
    pub target_position: Option<u8>,
    pub donors: Vec<GenericDonorChoice>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenericDonorChoice {
    pub source_slot: u8,
    pub source_position: u8,
    pub unit_position: u8,
    pub label: String,
    pub level: u8,
    pub sex: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditJobOptions {
    pub unit_position: u8,
    pub job_slots: Vec<u8>,
    pub locked_jobs: Vec<LockedJob>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockedJob {
    pub job_slot: u8,
    pub requires: Vec<RequiredJobLevel>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequiredJobLevel {
    pub job_slot: u8,
    pub level: u8,
    pub current_level: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditAbilityOptions {
    pub unit_position: u8,
    pub job_slot: u8,
    pub command_key: String,
    pub command_label: String,
    pub members: Vec<EditAbilityOption>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditAbilityOption {
    pub key: String,
    pub label: String,
    pub learned: bool,
    pub equipped: bool,
    pub kind: AbilityKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditLoadoutOptions {
    pub unit_position: u8,
    pub secondary_choices: Vec<LoadoutChoice>,
    pub reaction_choices: Vec<LoadoutChoice>,
    pub support_choices: Vec<LoadoutChoice>,
    pub movement_choices: Vec<LoadoutChoice>,
    pub slots: Vec<EditLoadoutSlot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditLoadoutSlot {
    pub combat_set: Option<u8>,
    pub slot: LoadoutKind,
    pub current_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditGearOptions {
    pub unit_position: u8,
    pub slots: Vec<EditGearSlot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditGearSlot {
    pub slot: GearSlot,
    pub current_key: String,
    pub choices: Vec<GearChoice>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GearChoice {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlotSummary {
    pub manual_slot: ManualSlotId,
    pub title: Option<String>,
    pub saved_at_unix_seconds: Option<i64>,
    pub play_time_seconds: Option<u64>,
}

fn parse_request(value: serde_json::Value) -> Result<LoadReaderRequest, IpcError> {
    let request: LoadReaderRequest = serde_json::from_value(value)
        .map_err(|_| IpcError::simple(IpcErrorCategory::Input, "invalid_reader_request", false))?;
    if !(1..=MAX_SAFE_GENERATION).contains(&request.request_id) {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_reader_request",
            false,
        ));
    }
    if request.manual_slot_id.is_some_and(|slot| slot >= 50) {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "manual_slot_out_of_range",
            false,
        ));
    }
    Ok(request)
}

#[tauri::command]
pub async fn load_reader(
    request: serde_json::Value,
    state: State<'_, DesktopState>,
) -> Result<LoadReaderResponse, IpcError> {
    let request = parse_request(request)?;
    let state = state.inner().clone();
    let generation = state.begin_load();
    if generation > MAX_SAFE_GENERATION {
        return Err(generation_exhausted());
    }
    tauri::async_runtime::spawn_blocking(move || load_selected_reader(&state, generation, request))
        .await
        .map_err(|_| worker_error())?
}

fn load_selected_reader(
    state: &DesktopState,
    generation: u64,
    request: LoadReaderRequest,
) -> Result<LoadReaderResponse, IpcError> {
    let settings = state
        .settings()?
        .load()
        .map_err(map_settings_error)?
        .ok_or_else(no_selection)?;
    let path = selected_candidate(state, &settings)?;
    let _serial = lock_recover(&state.load_serial);
    ensure_current(state, generation)?;
    let snapshot = state
        .reader
        .acquire(&path, &CancellationToken::default())
        .map_err(map_snapshot_error)?;
    ensure_current(state, generation)?;
    let dictionary = read_dictionary(&state.dictionary_path())?;
    let decoded =
        decode_enhanced_png(snapshot.bytes(), Some(&dictionary)).map_err(map_container_error)?;
    project_selected_reader(state, generation, request, &snapshot, &path, &decoded)
}

pub(super) fn project_selected_reader(
    state: &DesktopState,
    generation: u64,
    request: LoadReaderRequest,
    snapshot: &ivalice_infrastructure::Snapshot,
    path: &Path,
    decoded: &ivalice_save_format::DecodedContainer,
) -> Result<LoadReaderResponse, IpcError> {
    let occupied_manual_slots = decoded.occupied_manual_slots().map_err(map_manual_error)?;
    let slot_summaries = occupied_manual_slots
        .iter()
        .map(|slot| {
            let metadata = decoded
                .slot_metadata(slot.index())
                .map_err(map_manual_error)?
                .ok_or_else(|| {
                    IpcError::simple(IpcErrorCategory::Internal, "occupied_slot_missing", false)
                })?;
            // TICSaveEditor.Core/Save/SaveSlot.cs and Sections/{Card,Info} at 07ea857.
            Ok(SlotSummary {
                manual_slot: *slot,
                title: metadata.readable_title(),
                saved_at_unix_seconds: (metadata.saved_at_unix_seconds > 0)
                    .then(|| i64::from(metadata.saved_at_unix_seconds)),
                // Independent analysis: the upstream playtime field mirrors scripted progress.
                play_time_seconds: None,
            })
        })
        .collect::<Result<Vec<_>, IpcError>>()?;
    let mut response = LoadReaderResponse {
        request_id: request.request_id,
        occupied_manual_slots,
        slot_summaries,
        save: None,
        reader: None,
        reader_error: None,
        edit_context: None,
    };
    if let Some(slot) = request.manual_slot_id {
        let metadata = decoded.slot_metadata(slot).map_err(map_manual_error)?;
        let mut generic_donors = Vec::new();
        let mut named_bases = Vec::new();
        let mut story_donors = Vec::new();
        let mut guest_donors = Vec::new();
        if let Some(metadata) = metadata {
            let targets = decoded.unit_records(slot).map_err(map_manual_error)?;
            let target_position = targets.as_ref().and_then(|records| {
                (1..50)
                    .find(|position| !records[*position].is_active(*position))
                    .and_then(|position| u8::try_from(position).ok())
            });
            generic_donors = decoded
                .generic_creation_donors(slot)
                .map_err(map_manual_error)?;
            named_bases = decoded
                .named_creation_bases(slot)
                .map_err(map_manual_error)?;
            story_donors = decoded
                .story_addition_donors(slot)
                .map_err(map_manual_error)?;
            guest_donors = decoded
                .guest_addition_donors(slot)
                .map_err(map_manual_error)?;
            let mut donors = generic_donors
                .iter()
                .map(|donor| {
                    let sex = if donor.record.sex & 0x80 != 0 {
                        "Male"
                    } else {
                        "Female"
                    };
                    GenericDonorChoice {
                        source_slot: donor.source_slot,
                        source_position: donor.source_position,
                        unit_position: donor.unit_position,
                        label: format!(
                            "{sex} · Lv {} · save slot {}, member {}",
                            donor.record.level,
                            u16::from(donor.source_slot) + 1,
                            u16::from(donor.source_position) + 1
                        ),
                        level: donor.record.level,
                        sex: sex.into(),
                    }
                })
                .collect::<Vec<_>>();
            donors.sort_by(|left, right| {
                right
                    .source_slot
                    .cmp(&left.source_slot)
                    .then(left.source_position.cmp(&right.source_position))
            });
            let mut named_donors = named_bases
                .iter()
                .map(|base| GenericDonorChoice {
                    source_slot: base.source_slot,
                    source_position: base.source_position,
                    unit_position: base.unit_position,
                    label: format!(
                        "Starting record · save slot {}, member {}",
                        u16::from(base.source_slot) + 1,
                        u16::from(base.source_position) + 1
                    ),
                    level: base.record.level,
                    sex: if base.record.sex & 0x80 != 0 {
                        "Male".into()
                    } else {
                        "Female".into()
                    },
                })
                .collect::<Vec<_>>();
            named_donors.sort_by(|left, right| {
                (left.source_position == 0)
                    .cmp(&(right.source_position == 0))
                    .then(right.source_slot.cmp(&left.source_slot))
                    .then(left.source_position.cmp(&right.source_position))
            });
            let mut story_choices = story_donors
                .iter()
                .map(|donor| GenericDonorChoice {
                    source_slot: donor.source_slot,
                    source_position: donor.source_position,
                    unit_position: donor.unit_position,
                    label: format!(
                        "Story character · save slot {}, member {}",
                        u16::from(donor.source_slot) + 1,
                        u16::from(donor.source_position) + 1
                    ),
                    level: donor.record.level,
                    sex: if donor.record.sex & 0x80 != 0 {
                        "Male".into()
                    } else {
                        "Female".into()
                    },
                })
                .collect::<Vec<_>>();
            story_choices.sort_by(|left, right| {
                right
                    .source_slot
                    .cmp(&left.source_slot)
                    .then(left.source_position.cmp(&right.source_position))
            });
            let mut guest_choices = guest_donors
                .iter()
                .map(|donor| GenericDonorChoice {
                    source_slot: donor.source_slot,
                    source_position: donor.source_position,
                    unit_position: donor.unit_position,
                    label: format!(
                        "Guest character · save slot {}, guest {}",
                        u16::from(donor.source_slot) + 1,
                        u16::from(donor.source_position) - 49
                    ),
                    level: donor.record.level,
                    sex: if donor.record.sex & 0x80 != 0 {
                        "Male".into()
                    } else {
                        "Female".into()
                    },
                })
                .collect::<Vec<_>>();
            guest_choices.sort_by(|left, right| {
                right
                    .source_slot
                    .cmp(&left.source_slot)
                    .then(left.source_position.cmp(&right.source_position))
            });
            let job_options = JobRequirementsLoader::load(&state.resource_root)
                .ok()
                .and_then(|graph| {
                    decoded.unit_records(slot).ok().flatten().map(|units| {
                        units
                            .iter()
                            .enumerate()
                            .filter_map(|(position, unit)| {
                                if !unit.is_active(position) || (0x5e..=0x8d).contains(&unit.job) {
                                    return None;
                                }
                                super::job_preview::job_options(
                                    u8::try_from(position).ok()?,
                                    unit,
                                    &unit.job_levels,
                                    &graph,
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .unwrap_or_default();
            let ability_map = AbilityFlagsLoader::load(&state.resource_root).ok();
            let ability_options = ability_map
                .as_ref()
                .and_then(|map| {
                    decoded.unit_records(slot).ok().flatten().map(|units| {
                        job_options
                            .iter()
                            .flat_map(|option| {
                                let Some(unit) = units.get(usize::from(option.unit_position))
                                else {
                                    return Vec::new();
                                };
                                (0..20)
                                    .filter_map(|job_slot| {
                                        let job = map.job_for_save_slot(
                                            job_slot,
                                            unit.first_progress_job_id(),
                                        )?;
                                        let members = job
                                            .members
                                            .iter()
                                            .filter_map(|member| {
                                                Some(EditAbilityOption {
                                                    key: member.key.clone(),
                                                    label: member.label.clone(),
                                                    learned: unit
                                                        .ability_is_learned(job_slot, member.bit)?,
                                                    equipped: unit
                                                        .has_equipped_ability(member.ability_id()?),
                                                    kind: member.kind,
                                                })
                                            })
                                            .collect();
                                        Some(EditAbilityOptions {
                                            unit_position: option.unit_position,
                                            job_slot,
                                            command_key: format!("command:{}", job.command_id),
                                            command_label: job.command_label.clone(),
                                            members,
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .unwrap_or_default();
            let loadout_options = ability_map
                .as_ref()
                .and_then(|map| {
                    decoded.unit_records(slot).ok().flatten().map(|units| {
                        job_options
                            .iter()
                            .filter_map(|option| {
                                let unit = units.get(usize::from(option.unit_position))?;
                                Some(build_loadout_options(
                                    option.unit_position,
                                    unit,
                                    &option.job_slots,
                                    map,
                                ))
                            })
                            .collect()
                    })
                })
                .unwrap_or_default();
            let errands = ErrandsLoader::load(&state.resource_root).ok();
            response.edit_context = Some(EditContext {
                ability_descriptions: std::collections::BTreeMap::new(),
                snapshot_generation: generation,
                manual_slot_id: slot,
                gil: metadata.gil,
                story_step: metadata.story_progress[0],
                story_choices: StoryProgressLoader::load(&state.resource_root)
                    .map(|story| {
                        story
                            .steps()
                            .map(|progress| StoryStepChoice {
                                progress,
                                chapter: story.chapter(progress).map(Into::into),
                                objective: story.objective(progress).map(Into::into),
                                guests: Vec::new(),
                                joins: Vec::new(),
                                leaves: Vec::new(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                calendar: metadata
                    .calendar()
                    .filter(|(month, day)| {
                        ivalice_domain::calendar::day_of_year(*month, *day).is_some()
                    })
                    .map(|(month, day)| CalendarDate {
                        month,
                        day,
                        month_lengths: ivalice_domain::calendar::month_lengths(),
                    }),
                achievements: achievement_states(&state.resource_root, &metadata),
                side_quests: side_quest_states(&metadata),
                errands: errands.as_ref().map(errand_infos).unwrap_or_default(),
                collection: errands.as_ref().map(collection_infos).unwrap_or_default(),
                units_on_errands: targets
                    .as_ref()
                    .map(|units| {
                        units
                            .iter()
                            .enumerate()
                            .filter(|(position, unit)| {
                                unit.is_active(*position) && unit.in_trip != 0
                            })
                            .filter_map(|(position, _)| u8::try_from(position).ok())
                            .collect()
                    })
                    .unwrap_or_default(),
                backup_available: lock_recover(&state.last_backup).as_ref().is_some_and(
                    |receipt| {
                        receipt.path == path
                            && receipt.slot == slot
                            && receipt.edited_sha256 == snapshot.sha256()
                    },
                ),
                job_options,
                zodiac_options: targets
                    .as_ref()
                    .map(|units| {
                        units
                            .iter()
                            .enumerate()
                            .filter_map(|(position, unit)| {
                                if !unit.is_active(position) {
                                    return None;
                                }
                                Some(EditZodiacOptions {
                                    unit_position: u8::try_from(position).ok()?,
                                    current: unit.zodiac()?,
                                    choices: ivalice_domain::identity::ZodiacSign::ALL
                                        .into_iter()
                                        .map(|key| ZodiacChoice {
                                            key,
                                            label: key.label().into(),
                                        })
                                        .collect(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                sex_options: targets
                    .as_ref()
                    .map(|units| {
                        units
                            .iter()
                            .enumerate()
                            .filter_map(|(position, unit)| {
                                if position >= 50 || !unit.is_active(position) {
                                    return None;
                                }
                                let current = unit.generic_sex()?;
                                let choices = ivalice_domain::identity::UnitSex::ALL
                                    .into_iter()
                                    .filter(|sex| {
                                        *sex == current || unit.can_change_generic_sex_to(*sex)
                                    })
                                    .map(|key| SexChoice {
                                        key,
                                        label: key.label().into(),
                                    })
                                    .collect::<Vec<_>>();
                                Some(EditSexOptions {
                                    unit_position: u8::try_from(position).ok()?,
                                    current,
                                    unavailable_reason: (choices.len() == 1).then(|| {
                                        "Change Bard/Dancer job or commands before switching Sex."
                                            .into()
                                    }),
                                    choices,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                ability_options,
                loadout_options,
                gear_options: Vec::new(),
                job_level_total_jp: JOB_LEVEL_TOTAL_JP,
                generic_creation: GenericCreationContext {
                    target_position,
                    donors,
                },
                story_addition: GenericCreationContext {
                    target_position,
                    donors: story_choices,
                },
                guest_addition: GenericCreationContext {
                    target_position,
                    donors: guest_choices,
                },
                creature_addition: CreatureAdditionContext {
                    target_position,
                    monsters: Vec::new(),
                    enemies: Vec::new(),
                },
                named_addition: NamedAdditionContext {
                    target_position,
                    donors: named_donors,
                    characters: Vec::new(),
                },
            });
        }
        let byte_length = u64::try_from(snapshot.len())
            .map_err(|_| IpcError::simple(IpcErrorCategory::Limit, "snapshot_length", false))?;
        let byte_length = SnapshotByteLength::new(byte_length).map_err(|_| {
            IpcError::simple(IpcErrorCategory::Internal, "snapshot_invariant", false)
        })?;
        response.save = Some(
            decoded
                .normalized_manual_save(byte_length, slot)
                .map_err(map_manual_error)?,
        );
        let mut donor_labels = std::collections::BTreeMap::new();
        let mut named_choices = Vec::new();
        let projection = with_catalogue(state, generation, slot, |resource, identity| {
            if let Some(context) = &mut response.edit_context {
                context.ability_descriptions = context
                    .ability_options
                    .iter()
                    .flat_map(|option| {
                        std::iter::once(&option.command_key)
                            .chain(option.members.iter().map(|member| &member.key))
                    })
                    .chain(context.loadout_options.iter().flat_map(|option| {
                        option
                            .secondary_choices
                            .iter()
                            .chain(&option.reaction_choices)
                            .chain(&option.support_choices)
                            .chain(&option.movement_choices)
                            .map(|choice| &choice.key)
                    }))
                    .filter_map(|key| {
                        let entry = resource.lookup(key, SpoilerLevel::Full)?;
                        match entry.description {
                            ValueState::Known(text) if !text.trim().is_empty() => {
                                Some((key.clone(), text))
                            }
                            _ => None,
                        }
                    })
                    .collect();
            }
            let records = decoded.unit_records(slot).map_err(map_manual_error)?;
            if let (Some(context), Ok(roster)) = (
                &mut response.edit_context,
                StoryRosterLoader::load(&state.resource_root),
            ) {
                let name = |unit: &RosterUnit| {
                    resource
                        .lookup(&format!("character_name:{}", unit.name), SpoilerLevel::Full)
                        .and_then(|entry| match entry.label {
                            ValueState::Known(label) => Some(label),
                            _ => None,
                        })
                        .unwrap_or_else(|| "Unnamed unit".into())
                };
                let active: Vec<(u8, u16)> = records
                    .iter()
                    .flatten()
                    .enumerate()
                    .take(50)
                    .filter(|(position, unit)| unit.is_active(*position))
                    .map(|(_, unit)| (unit.character, unit.chara_name_key))
                    .collect();
                for choice in &mut context.story_choices {
                    choice.guests = roster.guests_at(choice.progress).map(name).collect();
                    let (joins, leaves) = roster.member_changes(choice.progress, &active);
                    choice.joins = joins.into_iter().map(name).collect();
                    choice.leaves = leaves.into_iter().map(name).collect();
                }
            }
            if let Some(context) = &mut response.edit_context {
                context.gear_options = records
                    .as_deref()
                    .map(|units| build_gear_options(units, resource))
                    .unwrap_or_default();
                for form in ivalice_save_format::creature_forms() {
                    let lookup = |key: String| {
                        resource.lookup(&key, SpoilerLevel::Full).and_then(|entry| {
                            match entry.label {
                                ValueState::Known(label) => Some(label),
                                _ => None,
                            }
                        })
                    };
                    let job =
                        lookup(format!("job:{}", form.job)).unwrap_or_else(|| "Creature".into());
                    let label = if form.name_key != 0 {
                        let name = lookup(format!("character_name:{}", form.name_key))
                            .unwrap_or_else(|| "Special creature".into());
                        format!("{name} \u{00b7} {job}")
                    } else if form.job >= 169 {
                        format!("{job} \u{00b7} encounter variant")
                    } else {
                        job
                    };
                    let available = form.can_create()
                        && records.as_ref().is_some_and(|units| {
                            units.iter().enumerate().all(|(index, unit)| {
                                !unit.is_active(index) || !form.collides_with(unit)
                            })
                        });
                    let choice = NamedCharacterChoice {
                        key: format!("creature:{}", form.id),
                        label,
                        available,
                        sex: None,
                        unavailable_reason: if !form.can_create() {
                            Some("game roster crash reported".into())
                        } else if !available {
                            Some("already in party".into())
                        } else {
                            None
                        },
                    };
                    match form.category() {
                        ivalice_save_format::CreatureCategory::Monster => {
                            context.creature_addition.monsters.push(choice)
                        }
                        ivalice_save_format::CreatureCategory::Enemy => {
                            context.creature_addition.enemies.push(choice)
                        }
                    }
                }
                context
                    .creature_addition
                    .monsters
                    .sort_by(|a, b| a.label.cmp(&b.label));
                context
                    .creature_addition
                    .enemies
                    .sort_by(|a, b| a.label.cmp(&b.label));
            }
            named_choices = ivalice_save_format::named_character_ids()
                .filter(|character| {
                    ivalice_save_format::named_human_initialization_supported(*character)
                })
                .filter_map(|character| {
                    let key = format!("character_name:{character}");
                    let label = resource
                        .lookup(&key, SpoilerLevel::Full)
                        .and_then(|entry| {
                            if let ValueState::Known(label) = entry.label {
                                Some(label)
                            } else {
                                None
                            }
                        })?;
                    let sex =
                        ivalice_save_format::named_character_sex(character).map(|sex| match sex {
                            ivalice_domain::identity::UnitSex::Male => "Male",
                            ivalice_domain::identity::UnitSex::Female => "Female",
                        });
                    let has_base = sex.is_some_and(|sex| {
                        response.edit_context.as_ref().is_some_and(|context| {
                            context
                                .named_addition
                                .donors
                                .iter()
                                .any(|donor| donor.sex == sex)
                        })
                    });
                    let available = has_base
                        && records.as_ref().is_some_and(|units| {
                            units.iter().enumerate().all(|(index, unit)| {
                                !unit.is_active(index)
                                    || (unit.character != character
                                        && unit.chara_name_key != u16::from(character))
                            })
                        });
                    Some(NamedCharacterChoice {
                        key,
                        label,
                        available,
                        sex: sex.map(str::to_owned),
                        unavailable_reason: if sex.is_none() {
                            Some("Sex not verified for this identity".into())
                        } else if !has_base {
                            Some("no matching starting record in this save".into())
                        } else if !available {
                            Some("already in party".into())
                        } else {
                            None
                        },
                    })
                })
                .collect();
            let mut totals = std::collections::BTreeMap::<String, usize>::new();
            for choice in &named_choices {
                *totals.entry(choice.label.clone()).or_default() += 1;
            }
            let mut seen = std::collections::BTreeMap::<String, usize>::new();
            for choice in &mut named_choices {
                if totals.get(&choice.label).copied().unwrap_or_default() > 1 {
                    let count = seen.entry(choice.label.clone()).or_default();
                    *count += 1;
                    choice.label = format!("{} · variant {}", choice.label, count);
                }
            }
            donor_labels = generic_donors
                .iter()
                .map(|donor| {
                    (
                        (donor.source_slot, donor.source_position),
                        donor_label(
                            &donor.record,
                            donor.source_slot,
                            donor.source_position,
                            resource,
                        ),
                    )
                })
                .chain(story_donors.iter().map(|donor| {
                    (
                        (donor.source_slot, donor.source_position),
                        donor_label(
                            &donor.record,
                            donor.source_slot,
                            donor.source_position,
                            resource,
                        ),
                    )
                }))
                .chain(named_bases.iter().map(|base| {
                    (
                        (base.source_slot, base.source_position),
                        donor_label(
                            &base.record,
                            base.source_slot,
                            base.source_position,
                            resource,
                        ),
                    )
                }))
                .chain(guest_donors.iter().map(|donor| {
                    (
                        (donor.source_slot, donor.source_position),
                        donor_label(
                            &donor.record,
                            donor.source_slot,
                            donor.source_position,
                            resource,
                        ),
                    )
                }))
                .collect();
            let reader = decoded
                .reader_manual_save_v2(identity, |id, ceiling| resource.lookup(id, ceiling))
                .map_err(map_manual_error)?;
            let mut document = reader.document().clone();
            ivalice_domain::reader::stats::apply_effective(&mut document, resource.mechanics());
            ivalice_domain::reader::commands::apply_primary_commands(&mut document, |job| {
                resource.command_for_job(job)
            });
            if let Ok(story) = StoryProgressLoader::load(&state.resource_root) {
                story.apply(&mut document.progress);
            }
            ValidatedReader::validate(document).map_err(|_| worker_error())
        })?;
        match projection {
            Ok(reader) => {
                if let Some(context) = &mut response.edit_context {
                    for donor in &mut context.generic_creation.donors {
                        if let Some(label) =
                            donor_labels.get(&(donor.source_slot, donor.source_position))
                        {
                            donor.label.clone_from(label);
                        }
                    }
                    for donor in &mut context.named_addition.donors {
                        if let Some(label) =
                            donor_labels.get(&(donor.source_slot, donor.source_position))
                        {
                            donor.label.clone_from(label);
                        }
                    }
                    context.named_addition.characters = named_choices;
                    for donor in &mut context.story_addition.donors {
                        if let Some(label) =
                            donor_labels.get(&(donor.source_slot, donor.source_position))
                        {
                            donor.label.clone_from(label);
                        }
                    }
                    for donor in &mut context.guest_addition.donors {
                        if let Some(label) =
                            donor_labels.get(&(donor.source_slot, donor.source_position))
                        {
                            donor.label.clone_from(label);
                        }
                    }
                }
                response.reader = Some(reader);
            }
            Err(error) => response.reader_error = Some(error),
        }
    }
    ensure_current(state, generation)?;
    if let Some(context) = &response.edit_context {
        *lock_recover(&state.loaded_edit) = Some(LoadedEdit {
            generation,
            path: path.to_path_buf(),
            sha256: snapshot.sha256(),
            slot: context.manual_slot_id,
            gil: context.gil,
            units: decoded
                .unit_records(context.manual_slot_id)
                .map_err(map_manual_error)?
                .unwrap_or_default()
                .to_vec(),
            catalogue_token: response.reader.as_ref().and_then(|reader| {
                match &reader.identity.resource_token {
                    ValueState::Known(token) => Some(token.clone()),
                    _ => None,
                }
            }),
        });
    }
    Ok(response)
}

fn donor_label(
    record: &ivalice_save_format::UnitRecord,
    source_slot: u8,
    source_position: u8,
    resource: &ReaderCatalogueResource,
) -> String {
    let nickname = record.nickname_raw.iter().take_while(|byte| **byte != 0);
    let nickname = nickname
        .map(|byte| {
            if byte.is_ascii() {
                char::from(*byte)
            } else {
                '?'
            }
        })
        .collect::<String>();
    let name = if !nickname.is_empty() {
        Some(nickname)
    } else {
        [record.name_no, record.chara_name_key]
            .into_iter()
            .filter(|key| *key != 0)
            .find_map(|key| {
                resource
                    .lookup(&format!("character_name:{key}"), SpoilerLevel::Full)
                    .and_then(|reference| match reference.label {
                        ValueState::Known(label) if !label.trim().is_empty() => Some(label),
                        _ => None,
                    })
            })
    }
    .unwrap_or_else(|| "Generic".into());
    let job = resource
        .lookup(&format!("job:{}", record.job), SpoilerLevel::Full)
        .and_then(|reference| match reference.label {
            ValueState::Known(label) if !label.trim().is_empty() => Some(label),
            _ => None,
        })
        .unwrap_or_else(|| "Job unknown".into());
    let sex = if record.sex & 0x80 != 0 {
        "Male"
    } else {
        "Female"
    };
    let position = if source_position >= 50 {
        format!("guest {}", u16::from(source_position) - 49)
    } else {
        format!("member {}", u16::from(source_position) + 1)
    };
    format!(
        "{name} · {job} · {sex} · Lv {} · save slot {}, {position}",
        record.level,
        u16::from(source_slot) + 1
    )
}

fn build_gear_options(
    units: &[ivalice_save_format::UnitRecord],
    resource: &ivalice_infrastructure::ReaderCatalogueResource,
) -> Vec<EditGearOptions> {
    let Ok(facts) = EquipmentFacts::bundled() else {
        return Vec::new();
    };
    let named_items = (1..=260_u16)
        .filter_map(|item_id| {
            let key = format!("item:{item_id}");
            let entry = resource.lookup(&key, SpoilerLevel::Full)?;
            let ValueState::Known(label) = entry.label else {
                return None;
            };
            Some((item_id, GearChoice { key, label }))
        })
        .collect::<Vec<_>>();
    units
        .iter()
        .enumerate()
        .take(50)
        .filter_map(|(position, record)| {
            if !record.is_active(position) || (0x5e..=0x8d).contains(&record.job) {
                return None;
            }
            let unit = GearUnit {
                job_id: u16::from(record.job),
                support_ability: record.support_ability,
                equipment: record.active_equipment(),
            };
            let slots = GearSlot::ALL
                .into_iter()
                .map(|slot| {
                    let current = unit.current(slot);
                    let choices = named_items
                        .iter()
                        .filter(|(item_id, _)| {
                            current == Some(*item_id)
                                || unit
                                    .plan(facts, slot, Some(*item_id), Some(GearSource::Held))
                                    .is_ok()
                        })
                        .map(|(_, choice)| choice.clone())
                        .collect();
                    EditGearSlot {
                        slot,
                        current_key: current.map_or_else(String::new, |id| format!("item:{id}")),
                        choices,
                    }
                })
                .collect();
            Some(EditGearOptions {
                unit_position: u8::try_from(position).ok()?,
                slots,
            })
        })
        .collect()
}

fn build_loadout_options(
    position: u8,
    unit: &ivalice_save_format::UnitRecord,
    job_slots: &[u8],
    map: &ValidatedAbilityFlags,
) -> EditLoadoutOptions {
    let choices =
        map.compatible_loadout_choices(unit.first_progress_job_id(), job_slots, |slot, bit| {
            unit.ability_is_learned(slot, bit) == Some(true)
        });
    let mut slots = Vec::with_capacity(16);
    for combat_set in [None, Some(0), Some(1), Some(2)] {
        for kind in [
            LoadoutKind::SecondaryCommand,
            LoadoutKind::Reaction,
            LoadoutKind::Support,
            LoadoutKind::Movement,
        ] {
            let value = match combat_set {
                None => match kind {
                    LoadoutKind::SecondaryCommand => u16::from(unit.secondary_action),
                    LoadoutKind::Reaction => unit.reaction_ability,
                    LoadoutKind::Support => unit.support_ability,
                    LoadoutKind::Movement => unit.movement_ability,
                },
                Some(index) => {
                    let set = &unit.combat_sets[index];
                    match kind {
                        LoadoutKind::SecondaryCommand => {
                            u16::try_from(set.skillsets[1]).unwrap_or(0)
                        }
                        LoadoutKind::Reaction => set.abilities[0],
                        LoadoutKind::Support => set.abilities[1],
                        LoadoutKind::Movement => set.abilities[2],
                    }
                }
            };
            slots.push(EditLoadoutSlot {
                combat_set: combat_set.and_then(|index| u8::try_from(index).ok()),
                slot: kind,
                current_key: if value == 0 {
                    String::new()
                } else if kind == LoadoutKind::SecondaryCommand {
                    format!("command:{value}")
                } else {
                    format!("ability:{value}")
                },
            });
        }
    }
    EditLoadoutOptions {
        unit_position: position,
        secondary_choices: choices.secondary,
        reaction_choices: choices.reaction,
        support_choices: choices.support,
        movement_choices: choices.movement,
        slots,
    }
}

/// Initial provisioning failures leave the V5 baseline usable. Once a resource is acquired,
/// every failure invalidates publication of the result, including labels already projected.
pub(super) fn with_catalogue(
    state: &DesktopState,
    generation: u64,
    slot: u8,
    project: impl FnOnce(&ReaderCatalogueResource, ReaderIdentity) -> Result<ValidatedReader, IpcError>,
) -> Result<Result<ReaderDocument, IpcError>, IpcError> {
    ensure_current(state, generation)?;
    let resource_generation = next_generation(&state.resource_generation);
    if resource_generation > MAX_SAFE_GENERATION {
        return Err(generation_exhausted());
    }
    let loader = match &state.catalogue {
        Ok(loader) => loader,
        Err(error) => return Ok(Err(map_catalogue_error(*error))),
    };
    let resource = match loader.load(&CancellationToken::default()) {
        Ok(resource) => resource,
        Err(error) => {
            ensure_current(state, generation)?;
            if matches!(
                error.code,
                ReaderCatalogueLoadErrorCode::Stale
                    | ReaderCatalogueLoadErrorCode::Snapshot(
                        SnapshotErrorCode::SourceChanged
                            | SnapshotErrorCode::RetryExhausted
                            | SnapshotErrorCode::Cancelled
                    )
            ) {
                return Err(IpcError::stale());
            }
            return Ok(Err(map_catalogue_error(error)));
        }
    };
    ensure_current(state, generation)?;
    let identity = ReaderIdentity {
        session: state.reader_session.to_string(),
        snapshot_generation: generation,
        resource_generation,
        resource_token: ValueState::Known(token(resource.sha256())),
        manual_slot: slot,
    };
    let reader = project(&resource, identity.clone())?;
    // The supplied projector must preserve the acquired resource and snapshot identity.
    reader
        .for_identity(&identity)
        .map_err(|_| IpcError::stale())?;
    ensure_resource_current(state, generation, resource_generation)?;
    loader
        .load_matching(resource.sha256(), &CancellationToken::default())
        .map_err(|_| IpcError::stale())?;
    ensure_resource_current(state, generation, resource_generation)?;
    Ok(Ok(reader.document().clone()))
}

fn ensure_resource_current(
    state: &DesktopState,
    generation: u64,
    resource: u64,
) -> Result<(), IpcError> {
    ensure_current(state, generation)?;
    if state.resource_generation.load(Ordering::Acquire) != resource {
        return Err(IpcError::stale());
    }
    Ok(())
}

pub(super) fn token(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn generation_exhausted() -> IpcError {
    IpcError::simple(
        IpcErrorCategory::Limit,
        "reader_generation_exhausted",
        false,
    )
}

/// Errand postings from the game's Profit table; their saved status is not decoded.
fn errand_infos(errands: &ivalice_domain::errands::ValidatedErrands) -> Vec<ErrandInfo> {
    errands
        .errands()
        .iter()
        .map(|errand| ErrandInfo {
            index: errand.index,
            title: errand.title.clone(),
            client: errand.client.clone(),
            posting: errand.posting.clone(),
        })
        .collect()
}

/// Artefacts and wonders from the game's Collection table; found status is not decoded.
fn collection_infos(errands: &ivalice_domain::errands::ValidatedErrands) -> Vec<CollectionInfo> {
    errands
        .collection()
        .iter()
        .map(|entry| CollectionInfo {
            index: entry.index,
            kind: entry.kind,
            name: entry.name.clone(),
            description: entry.description.clone(),
        })
        .collect()
}

/// Side-quest scenes seen in the slot, from the domain's event-flag table.
fn side_quest_states(metadata: &ivalice_save_format::SlotMetadata) -> Vec<SideQuestState> {
    ivalice_domain::side_quests::SIDE_QUESTS
        .iter()
        .filter_map(|quest| {
            let seen =
                ivalice_domain::side_quests::scenes_seen(quest, |id| metadata.event_flag(id))?;
            Some(SideQuestState {
                name: quest.name.into(),
                scenes: quest
                    .scenes
                    .iter()
                    .zip(seen)
                    .map(|(scene, seen)| SideQuestSceneState {
                        label: scene.label.into(),
                        seen,
                    })
                    .collect(),
                counter: quest.counter.and_then(|counter| {
                    Some(SideQuestCounterState {
                        label: counter.label.into(),
                        value: *metadata
                            .event_variables
                            .get(usize::from(counter.variable))?,
                    })
                }),
            })
        })
        .collect()
}

/// Achievement descriptions joined to the slot's unlocked bytes and counters;
/// empty when the description resource or the save fields are unavailable.
fn achievement_states(
    resource_root: &std::path::Path,
    metadata: &ivalice_save_format::SlotMetadata,
) -> Vec<AchievementState> {
    let (Ok(table), Some((unlocked, progress))) = (
        AchievementsLoader::load(resource_root),
        metadata.achievements(),
    ) else {
        return Vec::new();
    };
    table
        .achievements()
        .iter()
        .filter_map(|achievement| {
            let index = usize::from(achievement.index);
            Some(AchievementState {
                index: achievement.index,
                description: achievement.description.clone(),
                unlocked: *unlocked.get(index)? != 0,
                progress: *progress.get(index)?,
            })
        })
        .collect()
}

pub(super) fn map_catalogue_error(error: ReaderCatalogueLoadError) -> IpcError {
    let (category, code, retryable) = match error.code {
        ReaderCatalogueLoadErrorCode::KnownFolderUnavailable => (
            IpcErrorCategory::Resource,
            "reader_catalogue_unavailable",
            false,
        ),
        ReaderCatalogueLoadErrorCode::NonCanonical => (
            IpcErrorCategory::Corrupt,
            "reader_catalogue_noncanonical",
            false,
        ),
        ReaderCatalogueLoadErrorCode::Stale => return IpcError::stale(),
        ReaderCatalogueLoadErrorCode::Validation(ReaderCatalogueError::TooLarge) => {
            (IpcErrorCategory::Limit, "reader_catalogue_too_large", false)
        }
        ReaderCatalogueLoadErrorCode::Validation(
            ReaderCatalogueError::UnsupportedSchema | ReaderCatalogueError::SourceMismatch,
        ) => (
            IpcErrorCategory::Unsupported,
            "reader_catalogue_profile",
            false,
        ),
        ReaderCatalogueLoadErrorCode::Validation(_) => {
            (IpcErrorCategory::Corrupt, "reader_catalogue_invalid", false)
        }
        ReaderCatalogueLoadErrorCode::Snapshot(code) => match code {
            SnapshotErrorCode::NotFound => (
                IpcErrorCategory::Resource,
                "reader_catalogue_missing",
                false,
            ),
            SnapshotErrorCode::AccessDenied => (
                IpcErrorCategory::Resource,
                "reader_catalogue_access_denied",
                false,
            ),
            SnapshotErrorCode::TooLarge | SnapshotErrorCode::ResourceLimit => {
                (IpcErrorCategory::Limit, "reader_catalogue_too_large", false)
            }
            SnapshotErrorCode::Cancelled => (
                IpcErrorCategory::Cancelled,
                "reader_catalogue_cancelled",
                false,
            ),
            SnapshotErrorCode::SourceBusy
            | SnapshotErrorCode::SourceChanged
            | SnapshotErrorCode::RetryExhausted
            | SnapshotErrorCode::TimedOut => {
                (IpcErrorCategory::Resource, "reader_catalogue_changed", true)
            }
            _ => (IpcErrorCategory::Resource, "reader_catalogue_read", false),
        },
    };
    IpcError::simple(category, code, retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ivalice_domain::game_data::reader_catalogue::{
        ReaderCatalogueDocument, ReaderCatalogueEntry, ReaderCatalogueSource, ReaderCategory,
        ValidatedReaderCatalogue, READER_CATALOGUE_PROFILE, READER_CATALOGUE_REVISION,
        READER_CATALOGUE_SCHEMA,
    };
    use ivalice_domain::game_data::SpoilerLevel;
    use std::{
        error::Error,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn named_ability_edit_context_uses_camel_case_wire_fields() -> Result<(), Box<dyn Error>> {
        let context = EditContext {
            ability_descriptions: std::collections::BTreeMap::from([(
                "ability:394".into(),
                "Jump one tile farther.".into(),
            )]),
            snapshot_generation: 1,
            manual_slot_id: 33,
            gil: 100,
            story_step: 940,
            calendar: Some(CalendarDate {
                month: 10,
                day: 21,
                month_lengths: ivalice_domain::calendar::month_lengths(),
            }),
            achievements: vec![AchievementState {
                index: 7,
                description: "Reach level 99.".into(),
                unlocked: false,
                progress: 42,
            }],
            side_quests: vec![SideQuestState {
                name: "Into the Dark".into(),
                scenes: vec![SideQuestSceneState {
                    label: "Terminus".into(),
                    seen: true,
                }],
                counter: Some(SideQuestCounterState {
                    label: "Deeper passages found".into(),
                    value: 3,
                }),
            }],
            story_choices: vec![StoryStepChoice {
                progress: 940,
                chapter: Some("Chapter 3".into()),
                objective: None,
                guests: vec!["Agrias".into()],
                joins: vec!["Mustadio".into()],
                leaves: Vec::new(),
            }],
            backup_available: false,
            errands: vec![ErrandInfo {
                index: 0,
                title: "The Fate of Our Company".into(),
                client: "Client".into(),
                posting: "Salvage the ship.".into(),
            }],
            collection: vec![CollectionInfo {
                index: 16,
                kind: ivalice_domain::errands::CollectionKind::Artefact,
                name: "Four-Deity Plate".into(),
                description: "Brooches.".into(),
            }],
            units_on_errands: vec![5, 6],
            zodiac_options: vec![EditZodiacOptions {
                unit_position: 0,
                current: ivalice_domain::identity::ZodiacSign::Aries,
                choices: ivalice_domain::identity::ZodiacSign::ALL
                    .into_iter()
                    .map(|key| ZodiacChoice {
                        key,
                        label: key.label().into(),
                    })
                    .collect(),
            }],
            sex_options: vec![EditSexOptions {
                unit_position: 1,
                current: ivalice_domain::identity::UnitSex::Female,
                choices: ivalice_domain::identity::UnitSex::ALL
                    .into_iter()
                    .map(|key| SexChoice {
                        key,
                        label: key.label().into(),
                    })
                    .collect(),
                unavailable_reason: None,
            }],
            job_options: vec![EditJobOptions {
                unit_position: 0,
                job_slots: vec![13],
                locked_jobs: vec![LockedJob {
                    job_slot: 14,
                    requires: vec![RequiredJobLevel {
                        job_slot: 13,
                        level: 4,
                        current_level: 3,
                    }],
                }],
            }],
            ability_options: vec![EditAbilityOptions {
                unit_position: 0,
                job_slot: 13,
                command_key: "command:21".into(),
                command_label: "Jump".into(),
                members: vec![EditAbilityOption {
                    key: "ability:394".into(),
                    label: "Horizontal Jump +1".into(),
                    learned: false,
                    equipped: false,
                    kind: AbilityKind::Action,
                }],
            }],
            loadout_options: vec![EditLoadoutOptions {
                unit_position: 0,
                secondary_choices: vec![],
                reaction_choices: vec![],
                support_choices: vec![],
                movement_choices: vec![],
                slots: vec![EditLoadoutSlot {
                    combat_set: None,
                    slot: LoadoutKind::Reaction,
                    current_key: String::new(),
                }],
            }],
            gear_options: vec![EditGearOptions {
                unit_position: 0,
                slots: vec![EditGearSlot {
                    slot: GearSlot::RightHand,
                    current_key: "item:1".into(),
                    choices: vec![GearChoice {
                        key: "item:256".into(),
                        label: "Enhanced Sword".into(),
                    }],
                }],
            }],
            job_level_total_jp: JOB_LEVEL_TOTAL_JP,
            generic_creation: GenericCreationContext {
                target_position: Some(12),
                donors: vec![GenericDonorChoice {
                    source_slot: 37,
                    source_position: 12,
                    unit_position: 12,
                    label: "Female · Lv 1 · save slot 38, member 13".into(),
                    level: 1,
                    sex: "Female".into(),
                }],
            },
            story_addition: GenericCreationContext {
                target_position: Some(12),
                donors: Vec::new(),
            },
            guest_addition: GenericCreationContext {
                target_position: Some(12),
                donors: Vec::new(),
            },
            creature_addition: CreatureAdditionContext {
                target_position: None,
                monsters: Vec::new(),
                enemies: Vec::new(),
            },
            named_addition: NamedAdditionContext {
                target_position: Some(12),
                donors: Vec::new(),
                characters: vec![NamedCharacterChoice {
                    key: "character_name:32".into(),
                    label: "Wiegraf · variant 1".into(),
                    available: true,
                    sex: Some("Male".into()),
                    unavailable_reason: None,
                }],
            },
        };
        let wire = serde_json::to_value(context)?;
        assert_eq!(wire["storyStep"], 940);
        assert_eq!(wire["calendar"]["month"], 10);
        assert_eq!(wire["calendar"]["day"], 21);
        assert_eq!(wire["calendar"]["monthLengths"][1], 28);
        assert_eq!(wire["achievements"][0]["index"], 7);
        assert_eq!(wire["achievements"][0]["unlocked"], false);
        assert_eq!(wire["achievements"][0]["progress"], 42);
        assert_eq!(wire["sideQuests"][0]["name"], "Into the Dark");
        assert_eq!(wire["sideQuests"][0]["scenes"][0]["seen"], true);
        assert_eq!(wire["sideQuests"][0]["counter"]["value"], 3);
        assert_eq!(wire["errands"][0]["posting"], "Salvage the ship.");
        assert_eq!(wire["collection"][0]["kind"], "artefact");
        assert_eq!(wire["unitsOnErrands"][1], 6);
        assert_eq!(wire["storyChoices"][0]["chapter"], "Chapter 3");
        assert!(wire["storyChoices"][0]["objective"].is_null());
        assert_eq!(wire["storyChoices"][0]["guests"][0], "Agrias");
        assert_eq!(wire["storyChoices"][0]["joins"][0], "Mustadio");
        assert_eq!(
            wire["abilityDescriptions"]["ability:394"],
            "Jump one tile farther."
        );
        assert_eq!(
            wire["abilityOptions"][0]["members"][0]["key"],
            "ability:394"
        );
        assert_eq!(wire["zodiacOptions"][0]["unitPosition"], 0);
        assert_eq!(wire["zodiacOptions"][0]["current"], "aries");
        assert_eq!(
            wire["zodiacOptions"][0]["choices"].as_array().map(Vec::len),
            Some(12)
        );
        assert_eq!(wire["zodiacOptions"][0]["choices"][11]["key"], "pisces");
        assert_eq!(wire["sexOptions"][0]["unitPosition"], 1);
        assert_eq!(wire["sexOptions"][0]["current"], "female");
        assert_eq!(wire["sexOptions"][0]["choices"][0]["key"], "male");
        assert_eq!(wire["sexOptions"][0]["choices"][1]["key"], "female");
        assert_eq!(wire["abilityOptions"][0]["jobSlot"], 13);
        assert_eq!(wire["genericCreation"]["targetPosition"], 12);
        assert_eq!(wire["genericCreation"]["donors"][0]["sourceSlot"], 37);
        assert_eq!(wire["storyAddition"]["targetPosition"], 12);
        assert_eq!(wire["guestAddition"]["targetPosition"], 12);
        assert_eq!(
            wire["namedAddition"]["characters"][0]["key"],
            "character_name:32"
        );
        assert_eq!(
            wire["jobOptions"][0]["lockedJobs"][0]["requires"][0]["currentLevel"],
            3
        );
        assert_eq!(wire["loadoutOptions"][0]["slots"][0]["slot"], "reaction");
        assert_eq!(wire["gearOptions"][0]["slots"][0]["slot"], "right_hand");
        assert_eq!(
            wire["gearOptions"][0]["slots"][0]["choices"][0]["key"],
            "item:256"
        );
        assert!(wire.get("ability_options").is_none());
        Ok(())
    }

    fn setup() -> Result<(PathBuf, DesktopState), Box<dyn Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ivalice-reader-host-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("local.ivalice.companion"))?;
        let state = DesktopState::new(
            SettingsStore::for_local_app_data(root.clone())?,
            root.clone(),
        );
        Ok((root, state))
    }

    fn catalogue_path(root: &Path) -> PathBuf {
        root.join("local.ivalice.companion/reader-catalogue-v1.json")
    }

    fn catalogue(label: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(ValidatedReaderCatalogue::validate(ReaderCatalogueDocument {
            schema: READER_CATALOGUE_SCHEMA.into(),
            profile: READER_CATALOGUE_PROFILE.into(),
            revision: READER_CATALOGUE_REVISION.into(),
            locale: "en".into(),
            sources: [
                ReaderCategory::Job,
                ReaderCategory::Command,
                ReaderCategory::Ability,
                ReaderCategory::Item,
                ReaderCategory::CharacterName,
            ]
            .into_iter()
            .map(|category| ReaderCatalogueSource {
                category,
                input_sha256: category.input_sha256().into(),
            })
            .collect(),
            entries: vec![ReaderCatalogueEntry {
                category: ReaderCategory::CharacterName,
                id: "character_name:1".into(),
                label: ValueState::Known(label.into()),
                description: ValueState::Unknown,
                spoiler: SpoilerLevel::Full,
            }],
            job_commands: vec![],
            mechanics: None,
        })?
        .canonical_json()?)
    }

    fn projected(identity: ReaderIdentity) -> Result<ValidatedReader, IpcError> {
        let mut document: ReaderDocument =
            serde_json::from_slice(include_bytes!("../../../tests/fixtures/reader-v2.json"))
                .map_err(|_| worker_error())?;
        document.identity = identity;
        ValidatedReader::validate(document).map_err(|_| worker_error())
    }

    #[test]
    fn strict_reader_requests_reject_unknown_unsafe_and_invalid_fields() {
        for request in [
            serde_json::json!({"requestId":0,"manualSlotId":null}),
            serde_json::json!({"requestId":MAX_SAFE_GENERATION+1,"manualSlotId":null}),
            serde_json::json!({"requestId":1,"manualSlotId":null,"spoilerLevel":"full"}),
            serde_json::json!({"requestId":1,"manualSlotId":null,"path":"secret"}),
            serde_json::json!({"requestId":1.5,"manualSlotId":null}),
            serde_json::json!({"requestId":"1","manualSlotId":null}),
        ] {
            assert_eq!(
                parse_request(request).err().map(|error| error.code),
                Some("invalid_reader_request")
            );
        }
        assert_eq!(
            parse_request(serde_json::json!({"requestId":1,"manualSlotId":50}))
                .err()
                .map(|error| error.code),
            Some("manual_slot_out_of_range")
        );
        let request =
            parse_request(serde_json::json!({"requestId":MAX_SAFE_GENERATION,"manualSlotId":49}))
                .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(request.manual_slot_id, Some(49));
    }

    #[test]
    fn generation_exhaustion_never_wraps_or_reuses_an_old_snapshot() -> Result<(), Box<dyn Error>> {
        let (_, state) = setup()?;
        state
            .load_generation
            .store(MAX_SAFE_GENERATION - 1, Ordering::Release);
        assert_eq!(state.begin_load(), MAX_SAFE_GENERATION);
        assert!(state.is_current(MAX_SAFE_GENERATION));
        assert_eq!(state.begin_load(), MAX_SAFE_GENERATION + 1);
        assert!(!state.is_current(MAX_SAFE_GENERATION));
        assert!(!state.is_current(MAX_SAFE_GENERATION + 1));
        assert_eq!(state.begin_load(), MAX_SAFE_GENERATION + 1);
        state
            .resource_generation
            .store(MAX_SAFE_GENERATION, Ordering::Release);
        state.load_generation.store(1, Ordering::Release);
        assert_eq!(
            with_catalogue(&state, 1, 0, |_, _| panic!("must not project"))
                .err()
                .map(|error| error.code),
            Some("reader_generation_exhausted")
        );
        Ok(())
    }

    #[test]
    fn initial_missing_or_invalid_catalogue_is_separate_from_a_baseline_failure(
    ) -> Result<(), Box<dyn Error>> {
        let (root, state) = setup()?;
        let generation = state.begin_load();
        let missing = with_catalogue(&state, generation, 0, |_, _| {
            panic!("missing resource projected")
        })?;
        assert_eq!(
            missing.err().map(|error| error.code),
            Some("reader_catalogue_missing")
        );
        fs::write(catalogue_path(&root), b"secret invalid text")?;
        let bad = with_catalogue(&state, generation, 0, |_, _| {
            panic!("bad resource projected")
        })?;
        let error = bad.err().ok_or("expected invalid catalogue")?;
        assert_eq!(error.code, "reader_catalogue_invalid");
        let response = LoadReaderResponse {
            request_id: 1,
            occupied_manual_slots: vec![],
            slot_summaries: vec![],
            save: None,
            reader: None,
            reader_error: Some(error),
            edit_context: None,
        };
        let wire = serde_json::to_value(response)?;
        assert!(wire.get("requestId").is_some());
        assert!(wire.get("occupiedManualSlots").is_some());
        assert_eq!(wire["readerError"]["code"], "reader_catalogue_invalid");
        assert!(!wire.to_string().contains("secret"));
        assert!(!wire.to_string().contains(root.to_string_lossy().as_ref()));
        Ok(())
    }

    #[test]
    fn projection_binds_actual_resource_session_snapshot_and_monotonic_resource_generation(
    ) -> Result<(), Box<dyn Error>> {
        let (root, state) = setup()?;
        let bytes = catalogue("Synthetic label")?;
        fs::write(catalogue_path(&root), &bytes)?;
        let generation = state.begin_load();
        let first = with_catalogue(&state, generation, 3, |resource, identity| {
            assert_eq!(
                identity.resource_token,
                ValueState::Known(token(resource.sha256()))
            );
            assert_eq!(identity.session, state.reader_session.as_ref());
            assert_eq!(identity.snapshot_generation, generation);
            assert_eq!(identity.manual_slot, 3);
            assert!(resource
                .lookup("character_name:1", SpoilerLevel::Minimal)
                .is_none());
            assert!(resource
                .lookup("character_name:1", SpoilerLevel::Full)
                .is_some());
            projected(identity)
        })??;
        let second = with_catalogue(&state, generation, 3, |_, identity| projected(identity))??;
        assert!(second.identity.resource_generation > first.identity.resource_generation);
        assert_eq!(
            first.identity.resource_token,
            second.identity.resource_token
        );
        assert_eq!(fs::read(catalogue_path(&root))?, bytes);
        let (_, different_session) = setup()?;
        assert_ne!(state.reader_session, different_session.reader_session);
        Ok(())
    }

    #[test]
    fn mismatched_projection_identity_or_newer_generation_cannot_publish(
    ) -> Result<(), Box<dyn Error>> {
        let (root, state) = setup()?;
        fs::write(catalogue_path(&root), catalogue("Synthetic label")?)?;
        for mutate in 0..4 {
            let generation = state.begin_load();
            let result = with_catalogue(&state, generation, 0, |_, mut identity| {
                match mutate {
                    0 => identity.session = "different-session".into(),
                    1 => identity.resource_generation += 1,
                    2 => identity.resource_token = ValueState::Known("different-token".into()),
                    _ => identity.snapshot_generation += 1,
                }
                projected(identity)
            });
            assert_eq!(result.err().map(|error| error.code), Some("stale_result"));
        }
        let generation = state.begin_load();
        let stale = with_catalogue(&state, generation, 0, |_, identity| {
            state.begin_load();
            projected(identity)
        });
        assert_eq!(stale.err().map(|error| error.code), Some("stale_result"));
        let generation = state.begin_load();
        let stale = with_catalogue(&state, generation, 0, |_, identity| {
            next_generation(&state.resource_generation);
            projected(identity)
        });
        assert_eq!(stale.err().map(|error| error.code), Some("stale_result"));
        Ok(())
    }

    #[test]
    fn replaced_or_corrupt_resource_after_projection_is_never_returned_as_a_fallback(
    ) -> Result<(), Box<dyn Error>> {
        let (root, state) = setup()?;
        let original = catalogue("Synthetic first")?;
        let replacement = catalogue("Synthetic second")?;
        for after in [replacement, b"corrupt".to_vec()] {
            fs::write(catalogue_path(&root), &original)?;
            let generation = state.begin_load();
            let result = with_catalogue(&state, generation, 0, |_, identity| {
                fs::write(catalogue_path(&root), &after).map_err(|_| worker_error())?;
                projected(identity)
            });
            assert_eq!(result.err().map(|error| error.code), Some("stale_result"));
            assert_eq!(fs::read(catalogue_path(&root))?, after);
        }
        fs::write(catalogue_path(&root), &original)?;
        let generation = state.begin_load();
        let removed = with_catalogue(&state, generation, 0, |_, identity| {
            fs::remove_file(catalogue_path(&root)).map_err(|_| worker_error())?;
            projected(identity)
        });
        assert_eq!(removed.err().map(|error| error.code), Some("stale_result"));
        assert!(!catalogue_path(&root).exists());
        Ok(())
    }

    #[tauri::command]
    fn test_reader_contract() -> Result<LoadReaderResponse, IpcError> {
        let identity = ReaderIdentity {
            session: "synthetic-session".into(),
            snapshot_generation: 1,
            resource_generation: 1,
            resource_token: ValueState::Known("synthetic-resource".into()),
            manual_slot: 0,
        };
        Ok(LoadReaderResponse {
            request_id: 17,
            occupied_manual_slots: vec![],
            slot_summaries: vec![],
            save: None,
            reader: Some(projected(identity)?.document().clone()),
            reader_error: None,
            edit_context: None,
        })
    }

    #[test]
    fn actual_invoke_validates_reader_requests_and_serializes_reader_contract(
    ) -> Result<(), Box<dyn Error>> {
        let (_, state) = setup()?;
        let app = tauri::test::mock_builder()
            .manage(state)
            .invoke_handler(tauri::generate_handler![load_reader, test_reader_contract])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build()?;
        let invoke = |command: &str, body: serde_json::Value| {
            tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd: command.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "http://tauri.localhost"
                        .parse()
                        .unwrap_or_else(|error| panic!("{error}")),
                    body: tauri::ipc::InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_string(),
                },
            )
        };
        let failure = invoke(
            "load_reader",
            serde_json::json!({"request":{"requestId":1,"manualSlotId":null,"path":"not-allowed"}}),
        )
        .err()
        .ok_or("invalid invoke accepted")?;
        assert_eq!(failure["code"], "invalid_reader_request");
        let failure = invoke(
            "load_reader",
            serde_json::json!({"request":{"requestId":2,"manualSlotId":null}}),
        )
        .err()
        .ok_or("missing selection accepted")?;
        assert_eq!(failure["code"], "no_selection");
        let success: serde_json::Value = invoke("test_reader_contract", serde_json::json!({}))
            .map_err(|error| std::io::Error::other(error.to_string()))?
            .deserialize()?;
        assert_eq!(success["requestId"], 17);
        assert_eq!(success["reader"]["schema"], "reader_v2");
        assert_eq!(
            success["reader"]["roster"]["value"]["value"][0]["growth"]["value"]["state"],
            "unknown"
        );
        assert!(success["readerError"].is_null());
        assert!(success["save"].is_null());
        drop(webview);
        drop(app);
        Ok(())
    }
}
