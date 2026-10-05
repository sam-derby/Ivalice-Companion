//! Evidence-aware normalized save values.
//!
//! This crate deliberately contains no binary coordinates, paths, filesystem
//! access, Tauri types, or game-data labels. Adapters translate verified binary
//! records into these values only after their own bounds and support checks.

use serde::{Deserialize, Serialize};

pub mod ability_flags;
pub mod achievements;
pub mod calendar;
pub mod equipment_facts;
pub mod equipment_rules;
pub mod errands;
pub mod game_data;
pub mod identity;
pub mod job_eligibility;
pub mod reader;
pub mod side_quests;
pub mod story_progress;
pub mod story_roster;

/// The first stable wire schema for normalized save values.
///
/// This is independent of an embedded payload version or a game writer build.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveDomainSchema {
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
}

/// A value whose availability is explicit in the wire contract.
///
/// `Absent` means evidence establishes that no value is present. `Unknown`
/// means a value may exist but is not verified. `Unsupported` means the current
/// implementation does not support that part of the input. None of those
/// states serialize as a placeholder value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ValueState<T> {
    Known(T),
    Unknown,
    Absent,
    Unsupported,
}

/// Result of comparing the redundant stored zlib Adler trailer.
///
/// A mismatched value is only supplied by the container adapter after the
/// independent payload CRC, stream, length, and structure checks have passed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredAdlerStatus {
    Matched,
    Mismatched,
}

/// Byte length of a stable, bounded snapshot supplied to the format adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SnapshotByteLength(u64);

impl SnapshotByteLength {
    /// Construct a snapshot length. Empty input cannot be a published snapshot.
    pub fn new(value: u64) -> Result<Self, DomainValueError> {
        if value == 0 {
            return Err(DomainValueError::EmptySnapshot);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Non-identifying provenance retained for a stable snapshot.
///
/// Paths and source contents are intentionally not part of this DTO.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotProvenance {
    pub byte_length: SnapshotByteLength,
}

/// An evidence-scoped game writer build label.
///
/// The current supported structure has no verified writer build, so production
/// D004 values use `ValueState::Unknown` rather than inventing one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GameBuild(String);

impl GameBuild {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainValueError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainValueError::EmptyGameBuild);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Snapshot and writer-build provenance kept separate from the domain schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveProvenance {
    pub snapshot: SnapshotProvenance,
    pub writer_build: ValueState<GameBuild>,
}

/// Verified structural metadata returned by the bounded container decoder.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerMetadata {
    pub embedded_payload_version: u32,
    pub format_discriminator: u64,
    pub payload_byte_length: u32,
    pub stored_adler_status: StoredAdlerStatus,
}

/// A stable manual-slot identity in payload order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ManualSlotId(u8);

impl ManualSlotId {
    /// The R005 manual layout contains exactly fifty positions, indexed 0–49.
    pub fn new(index: u8) -> Result<Self, DomainValueError> {
        if index >= 50 {
            return Err(DomainValueError::ManualSlotOutOfRange(index));
        }
        Ok(Self(index))
    }

    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// The selected Steel cost corroborated by the owner's screen. This is a
/// numeric reference, not a purchase rule.
const SELECTED_STEEL_COST_JP: u16 = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericCostState {
    Enough,
    Shortfall,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SteelCostProgress {
    pub cost_jp: u16,
    pub shortfall_jp: u16,
    pub state: NumericCostState,
}

/// Compare a selected JP display candidate to Steel's installed cost only.
#[must_use]
pub fn selected_steel_cost_progress(squire_jp_candidate: u16) -> SteelCostProgress {
    let shortfall_jp = SELECTED_STEEL_COST_JP.saturating_sub(squire_jp_candidate);
    SteelCostProgress {
        cost_jp: SELECTED_STEEL_COST_JP,
        shortfall_jp,
        state: if shortfall_jp == 0 {
            NumericCostState::Enough
        } else {
            NumericCostState::Shortfall
        },
    }
}

/// Screen-corroborated ordinary job levels for Samurai's selected closure.
/// Actual job availability is a separate unknown predicate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamuraiPrerequisiteLevels {
    pub squire: u8,
    pub knight: u8,
    pub archer: u8,
    pub monk: u8,
    pub thief: u8,
    pub dragoon: u8,
}

/// The installed selected Samurai row from R027/R028. This evaluates level
/// requirements only; per-unit Change Job availability remains unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementState {
    Met,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelRequirementProgress {
    pub current_level: u8,
    pub required_level: u8,
    pub remaining_levels: u8,
    pub state: RequirementState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamuraiPrerequisiteProgress {
    pub squire: LevelRequirementProgress,
    pub knight: LevelRequirementProgress,
    pub archer: LevelRequirementProgress,
    pub monk: LevelRequirementProgress,
    pub thief: LevelRequirementProgress,
    pub dragoon: LevelRequirementProgress,
    pub all_level_requirements_met: bool,
}

fn compare_level(current_level: u8, required_level: u8) -> LevelRequirementProgress {
    let remaining_levels = required_level.saturating_sub(current_level);
    LevelRequirementProgress {
        current_level,
        required_level,
        remaining_levels,
        state: if remaining_levels == 0 {
            RequirementState::Met
        } else {
            RequirementState::Missing
        },
    }
}

/// Compare only the six installed Samurai level thresholds verified by R027.
#[must_use]
pub fn compare_samurai_prerequisites(
    levels: &SamuraiPrerequisiteLevels,
) -> SamuraiPrerequisiteProgress {
    let squire = compare_level(levels.squire, 2);
    let knight = compare_level(levels.knight, 4);
    let archer = compare_level(levels.archer, 3);
    let monk = compare_level(levels.monk, 5);
    let thief = compare_level(levels.thief, 4);
    let dragoon = compare_level(levels.dragoon, 2);
    let all_level_requirements_met = [&squire, &knight, &archer, &monk, &thief, &dragoon]
        .into_iter()
        .all(|progress| progress.state == RequirementState::Met);
    SamuraiPrerequisiteProgress {
        squire,
        knight,
        archer,
        monk,
        thief,
        dragoon,
        all_level_requirements_met,
    }
}

/// An occupied manual slot. Unit details belong to the versioned reader.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualSlot {
    pub id: ManualSlotId,
}

/// The complete D004 normalized result prepared for later IPC serialization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedSave {
    pub schema: SaveDomainSchema,
    pub provenance: SaveProvenance,
    pub container: ValueState<ContainerMetadata>,
    pub selected_manual_slot: ValueState<ManualSlot>,
}

impl NormalizedSave {
    #[must_use]
    pub fn supported(
        provenance: SaveProvenance,
        container: ContainerMetadata,
        selected_manual_slot: ValueState<ManualSlot>,
    ) -> Self {
        Self {
            schema: SaveDomainSchema::V6,
            provenance,
            container: ValueState::Known(container),
            selected_manual_slot,
        }
    }

