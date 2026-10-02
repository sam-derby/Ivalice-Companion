//! Verified generic-job prerequisite graph for the explicit save editor.
//! FFT Nex GeneralJob.layout at 335747e defines RequiredJobIds/Levels;
//! TICSaveEditor UnitSaveData.cs at 07ea857 maps slots 0..19 and reserves 20..21.

use serde::{Deserialize, Serialize};

const GENERAL_JOB_SHA256: &str = "592d06922d0abec9c3e5ecfb26bbd862e091cd8ded96abe86b11607c15c79134";
const LAYOUT_SHA256: &str = "359b6d8671e2d5106ceb5a3f0b46480c481e24731f349c5df0675191d5b22221";
const JOB_SHA256: &str = "0bb0d6ac321c65ed80e9597cd5c8f6324e02bbc1a8446f9344cceb1643f4cd44";
const GENERIC_JOBS: usize = 20;
// FFT Nex GeneralJob.layout at 335747e, RequiredJobExp in the pinned
// GeneralJob table: cumulative total JP needed for levels 0 through 8.
pub const JOB_LEVEL_TOTAL_JP: [u16; 9] = [0, 100, 200, 400, 700, 1100, 1600, 2200, 3000];

pub fn level_from_total_jp(total_jp: u16) -> u8 {
    u8::try_from(
        JOB_LEVEL_TOTAL_JP
            .iter()
            .rposition(|threshold| total_jp >= *threshold)
            .unwrap_or(0),
    )
    .unwrap_or(0)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobPrerequisite {
    pub slot: u8,
    pub level: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRequirement {
    pub slot: u8,
    pub job_id: u16,
    pub requires: Vec<JobPrerequisite>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRequirementsDocument {
    pub schema: String,
    pub profile: String,
    pub general_job_sha256: String,
    pub layout_sha256: String,
    pub job_sha256: String,
    pub jobs: Vec<JobRequirement>,
    pub disabled_slots: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobRequirementsError {
    InvalidJson,
    TooLarge,
    SourceMismatch,
    InvalidGraph,
}

#[derive(Clone, Debug)]
pub struct ValidatedJobRequirements(JobRequirementsDocument);

impl ValidatedJobRequirements {
    pub fn from_json(bytes: &[u8]) -> Result<Self, JobRequirementsError> {
        if bytes.len() > 16 * 1024 {
            return Err(JobRequirementsError::TooLarge);
        }
        let document =
            serde_json::from_slice(bytes).map_err(|_| JobRequirementsError::InvalidJson)?;
        Self::validate(document)
    }

    pub fn validate(document: JobRequirementsDocument) -> Result<Self, JobRequirementsError> {
        if document.schema != "job_requirements_v1"
            || document.profile != "english_steam_enhanced_manual"
            || document.general_job_sha256 != GENERAL_JOB_SHA256
            || document.layout_sha256 != LAYOUT_SHA256
            || document.job_sha256 != JOB_SHA256
        {
            return Err(JobRequirementsError::SourceMismatch);
        }
        if document.jobs.len() != GENERIC_JOBS || document.disabled_slots != [20, 21] {
            return Err(JobRequirementsError::InvalidGraph);
        }
        for (index, row) in document.jobs.iter().enumerate() {
            if usize::from(row.slot) != index
                || row.job_id != 0x4a + u16::from(row.slot)
                || row.requires.len() > 8
                || (index < 2 && !row.requires.is_empty())
                || (index >= 2 && row.requires.is_empty())
            {
                return Err(JobRequirementsError::InvalidGraph);
            }
            let mut seen = [false; GENERIC_JOBS];
            for prerequisite in &row.requires {
                let required_slot = usize::from(prerequisite.slot);
                if required_slot >= GENERIC_JOBS
                    || !(1..=8).contains(&prerequisite.level)
                    || seen[required_slot]
                {
                    return Err(JobRequirementsError::InvalidGraph);
                }
                seen[required_slot] = true;
            }
        }
        let mut marks = [0_u8; GENERIC_JOBS];
        for slot in 0..GENERIC_JOBS {
            if !visit(slot, &document.jobs, &mut marks) {
                return Err(JobRequirementsError::InvalidGraph);
            }
        }
        Ok(Self(document))
    }

    /// A path is complete only when every prerequisite in its full chain is
    /// reachable and its saved level meets the corresponding threshold.
    /// This gives prerequisite reachability; callers must also check the
    /// saved unlock flags before offering an edit.
    pub fn reachable_paths(&self, levels: &[u8; 24]) -> [bool; GENERIC_JOBS] {
        let mut path = [false; GENERIC_JOBS];
        path[0] = true;
        path[1] = true;
        for _ in 0..GENERIC_JOBS {
            let mut changed = false;
            for row in &self.0.jobs {
                let slot = usize::from(row.slot);
                if !path[slot]
                    && row.requires.iter().all(|required| {
                        let dependency = usize::from(required.slot);
                        path[dependency] && levels[dependency] >= required.level
                    })
                {
                    path[slot] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        path
    }

    pub fn requirements(&self) -> &[JobRequirement] {
        &self.0.jobs
    }

    /// Keep unusual saved unlocks, but reject decreases that break a previously
    /// complete prerequisite chain. The same rule serves preview and writeback.
    pub fn invalidated_paths(&self, before: &[u8; 24], after: &[u8; 24]) -> Vec<u8> {
        let before = self.reachable_paths(before);
        let after = self.reachable_paths(after);
        (0..20)
            .filter(|index| before[*index] && !after[*index])
            .filter_map(|index| u8::try_from(index).ok())
            .collect()
    }

    /// Generic jobs are stored most-significant-bit first in each saved byte.
    /// The owned slot 9/10 Geomancer transition corroborates this byte order.
    pub fn saved_unlocks(&self, flags: u32) -> Option<[bool; GENERIC_JOBS]> {
        if flags > 0x00ff_ffff {
            return None;
        }
        Some(std::array::from_fn(|slot| {
            let bit = (slot / 8) * 8 + 7 - (slot % 8);
            flags & (1 << bit) != 0
        }))
    }
}

fn visit(slot: usize, rows: &[JobRequirement], marks: &mut [u8; GENERIC_JOBS]) -> bool {
    if marks[slot] == 2 {
        return true;
    }
    if marks[slot] == 1 {
        return false;
    }
    marks[slot] = 1;
    for dependency in &rows[slot].requires {
        if !visit(usize::from(dependency.slot), rows, marks) {
            return false;
        }
    }
    marks[slot] = 2;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> JobRequirementsDocument {
        let jobs = (0..20)
            .map(|slot| JobRequirement {
                slot,
                job_id: 0x4a + u16::from(slot),
                requires: (slot >= 2)
                    .then(|| JobPrerequisite {
                        slot: slot - 1,
                        level: 2,
                    })
                    .into_iter()
                    .collect(),
            })
            .collect();
        JobRequirementsDocument {
            schema: "job_requirements_v1".into(),
            profile: "english_steam_enhanced_manual".into(),
            general_job_sha256: GENERAL_JOB_SHA256.into(),
            layout_sha256: LAYOUT_SHA256.into(),
            job_sha256: JOB_SHA256.into(),
            jobs,
            disabled_slots: vec![20, 21],
        }
    }

    #[test]
    fn direct_and_chained_paths_require_each_step() -> Result<(), Box<dyn std::error::Error>> {
        let graph = ValidatedJobRequirements::validate(example())
            .map_err(|error| format!("valid graph rejected: {error:?}"))?;
        let mut levels = [0_u8; 24];
        levels[1] = 2;
        levels[2] = 2;
        let reachable = graph.reachable_paths(&levels);
        assert!(reachable[2] && reachable[3]);
        assert!(!reachable[4]);
        levels[1] = 0;
        let reachable = graph.reachable_paths(&levels);
        assert!(!reachable[2] && !reachable[3]);
        Ok(())
    }

    #[test]
    fn missing_cycle_special_and_wrong_sources_fail_closed() {
        let mut document = example();
        document.jobs[3].requires[0].slot = 20;
        assert!(matches!(
            ValidatedJobRequirements::validate(document),
            Err(JobRequirementsError::InvalidGraph)
        ));
        let mut document = example();
        document.jobs[2].requires[0].slot = 3;
        assert!(matches!(
            ValidatedJobRequirements::validate(document),
            Err(JobRequirementsError::InvalidGraph)
        ));
        let mut document = example();
        document.disabled_slots.clear();
        assert!(matches!(
            ValidatedJobRequirements::validate(document),
            Err(JobRequirementsError::InvalidGraph)
        ));
        let mut document = example();
        document.job_sha256.clear();
        assert!(matches!(
            ValidatedJobRequirements::validate(document),
            Err(JobRequirementsError::SourceMismatch)
        ));
    }

    #[test]
    fn saved_unlock_bits_reverse_within_each_byte() -> Result<(), Box<dyn std::error::Error>> {
        let graph = ValidatedJobRequirements::validate(example())
            .map_err(|error| format!("valid graph rejected: {error:?}"))?;
        for slot in 0..20 {
            let bit = (slot / 8) * 8 + 7 - (slot % 8);
            let unlocks = graph
                .saved_unlocks(1 << bit)
                .ok_or("valid flags rejected")?;
            assert!(unlocks[slot]);
            assert_eq!(unlocks.iter().filter(|unlocked| **unlocked).count(), 1);
        }
        assert!(graph.saved_unlocks(0x0100_0000).is_none());
        Ok(())
    }

    #[test]
    fn decreases_protect_complete_chains_but_allow_independent_jp_and_leaf_levels(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let graph = ValidatedJobRequirements::validate(example())
            .map_err(|error| format!("invalid graph: {error:?}"))?;
        let mut before = [0; 24];
        before[1] = 3;
        before[2] = 2;
        let mut after = before;
        after[1] = 1;
        assert_eq!(graph.invalidated_paths(&before, &after), [2, 3]);
        after[1] = 2;
        assert!(graph.invalidated_paths(&before, &after).is_empty());
        after[3] = 1;
        assert!(graph.invalidated_paths(&before, &after).is_empty());
        Ok(())
    }

    #[test]
    fn total_jp_has_exact_level_boundaries() -> Result<(), Box<dyn std::error::Error>> {
        for level in 0..8 {
            let minimum = JOB_LEVEL_TOTAL_JP[level];
            assert_eq!(level_from_total_jp(minimum), u8::try_from(level)?);
            assert_eq!(
                level_from_total_jp(JOB_LEVEL_TOTAL_JP[level + 1] - 1),
                u8::try_from(level)?
            );
        }
        assert_eq!(level_from_total_jp(3000), 8);
        assert_eq!(level_from_total_jp(u16::MAX), 8);
        Ok(())
    }
}
