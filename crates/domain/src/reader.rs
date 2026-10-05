//! Versioned reader facts. Validation checks structure and bounded values.

use crate::{SamuraiPrerequisiteProgress, SteelCostProgress, ValueState};
use serde::{Deserialize, Serialize};

pub mod commands;
pub mod level_growth;
pub mod stat_edit;
pub mod stats;
mod validation;
pub use validation::{ReaderError, ValidatedReader};

pub const MAX_READER_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_READER_UNITS: usize = 54;
pub const MAX_READER_ITEMS: usize = 1024;
pub const MAX_READER_ABILITIES: usize = 1024;
pub const MAX_READER_JOBS: usize = 256;
pub const MAX_PROGRESS_ENTRIES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderSchema {
    ReaderV2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderProfile {
    EnglishSteamEnhancedManual,
}

/// Session is an opaque application token, never a path or account identifier.
/// Generations are independently monotonic within that session, not timestamps.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderIdentity {
    pub session: String,
    pub snapshot_generation: u64,
    pub resource_generation: u64,
    pub resource_token: ValueState<String>,
    pub manual_slot: u8,
}

/// A decoded or catalogue-joined value. The reader does not apply companion
/// spoiler settings to the player's own save data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact<T> {
    pub value: ValueState<T>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueRef {
    /// A normalized namespaced catalogue key, never an unjoined save-table ID.
    pub id: String,
    pub label: ValueState<String>,
    pub description: ValueState<String>,
    pub asset_key: ValueState<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    RegularHuman,
    UniqueHuman,
    Monster,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Membership {
    Party,
    Guest,
    Inactive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameOrigin {
    SavedRename,
    LocalizedCatalogue,
    UpstreamFallback,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitName {
    pub text: String,
    pub origin: NameOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredStats {
    pub level: Fact<u8>,
    pub experience: Fact<u8>,
    pub brave: Fact<u8>,
    pub faith: Fact<u8>,
    pub sex: Fact<String>,
    pub birthday: Fact<String>,
    pub zodiac: Fact<String>,
    pub statuses: Fact<Vec<CatalogueRef>>,
    pub bases: StoredBases,
}

/// These are stored integers, never status-screen values. No formula is implied.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredBases {
    pub hp: Fact<u32>,
    pub mp: Fact<u32>,
    pub speed: Fact<u32>,
    pub physical_attack: Fact<u32>,
    pub magical_attack: Fact<u32>,
}

/// JobData growth coefficients, not percentages or accumulated base stats.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthCoefficients {
    pub hp: u8,
    pub mp: u8,
    pub speed: u8,
    pub physical_attack: u8,
    pub magical_attack: u8,
}

fn unknown_growth() -> Fact<GrowthCoefficients> {
    Fact {
        value: ValueState::Unknown,
    }
}

/// Evasion uses basis points (one hundredth of one percent), not raw table units.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveStats {
    pub hp: Fact<u32>,
    pub mp: Fact<u32>,
    pub speed: Fact<u32>,
    pub physical_attack: Fact<u32>,
    pub magical_attack: Fact<u32>,
    pub movement_tiles: Fact<u16>,
    pub jump_tiles: Fact<u16>,
    pub evasion: Fact<Vec<Evasion>>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub breakdown: std::collections::BTreeMap<stat_edit::BaseStatKind, StatBreakdown>,
}

/// Game-visible value before equipment, not the stored fixed-point integer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatBreakdown {
    pub base: Fact<u32>,
    pub equipment_bonus: Fact<i32>,
    pub job_multiplier: Fact<u16>,
    pub maximum: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvasionSource {
    Character,
    Accessory,
    RightShield,
    LeftShield,
    RightWeapon,
    LeftWeapon,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evasion {
    pub source: EvasionSource,
    pub physical_basis_points: Fact<u16>,
    pub magical_basis_points: Fact<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobState {
    /// Upstream's dense progress position; `job` is resolved from its class map.
    pub slot: u8,
    pub job: Fact<CatalogueRef>,
    pub level: Fact<u8>,
    pub current_jp: Fact<u16>,
    pub total_jp: Fact<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearnedAbility {
    pub ability: Fact<CatalogueRef>,
    pub learned: Fact<bool>,
    pub jp_cost: Fact<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedAbilityFlagGroup {
    pub slot: u8,
    pub active_positions: Vec<u8>,
    pub passive_positions: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityLoadout {
    pub primary_command: Fact<CatalogueRef>,
    pub secondary_command: Fact<CatalogueRef>,
    pub reaction: Fact<CatalogueRef>,
    pub support: Fact<CatalogueRef>,
    pub movement: Fact<CatalogueRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentEquipment {
    pub head: Fact<CatalogueRef>,
    pub body: Fact<CatalogueRef>,
    pub accessory: Fact<CatalogueRef>,
    pub right_weapon: Fact<CatalogueRef>,
    pub right_shield: Fact<CatalogueRef>,
    pub left_weapon: Fact<CatalogueRef>,
    pub left_shield: Fact<CatalogueRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatSet {
    /// Distinct saved-set identity within this unit; not a catalogue ID.
    pub key: u8,
    pub assigned: Fact<bool>,
    pub name: Fact<String>,
    pub job: Fact<CatalogueRef>,
    pub head: Fact<CatalogueRef>,
    pub body: Fact<CatalogueRef>,
    pub accessory: Fact<CatalogueRef>,
    pub right_hand: Fact<CatalogueRef>,
    pub left_hand: Fact<CatalogueRef>,
    pub abilities: AbilityLoadout,
    pub double_hand: Fact<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitGuidance {
    pub target_job: Fact<CatalogueRef>,
    pub samurai_prerequisites: Fact<SamuraiPrerequisiteProgress>,
    pub steel_cost: Fact<SteelCostProgress>,
    /// These require separate positive evidence; comparisons do not set them.
    pub job_eligible: Fact<bool>,
    pub ability_purchasable: Fact<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderUnit {
    /// Snapshot-local selection key; neither a persistent identity nor a name.
    pub key: u16,
    pub persistent_identity: Fact<String>,
    pub name: Fact<UnitName>,
    pub kind: Fact<UnitKind>,
    pub membership: Fact<Membership>,
    pub sprite: Fact<String>,
    pub portrait: Fact<String>,
    pub stored: StoredStats,
    pub effective: EffectiveStats,
    #[serde(default = "unknown_growth")]
    pub growth: Fact<GrowthCoefficients>,
    pub current_job: Fact<CatalogueRef>,
    pub jobs: Fact<Vec<JobState>>,
    pub learned_abilities: Fact<Vec<LearnedAbility>>,
    pub saved_ability_flags: Fact<Vec<SavedAbilityFlagGroup>>,
    pub abilities: AbilityLoadout,
    pub equipment: CurrentEquipment,
    pub combat_sets: Fact<Vec<CombatSet>>,
    pub selected_combat_set: Fact<u8>,
    pub guidance: UnitGuidance,
    /// Complete upstream unit record for field-by-field parity checks.
    pub saved: SavedUnitRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedUnitRecord {
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
    pub combat_sets: [SavedCombatSet; 3],
    pub pad: u16,
    pub chara_name_key: u16,
    pub pad2: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedCombatSet {
    pub name_raw: [u8; 16],
    pub name_padding: Vec<u8>,
    pub equipment: [u16; 5],
    pub skillsets: [i16; 2],
    pub abilities: [u16; 3],
    pub job: u8,
    pub is_double_hand: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Holding {
    /// Snapshot-local row key allows unidentified holdings without a false join.
    pub key: u16,
    pub item: Fact<CatalogueRef>,
    pub category: Fact<String>,
    pub quantity: Fact<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressEntry {
    pub subject: Fact<CatalogueRef>,
    /// An evidenced label, not an invented universal event-flag interpretation.
    pub saved_state: Fact<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedProgress {
    pub title: Fact<String>,
    pub saved_at_unix_seconds: Fact<i64>,
    pub hero_name: Fact<String>,
    pub location: Fact<CatalogueRef>,
    pub difficulty: Fact<CatalogueRef>,
    pub difficulty_code: Fact<u8>,
    pub chapter: Fact<String>,
    /// Saved main story track value, before any table join.
    pub story_progress: Fact<i32>,
    pub objective: Fact<String>,
    /// Saved world-map area index, before any table join.
    pub area_index: Fact<u8>,
    pub ramza_level: Fact<u8>,
    pub story: Fact<Vec<ProgressEntry>>,
    pub play_time_seconds: Fact<u64>,
    pub next_event_id: Fact<i32>,
    pub unnamed_event_values: Fact<u16>,
    pub errands: Fact<Vec<ProgressEntry>>,
    pub events: Fact<Vec<ProgressEntry>>,
    pub recruitment: Fact<Vec<ProgressEntry>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderDocument {
    pub schema: ReaderSchema,
    pub profile: ReaderProfile,
    pub identity: ReaderIdentity,
    pub roster: Fact<Vec<ReaderUnit>>,
    pub inventory: Fact<Vec<Holding>>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub item_details: std::collections::BTreeMap<String, Vec<String>>,
    pub gil: Fact<u64>,
    pub progress: SavedProgress,
}