    #[must_use]
    pub fn unsupported(provenance: SaveProvenance) -> Self {
        Self {
            schema: SaveDomainSchema::V6,
            provenance,
            container: ValueState::Unsupported,
            selected_manual_slot: ValueState::Unsupported,
        }
    }
}

/// Invalid input rejected by constructors with evidence-backed invariants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainValueError {
    EmptySnapshot,
    EmptyGameBuild,
    ManualSlotOutOfRange(u8),
}

impl std::fmt::Display for DomainValueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptySnapshot => formatter.write_str("a snapshot length must be nonzero"),
            Self::EmptyGameBuild => formatter.write_str("a game build label must not be empty"),
            Self::ManualSlotOutOfRange(index) => {
                write!(formatter, "manual slot index {index} is outside 0..50")
            }
        }
    }
}

impl std::error::Error for DomainValueError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance() -> SaveProvenance {
        SaveProvenance {
            snapshot: SnapshotProvenance {
                byte_length: SnapshotByteLength::new(512).unwrap_or_else(|error| panic!("{error}")),
            },
            writer_build: ValueState::Unknown,
        }
    }

    fn metadata() -> ContainerMetadata {
        ContainerMetadata {
            embedded_payload_version: 16,
            format_discriminator: 14,
            payload_byte_length: 2_008_216,
            stored_adler_status: StoredAdlerStatus::Mismatched,
        }
    }

    #[test]
    fn known_values_round_trip_without_replacing_the_unknown_build() {
        let slot = ManualSlot {
            id: ManualSlotId::new(7).unwrap_or_else(|error| panic!("{error}")),
        };
        let value = NormalizedSave::supported(provenance(), metadata(), ValueState::Known(slot));

        let encoded = serde_json::to_string(&value).unwrap_or_else(|error| panic!("{error}"));
        assert!(encoded.contains("\"schema\":\"v6\""));
        assert!(
            encoded.contains("\"selected_manual_slot\":{\"state\":\"known\",\"value\":{\"id\":7}}")
        );
        assert!(encoded.contains("\"writer_build\":{\"state\":\"unknown\"}"));
        assert!(encoded.contains("\"stored_adler_status\":\"mismatched\""));
        let decoded: NormalizedSave =
            serde_json::from_str(&encoded).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(decoded, value);
    }

    #[test]
    fn availability_states_have_distinct_serialized_forms() {
        let known = serde_json::to_string(&ValueState::Known(3_u8))
            .unwrap_or_else(|error| panic!("{error}"));
        let unknown = serde_json::to_string(&ValueState::<u8>::Unknown)
            .unwrap_or_else(|error| panic!("{error}"));
        let absent = serde_json::to_string(&ValueState::<u8>::Absent)
            .unwrap_or_else(|error| panic!("{error}"));
        let unsupported = serde_json::to_string(&ValueState::<u8>::Unsupported)
            .unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(known, r#"{"state":"known","value":3}"#);
        assert_eq!(unknown, r#"{"state":"unknown"}"#);
        assert_eq!(absent, r#"{"state":"absent"}"#);
        assert_eq!(unsupported, r#"{"state":"unsupported"}"#);
    }

    #[test]
    fn selected_samurai_progress_has_exact_thresholds_and_missing_counts() {
        let levels = SamuraiPrerequisiteLevels {
            squire: 1,
            knight: 3,
            archer: 3,
            monk: 7,
            thief: 2,
            dragoon: 2,
        };
        let progress = compare_samurai_prerequisites(&levels);
        assert!(!progress.all_level_requirements_met);
        assert_eq!(progress.squire.remaining_levels, 1);
        assert_eq!(progress.knight.required_level, 4);
        assert_eq!(progress.knight.state, RequirementState::Missing);
        assert_eq!(progress.thief.remaining_levels, 2);
        assert_eq!(progress.archer.state, RequirementState::Met);
        assert_eq!(progress.monk.remaining_levels, 0);
        assert_eq!(progress.dragoon.state, RequirementState::Met);

        let at_threshold = SamuraiPrerequisiteLevels {
            squire: 2,
            knight: 4,
            archer: 3,
            monk: 5,
            thief: 4,
            dragoon: 2,
        };
        let exactly_met = compare_samurai_prerequisites(&at_threshold);
        assert!(exactly_met.all_level_requirements_met);
        let above = SamuraiPrerequisiteLevels {
            knight: 8,
            ..at_threshold
        };
        let above_progress = compare_samurai_prerequisites(&above);
        assert_eq!(above_progress.knight.remaining_levels, 0);
        assert_eq!(above_progress.knight.state, RequirementState::Met);
        assert_eq!(
            serde_json::to_value(&above_progress).unwrap_or_else(|error| panic!("{error}"))
                ["knight"]["state"],
            "met"
        );
    }

    #[test]
    fn selected_steel_numeric_cost_handles_below_equal_above_and_zero() {
        for (jp, shortfall, state) in [
            (0, 200, NumericCostState::Shortfall),
            (18, 182, NumericCostState::Shortfall),
            (199, 1, NumericCostState::Shortfall),
            (200, 0, NumericCostState::Enough),
            (218, 0, NumericCostState::Enough),
            (u16::MAX, 0, NumericCostState::Enough),
        ] {
            let progress = selected_steel_cost_progress(jp);
            assert_eq!(progress.cost_jp, 200);
            assert_eq!(progress.shortfall_jp, shortfall);
            assert_eq!(progress.state, state);
            let decoded: SteelCostProgress = serde_json::from_value(
                serde_json::to_value(&progress).unwrap_or_else(|error| panic!("{error}")),
            )
            .unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(decoded, progress);
        }
    }

    #[test]
    fn unsupported_save_never_invents_a_slot_or_container() {
        let value = NormalizedSave::unsupported(provenance());
        let encoded = serde_json::to_value(value).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(encoded["container"]["state"], "unsupported");
        assert_eq!(encoded["selected_manual_slot"]["state"], "unsupported");
    }

    #[test]
    fn stable_slot_identifiers_and_constructors_reject_invalid_values() {
        assert_eq!(ManualSlotId::new(49).map(ManualSlotId::index), Ok(49));
        assert_eq!(
            ManualSlotId::new(50),
            Err(DomainValueError::ManualSlotOutOfRange(50))
        );
        assert_eq!(
            SnapshotByteLength::new(0),
            Err(DomainValueError::EmptySnapshot)
        );
        assert_eq!(GameBuild::new("  "), Err(DomainValueError::EmptyGameBuild));
    }
}
