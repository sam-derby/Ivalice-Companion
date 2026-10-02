//! Offline source text only. Catalogue membership never establishes a save join.
//! Reuses the pinned TICSaveEditor English Job, JobCommand, Ability, Item and
//! CharaName JSON resources. Importers verify input bytes; this boundary checks
//! the declared provenance and normalized structure, not the truth of arbitrary text.

use super::SpoilerLevel;
use crate::{reader::CatalogueRef, ValueState};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Write};

pub const MAX_READER_CATALOGUE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_READER_CATALOGUE_ENTRIES: usize = 4096;
pub const READER_CATALOGUE_SCHEMA: &str = "reader_catalogue_v1";
pub const READER_CATALOGUE_PROFILE: &str = "ticsaveeditor_english_text_v1";
pub const READER_CATALOGUE_REVISION: &str = "07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b";
const MAX_LABEL_BYTES: usize = 256;
const MAX_DESCRIPTION_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderCategory {
    Job,
    Command,
    Ability,
    Item,
    CharacterName,
}

impl ReaderCategory {
    #[must_use]
    pub const fn namespace(self) -> &'static str {
        match self {
            Self::Job => "job",
            Self::Command => "command",
            Self::Ability => "ability",
            Self::Item => "item",
            Self::CharacterName => "character_name",
        }
    }

    /// SHA-256 of the canonical Git object, not a platform-normalized checkout file.
    #[must_use]
    pub const fn input_sha256(self) -> &'static str {
        match self {
            Self::Job => "0bb0d6ac321c65ed80e9597cd5c8f6324e02bbc1a8446f9344cceb1643f4cd44",
            Self::Command => "f6dd33581c5880200c7f3e0b45e2e6f706061214227f989fef368baf25a6ef88",
            Self::Ability => "349d1a3640c6772872b24c3bf3a85dadfe54ddfa47ebbcd2b0eead16fbb77aa5",
            Self::Item => "300da30234c8434cfc2ed018e59cf8166b07432dfdd65456ff171d05908389a8",
            Self::CharacterName => {
                "b7f75515431176a497fb5ae32802bf912e6b9e7e26c12fb9bcc55e5226c22b82"
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderCatalogueSource {
    pub category: ReaderCategory,
    pub input_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderCatalogueEntry {
    pub category: ReaderCategory,
    pub id: String,
    pub label: ValueState<String>,
    /// Plain source text; LF is preserved. Consumers must not interpret it as markup.
    pub description: ValueState<String>,
    pub spoiler: SpoilerLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderCatalogueDocument {
    pub schema: String,
    pub profile: String,
    pub revision: String,
    pub locale: String,
    pub sources: Vec<ReaderCatalogueSource>,
    pub entries: Vec<ReaderCatalogueEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub job_commands: Vec<JobCommandRelation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mechanics: Option<ReaderMechanics>,
}

/// The pinned Job Nex row's command reference, kept separate from text entries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobCommandRelation {
    pub job: String,
    pub command: String,
}

/// Normalized selections from the pinned FFTIVC mod loader's hardcoded tables.
/// Keys are catalogue references; callers never interpret source table IDs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderMechanics {
    pub revision: String,
    pub sources: Vec<MechanicsSource>,
    pub jobs: Vec<JobMechanics>,
    pub items: Vec<ItemMechanics>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicsSource {
    pub table: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobMechanics {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub growth: Option<crate::reader::GrowthCoefficients>,
    pub hp_multiplier: u16,
    pub mp_multiplier: u16,
    pub speed_multiplier: u16,
    pub pa_multiplier: u16,
    pub ma_multiplier: u16,
    pub movement_tiles: u16,
    pub jump_tiles: u16,
    pub character_evasion_percent: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemMechanics {
    pub id: String,
    pub kind: ItemMechanicsKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    pub hp_bonus: i16,
    pub mp_bonus: i16,
    pub speed_bonus: i16,
    pub pa_bonus: i16,
    pub ma_bonus: i16,
    pub move_bonus: i16,
    pub jump_bonus: i16,
    pub physical_evasion_percent: u16,
    pub magical_evasion_percent: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemMechanicsKind {
    Weapon,
    Shield,
    Armor,
    Accessory,
    Other,
}

/// Errors never echo source text, local paths or identifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReaderCatalogueError {
    TooLarge,
    InvalidJson,
    UnsupportedSchema,
    SourceMismatch,
    InvalidId,
    DuplicateId,
    InvalidText,
    InvalidMechanics,
    InvalidJobCommands,
}

impl std::fmt::Display for ReaderCatalogueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "reader catalogue {self:?}")
    }
}

impl std::error::Error for ReaderCatalogueError {}

#[derive(Clone, Debug)]
pub struct ValidatedReaderCatalogue(ReaderCatalogueDocument);

impl ValidatedReaderCatalogue {
    pub fn from_json(bytes: &[u8]) -> Result<Self, ReaderCatalogueError> {
        if bytes.len() > MAX_READER_CATALOGUE_BYTES {
            return Err(ReaderCatalogueError::TooLarge);
        }
        let document =
            serde_json::from_slice(bytes).map_err(|_| ReaderCatalogueError::InvalidJson)?;
        Self::validate(document)
    }

    pub fn validate(mut document: ReaderCatalogueDocument) -> Result<Self, ReaderCatalogueError> {
        if document.schema != READER_CATALOGUE_SCHEMA {
            return Err(ReaderCatalogueError::UnsupportedSchema);
        }
        if document.profile != READER_CATALOGUE_PROFILE
            || document.revision != READER_CATALOGUE_REVISION
            || document.locale != "en"
            || document.sources.len() != 5
        {
            return Err(ReaderCatalogueError::SourceMismatch);
        }
        if document.entries.len() > MAX_READER_CATALOGUE_ENTRIES {
            return Err(ReaderCatalogueError::TooLarge);
        }
        let mut sources = BTreeSet::new();
        for source in &document.sources {
            if !sources.insert(source.category)
                || source.input_sha256 != source.category.input_sha256()
            {
                return Err(ReaderCatalogueError::SourceMismatch);
            }
        }
        let mut ids = BTreeSet::new();
        for entry in &document.entries {
            if !valid_id(&entry.id, entry.category) {
                return Err(ReaderCatalogueError::InvalidId);
            }
            if !ids.insert(&entry.id) {
                return Err(ReaderCatalogueError::DuplicateId);
            }
            if !valid_text(&entry.label, MAX_LABEL_BYTES, false)
                || !valid_text(&entry.description, MAX_DESCRIPTION_BYTES, true)
            {
                return Err(ReaderCatalogueError::InvalidText);
            }
        }
        if !document.job_commands.is_empty() {
            let jobs = document
                .entries
                .iter()
                .filter(|entry| entry.category == ReaderCategory::Job)
                .count();
            if document.job_commands.len() != jobs {
                return Err(ReaderCatalogueError::InvalidJobCommands);
            }
            let mut mapped = BTreeSet::new();
            for relation in &document.job_commands {
                if !valid_id(&relation.job, ReaderCategory::Job)
                    || !valid_id(&relation.command, ReaderCategory::Command)
                    || !ids.contains(&relation.job)
                    || !ids.contains(&relation.command)
                    || !mapped.insert(&relation.job)
                {
                    return Err(ReaderCatalogueError::InvalidJobCommands);
                }
            }
            document
                .job_commands
                .sort_by(|left, right| left.job.cmp(&right.job));
        }
        if let Some(mechanics) = &document.mechanics {
            validate_mechanics(mechanics)?;
        }
        document
            .sources
            .sort_by_key(|source| source.category.namespace());
        document
            .entries
            .sort_by(|left, right| left.id.cmp(&right.id));
        let validated = Self(document);
        // DTO construction has the same wire bound as loading JSON.
        validated.canonical_json()?;
        Ok(validated)
    }

    /// Persistence output is unfiltered. Never use this artifact as a UI projection.
    /// The infrastructure owner derives the resource token from these canonical bytes.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ReaderCatalogueError> {
        let mut output = BoundedJson {
            bytes: Vec::with_capacity(MAX_READER_CATALOGUE_BYTES),
            full: false,
        };
        if serde_json::to_writer(&mut output, &self.0).is_err() {
            return Err(if output.full {
                ReaderCatalogueError::TooLarge
            } else {
                ReaderCatalogueError::InvalidJson
            });
        }
        output
            .write_all(b"\n")
            .map_err(|_| ReaderCatalogueError::TooLarge)?;
        Ok(output.bytes)
    }

    /// Hidden and missing entries both return None, without a leaked label or description.
    /// A reference result contains no evidence of a saved identity or mechanical behavior.
    #[must_use]
    pub fn lookup(&self, id: &str, ceiling: SpoilerLevel) -> Option<CatalogueRef> {
        let index = self
            .0
            .entries
            .binary_search_by(|entry| entry.id.as_str().cmp(id))
            .ok()?;
        let entry = &self.0.entries[index];
        if entry.spoiler > ceiling {
            return None;
        }
        Some(CatalogueRef {
            id: entry.id.clone(),
            label: entry.label.clone(),
            description: entry.description.clone(),
            asset_key: ValueState::Unknown,
        })
    }

    #[must_use]
    pub fn mechanics(&self) -> Option<&ReaderMechanics> {
        self.0.mechanics.as_ref()
    }

    #[must_use]
    pub fn has_job_commands(&self) -> bool {
        !self.0.job_commands.is_empty()
    }

    #[must_use]
    pub fn command_for_job(&self, job: &str) -> ValueState<CatalogueRef> {
        let Some(index) = self
            .0
            .job_commands
            .binary_search_by(|relation| relation.job.as_str().cmp(job))
            .ok()
        else {
            return ValueState::Unknown;
        };
        let command = &self.0.job_commands[index].command;
        if command == "command:0" {
            ValueState::Absent
        } else {
            self.lookup(command, SpoilerLevel::Full)
                .filter(|reference| matches!(reference.label, ValueState::Known(_)))
                .map_or(ValueState::Unknown, ValueState::Known)
        }
    }

    /// A bundled mechanics extension may replace an older profile copy only
    /// when every catalogue text and provenance field is unchanged.
    #[must_use]
    pub fn same_text_as(&self, other: &Self) -> bool {
        self.0.schema == other.0.schema
            && self.0.profile == other.0.profile
            && self.0.revision == other.0.revision
            && self.0.locale == other.0.locale
            && self.0.sources == other.0.sources
            && self.0.entries == other.0.entries
    }
}

// fftivc.utility.modloader/TableData/*.xml at d3123d2 (v1.7.0).
const MECHANICS_SOURCES: [(&str, &str); 8] = [
    (
        "ItemAccessoryData",
        "f706004c1d155923478b27cea7fd9e9c49f09a60be18e5b4c5c8d8e7a3afad1e",
    ),
    (
        "ItemArmorData",
        "1dc02d979f91532383fc98fb02e1cd2eecc8fa82f51e2e3908d7e27cf2ee62c1",
    ),
    (
        "ItemData",
        "c724d1e5475eff2633d1ff3295f641f450dc341d90c443491860b5f5df1d49c5",
    ),
    (
        "ItemEquipBonusData",
        "5ebcad4babd93d796dc7c53c8d5e15e74deb334a5ab2ec1d86ef90fcfd1ea3f7",
    ),
    (
        "ItemOptionsData",
        "e15b1f24ce1425972a26015423f8a6954c15dd289151b3b4d1cbbc7ace7ad0cf",
    ),
    (
        "ItemShieldData",
        "e345891fa82dc4eecacd2956e93f423351832027ce8467cb125a6b0891022b56",
    ),
    (
        "ItemWeaponData",
        "30ca54ac24746960d86cf97f6f4467895960bf925a4437e20297f549a55b8de1",
    ),
    (
        "JobData",
        "63e49670e9f237824aa0fa0b4c333280bc8cbd041440fcf1638bdb5e35cf7f2b",
    ),
];

fn validate_mechanics(value: &ReaderMechanics) -> Result<(), ReaderCatalogueError> {
    if value.revision != "d3123d2"
        || (value.sources.len() != 7 && value.sources.len() != MECHANICS_SOURCES.len())
    {
        return Err(ReaderCatalogueError::SourceMismatch);
    }
    let expected: Vec<_> = MECHANICS_SOURCES
        .iter()
        .filter(|(name, _)| value.sources.len() == 8 || *name != "ItemOptionsData")
        .collect();
    for (actual, expected) in value.sources.iter().zip(expected) {
        if actual.table != expected.0 || actual.sha256 != expected.1 {
            return Err(ReaderCatalogueError::SourceMismatch);
        }
    }
    if value.jobs.len() != 174 || value.items.len() != 261 {
        return Err(ReaderCatalogueError::InvalidMechanics);
    }
    for (index, job) in value.jobs.iter().enumerate() {
        if job.id != format!("job:{index}")
            || [
                job.hp_multiplier,
                job.mp_multiplier,
                job.speed_multiplier,
                job.pa_multiplier,
                job.ma_multiplier,
            ]
            .iter()
            .any(|number| *number > 255)
            || job.movement_tiles > 99
            || job.jump_tiles > 255
            || job.character_evasion_percent > 100
        {
            return Err(ReaderCatalogueError::InvalidMechanics);
        }
    }
    for (index, item) in value.items.iter().enumerate() {
        if (value.sources.len() == 7 && !item.details.is_empty())
            || item.details.len() > 32
            || item.details.iter().any(|text| {
                text.is_empty() || text.len() > 512 || text.chars().any(char::is_control)
            })
            || item.id != format!("item:{index}")
            || [
                item.hp_bonus,
                item.mp_bonus,
                item.speed_bonus,
                item.pa_bonus,
                item.ma_bonus,
                item.move_bonus,
                item.jump_bonus,
            ]
            .iter()
            .any(|number| i32::from(*number).abs() > 999)
            || item.physical_evasion_percent > 100
            || item.magical_evasion_percent > 100
        {
            return Err(ReaderCatalogueError::InvalidMechanics);
        }
    }
    Ok(())
}

fn valid_id(value: &str, category: ReaderCategory) -> bool {
    let Some((namespace, key)) = value.split_once(':') else {
        return false;
    };
    namespace == category.namespace()
        && !key.is_empty()
        && (key.len() == 1 || !key.starts_with('0'))
        && key.bytes().all(|byte| byte.is_ascii_digit())
        && key.parse::<u16>().is_ok()
}

fn valid_text(value: &ValueState<String>, limit: usize, description: bool) -> bool {
    match value {
        ValueState::Known(text) => {
            !text.trim().is_empty()
                && text.len() <= limit
                && !text
                    .chars()
                    .any(|character| character.is_control() && !(description && character == '\n'))
        }
        ValueState::Unknown => true,
        ValueState::Absent | ValueState::Unsupported => false,
    }
}

/// Capped memory sink, following the reader contract's pre-growth output check.
struct BoundedJson {
    bytes: Vec<u8>,
    full: bool,
}

impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_READER_CATALOGUE_BYTES.saturating_sub(self.bytes.len()) {
            self.full = true;
            return Err(std::io::Error::other("reader_catalogue_output_limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
