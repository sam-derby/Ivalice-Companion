//! TICSaveEditor.Core/Records/UnitSaveData.cs, CombatSet.cs and
//! GUI/ViewModels/UnitListItemViewModel.cs at 07ea857. This is the general
//! read-only projection; job storage slots without a documented ID join stay
//! named only by their dense upstream position.

use super::{
    inventory::BattleStores, occupancy_marker, slot_metadata::SlotMetadata, unit_record,
    unit_record::UnitRecord, ManualParseError, ManualPayload, BATTLE_OFFSET, BATTLE_SIZE,
    UNIT_COUNT,
};
use crate::DecodedContainer;
use ivalice_domain::{
    game_data::{inventory_category::inventory_category, SpoilerLevel},
    reader::{
        AbilityLoadout, CatalogueRef, CombatSet, CurrentEquipment, EffectiveStats, Fact, Holding,
        JobState, Membership, NameOrigin, ReaderDocument, ReaderIdentity, ReaderProfile,
        ReaderSchema, ReaderUnit, SavedAbilityFlagGroup, SavedProgress, StoredBases, StoredStats,
        UnitGuidance, UnitKind, UnitName, ValidatedReader,
    },
    ValueState,
};

const EMPTY_ITEM: u16 = 0x00ff;
// TICSaveEditor.Core/Records/UnitSaveData.cs BuildJobSlotTable at 07ea857:
// canonical generic class IDs for storage slots 0..21; slot 22 is unmapped.
const JOB_SLOT_IDS: [u16; 22] = [
    0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59,
    0x5a, 0x5b, 0x5c, 0x5d, 0xa0, 0xa1,
];

impl DecodedContainer {
    /// Read every nonempty unit position from an occupied manual slot, including
    /// inactive records and positions used for guests. Player save data is not
    /// subject to companion-content spoiler settings.
    pub fn reader_manual_save_v2(
        &self,
        identity: ReaderIdentity,
        lookup: impl Fn(&str, SpoilerLevel) -> Option<CatalogueRef>,
    ) -> Result<ValidatedReader, ManualParseError> {
        let payload = ManualPayload::parse(self.payload())?;
        let record = payload.slot_record(usize::from(identity.manual_slot))?;
        let metadata = if occupancy_marker(record)? == 0 {
            None
        } else {
            Some(SlotMetadata::parse(record)?)
        };
        let roster = if occupancy_marker(record)? == 0 {
            absent()
        } else {
            let battle_end = BATTLE_OFFSET
                .checked_add(BATTLE_SIZE)
                .ok_or(ManualParseError::BattleBounds)?;
            let mut units = Vec::with_capacity(UNIT_COUNT);
            for position in 0..UNIT_COUNT {
                let bytes = unit_record(record, position, battle_end)?;
                let unit = UnitRecord::parse(bytes)?;
                if unit.is_empty() {
                    continue;
                }
                let key = u16::try_from(position).map_err(|_| ManualParseError::DomainInvariant)?;
                units.push(project_unit(&unit, key, &lookup)?);
            }
            known(units)
        };
        let inventory = if occupancy_marker(record)? == 0 {
            absent()
        } else {
            let stores = BattleStores::parse(record)?;
            let holdings = stores
                .party
                .iter()
                .enumerate()
                .map(|(index, count)| {
                    let key =
                        u16::try_from(index).map_err(|_| ManualParseError::DomainInvariant)?;
                    // FFT_enhanced.exe SHA-256 937233f7fe76182a665c487c8802f5cec6662ddd09967e87cd09fb146fc6b5d5: RVA 0x101058 indexes PartyItem by item ID.
                    // RVA 0x279bf4 copies SaveWork PartyItem[261] into that indexed runtime array.
                    let item = if matches!(key, 0 | 254 | 255) {
                        unknown()
                    } else {
                        named("item", key, &lookup)
                    };
                    let category = if matches!(&item.value, ValueState::Known(_)) {
                        inventory_category(key).map_or_else(unknown, |name| known(name.to_owned()))
                    } else {
                        unknown()
                    };
                    Ok(Holding {
                        key,
                        item,
                        category,
                        quantity: known(u16::from(*count)),
                    })
                })
                .collect::<Result<Vec<_>, ManualParseError>>()?;
            known(holdings)
        };
        // TICSaveEditor.Core/Records/UnitSaveData.cs at 07ea857 provides the
        // saved level; pinned CharaName.json labels identities 1, 2 and 3 Ramza.
        // Require one resolved Ramza identity, regardless of chapter variant.
        let ramza_level = if let ValueState::Known(units) = &roster.value {
            let mut levels = units
                .iter()
                .filter(|unit| {
                    matches!(&unit.portrait.value,
                        ValueState::Known(identity) if matches!(identity.as_str(),
                            "character:1" | "character:2" | "character:3"))
                })
                .map(|unit| unit.saved.level);
            match (levels.next(), levels.next()) {
                (Some(level), None) => known(level),
                _ => unknown(),
            }
        } else {
            unknown()
        };
        let document = ReaderDocument {
            schema: ReaderSchema::ReaderV2,
            profile: ReaderProfile::EnglishSteamEnhancedManual,
            identity,
            roster,
            inventory,
            item_details: Default::default(),
            gil: metadata
                .as_ref()
                .map_or_else(absent, |value| known(u64::from(value.gil))),
            progress: SavedProgress {
                // TICSaveEditor.Core/Sections/CardSection.cs at 07ea857.
                title: metadata
                    .as_ref()
                    .map_or_else(unknown, |value| saved_text(value.readable_title())),
                saved_at_unix_seconds: metadata.as_ref().map_or_else(unknown, |value| {
                    if value.saved_at_unix_seconds > 0 {
                        known(i64::from(value.saved_at_unix_seconds))
                    } else {
                        unknown()
                    }
                }),
                // TICSaveEditor.Core/Sections/InfoSection.cs at 07ea857.
                hero_name: metadata
                    .as_ref()
                    .map_or_else(unknown, |value| saved_text(value.readable_hero_name())),
                location: unknown(),
                difficulty: unknown(),
                // TICSaveEditor.Core/Sections/FftoConfigSection.cs at 07ea857.
                difficulty_code: metadata
                    .as_ref()
                    .map_or_else(unknown, |value| known(value.difficulty_level)),
                chapter: unknown(),
                // Independent analysis: User GameProgressRaw track 0 is the main story;
                // labels are joined later from the story progress resource.
                story_progress: metadata
                    .as_ref()
                    .map_or_else(unknown, |value| known(value.story_progress[0])),
                objective: unknown(),
                area_index: metadata.as_ref().map_or_else(unknown, |value| {
                    value.current_area_index().map_or_else(unknown, known)
                }),
                ramza_level,
                story: unknown(),
                // Independent analysis: 0x120 mirrors scripted progress, despite upstream's
                // playtime name. The 0x1b4 seconds counter drops in the
                // load/retreat trial; neither supplies a reliable total.
                play_time_seconds: unknown(),
                // TICSaveEditor.Core/Sections/InfoSection.cs at 07ea857; event values
                // use the 4-aligned variable array, not upstream's EventWork window.
                next_event_id: metadata
                    .as_ref()
                    .map_or_else(unknown, |value| known(value.next_event_id)),
                unnamed_event_values: metadata.as_ref().map_or_else(unknown, |value| {
                    u16::try_from(value.unnamed_event_values()).map_or_else(|_| unknown(), known)
                }),
                errands: unknown(),
                events: unknown(),
                recruitment: unknown(),
            },
        };
        ValidatedReader::validate(document).map_err(|_| ManualParseError::DomainInvariant)
    }
}

