//! Pure, visibility-first reference projections. Hidden targets never enter returned relations.

use super::{
    Ability, AbilityId, Command, CommandId, Job, JobId, Membership, Requirement, SpoilerLevel,
    ValidatedGameData,
};
use crate::ValueState;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct VisibleCoverage {
    pub jobs: usize,
    pub commands: usize,
    pub abilities: usize,
    pub memberships: usize,
    pub prerequisites: usize,
    pub partial: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReferenceIdentity {
    pub schema: super::GameDataSchema,
    pub provenance: super::Provenance,
    pub coverage: VisibleCoverage,
}

pub struct ReferenceQueries<'a> {
    data: &'a ValidatedGameData,
    ceiling: SpoilerLevel,
}

impl<'a> ReferenceQueries<'a> {
    #[must_use]
    pub fn new(data: &'a ValidatedGameData, ceiling: SpoilerLevel) -> Self {
        Self { data, ceiling }
    }

    fn visible(&self, spoiler: SpoilerLevel) -> bool {
        spoiler <= self.ceiling
    }

    fn job_visible(&self, id: &JobId) -> bool {
        self.data
            .0
            .jobs
            .iter()
            .any(|job| &job.id == id && self.visible(job.spoiler))
    }

    fn command_visible(&self, id: &CommandId) -> bool {
        self.data
            .0
            .commands
            .iter()
            .any(|command| &command.id == id && self.visible(command.spoiler))
    }

    fn ability_visible(&self, id: &AbilityId) -> bool {
        self.data
            .0
            .abilities
            .iter()
            .any(|ability| &ability.id == id && self.visible(ability.spoiler))
    }

    fn requirement_visible(&self, requirement: &Requirement) -> bool {
        match requirement {
            Requirement::AtLeastLevel { job, .. } => self.job_visible(job),
            Requirement::All { terms } => terms.iter().all(|term| self.requirement_visible(term)),
        }
    }

    fn visible_membership(&self, membership: &Membership) -> bool {
        self.visible(membership.spoiler)
            && self.command_visible(&membership.command)
            && self.ability_visible(&membership.ability)
    }

    /// Counts describe only visible objects and edges. The source pool's hidden counts stay private.
    #[must_use]
    pub fn identity(&self) -> ReferenceIdentity {
        ReferenceIdentity {
            schema: self.data.0.schema,
            provenance: self.data.0.provenance.clone(),
            coverage: VisibleCoverage {
                jobs: self.data.0.jobs.iter().filter(|row| self.visible(row.spoiler)).count(),
                commands: self.data.0.commands.iter().filter(|row| self.visible(row.spoiler)).count(),
                abilities: self.data.0.abilities.iter().filter(|row| self.visible(row.spoiler)).count(),
                memberships: self.data.0.memberships.iter().filter(|row| self.visible_membership(row)).count(),
                prerequisites: self.data.0.jobs.iter().filter(|row| {
                    self.visible(row.spoiler)
                        && matches!(&row.requirement, ValueState::Known(req) if self.requirement_visible(req))
                }).count(),
                partial: true,
            },
        }
    }

    /// V1 validation caps each source collection at 1,024 rows and fixes its ordering.
    #[must_use]
    pub fn jobs(&self) -> Vec<Job> {
        self.data
            .0
            .jobs
            .iter()
            .filter_map(|row| self.job(&row.id))
            .collect()
    }

    #[must_use]
    pub fn job(&self, id: &JobId) -> Option<Job> {
        let mut row = self
            .data
            .0
            .jobs
            .iter()
            .find(|row| &row.id == id && self.visible(row.spoiler))?
            .clone();
        if matches!(&row.command, ValueState::Known(command) if !self.command_visible(command)) {
            row.command = ValueState::Unknown;
        }
        if matches!(&row.requirement, ValueState::Known(req) if !self.requirement_visible(req)) {
            row.requirement = ValueState::Unknown;
        }
        Some(row)
    }

