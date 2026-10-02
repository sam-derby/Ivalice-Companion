//! Bounded, validated reference facts. Raw table columns and save IDs do not enter this module.

use crate::ValueState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod inventory_category;
mod query;
pub mod reader_catalogue;
pub use query::{ReferenceIdentity, ReferenceQueries, VisibleCoverage};

pub const MAX_ARTIFACT_BYTES: usize = 256 * 1024;
const MAX_LABEL_BYTES: usize = 256;
const MAX_NODES: usize = 1024;
const MAX_EDGES: usize = 4096;
const MAX_EXPRESSION_DEPTH: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameDataSchema {
    V1,
}

/// This profile names the exact R011 catalogue and R022 installed-job parity/evaluator evidence.
/// It does not assert that the other R011 tables came from installed build 24304444.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceProfile {
    R011R022English,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub profile: SourceProfile,
    pub catalogue_revision: String,
    pub installed_job_build: String,
    pub locale: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    R011,
    R022,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpoilerLevel {
    Minimal,
    Hints,
    Gameplay,
    Full,
}

/// Namespaced IDs remain distinct even if the localized labels collide.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub String);
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(pub String);
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AbilityId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: JobId,
    pub english_label: ValueState<String>,
    pub command: ValueState<CommandId>,
    pub requirement: ValueState<Requirement>,
    pub evidence: Evidence,
    pub spoiler: SpoilerLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub id: CommandId,
    pub english_label: ValueState<String>,
    pub evidence: Evidence,
    pub spoiler: SpoilerLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ability {
    pub id: AbilityId,
    pub english_label: ValueState<String>,
    pub jp_cost: ValueState<u16>,
    pub evidence: Evidence,
    pub spoiler: SpoilerLevel,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotFamily {
    Action,
    ReactionSupportMovement,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Membership {
    pub command: CommandId,
    pub family: SlotFamily,
    pub ordinal: u8,
    pub ability: AbilityId,
    pub evidence: Evidence,
    pub spoiler: SpoilerLevel,
}

/// Only conjunction and a level lower bound are evidenced. No OR or eligibility result exists.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Requirement {
    AtLeastLevel { job: JobId, level: u8 },
    All { terms: Vec<Requirement> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Count {
    pub accepted: u16,
    pub unknown: u16,
    pub rejected: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    /// R011's 174 job identities. A selected job may still have unknown fields.
    pub jobs: Count,
    /// R011's 227 command identities. V1 accepts only command 25 for memberships.
    pub commands: Count,
    /// R011's 512 ability identities; only accepted cost facts count here.
    pub ability_costs: Count,
    /// R011's 1,061 nonzero accepted membership edges are the bounded source pool.
    pub memberships: Count,
    /// R022's 32 GeneralJob numeric requirements, one accepted.
    pub prerequisites: Count,
    /// R022 accepts no job as having no requirements.
    pub no_requirement_jobs: Count,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameDataArtifact {
    pub schema: GameDataSchema,
    pub provenance: Provenance,
    pub coverage: Coverage,
    pub jobs: Vec<Job>,
    pub commands: Vec<Command>,
    pub abilities: Vec<Ability>,
    pub memberships: Vec<Membership>,
}

/// The only queryable form. Its field is private so consumers must validate first.
#[derive(Clone, Debug)]
pub struct ValidatedGameData(GameDataArtifact);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationCode {
    TooLarge,
    InvalidJson,
    UnsupportedSchema,
    SourceMismatch,
    InvalidId,
    DuplicateId,
    InvalidLabel,
    InvalidValue,
    MissingReference,
    InvalidExpression,
    Cycle,
    InvalidCoverage,
    InvalidEvidence,
    InvalidOrdering,
    MissingSpoiler,
}

/// No input row, path or label is echoed in an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationError {
    pub code: ValidationCode,
    pub index: usize,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "game-data validation {:?} at index {}",
            self.code, self.index
        )
    }
}
impl std::error::Error for ValidationError {}

fn error(code: ValidationCode, index: usize) -> ValidationError {
    ValidationError { code, index }
}

fn valid_id(value: &str, prefix: &str, max: u16) -> bool {
    let Some(suffix) = value.strip_prefix(prefix) else {
        return false;
    };
    if suffix.is_empty()
        || (suffix.len() > 1 && suffix.starts_with('0'))
        || !suffix.bytes().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    suffix.parse::<u16>().is_ok_and(|number| number <= max)
}

fn valid_label(value: &ValueState<String>) -> bool {
    match value {
        ValueState::Known(label) => {
            !label.trim().is_empty()
                && label.len() <= MAX_LABEL_BYTES
                && !label.chars().any(char::is_control)
        }
        ValueState::Unknown | ValueState::Absent | ValueState::Unsupported => true,
    }
}

fn check_count(
    count: Count,
    total: u16,
    accepted: usize,
    index: usize,
) -> Result<(), ValidationError> {
    if usize::from(count.accepted) != accepted
        || u32::from(count.accepted) + u32::from(count.unknown) + u32::from(count.rejected)
            != u32::from(total)
    {
        return Err(error(ValidationCode::InvalidCoverage, index));
    }
    Ok(())
}

fn collect_requirements<'a>(
    requirement: &'a Requirement,
    depth: usize,
    out: &mut Vec<(&'a JobId, u8)>,
) -> Result<(), ValidationError> {
    if depth > MAX_EXPRESSION_DEPTH || out.len() > MAX_EDGES {
        return Err(error(ValidationCode::InvalidExpression, 0));
    }
    match requirement {
        Requirement::AtLeastLevel { job, level } => {
            if !(1..=99).contains(level) {
                return Err(error(ValidationCode::InvalidExpression, 0));
            }
            out.push((job, *level));
        }
        Requirement::All { terms } => {
            if !(2..=16).contains(&terms.len()) {
                return Err(error(ValidationCode::InvalidExpression, 0));
            }
            for term in terms {
                collect_requirements(term, depth + 1, out)?;
            }
        }
    }
    Ok(())
}

fn visit<'a>(
    node: &'a str,
    graph: &BTreeMap<&'a str, Vec<&'a str>>,
    active: &mut BTreeSet<&'a str>,
    done: &mut BTreeSet<&'a str>,
) -> Result<(), ValidationError> {
    if done.contains(node) {
        return Ok(());
    }
    if !active.insert(node) {
        return Err(error(ValidationCode::Cycle, 0));
    }
    if let Some(next) = graph.get(node) {
        for child in next {
            visit(child, graph, active, done)?;
        }
    }
    active.remove(node);
    done.insert(node);
    Ok(())
}