fn saved_text(value: Option<String>) -> Fact<String> {
    value.map_or_else(unknown, known)
}

fn project_unit(
    unit: &UnitRecord,
    key: u16,
    lookup: &impl Fn(&str, SpoilerLevel) -> Option<CatalogueRef>,
) -> Result<ReaderUnit, ManualParseError> {
    let current_job = named("job", u16::from(unit.job), lookup);
    let mut jobs = Vec::with_capacity(unit.job_points.len());
    for index in 0..unit.job_points.len() {
        let job = if index == 0 {
            named("job", unit.first_progress_job_id(), lookup)
        } else {
            JOB_SLOT_IDS
                .get(index)
                .map_or_else(unknown, |id| named("job", *id, lookup))
        };
        jobs.push(JobState {
            slot: u8::try_from(index).map_err(|_| ManualParseError::DomainInvariant)?,
            job,
            level: known(unit.job_levels[index]),
            current_jp: known(unit.job_points[index]),
            // TICSaveEditor UnitSaveData.cs at 07ea857 calls this TotalJobPoint;
            // the owner's T033 game load displays the same value as Job EXP.
            total_jp: known(unit.total_job_points[index]),
        });
    }
    let membership = if unit.is_active(usize::from(key)) {
        if key < 50 {
            known(Membership::Party)
        } else {
            known(Membership::Guest)
        }
    } else {
        known(Membership::Inactive)
    };
    let combat_sets = unit
        .combat_sets
        .iter()
        .enumerate()
        .map(|(index, set)| {
            let key = u8::try_from(index).map_err(|_| ManualParseError::DomainInvariant)?;
            let name = ascii(&set.name_raw);
            // TICSaveEditor.Core/Records/Layouts/CombatSetLayout.cs at 07ea857
            // defines these fields. The upstream no-job/empty-item sentinels
            // and absent skillsets leave an otherwise blank set unassigned.
            // A saved name or job alone does not equip a loadout; item zero
            // is the catalogue's "Nothing Equipped" entry.
            let assigned = set
                .equipment
                .iter()
                .any(|item| *item != 0 && *item != EMPTY_ITEM)
                || set.skillsets.iter().any(|command| *command > 0)
                || set.abilities.iter().any(|ability| *ability != 0)
                || set.is_double_hand;
            Ok(CombatSet {
                key,
                assigned: known(assigned),
                name: if name.is_empty() {
                    absent()
                } else {
                    known(name)
                },
                job: named("job", u16::from(set.job), lookup),
                head: item(set.equipment[2], lookup),
                body: item(set.equipment[3], lookup),
                accessory: item(set.equipment[4], lookup),
                right_hand: item(set.equipment[0], lookup),
                left_hand: item(set.equipment[1], lookup),
                abilities: AbilityLoadout {
                    primary_command: command(set.skillsets[0], lookup),
                    secondary_command: command(set.skillsets[1], lookup),
                    reaction: ability(set.abilities[0], lookup),
                    support: ability(set.abilities[1], lookup),
                    movement: ability(set.abilities[2], lookup),
                },
                double_hand: known(set.is_double_hand),
            })
        })
        .collect::<Result<Vec<_>, ManualParseError>>()?;
    Ok(ReaderUnit {
        key,
        persistent_identity: unknown(),
        name: unit_name(unit, &current_job, lookup),
        // TICSaveEditor.Core/Records/UnitSaveData.cs at 07ea857 labels this
        // whole job-byte interval as monster classes.
        kind: if (0x5e..=0x8d).contains(&unit.job) {
            known(UnitKind::Monster)
        } else {
            unknown()
        },
        membership,
        sprite: unknown(),
        portrait: portrait_identity(unit, lookup),
        stored: StoredStats {
            level: known(unit.level),
            experience: known(unit.exp),
            brave: known(unit.start_bcp),
            faith: known(unit.start_faith),
            sex: if (0x5e..=0x8d).contains(&unit.job) {
                unknown()
            } else {
                known(stored_sex(unit.sex).into())
            },
            birthday: unknown(),
            zodiac: zodiac_name(unit.zodiac_sign),
            statuses: unknown(),
            bases: StoredBases {
                hp: known(unit.hp_max_base),
                mp: known(unit.mp_max_base),
                speed: known(unit.wt_base),
                physical_attack: known(unit.at_base),
                magical_attack: known(unit.mat_base),
            },
        },
        effective: EffectiveStats {
            breakdown: std::collections::BTreeMap::new(),
            hp: unknown(),
            mp: unknown(),
            speed: unknown(),
            physical_attack: unknown(),
            magical_attack: unknown(),
            movement_tiles: unknown(),
            jump_tiles: unknown(),
            evasion: unknown(),
        },
        growth: unknown(),
        current_job,
        jobs: known(jobs),
        learned_abilities: unknown(),
        // TICSaveEditor.Core/Records/Entries/JobAbilityFlagsEntry.cs at 07ea857
        // numbers 16 active and 8 passive bits per dense job storage slot.
        saved_ability_flags: known(
            unit.ability_flags
                .iter()
                .enumerate()
                .map(|(slot, raw)| {
                    let mut active_positions = Vec::new();
                    let mut passive_positions = Vec::new();
                    for position in 0..24_u8 {
                        let byte = usize::from(position / 8);
                        let bit = position % 8;
                        if raw[byte] & (1 << bit) != 0 {
                            if position < 16 {
                                active_positions.push(position);
                            } else {
                                passive_positions.push(position - 16);
                            }
                        }
                    }
                    Ok(SavedAbilityFlagGroup {
                        slot: u8::try_from(slot).map_err(|_| ManualParseError::DomainInvariant)?,
                        active_positions,
                        passive_positions,
                    })
                })
                .collect::<Result<Vec<_>, ManualParseError>>()?,
        ),
        abilities: AbilityLoadout {
            primary_command: unknown(),
            secondary_command: named("command", u16::from(unit.secondary_action), lookup),
            reaction: ability(unit.reaction_ability, lookup),
            support: ability(unit.support_ability, lookup),
            movement: ability(unit.movement_ability, lookup),
        },
        equipment: CurrentEquipment {
            head: item(unit.equip_items[0], lookup),
            body: item(unit.equip_items[1], lookup),
            accessory: item(unit.equip_items[2], lookup),
            right_weapon: item(unit.equip_items[3], lookup),
            right_shield: item(unit.equip_items[4], lookup),
            left_weapon: item(unit.equip_items[5], lookup),
            left_shield: item(unit.equip_items[6], lookup),
        },
        combat_sets: known(combat_sets),
        selected_combat_set: if unit.current_combat_set < 3 {
            known(unit.current_combat_set)
        } else {
            unknown()
        },
        guidance: UnitGuidance {
            target_job: unknown(),
            samurai_prerequisites: unknown(),
            steel_cost: unknown(),
            job_eligible: unknown(),
            ability_purchasable: unknown(),
        },
        saved: unit.to_saved(),
    })
}