    #[must_use]
    pub fn commands(&self) -> Vec<Command> {
        self.data
            .0
            .commands
            .iter()
            .filter(|row| self.visible(row.spoiler))
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn command(&self, id: &CommandId) -> Option<Command> {
        self.data
            .0
            .commands
            .iter()
            .find(|row| &row.id == id && self.visible(row.spoiler))
            .cloned()
    }

    #[must_use]
    pub fn abilities(&self) -> Vec<Ability> {
        self.data
            .0
            .abilities
            .iter()
            .filter(|row| self.visible(row.spoiler))
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn ability(&self, id: &AbilityId) -> Option<Ability> {
        self.data
            .0
            .abilities
            .iter()
            .find(|row| &row.id == id && self.visible(row.spoiler))
            .cloned()
    }

    /// Return only edges whose own classification and both endpoints are visible.
    #[must_use]
    pub fn memberships(&self, command: &CommandId) -> Vec<Membership> {
        self.data
            .0
            .memberships
            .iter()
            .filter(|row| &row.command == command && self.visible_membership(row))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_data::{GameDataArtifact, ValidatedGameData};

    fn sample() -> GameDataArtifact {
        serde_json::from_str(include_str!("../../../../tests/fixtures/reference-v1.json"))
            .unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn unknown_ids_and_empty_visible_collections_are_stable() {
        let data = ValidatedGameData::validate(sample()).unwrap_or_else(|error| panic!("{error}"));
        let query = ReferenceQueries::new(&data, SpoilerLevel::Minimal);
        assert!(query.job(&JobId("job:999".into())).is_none());
        assert!(query.ability(&AbilityId("ability:999".into())).is_none());
        assert!(query
            .memberships(&CommandId("command:999".into()))
            .is_empty());
        assert_eq!(
            query
                .jobs()
                .iter()
                .map(|row| row.id.0.as_str())
                .collect::<Vec<_>>(),
            ["job:1", "job:74", "job:76"]
        );
        assert!(query.identity().coverage.partial);
    }

    #[test]
    fn hidden_targets_and_edges_never_reveal_ids_or_counts() {
        let mut artifact = sample();
        artifact.jobs[1].spoiler = SpoilerLevel::Gameplay;
        artifact.commands[0].spoiler = SpoilerLevel::Hints;
        artifact.abilities[0].spoiler = SpoilerLevel::Full;
        let data = ValidatedGameData::validate(artifact).unwrap_or_else(|error| panic!("{error}"));
        let minimal = ReferenceQueries::new(&data, SpoilerLevel::Minimal);
        assert_eq!(minimal.jobs().len(), 2);
        assert!(minimal.commands().is_empty());
        assert!(minimal.abilities().is_empty());
        assert!(minimal
            .memberships(&CommandId("command:25".into()))
            .is_empty());
        assert_eq!(minimal.identity().coverage.prerequisites, 0);
        assert_eq!(minimal.identity().coverage.memberships, 0);
        assert!(matches!(
            minimal.job(&JobId("job:1".into())).map(|row| row.command),
            Some(ValueState::Unknown)
        ));
        assert!(matches!(
            minimal
                .job(&JobId("job:76".into()))
                .map(|row| row.requirement),
            Some(ValueState::Unknown)
        ));
        assert!(minimal.job(&JobId("job:74".into())).is_none());
        let full = ReferenceQueries::new(&data, SpoilerLevel::Full);
        assert_eq!(full.identity().coverage.prerequisites, 1);
        assert_eq!(full.memberships(&CommandId("command:25".into())).len(), 1);
    }

    #[test]
    fn each_spoiler_ceiling_is_monotone_and_unknown_values_remain_unknown() {
        let mut artifact = sample();
        artifact.jobs[0].english_label = ValueState::Unknown;
        artifact.jobs[1].spoiler = SpoilerLevel::Hints;
        artifact.abilities[0].spoiler = SpoilerLevel::Gameplay;
        let data = ValidatedGameData::validate(artifact).unwrap_or_else(|error| panic!("{error}"));
        let sizes = [
            SpoilerLevel::Minimal,
            SpoilerLevel::Hints,
            SpoilerLevel::Gameplay,
            SpoilerLevel::Full,
        ]
        .map(|level| ReferenceQueries::new(&data, level).identity().coverage);
        assert_eq!(sizes.map(|entry| entry.jobs), [2, 3, 3, 3]);
        assert_eq!(sizes.map(|entry| entry.abilities), [0, 0, 1, 1]);
        assert!(matches!(
            ReferenceQueries::new(&data, SpoilerLevel::Minimal)
                .job(&JobId("job:1".into()))
                .map(|row| row.english_label),
            Some(ValueState::Unknown)
        ));
    }

    #[test]
    fn an_empty_visible_view_exposes_no_objects_or_source_counts() {
        let mut artifact = sample();
        for job in &mut artifact.jobs {
            job.spoiler = SpoilerLevel::Full;
        }
        for command in &mut artifact.commands {
            command.spoiler = SpoilerLevel::Full;
        }
        for ability in &mut artifact.abilities {
            ability.spoiler = SpoilerLevel::Full;
        }
        let data = ValidatedGameData::validate(artifact).unwrap_or_else(|error| panic!("{error}"));
        let query = ReferenceQueries::new(&data, SpoilerLevel::Minimal);
        assert!(query.jobs().is_empty());
        assert!(query.commands().is_empty());
        assert!(query.abilities().is_empty());
        assert!(query
            .memberships(&CommandId("command:25".into()))
            .is_empty());
        assert_eq!(query.identity().coverage.jobs, 0);
        assert_eq!(query.identity().coverage.prerequisites, 0);
    }
}