impl ValidatedGameData {
    /// Deserialize a bounded UTF-8 JSON artifact, then validate every queryable fact.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ValidationError> {
        if bytes.len() > MAX_ARTIFACT_BYTES {
            return Err(error(ValidationCode::TooLarge, 0));
        }
        let artifact: GameDataArtifact = match serde_json::from_slice(bytes) {
            Ok(artifact) => artifact,
            Err(_) => {
                // The typed pass detects duplicate object keys. Inspect a failed input
                // only to select a stable error code; never deserialize from Value.
                let value: serde_json::Value = serde_json::from_slice(bytes)
                    .map_err(|_| error(ValidationCode::InvalidJson, 0))?;
                if value.get("schema").and_then(serde_json::Value::as_str) != Some("v1") {
                    return Err(error(ValidationCode::UnsupportedSchema, 0));
                }
                if value
                    .pointer("/provenance/profile")
                    .and_then(serde_json::Value::as_str)
                    != Some("r011_r022_english")
                {
                    return Err(error(ValidationCode::SourceMismatch, 0));
                }
                for group in ["jobs", "commands", "abilities", "memberships"] {
                    if let Some(rows) = value.get(group).and_then(serde_json::Value::as_array) {
                        if let Some(index) =
                            rows.iter().position(|row| row.get("spoiler").is_none())
                        {
                            return Err(error(ValidationCode::MissingSpoiler, index));
                        }
                    }
                }
                return Err(error(ValidationCode::InvalidJson, 0));
            }
        };
        Self::validate(artifact)
    }

    pub fn validate(data: GameDataArtifact) -> Result<Self, ValidationError> {
        if data.schema != GameDataSchema::V1 {
            return Err(error(ValidationCode::UnsupportedSchema, 0));
        }
        let p = &data.provenance;
        if p.profile != SourceProfile::R011R022English
            || p.catalogue_revision != "07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b"
            || p.installed_job_build != "24304444"
            || p.locale != "en"
        {
            return Err(error(ValidationCode::SourceMismatch, 0));
        }
        if data.jobs.len() > MAX_NODES
            || data.commands.len() > MAX_NODES
            || data.abilities.len() > MAX_NODES
            || data.memberships.len() > MAX_EDGES
        {
            return Err(error(ValidationCode::TooLarge, 0));
        }
        let mut job_ids = BTreeSet::new();
        for (index, job) in data.jobs.iter().enumerate() {
            if !valid_id(&job.id.0, "job:", 173) {
                return Err(error(ValidationCode::InvalidId, index));
            }
            if !job_ids.insert(job.id.0.as_str()) {
                return Err(error(ValidationCode::DuplicateId, index));
            }
            if !valid_label(&job.english_label) {
                return Err(error(ValidationCode::InvalidLabel, index));
            }
            if index > 0 && data.jobs[index - 1].id >= job.id {
                return Err(error(ValidationCode::InvalidOrdering, index));
            }
        }
        let mut command_ids = BTreeSet::new();
        for (index, command) in data.commands.iter().enumerate() {
            if !valid_id(&command.id.0, "command:", 226) {
                return Err(error(ValidationCode::InvalidId, index));
            }
            if !command_ids.insert(command.id.0.as_str()) {
                return Err(error(ValidationCode::DuplicateId, index));
            }
            if !valid_label(&command.english_label) {
                return Err(error(ValidationCode::InvalidLabel, index));
            }
            if index > 0 && data.commands[index - 1].id >= command.id {
                return Err(error(ValidationCode::InvalidOrdering, index));
            }
        }
        let mut ability_ids = BTreeSet::new();
        for (index, ability) in data.abilities.iter().enumerate() {
            if !valid_id(&ability.id.0, "ability:", 511) {
                return Err(error(ValidationCode::InvalidId, index));
            }
            if !ability_ids.insert(ability.id.0.as_str()) {
                return Err(error(ValidationCode::DuplicateId, index));
            }
            if !valid_label(&ability.english_label) {
                return Err(error(ValidationCode::InvalidLabel, index));
            }
            if index > 0 && data.abilities[index - 1].id >= ability.id {
                return Err(error(ValidationCode::InvalidOrdering, index));
            }
        }
        let mut graph: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut accepted_reqs = 0;
        for (index, job) in data.jobs.iter().enumerate() {
            if (job.id.0 == "job:74" || job.id.0 == "job:76") && job.evidence != Evidence::R022 {
                return Err(error(ValidationCode::InvalidEvidence, index));
            }
            if let ValueState::Known(command) = &job.command {
                if !command_ids.contains(command.0.as_str()) {
                    return Err(error(ValidationCode::MissingReference, index));
                }
                if job.id.0 != "job:1"
                    || command.0 != "command:25"
                    || job.evidence != Evidence::R011
                {
                    return Err(error(ValidationCode::InvalidEvidence, index));
                }
            }
            if let ValueState::Known(requirement) = &job.requirement {
                let mut edges = Vec::new();
                collect_requirements(requirement, 0, &mut edges)?;
                accepted_reqs += edges.len();
                for (target, _) in &edges {
                    if !job_ids.contains(target.0.as_str()) {
                        return Err(error(ValidationCode::MissingReference, index));
                    }
                    graph
                        .entry(job.id.0.as_str())
                        .or_default()
                        .push(target.0.as_str());
                }
            } else if matches!(job.requirement, ValueState::Absent) {
                return Err(error(ValidationCode::InvalidEvidence, index));
            }
        }
        let mut active = BTreeSet::new();
        let mut done = BTreeSet::new();
        for node in &job_ids {
            visit(node, &graph, &mut active, &mut done)?;
        }
        for (index, job) in data.jobs.iter().enumerate() {
            if let ValueState::Known(requirement) = &job.requirement {
                if job.id.0 != "job:76"
                    || job.evidence != Evidence::R022
                    || *requirement
                        != (Requirement::AtLeastLevel {
                            job: JobId("job:74".into()),
                            level: 2,
                        })
                {
                    return Err(error(ValidationCode::InvalidEvidence, index));
                }
            }
        }
        for (index, member) in data.memberships.iter().enumerate() {
            if !command_ids.contains(member.command.0.as_str())
                || !ability_ids.contains(member.ability.0.as_str())
            {
                return Err(error(ValidationCode::MissingReference, index));
            }
            let max_ordinal = match member.family {
                SlotFamily::Action => 16,
                SlotFamily::ReactionSupportMovement => 6,
            };
            if !(1..=max_ordinal).contains(&member.ordinal) {
                return Err(error(ValidationCode::InvalidValue, index));
            }
            if member.command.0 != "command:25" || member.evidence != Evidence::R011 {
                return Err(error(ValidationCode::InvalidEvidence, index));
            }
            if index > 0 {
                let previous = &data.memberships[index - 1];
                let key = (&member.command, member.family, member.ordinal);
                let prior = (&previous.command, previous.family, previous.ordinal);
                if prior == key {
                    return Err(error(ValidationCode::DuplicateId, index));
                }
                if prior > key {
                    return Err(error(ValidationCode::InvalidOrdering, index));
                }
            }
        }
        for (index, command) in data.commands.iter().enumerate() {
            if command.id.0 != "command:25" || command.evidence != Evidence::R011 {
                return Err(error(ValidationCode::InvalidEvidence, index));
            }
        }
        for (index, ability) in data.abilities.iter().enumerate() {
            if ability.evidence != Evidence::R011 {
                return Err(error(ValidationCode::InvalidEvidence, index));
            }
            const REJECTED_COSTS: [&str; 10] = [
                "ability:44",
                "ability:67",
                "ability:260",
                "ability:264",
                "ability:413",
                "ability:480",
                "ability:491",
                "ability:498",
                "ability:507",
                "ability:510",
            ];
            if matches!(ability.jp_cost, ValueState::Known(_))
                && REJECTED_COSTS.contains(&ability.id.0.as_str())
            {
                return Err(error(ValidationCode::InvalidEvidence, index));
            }
        }
        check_count(data.coverage.jobs, 174, data.jobs.len(), 0)?;
        check_count(data.coverage.commands, 227, data.commands.len(), 1)?;
        if data.coverage.ability_costs.rejected != 10 {
            return Err(error(ValidationCode::InvalidCoverage, 2));
        }
        check_count(
            data.coverage.ability_costs,
            512,
            data.abilities
                .iter()
                .filter(|a| matches!(a.jp_cost, ValueState::Known(_)))
                .count(),
            2,
        )?;
        check_count(data.coverage.memberships, 1061, data.memberships.len(), 3)?;
        check_count(data.coverage.prerequisites, 32, accepted_reqs, 4)?;
        check_count(data.coverage.no_requirement_jobs, 0, 0, 5)?;
        if accepted_reqs != 1
            || !data.jobs.iter().any(|j| j.id.0 == "job:74")
            || !data.jobs.iter().any(|j| j.id.0 == "job:76")
        {
            return Err(error(ValidationCode::InvalidEvidence, 0));
        }
        let encoded =
            serde_json::to_vec(&data).map_err(|_| error(ValidationCode::InvalidJson, 0))?;
        if encoded.len() + 1 > MAX_ARTIFACT_BYTES {
            return Err(error(ValidationCode::TooLarge, 0));
        }
        Ok(Self(data))
    }

    #[must_use]
    pub fn artifact(&self) -> &GameDataArtifact {
        &self.0
    }

    /// Deterministic compact UTF-8 JSON with one terminal LF. Input arrays must already be sorted.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ValidationError> {
        let mut bytes =
            serde_json::to_vec(&self.0).map_err(|_| error(ValidationCode::InvalidJson, 0))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> GameDataArtifact {
        GameDataArtifact {
            schema: GameDataSchema::V1,
            provenance: Provenance {
                profile: SourceProfile::R011R022English,
                catalogue_revision: "07ea857a0d2b7a96190f18ee10cfd99dbcf37a1b".into(),
                installed_job_build: "24304444".into(),
                locale: "en".into(),
            },
            coverage: Coverage {
                jobs: Count {
                    accepted: 3,
                    unknown: 171,
                    rejected: 0,
                },
                commands: Count {
                    accepted: 1,
                    unknown: 226,
                    rejected: 0,
                },
                ability_costs: Count {
                    accepted: 1,
                    unknown: 501,
                    rejected: 10,
                },
                memberships: Count {
                    accepted: 1,
                    unknown: 1060,
                    rejected: 0,
                },
                prerequisites: Count {
                    accepted: 1,
                    unknown: 31,
                    rejected: 0,
                },
                no_requirement_jobs: Count {
                    accepted: 0,
                    unknown: 0,
                    rejected: 0,
                },
            },
            jobs: vec![
                Job {
                    id: JobId("job:1".into()),
                    english_label: ValueState::Known("Squire".into()),
                    command: ValueState::Known(CommandId("command:25".into())),
                    requirement: ValueState::Unknown,
                    evidence: Evidence::R011,
                    spoiler: SpoilerLevel::Minimal,
                },
                Job {
                    id: JobId("job:74".into()),
                    english_label: ValueState::Known("Squire".into()),
                    command: ValueState::Unknown,
                    requirement: ValueState::Unknown,
                    evidence: Evidence::R022,
                    spoiler: SpoilerLevel::Minimal,
                },
                Job {
                    id: JobId("job:76".into()),
                    english_label: ValueState::Known("Knight".into()),
                    command: ValueState::Unknown,
                    requirement: ValueState::Known(Requirement::AtLeastLevel {
                        job: JobId("job:74".into()),
                        level: 2,
                    }),
                    evidence: Evidence::R022,
                    spoiler: SpoilerLevel::Minimal,
                },
            ],
            commands: vec![Command {
                id: CommandId("command:25".into()),
                english_label: ValueState::Known("Mettle".into()),
                evidence: Evidence::R011,
                spoiler: SpoilerLevel::Minimal,
            }],
            abilities: vec![Ability {
                id: AbilityId("ability:152".into()),
                english_label: ValueState::Known("Chant".into()),
                jp_cost: ValueState::Known(0),
                evidence: Evidence::R011,
                spoiler: SpoilerLevel::Minimal,
            }],
            memberships: vec![Membership {
                command: CommandId("command:25".into()),
                family: SlotFamily::Action,
                ordinal: 6,
                ability: AbilityId("ability:152".into()),
                evidence: Evidence::R011,
                spoiler: SpoilerLevel::Minimal,
            }],
        }
    }

    fn fails(data: GameDataArtifact, code: ValidationCode) {
        assert_eq!(
            ValidatedGameData::validate(data).err().map(|e| e.code),
            Some(code)
        );
    }

    #[test]
    fn canonical_round_trip_preserves_zero_unknown_and_colliding_labels() {
        let documented = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/imported/example-v1.json"
        ));
        assert!(ValidatedGameData::from_json(documented).is_ok());
        let validated = ValidatedGameData::validate(fixture()).unwrap_or_else(|e| panic!("{e}"));
        let first = validated.canonical_json().unwrap_or_else(|e| panic!("{e}"));
        let second = ValidatedGameData::from_json(&first)
            .unwrap_or_else(|e| panic!("{e}"))
            .canonical_json()
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(first, second);
        assert!(first.ends_with(b"\n"));
        assert_eq!(
            validated.artifact().abilities[0].jp_cost,
            ValueState::Known(0)
        );
        assert_eq!(
            validated.artifact().jobs[0].requirement,
            ValueState::Unknown
        );
        assert_eq!(
            validated.artifact().jobs[0].english_label,
            validated.artifact().jobs[1].english_label
        );
        assert_ne!(
            validated.artifact().jobs[0].id,
            validated.artifact().jobs[1].id
        );
        let mut absent = fixture();
        absent.abilities[0].jp_cost = ValueState::Absent;
        absent.coverage.ability_costs = Count {
            accepted: 0,
            unknown: 502,
            rejected: 10,
        };
        let absent = ValidatedGameData::validate(absent).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(absent.artifact().abilities[0].jp_cost, ValueState::Absent);
    }

    #[test]
    fn rejects_collisions_dangling_references_and_bad_identifiers() {
        let mut data = fixture();
        data.jobs.insert(1, data.jobs[0].clone());
        fails(data, ValidationCode::DuplicateId);
        let mut data = fixture();
        data.memberships[0].ability = AbilityId("ability:511".into());
        fails(data, ValidationCode::MissingReference);
        let mut data = fixture();
        data.jobs[0].command = ValueState::Known(CommandId("command:1".into()));
        fails(data, ValidationCode::MissingReference);
        let mut data = fixture();
        data.jobs[2].requirement = ValueState::Known(Requirement::AtLeastLevel {
            job: JobId("job:2".into()),
            level: 2,
        });
        fails(data, ValidationCode::MissingReference);
        let mut data = fixture();
        data.jobs[1].id = JobId("job:074".into());
        fails(data, ValidationCode::InvalidId);
        let mut data = fixture();
        data.memberships.push(data.memberships[0].clone());
        fails(data, ValidationCode::DuplicateId);
    }

    #[test]
    fn rejects_malformed_expressions_cycles_and_unevidenced_facts() {
        let mut data = fixture();
        data.jobs[2].requirement = ValueState::Known(Requirement::All { terms: vec![] });
        fails(data, ValidationCode::InvalidExpression);
        let mut data = fixture();
        data.jobs[2].requirement = ValueState::Known(Requirement::AtLeastLevel {
            job: JobId("job:74".into()),
            level: 0,
        });
        fails(data, ValidationCode::InvalidExpression);
        let mut data = fixture();
        data.jobs[1].requirement = ValueState::Known(Requirement::AtLeastLevel {
            job: JobId("job:76".into()),
            level: 1,
        });
        fails(data, ValidationCode::Cycle);
        let mut data = fixture();
        data.jobs[1].requirement = ValueState::Absent;
        fails(data, ValidationCode::InvalidEvidence);
        let mut data = fixture();
        data.jobs[2].requirement = ValueState::Known(Requirement::AtLeastLevel {
            job: JobId("job:74".into()),
            level: 3,
        });
        fails(data, ValidationCode::InvalidEvidence);
        let mut data = fixture();
        data.jobs[0].english_label = ValueState::Known(" ".into());
        fails(data, ValidationCode::InvalidLabel);
    }

    #[test]
    fn rejects_bad_coverage_source_and_bounds() {
        let mut data = fixture();
        data.coverage.prerequisites.unknown = 30;
        fails(data, ValidationCode::InvalidCoverage);
        let mut data = fixture();
        data.coverage.no_requirement_jobs.accepted = 1;
        fails(data, ValidationCode::InvalidCoverage);
        let mut data = fixture();
        data.coverage.commands.unknown = 225;
        fails(data, ValidationCode::InvalidCoverage);
        let mut data = fixture();
        data.coverage.ability_costs.rejected = 9;
        data.coverage.ability_costs.unknown = 502;
        fails(data, ValidationCode::InvalidCoverage);
        let mut data = fixture();
        data.abilities[0].id = AbilityId("ability:44".into());
        data.memberships[0].ability = AbilityId("ability:44".into());
        fails(data, ValidationCode::InvalidEvidence);
        let mut data = fixture();
        data.provenance.installed_job_build = "other".into();
        fails(data, ValidationCode::SourceMismatch);
        let mut data = fixture();
        data.memberships[0].ordinal = 17;
        fails(data, ValidationCode::InvalidValue);
        let mut data = fixture();
        data.jobs[0].english_label = ValueState::Known("x".repeat(MAX_LABEL_BYTES + 1));
        fails(data, ValidationCode::InvalidLabel);
        let mut data = fixture();
        data.jobs.swap(0, 1);
        fails(data, ValidationCode::InvalidOrdering);
    }

    #[test]
    fn rejects_unsupported_wire_schema_missing_spoilers_and_excessive_input() {
        let bytes = ValidatedGameData::validate(fixture())
            .unwrap_or_else(|e| panic!("{e}"))
            .canonical_json()
            .unwrap_or_else(|e| panic!("{e}"));
        let mut value: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{e}"));
        value["schema"] = serde_json::json!("v2");
        let changed = serde_json::to_vec(&value).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            ValidatedGameData::from_json(&changed).err().map(|e| e.code),
            Some(ValidationCode::UnsupportedSchema)
        );
        value["schema"] = serde_json::json!("v1");
        value["provenance"]["profile"] = serde_json::json!("other");
        let changed = serde_json::to_vec(&value).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            ValidatedGameData::from_json(&changed).err().map(|e| e.code),
            Some(ValidationCode::SourceMismatch)
        );
        value["provenance"]["profile"] = serde_json::json!("r011_r022_english");
        value["jobs"][0]
            .as_object_mut()
            .unwrap_or_else(|| panic!("object"))
            .remove("spoiler");
        let changed = serde_json::to_vec(&value).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            ValidatedGameData::from_json(&changed).err().map(|e| e.code),
            Some(ValidationCode::MissingSpoiler)
        );
        assert_eq!(
            ValidatedGameData::from_json(&vec![b' '; MAX_ARTIFACT_BYTES + 1])
                .err()
                .map(|e| e.code),
            Some(ValidationCode::TooLarge)
        );
        let canonical = String::from_utf8(bytes).unwrap_or_else(|e| panic!("{e}"));
        let duplicate = canonical.replacen(
            "\"schema\":\"v1\",",
            "\"schema\":\"v1\",\"schema\":\"v1\",",
            1,
        );
        assert_eq!(
            ValidatedGameData::from_json(duplicate.as_bytes())
                .err()
                .map(|e| e.code),
            Some(ValidationCode::InvalidJson)
        );
        let extra_state = canonical.replacen(
            "\"state\":\"unknown\"",
            "\"state\":\"unknown\",\"value\":0",
            1,
        );
        assert_eq!(
            ValidatedGameData::from_json(extra_state.as_bytes())
                .err()
                .map(|e| e.code),
            Some(ValidationCode::InvalidJson)
        );
    }
}