fn unit_name(
    unit: &UnitRecord,
    current_job: &Fact<CatalogueRef>,
    lookup: &impl Fn(&str, SpoilerLevel) -> Option<CatalogueRef>,
) -> Fact<UnitName> {
    if unit.nickname_raw[0] != 0 {
        return known(UnitName {
            text: ascii(&unit.nickname_raw),
            origin: NameOrigin::SavedRename,
        });
    }
    if unit.character == 1 {
        return known(UnitName {
            text: "Ramza".into(),
            origin: NameOrigin::UpstreamFallback,
        });
    }
    for index in [unit.name_no, unit.chara_name_key] {
        if index != 0 {
            if let ValueState::Known(reference) = named("character_name", index, lookup).value {
                if let ValueState::Known(text) = reference.label {
                    return known(UnitName {
                        text,
                        origin: NameOrigin::LocalizedCatalogue,
                    });
                }
            }
        }
    }
    let ValueState::Known(job) = &current_job.value else {
        return unknown();
    };
    let ValueState::Known(job_name) = &job.label else {
        return unknown();
    };
    if unit.sex & 0x20 != 0 {
        return known(UnitName {
            text: job_name.clone(),
            origin: NameOrigin::UpstreamFallback,
        });
    }
    let sex = if unit.sex & 0x80 != 0 {
        "Male"
    } else {
        "Female"
    };
    known(UnitName {
        text: format!("Generic {job_name} ({sex})"),
        origin: NameOrigin::UpstreamFallback,
    })
}

