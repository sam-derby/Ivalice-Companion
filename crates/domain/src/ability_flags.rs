//! Named job ability map imported from TICSaveEditor Job/JobCommand/Ability
//! resources at 07ea857. The owner Dragoon pair verifies the first action and
//! passive bits; 535 equipped passive references corroborate MSB-first order.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const SOURCE_HASHES: [&str; 6] = [
    "0bb0d6ac321c65ed80e9597cd5c8f6324e02bbc1a8446f9344cceb1643f4cd44",
    "1ee9761b6e94e9be2d1895bff5d5c14b1df2059a4560cdf5e4ad0de30d074650",
    "349d1a3640c6772872b24c3bf3a85dadfe54ddfa47ebbcd2b0eead16fbb77aa5",
    "46f5105ff1874e85572dfdda016cc2a6db4ca4a62b14a16d4f3e0279756b7391",
    "f6dd33581c5880200c7f3e0b45e2e6f706061214227f989fef368baf25a6ef88",
    "da9a9d77ed4e441e4b554ed4a98805b7e0e6765d76a0bde8e79653b8d0cb311b",
];

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityFlagMember {
    pub key: String,
    pub bit: u8,
    pub label: String,
    pub kind: AbilityKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityKind {
    Action,
    Reaction,
    Support,
    Movement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadoutKind {
    SecondaryCommand,
    Reaction,
    Support,
    Movement,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LoadoutChoice {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibleLoadoutChoices {
    pub secondary: Vec<LoadoutChoice>,
    pub reaction: Vec<LoadoutChoice>,
    pub support: Vec<LoadoutChoice>,
    pub movement: Vec<LoadoutChoice>,
}

impl AbilityFlagMember {
    pub fn ability_id(&self) -> Option<u16> {
        self.key.strip_prefix("ability:")?.parse().ok()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityFlagJob {
    pub slot: u8,
    pub job_id: u16,
    pub command_id: u16,
    pub command_label: String,
    pub members: Vec<AbilityFlagMember>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityFlagsDocument {
    pub schema: String,
    pub profile: String,
    pub job_sha256: String,
    pub job_xml_sha256: String,
    pub ability_sha256: String,
    pub ability_xml_sha256: String,
    pub command_sha256: String,
    pub command_xml_sha256: String,
    pub jobs: Vec<AbilityFlagJob>,
    pub special_jobs: Vec<AbilityFlagJob>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityFlagsError {
    Invalid,
}

#[derive(Clone, Debug)]
pub struct ValidatedAbilityFlags(AbilityFlagsDocument);

impl ValidatedAbilityFlags {
    pub fn from_json(bytes: &[u8]) -> Result<Self, AbilityFlagsError> {
        if bytes.is_empty() || bytes.len() > 256 * 1024 {
            return Err(AbilityFlagsError::Invalid);
        }
        let document = serde_json::from_slice(bytes).map_err(|_| AbilityFlagsError::Invalid)?;
        Self::validate(document)
    }

    pub fn validate(document: AbilityFlagsDocument) -> Result<Self, AbilityFlagsError> {
        if document.schema != "ability_flags_v1"
            || document.profile != "english_steam_enhanced_manual"
            || [
                document.job_sha256.as_str(),
                document.job_xml_sha256.as_str(),
                document.ability_sha256.as_str(),
                document.ability_xml_sha256.as_str(),
                document.command_sha256.as_str(),
                document.command_xml_sha256.as_str(),
            ] != SOURCE_HASHES
            || document.jobs.len() != 20
            || document.special_jobs.len() > 73
        {
            return Err(AbilityFlagsError::Invalid);
        }
        for (slot, job) in document.jobs.iter().enumerate() {
            if usize::from(job.slot) != slot
                || job.job_id != 74 + u16::from(job.slot)
                || job.command_id == 0
                || job.command_id > 175
                || !valid_label(&job.command_label)
                || job.members.len() > 22
                || (slot == 19 && !job.members.is_empty())
                || (slot != 19 && job.members.is_empty())
            {
                return Err(AbilityFlagsError::Invalid);
            }
            validate_members(job)?;
        }
        let mut previous_id = 0;
        for job in &document.special_jobs {
            if job.slot != 0
                || job.job_id <= previous_id
                || !story_job_uses_slot_zero(job.job_id)
                || job.command_id == 0
                || job.command_id > 175
                || !valid_label(&job.command_label)
                || job.members.len() > 22
            {
                return Err(AbilityFlagsError::Invalid);
            }
            validate_members(job)?;
            previous_id = job.job_id;
        }
        Ok(Self(document))
    }

    pub fn job(&self, slot: u8) -> Option<&AbilityFlagJob> {
        self.0.jobs.get(usize::from(slot))
    }

    pub fn member(&self, slot: u8, key: &str) -> Option<&AbilityFlagMember> {
        self.job(slot)?
            .members
            .iter()
            .find(|member| member.key == key)
    }

    pub fn member_by_id(&self, slot: u8, id: u16) -> Option<&AbilityFlagMember> {
        self.job(slot)?
            .members
            .iter()
            .find(|member| member.ability_id() == Some(id))
    }

    pub fn job_for_save_slot(&self, slot: u8, first_job_id: u16) -> Option<&AbilityFlagJob> {
        if slot == 0 && first_job_id != 0x4a {
            self.0
                .special_jobs
                .iter()
                .find(|job| job.job_id == first_job_id)
        } else {
            self.job(slot)
        }
    }

    pub fn member_by_id_for_save_slot(
        &self,
        slot: u8,
        first_job_id: u16,
        id: u16,
    ) -> Option<&AbilityFlagMember> {
        self.job_for_save_slot(slot, first_job_id)?
            .members
            .iter()
            .find(|member| member.ability_id() == Some(id))
    }

    pub fn jobs_for_unit(&self, first_job_id: u16) -> impl Iterator<Item = &AbilityFlagJob> {
        self.0
            .jobs
            .iter()
            .skip(1)
            .chain(self.job_for_save_slot(0, first_job_id))
    }

    pub fn compatible_loadout_choices(
        &self,
        first_job_id: u16,
        job_slots: &[u8],
        learned: impl Fn(u8, u8) -> bool,
    ) -> CompatibleLoadoutChoices {
        let mut secondary = BTreeMap::new();
        let mut reaction = BTreeMap::new();
        let mut support = BTreeMap::new();
        let mut movement = BTreeMap::new();
        for &slot in job_slots {
            let Some(job) = self.job_for_save_slot(slot, first_job_id) else {
                continue;
            };
            if job
                .members
                .iter()
                .any(|member| member.kind == AbilityKind::Action && learned(slot, member.bit))
            {
                secondary.insert(
                    format!("command:{}", job.command_id),
                    job.command_label.clone(),
                );
            }
            for member in job
                .members
                .iter()
                .filter(|member| learned(slot, member.bit))
            {
                match member.kind {
                    AbilityKind::Action => {}
                    AbilityKind::Reaction => {
                        reaction.insert(member.key.clone(), member.label.clone());
                    }
                    AbilityKind::Support => {
                        support.insert(member.key.clone(), member.label.clone());
                    }
                    AbilityKind::Movement => {
                        movement.insert(member.key.clone(), member.label.clone());
                    }
                }
            }
        }
        let collect = |items: BTreeMap<String, String>| {
            items
                .into_iter()
                .map(|(key, label)| LoadoutChoice { key, label })
                .collect()
        };
        CompatibleLoadoutChoices {
            secondary: collect(secondary),
            reaction: collect(reaction),
            support: collect(support),
            movement: collect(movement),
        }
    }

    pub fn is_compatible_loadout_choice(
        &self,
        first_job_id: u16,
        job_slots: &[u8],
        learned: impl Fn(u8, u8) -> bool,
        kind: LoadoutKind,
        id: u16,
    ) -> bool {
        if id == 0 {
            return true;
        }
        job_slots.iter().any(|&slot| {
            let Some(job) = self.job_for_save_slot(slot, first_job_id) else {
                return false;
            };
            if kind == LoadoutKind::SecondaryCommand {
                return job.command_id == id
                    && job.members.iter().any(|member| {
                        member.kind == AbilityKind::Action && learned(slot, member.bit)
                    });
            }
            job.members.iter().any(|member| {
                member.ability_id() == Some(id)
                    && member.kind
                        == match kind {
                            LoadoutKind::Reaction => AbilityKind::Reaction,
                            LoadoutKind::Support => AbilityKind::Support,
                            LoadoutKind::Movement => AbilityKind::Movement,
                            LoadoutKind::SecondaryCommand => AbilityKind::Action,
                        }
                    && learned(slot, member.bit)
            })
        })
    }
}

fn validate_members(job: &AbilityFlagJob) -> Result<(), AbilityFlagsError> {
    let mut bits = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for member in &job.members {
        let Some(id) = member
            .key
            .strip_prefix("ability:")
            .and_then(|part| part.parse::<u16>().ok())
        else {
            return Err(AbilityFlagsError::Invalid);
        };
        if !(1..=511).contains(&id)
            || !matches!(member.bit, 0..=15 | 18..=23)
            || !bits.insert(member.bit)
            || !keys.insert(&member.key)
            || !valid_label(&member.label)
            || (member.bit < 16) != (member.kind == AbilityKind::Action)
        {
            return Err(AbilityFlagsError::Invalid);
        }
    }
    Ok(())
}

fn valid_label(label: &str) -> bool {
    !label.trim().is_empty() && label.len() <= 256 && !label.chars().any(char::is_control)
}

fn story_job_uses_slot_zero(job: u16) -> bool {
    // TICSaveEditor.Core/Records/UnitSaveData.cs BuildJobSlotTable at 07ea857.
    matches!(
        job,
        0x01..=0x34
            | 0x3c
            | 0x3e
            | 0x40
            | 0x41
            | 0x43
            | 0x45
            | 0x48
            | 0x49
            | 0x90
            | 0x91
            | 0x96..=0x9a
            | 0xa2
            | 0xa3
            | 0xa5..=0xa8
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_bundle_fails_closed() {
        assert!(ValidatedAbilityFlags::from_json(b"{}").is_err());
        assert!(ValidatedAbilityFlags::from_json(&vec![b' '; 65_537]).is_err());
    }

    #[test]
    fn story_command_and_typed_passive_choices_share_writer_rules() {
        let jobs = (0..20_u8)
            .map(|slot| AbilityFlagJob {
                slot,
                job_id: 74 + u16::from(slot),
                command_id: u16::from(slot) + 1,
                command_label: format!("Command {slot}"),
                members: if slot == 19 {
                    vec![]
                } else {
                    vec![AbilityFlagMember {
                        key: format!(
                            "ability:{}",
                            if slot == 13 { 427 } else { u16::from(slot) + 1 }
                        ),
                        bit: if slot == 13 { 23 } else { 7 },
                        label: format!("Ability {slot}"),
                        kind: if slot == 13 {
                            AbilityKind::Reaction
                        } else {
                            AbilityKind::Action
                        },
                    }]
                },
            })
            .collect();
        let special_jobs = vec![AbilityFlagJob {
            slot: 0,
            job_id: 30,
            command_id: 40,
            command_label: "Holy Sword".into(),
            members: vec![AbilityFlagMember {
                key: "ability:155".into(),
                bit: 7,
                label: "Judgment Blade".into(),
                kind: AbilityKind::Action,
            }],
        }];
        let map = ValidatedAbilityFlags::validate(AbilityFlagsDocument {
            schema: "ability_flags_v1".into(),
            profile: "english_steam_enhanced_manual".into(),
            job_sha256: SOURCE_HASHES[0].into(),
            job_xml_sha256: SOURCE_HASHES[1].into(),
            ability_sha256: SOURCE_HASHES[2].into(),
            ability_xml_sha256: SOURCE_HASHES[3].into(),
            command_sha256: SOURCE_HASHES[4].into(),
            command_xml_sha256: SOURCE_HASHES[5].into(),
            jobs,
            special_jobs,
        })
        .unwrap_or_else(|error| panic!("valid pinned fixture: {error:?}"));
        let learned = |slot, bit| matches!((slot, bit), (0, 7) | (13, 23));
        let choices = map.compatible_loadout_choices(30, &[0, 13], learned);
        assert_eq!(choices.secondary[0].key, "command:40");
        assert_eq!(choices.reaction[0].key, "ability:427");
        assert!(map.is_compatible_loadout_choice(
            30,
            &[0, 13],
            learned,
            LoadoutKind::Reaction,
            427
        ));
        assert!(!map.is_compatible_loadout_choice(
            30,
            &[0, 13],
            learned,
            LoadoutKind::Support,
            427
        ));
        assert!(!map.is_compatible_loadout_choice(
            30,
            &[0, 13],
            learned,
            LoadoutKind::SecondaryCommand,
            1
        ));
        assert!(!map.is_compatible_loadout_choice(30, &[0, 13], learned, LoadoutKind::Reaction, 1));
    }
}
