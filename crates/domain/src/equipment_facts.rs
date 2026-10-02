//! Normalized equipment table facts from fftivc.utility.modloader
//! ItemData.xml, ItemWeaponData.xml and JobData.xml at d3123d2.
//! Category matches are inputs to eligibility, not a replacement for the
//! unavailable game EquipCheck decision function.

use serde::Deserialize;
use std::sync::OnceLock;

const ITEM_COUNT: usize = 261;
const JOB_COUNT: usize = 174;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentItemFact {
    pub id: u16,
    pub category: String,
    pub family: String,
    pub rare: bool,
    pub weapon_flags: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentJobFact {
    pub id: u16,
    pub categories: Vec<String>,
    pub innate_abilities: [u16; 4],
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    table: String,
    sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentFacts {
    schema: String,
    revision: String,
    sources: Vec<Source>,
    items: Vec<EquipmentItemFact>,
    jobs: Vec<EquipmentJobFact>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EquipmentFactsError {
    InvalidJson,
    InvalidSource,
    InvalidItem,
    InvalidJob,
}

impl EquipmentFacts {
    pub fn bundled() -> Result<&'static Self, EquipmentFactsError> {
        static FACTS: OnceLock<Result<EquipmentFacts, EquipmentFactsError>> = OnceLock::new();
        FACTS
            .get_or_init(|| {
                let facts: Self = serde_json::from_str(include_str!(
                    "../../../data/imported/equipment-rules-v1.json"
                ))
                .map_err(|_| EquipmentFactsError::InvalidJson)?;
                facts.validate()?;
                Ok(facts)
            })
            .as_ref()
            .map_err(|error| *error)
    }

    fn validate(&self) -> Result<(), EquipmentFactsError> {
        if self.schema != "equipment_rules_v1"
            || self.revision != "d3123d2"
            || self.sources.len() != 3
            || self.sources[0].table != "ItemData"
            || self.sources[0].sha256
                != "c724d1e5475eff2633d1ff3295f641f450dc341d90c443491860b5f5df1d49c5"
            || self.sources[1].table != "ItemWeaponData"
            || self.sources[1].sha256
                != "30ca54ac24746960d86cf97f6f4467895960bf925a4437e20297f549a55b8de1"
            || self.sources[2].table != "JobData"
            || self.sources[2].sha256
                != "63e49670e9f237824aa0fa0b4c333280bc8cbd041440fcf1638bdb5e35cf7f2b"
        {
            return Err(EquipmentFactsError::InvalidSource);
        }
        if self.items.len() != ITEM_COUNT
            || self.items.iter().enumerate().any(|(index, item)| {
                usize::from(item.id) != index
                    || item.category.is_empty()
                    || !matches!(
                        item.family.as_str(),
                        "weapon" | "shield" | "headgear" | "armor" | "accessory" | "other"
                    )
                    || (item.family != "weapon" && !item.weapon_flags.is_empty())
                    || item.weapon_flags.iter().any(|flag| {
                        !matches!(
                            flag.as_str(),
                            "Arc"
                                | "Direct"
                                | "ForcedTwoHands"
                                | "Lunging"
                                | "Striking"
                                | "Throwable"
                                | "TwoHands"
                                | "TwoSwords"
                        )
                    })
            })
        {
            return Err(EquipmentFactsError::InvalidItem);
        }
        if self.jobs.len() != JOB_COUNT
            || self.jobs.iter().enumerate().any(|(index, job)| {
                usize::from(job.id) != index
                    || job.categories.iter().any(|category| category.is_empty())
                    || job.innate_abilities.iter().any(|ability| *ability > 511)
            })
        {
            return Err(EquipmentFactsError::InvalidJob);
        }
        Ok(())
    }

    #[must_use]
    pub fn item(&self, id: u16) -> Option<&EquipmentItemFact> {
        self.items.get(usize::from(id))
    }

    #[must_use]
    pub fn job(&self, id: u16) -> Option<&EquipmentJobFact> {
        self.jobs.get(usize::from(id))
    }

    #[must_use]
    pub fn category_match(&self, job_id: u16, item_id: u16) -> Option<bool> {
        let job = self.job(job_id)?;
        let item = self.item(item_id)?;
        Some(job.categories.contains(&item.category))
    }
}

#[cfg(test)]
mod tests {
    use super::{EquipmentFacts, EquipmentFactsError};

    #[test]
    fn bundled_facts_cover_enhanced_items_and_reject_unknown_ids() -> Result<(), EquipmentFactsError>
    {
        let facts = EquipmentFacts::bundled()?;
        assert_eq!(facts.item(260).map(|item| item.id), Some(260));
        assert_eq!(facts.item(261), None);
        assert_eq!(facts.job(173).map(|job| job.id), Some(173));
        assert_eq!(facts.job(174), None);
        assert_eq!(facts.category_match(1, 1), Some(true));
        assert_eq!(facts.category_match(1, 240), Some(false));
        Ok(())
    }
}