fn portrait_identity(
    unit: &UnitRecord,
    lookup: &impl Fn(&str, SpoilerLevel) -> Option<CatalogueRef>,
) -> Fact<String> {
    // TICSaveEditor.Core/Records/UnitSaveData.cs at 07ea857 keeps the saved
    // character-name keys separate from the reusable character template byte.
    for index in [unit.name_no, unit.chara_name_key, u16::from(unit.character)] {
        if index != 0
            && matches!(
                named("character_name", index, lookup).value,
                ValueState::Known(_)
            )
        {
            return known(format!("character:{index}"));
        }
    }
    unknown()
}

fn stored_sex(flags: u8) -> &'static str {
    // TICSaveEditor.Core/Records/UnitSaveData.cs at 07ea857 uses bit 7 for
    // human male; the other human branch is female.
    if flags & 0x80 != 0 {
        "Male"
    } else {
        "Female"
    }
}

fn zodiac_name(packed: u8) -> Fact<String> {
    // TICSaveEditor.Core/Records/UnitSaveData.cs at 07ea857 validates the high
    // nibble as sign 0..11; AeroStar's FFT Game Shark Handbook v4.1 maps the
    // preserved sign order from Aries through Pisces.
    const SIGNS: [&str; 12] = [
        "Aries",
        "Taurus",
        "Gemini",
        "Cancer",
        "Leo",
        "Virgo",
        "Libra",
        "Scorpio",
        "Sagittarius",
        "Capricorn",
        "Aquarius",
        "Pisces",
    ];
    match SIGNS.get(usize::from(packed >> 4)) {
        Some(sign) => known((*sign).into()),
        None => unknown(),
    }
}

fn ascii(raw: &[u8]) -> String {
    raw.iter()
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

fn named(
    namespace: &str,
    id: u16,
    lookup: &impl Fn(&str, SpoilerLevel) -> Option<CatalogueRef>,
) -> Fact<CatalogueRef> {
    let key = format!("{namespace}:{id}");
    match lookup(&key, SpoilerLevel::Full) {
        Some(reference)
            if reference.id == key
                && matches!(&reference.label, ValueState::Known(label) if !label.trim().is_empty()) =>
        {
            known(reference)
        }
        _ => unknown(),
    }
}

fn ability(
    id: u16,
    lookup: &impl Fn(&str, SpoilerLevel) -> Option<CatalogueRef>,
) -> Fact<CatalogueRef> {
    if id == 0 {
        absent()
    } else {
        named("ability", id, lookup)
    }
}

fn item(
    id: u16,
    lookup: &impl Fn(&str, SpoilerLevel) -> Option<CatalogueRef>,
) -> Fact<CatalogueRef> {
    if id == EMPTY_ITEM {
        absent()
    } else {
        named("item", id, lookup)
    }
}

fn command(
    id: i16,
    lookup: &impl Fn(&str, SpoilerLevel) -> Option<CatalogueRef>,
) -> Fact<CatalogueRef> {
    match u16::try_from(id) {
        Ok(0) | Err(_) => absent(),
        Ok(id) => named("command", id, lookup),
    }
}

fn unknown<T>() -> Fact<T> {
    Fact {
        value: ValueState::Unknown,
    }
}

fn known<T>(value: T) -> Fact<T> {
    Fact {
        value: ValueState::Known(value),
    }
}

fn absent<T>() -> Fact<T> {
    Fact {
        value: ValueState::Absent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        StoredAdlerStatus, SUPPORTED_FORMAT_DISCRIMINATOR, SUPPORTED_PAYLOAD_LENGTH,
        SUPPORTED_PAYLOAD_VERSION,
    };
    use ivalice_domain::reader::ReaderIdentity;

    #[test]
    fn human_sex_flag_and_all_twelve_packed_zodiac_signs() {
        assert_eq!(stored_sex(0x80), "Male");
        assert_eq!(stored_sex(0x40), "Female");
        let expected = [
            "Aries",
            "Taurus",
            "Gemini",
            "Cancer",
            "Leo",
            "Virgo",
            "Libra",
            "Scorpio",
            "Sagittarius",
            "Capricorn",
            "Aquarius",
            "Pisces",
        ];
        for (index, name) in expected.into_iter().enumerate() {
            let packed = u8::try_from(index).unwrap_or_else(|error| panic!("{error}")) << 4;
            assert_eq!(zodiac_name(packed).value, ValueState::Known(name.into()));
            assert_eq!(
                zodiac_name(packed | 0x0f).value,
                ValueState::Known(name.into())
            );
        }
        for invalid in 12..=15 {
            assert_eq!(zodiac_name(invalid << 4).value, ValueState::Unknown);
        }
    }

    #[test]
    fn job_progress_uses_canonical_names_and_current_personal_job() {
        let mut bytes = [0_u8; unit_record::SIZE];
        bytes[0] = 1;
        bytes[2] = 0x56;
        let lookup = |id: &str, _| {
            Some(CatalogueRef {
                id: id.into(),
                label: ValueState::Known(format!("Name {id}")),
                description: ValueState::Unknown,
                asset_key: ValueState::Unknown,
            })
        };
        let record = UnitRecord::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        let projected = project_unit(&record, 0, &lookup).unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(jobs) = projected.jobs.value else {
            panic!("jobs expected")
        };
        assert!(matches!(&jobs[0].job.value, ValueState::Known(job) if job.id == "job:1"));
        assert!(matches!(&jobs[12].job.value, ValueState::Known(job) if job.id == "job:86"));
        assert_eq!(jobs[22].job.value, ValueState::Unknown);

        bytes[2] = 0x03;
        let record = UnitRecord::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        let projected = project_unit(&record, 0, &lookup).unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(jobs) = projected.jobs.value else {
            panic!("jobs expected")
        };
        assert!(matches!(&jobs[0].job.value, ValueState::Known(job) if job.id == "job:3"));

        bytes[0] = 30;
        bytes[2] = 0x4c;
        let record = UnitRecord::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        let projected = project_unit(&record, 0, &lookup).unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(jobs) = projected.jobs.value else {
            panic!("jobs expected")
        };
        assert!(matches!(&jobs[0].job.value, ValueState::Known(job) if job.id == "job:30"));
    }

    fn fixture() -> DecodedContainer {
        let mut payload = vec![0; SUPPORTED_PAYLOAD_LENGTH];
        payload[..4].copy_from_slice(&SUPPORTED_PAYLOAD_VERSION.to_le_bytes());
        payload[8..16].copy_from_slice(&SUPPORTED_FORMAT_DISCRIMINATOR.to_le_bytes());
        let slot = &mut payload[0x10..0x10 + 0x9ce4];
        slot[..2].copy_from_slice(&1_u16.to_le_bytes());
        slot[0x44..0x48].copy_from_slice(&1_234_i32.to_le_bytes());
        for (position, character, stored_index, job) in
            [(0, 1, 0, 74), (50, 4, 50, 200), (53, 5, 255, 255)]
        {
            let start = 0x518 + position * 600;
            let unit = &mut slot[start..start + 600];
            unit[0] = character;
            unit[1] = stored_index;
            unit[2] = job;
            unit[0x1d] = 42;
            unit[0x1c] = 99;
            unit[0x1e] = 70;
            unit[0x1f] = 65;
            unit[0x74] = 0xa5;
            unit[0x80 + 22 * 2..0x82 + 22 * 2].copy_from_slice(&345_u16.to_le_bytes());
            unit[0xae + 22 * 2..0xb0 + 22 * 2].copy_from_slice(&600_u16.to_le_bytes());
            unit[0x0e..0x10].copy_from_slice(&999_u16.to_le_bytes());
            unit[0x10..0x12].copy_from_slice(&EMPTY_ITEM.to_le_bytes());
        }
        DecodedContainer {
            payload: payload.into_boxed_slice(),
            stored_adler_status: StoredAdlerStatus::Matched,
        }
    }

    fn identity(slot: u8) -> ReaderIdentity {
        ReaderIdentity {
            session: "synthetic-session".into(),
            snapshot_generation: 1,
            resource_generation: 1,
            resource_token: ValueState::Known("synthetic-resource".into()),
            manual_slot: slot,
        }
    }

    #[test]
    fn empty_combat_sets_are_unassigned_but_saved_equipment_assigns_one() {
        let mut bytes = [0_u8; unit_record::SIZE];
        bytes[0] = 1;
        bytes[0x126..0x12b].copy_from_slice(b"Set 1");
        let record = UnitRecord::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        let projected =
            project_unit(&record, 0, &|_, _| None).unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(sets) = projected.combat_sets.value else {
            panic!("combat sets expected")
        };
        assert!(sets
            .iter()
            .all(|set| set.assigned.value == ValueState::Known(false)));

        for index in 0..3 {
            let equipment = 0x126 + index * 88 + 0x42;
            for slot in 0..5 {
                bytes[equipment + slot * 2..equipment + slot * 2 + 2]
                    .copy_from_slice(&EMPTY_ITEM.to_le_bytes());
            }
            bytes[0x126 + index * 88 + 0x56] = 0xff;
            bytes[0x126 + index * 88 + 0x4c..0x126 + index * 88 + 0x50].fill(0xff);
        }
        let record = UnitRecord::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        let projected =
            project_unit(&record, 0, &|_, _| None).unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(sets) = projected.combat_sets.value else {
            panic!("combat sets expected")
        };
        assert!(sets
            .iter()
            .all(|set| set.assigned.value == ValueState::Known(false)));

        bytes[0x126 + 0x42..0x126 + 0x44].copy_from_slice(&1_u16.to_le_bytes());
        let record = UnitRecord::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        let projected =
            project_unit(&record, 0, &|_, _| None).unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(sets) = projected.combat_sets.value else {
            panic!("combat sets expected")
        };
        assert_eq!(sets[0].assigned.value, ValueState::Known(true));
        assert!(sets[1..]
            .iter()
            .all(|set| set.assigned.value == ValueState::Known(false)));
    }

    #[test]
    fn all_nonempty_positions_include_guests_inactive_and_every_job_pool() {
        let decoded = fixture();
        let before = decoded.payload().to_vec();
        let reader = decoded
            .reader_manual_save_v2(identity(0), |id, level| {
                assert_eq!(level, SpoilerLevel::Full);
                Some(CatalogueRef {
                    id: id.into(),
                    label: ValueState::Known(format!("Name {id}")),
                    description: ValueState::Unknown,
                    asset_key: ValueState::Unknown,
                })
            })
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(reader.document().schema, ReaderSchema::ReaderV2);
        assert_eq!(
            reader.document().progress.ramza_level.value,
            ValueState::Known(42)
        );
        let ValueState::Known(units) = &reader.document().roster.value else {
            panic!("occupied roster expected")
        };
        assert_eq!(
            units.iter().map(|unit| unit.key).collect::<Vec<_>>(),
            [0, 50, 53]
        );
        assert_eq!(
            units[0].membership.value,
            ValueState::Known(Membership::Party)
        );
        assert_eq!(
            units[1].membership.value,
            ValueState::Known(Membership::Guest)
        );
        assert_eq!(
            units[2].membership.value,
            ValueState::Known(Membership::Inactive)
        );
        assert_eq!(units[1].stored.experience.value, ValueState::Known(99));
        assert!(matches!(&units[0].name.value,
            ValueState::Known(name) if name.text == "Ramza"
                && name.origin == NameOrigin::UpstreamFallback));
        assert!(matches!(&units[1].equipment.head.value,
            ValueState::Known(item) if item.id == "item:999"));
        assert_eq!(units[1].equipment.body.value, ValueState::Absent);
        let ValueState::Known(jobs) = &units[1].jobs.value else {
            panic!("job pools expected")
        };
        assert_eq!(jobs.len(), 23);
        assert_eq!(jobs[22].slot, 22);
        assert_eq!(jobs[22].current_jp.value, ValueState::Known(345));
        assert_eq!(jobs[22].total_jp.value, ValueState::Known(600));
        assert_eq!(jobs[0].level.value, ValueState::Known(10));
        assert_eq!(jobs[1].level.value, ValueState::Known(5));
        assert_eq!(
            reader.document().progress.saved_at_unix_seconds.value,
            ValueState::Known(1_234)
        );
        assert_eq!(reader.document().gil.value, ValueState::Known(0));
        assert_eq!(decoded.payload(), before);
    }

    #[test]
    fn empty_slot_is_absent_and_invalid_slot_fails_closed() {
        let decoded = fixture();
        let empty = decoded
            .reader_manual_save_v2(identity(1), |_, _| None)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(empty.document().roster.value, ValueState::Absent);
        assert_eq!(empty.document().gil.value, ValueState::Absent);
        assert_eq!(
            decoded
                .reader_manual_save_v2(identity(50), |_, _| None)
                .err(),
            Some(ManualParseError::SlotIndex)
        );
    }

    #[test]
    fn occupied_slot_projects_full_width_gil() {
        let mut decoded = fixture();
        decoded.payload[0x10 + 0x86e4..0x10 + 0x86e8].copy_from_slice(&u32::MAX.to_le_bytes());
        let reader = decoded
            .reader_manual_save_v2(identity(0), |_, _| None)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            reader.document().gil.value,
            ValueState::Known(u64::from(u32::MAX))
        );
    }

    #[test]
    fn joins_every_party_position_to_its_item_key_without_losing_counts() {
        let mut decoded = fixture();
        let party_start = 0x10 + BATTLE_OFFSET + UNIT_COUNT * super::super::UNIT_SIZE;
        decoded.payload[party_start + 1] = 255;
        decoded.payload[party_start + 247] = 1;
        decoded.payload[party_start + 254] = 2;
        decoded.payload[party_start + 255] = 3;
        decoded.payload[party_start + 260] = 4;
        let before = decoded.payload().to_vec();
        let reader = decoded
            .reader_manual_save_v2(identity(0), |id, _| {
                Some(CatalogueRef {
                    id: id.into(),
                    label: ValueState::Known(format!("Name {id}")),
                    description: ValueState::Unknown,
                    asset_key: ValueState::Unknown,
                })
            })
            .unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(holdings) = &reader.document().inventory.value else {
            panic!("occupied inventory expected")
        };
        assert_eq!(holdings.len(), 261);
        for (index, holding) in holdings.iter().enumerate() {
            assert_eq!(usize::from(holding.key), index);
            if matches!(index, 0 | 254 | 255) {
                assert_eq!(holding.item.value, ValueState::Unknown);
                assert_eq!(holding.category.value, ValueState::Unknown);
            } else {
                assert!(matches!(&holding.item.value,
                    ValueState::Known(item) if item.id == format!("item:{index}")));
            }
        }
        assert_eq!(holdings[1].quantity.value, ValueState::Known(255));
        assert_eq!(holdings[2].quantity.value, ValueState::Known(0));
        assert_eq!(holdings[247].quantity.value, ValueState::Known(1));
        assert_eq!(
            holdings[247].category.value,
            ValueState::Known("Consumables".into())
        );
        assert_eq!(holdings[254].quantity.value, ValueState::Known(2));
        assert_eq!(holdings[255].quantity.value, ValueState::Known(3));
        assert_eq!(holdings[260].quantity.value, ValueState::Known(4));
        assert_eq!(
            holdings[260].category.value,
            ValueState::Known("Accessories".into())
        );
        assert_eq!(decoded.payload(), before);
    }

    #[test]
    fn missing_catalogue_leaves_party_identity_and_category_unknown() {
        let mut decoded = fixture();
        let party_start = 0x10 + BATTLE_OFFSET + UNIT_COUNT * super::super::UNIT_SIZE;
        decoded.payload[party_start + 247] = 1;
        let reader = decoded
            .reader_manual_save_v2(identity(0), |_, _| None)
            .unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(holdings) = &reader.document().inventory.value else {
            panic!("occupied inventory expected")
        };
        assert_eq!(holdings[247].item.value, ValueState::Unknown);
        assert_eq!(holdings[247].category.value, ValueState::Unknown);
        assert_eq!(holdings[247].quantity.value, ValueState::Known(1));
    }

    #[test]
    fn projects_verified_slot_progress_without_inventing_labels() {
        let mut decoded = fixture();
        let slot = &mut decoded.payload[0x10..0x10 + 0x9ce4];
        slot[4..11].copy_from_slice(b"Chapter");
        slot[0x101..0x105].copy_from_slice(b"Hero");
        slot[0x11c..0x120].copy_from_slice(&17_i32.to_le_bytes());
        slot[0x120..0x124].copy_from_slice(&90_i32.to_le_bytes());
        slot[0x983a] = 2;
        slot[0x518 + 0x32] = 0b0000_0011;
        slot[0x518 + 0x34] = 0b1000_0001;
        slot[0x518 + 50 * 600 + 2] = 0x5e;
        let event = 0x0518 + 54 * 600 + 0x105 + 0x105 + 0x80 + 2;
        slot[event..event + 4].copy_from_slice(&1_i32.to_le_bytes());
        slot[0x9460..0x9464].copy_from_slice(&465_i32.to_le_bytes());
        slot[event + 0x31 * 4..event + 0x32 * 4].copy_from_slice(&25_i32.to_le_bytes());
        let before = decoded.payload().to_vec();
        let reader = decoded
            .reader_manual_save_v2(identity(0), |_, _| None)
            .unwrap_or_else(|error| panic!("{error}"));
        let progress = &reader.document().progress;
        assert_eq!(progress.title.value, ValueState::Known("Chapter".into()));
        assert_eq!(progress.hero_name.value, ValueState::Known("Hero".into()));
        assert_eq!(progress.play_time_seconds.value, ValueState::Unknown);
        assert_eq!(progress.next_event_id.value, ValueState::Known(17));
        assert_eq!(progress.unnamed_event_values.value, ValueState::Known(2));
        assert_eq!(progress.difficulty_code.value, ValueState::Known(2));
        assert_eq!(progress.difficulty.value, ValueState::Unknown);
        assert_eq!(progress.location.value, ValueState::Unknown);
        assert_eq!(progress.chapter.value, ValueState::Unknown);
        assert_eq!(progress.story_progress.value, ValueState::Known(465));
        assert_eq!(progress.objective.value, ValueState::Unknown);
        assert_eq!(progress.area_index.value, ValueState::Known(25));
        let ValueState::Known(units) = &reader.document().roster.value else {
            panic!("roster expected")
        };
        let ValueState::Known(flags) = &units[0].saved_ability_flags.value else {
            panic!("flags expected")
        };
        assert_eq!(flags.len(), 22);
        assert_eq!(flags[0].active_positions, [0, 1]);
        assert_eq!(flags[0].passive_positions, [0, 7]);
        assert_eq!(units[1].kind.value, ValueState::Known(UnitKind::Monster));
        assert_eq!(decoded.payload(), before);
    }

    #[test]
    fn saved_nickname_precedes_character_and_catalogue_names() {
        let mut decoded = fixture();
        let nickname = 0x10 + 0x518 + 0xdc;
        decoded.payload[nickname..nickname + 6].copy_from_slice(b"Custom");
        let reader = decoded
            .reader_manual_save_v2(identity(0), |id, _| {
                Some(CatalogueRef {
                    id: id.into(),
                    label: ValueState::Known("Catalogue name".into()),
                    description: ValueState::Unknown,
                    asset_key: ValueState::Unknown,
                })
            })
            .unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(units) = &reader.document().roster.value else {
            panic!("occupied roster expected")
        };
        assert!(matches!(&units[0].name.value,
            ValueState::Known(name) if name.text == "Custom"
                && name.origin == NameOrigin::SavedRename));
    }

    #[test]
    fn portrait_identity_uses_exact_saved_name_key_before_character_template() {
        let mut decoded = fixture();
        let start = 0x10 + 0x518;
        decoded.payload[start] = 4;
        decoded.payload[start + 0x11c..start + 0x11e].copy_from_slice(&120_u16.to_le_bytes());
        decoded.payload[start + 0x230..start + 0x232].copy_from_slice(&121_u16.to_le_bytes());
        let lookup = |id: &str, _| {
            Some(CatalogueRef {
                id: id.into(),
                label: ValueState::Known(format!("Name {id}")),
                description: ValueState::Unknown,
                asset_key: ValueState::Unknown,
            })
        };
        let reader = decoded
            .reader_manual_save_v2(identity(0), lookup)
            .unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(units) = &reader.document().roster.value else {
            panic!("occupied roster expected")
        };
        assert_eq!(
            units[0].portrait.value,
            ValueState::Known("character:120".into())
        );

        let reader = decoded
            .reader_manual_save_v2(identity(0), |_, _| None)
            .unwrap_or_else(|error| panic!("{error}"));
        let ValueState::Known(units) = &reader.document().roster.value else {
            panic!("occupied roster expected")
        };
        assert_eq!(units[0].portrait.value, ValueState::Unknown);
    }

    #[test]
    fn ramza_level_stays_unknown_when_identity_is_duplicated() {
        let mut decoded = fixture();
        decoded.payload[0x10 + 0x518 + 50 * 600] = 2;
        let lookup = |id: &str, _| {
            Some(CatalogueRef {
                id: id.into(),
                label: ValueState::Known(format!("Name {id}")),
                description: ValueState::Unknown,
                asset_key: ValueState::Unknown,
            })
        };
        let reader = decoded
            .reader_manual_save_v2(identity(0), lookup)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            reader.document().progress.ramza_level.value,
            ValueState::Unknown
        );
    }

    #[test]
    fn ramza_level_accepts_later_chapter_identity() {
        let mut decoded = fixture();
        decoded.payload[0x10 + 0x518] = 3;
        let reader = decoded
            .reader_manual_save_v2(identity(0), |id, _| {
                Some(CatalogueRef {
                    id: id.into(),
                    label: ValueState::Known(format!("Name {id}")),
                    description: ValueState::Unknown,
                    asset_key: ValueState::Unknown,
                })
            })
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            reader.document().progress.ramza_level.value,
            ValueState::Known(42)
        );
    }
}
