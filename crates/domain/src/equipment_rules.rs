//! Conservative active-equipment rules. Item/job/weapon facts are from
//! fftivc.utility.modloader TableData at d3123d2; the five logical hand/gear
//! slots follow TICSaveEditor EquipmentLoadoutHelpers.cs at 07ea857.
//! Held-count transfers follow the owner's isolated game save slots 38/39.

use crate::equipment_facts::EquipmentFacts;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GearSlot {
    RightHand,
    LeftHand,
    Head,
    Body,
    Accessory,
}

impl GearSlot {
    pub const ALL: [Self; 5] = [
        Self::RightHand,
        Self::LeftHand,
        Self::Head,
        Self::Body,
        Self::Accessory,
    ];
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GearSource {
    Held,
    CreateAndEquip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GearError {
    UnknownItem,
    UnknownJob,
    UnverifiedRestriction,
    WrongSlot,
    JobCannotEquip,
    HandConflict,
    DuplicateField,
    NoChange,
    InvalidSource,
    StockUnderflow,
    StockOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GearUnit {
    pub job_id: u16,
    pub support_ability: u16,
    /// Head, body, accessory, right weapon/shield, left weapon/shield.
    pub equipment: [Option<u16>; 7],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GearPlan {
    pub equipment: [Option<u16>; 7],
    pub held_deltas: Vec<(u16, i16)>,
}

impl GearUnit {
    #[must_use]
    pub fn current(self, slot: GearSlot) -> Option<u16> {
        match slot {
            GearSlot::RightHand => self.equipment[4].or(self.equipment[3]),
            GearSlot::LeftHand => self.equipment[6].or(self.equipment[5]),
            GearSlot::Head => self.equipment[0],
            GearSlot::Body => self.equipment[1],
            GearSlot::Accessory => self.equipment[2],
        }
    }

    pub fn plan(
        self,
        facts: &EquipmentFacts,
        slot: GearSlot,
        target: Option<u16>,
        source: Option<GearSource>,
    ) -> Result<GearPlan, GearError> {
        if target.is_some() != source.is_some() {
            return Err(GearError::InvalidSource);
        }
        let job = facts.job(self.job_id).ok_or(GearError::UnknownJob)?;
        let (fields, old) = match slot {
            GearSlot::RightHand => ([Some(3), Some(4)], hand_item(&self.equipment, 3, 4)?),
            GearSlot::LeftHand => ([Some(5), Some(6)], hand_item(&self.equipment, 5, 6)?),
            GearSlot::Head => ([Some(0), None], self.equipment[0]),
            GearSlot::Body => ([Some(1), None], self.equipment[1]),
            GearSlot::Accessory => ([Some(2), None], self.equipment[2]),
        };
        let mut next = self.equipment;
        let target_item = if let Some(item_id) = target {
            let item = facts.item(item_id).ok_or(GearError::UnknownItem)?;
            if item_id == 0 || matches!(item_id, 254 | 255) || item.family == "other" {
                return Err(GearError::UnknownItem);
            }
            // The pinned tables omit the sex/character exception consumer.
            // Keep these categories unavailable until that rule is audited.
            if matches!(item.category.as_str(), "Bag" | "HairAdornment" | "Perfume") {
                return Err(GearError::UnverifiedRestriction);
            }
            if !job.categories.contains(&item.category) {
                // Support-only category extensions lack an audited consumer.
                return Err(GearError::JobCannotEquip);
            }
            match slot {
                GearSlot::RightHand | GearSlot::LeftHand
                    if matches!(item.family.as_str(), "weapon" | "shield") => {}
                GearSlot::Head
                    if item.family == "headgear"
                        && matches!(item.category.as_str(), "Hat" | "Helmet") => {}
                GearSlot::Body
                    if item.family == "armor"
                        && matches!(item.category.as_str(), "Armor" | "Clothing" | "Robe") => {}
                GearSlot::Accessory if item.family == "accessory" => {}
                _ => return Err(GearError::WrongSlot),
            }
            Some(item)
        } else {
            None
        };
        if let Some(first) = fields[0] {
            next[first] = None;
        }
        if let Some(second) = fields[1] {
            next[second] = None;
        }
        if let (Some(item_id), Some(item)) = (target, target_item) {
            let target_field = match slot {
                GearSlot::RightHand if item.family == "shield" => 4,
                GearSlot::RightHand => 3,
                GearSlot::LeftHand if item.family == "shield" => 6,
                GearSlot::LeftHand => 5,
                GearSlot::Head => 0,
                GearSlot::Body => 1,
                GearSlot::Accessory => 2,
            };
            next[target_field] = Some(item_id);
        }
        if next == self.equipment {
            return Err(GearError::NoChange);
        }
        if matches!(slot, GearSlot::RightHand | GearSlot::LeftHand) {
            validate_hands(facts, job.innate_abilities, self.support_ability, &next)?;
        }
        let mut held_deltas = Vec::new();
        if let Some(old_item) = old {
            if !valid_owned_item(facts, old_item) {
                return Err(GearError::UnknownItem);
            }
            held_deltas.push((old_item, 1));
        }
        if let Some(item_id) = target {
            // Create and equip intentionally leaves one new held copy as well
            // as the equipped copy, per the owner's explicit 2026-10-02 choice.
            held_deltas.push((
                item_id,
                match source {
                    Some(GearSource::Held) => -1,
                    Some(GearSource::CreateAndEquip) => 1,
                    None => return Err(GearError::InvalidSource),
                },
            ));
        }
        Ok(GearPlan {
            equipment: next,
            held_deltas,
        })
    }
}

fn valid_owned_item(facts: &EquipmentFacts, item_id: u16) -> bool {
    item_id != 0
        && !matches!(item_id, 254 | 255)
        && facts
            .item(item_id)
            .is_some_and(|item| item.family != "other")
}

fn hand_item(
    equipment: &[Option<u16>; 7],
    weapon: usize,
    shield: usize,
) -> Result<Option<u16>, GearError> {
    match (equipment[weapon], equipment[shield]) {
        (Some(_), Some(_)) => Err(GearError::HandConflict),
        (Some(item), None) | (None, Some(item)) => Ok(Some(item)),
        (None, None) => Ok(None),
    }
}

fn validate_hands(
    facts: &EquipmentFacts,
    innates: [u16; 4],
    support: u16,
    equipment: &[Option<u16>; 7],
) -> Result<(), GearError> {
    let right = hand_item(equipment, 3, 4)?;
    let left = hand_item(equipment, 5, 6)?;
    for (item_id, opposite) in [(right, left), (left, right)] {
        if let Some(item_id) = item_id {
            let item = facts.item(item_id).ok_or(GearError::UnknownItem)?;
            if item
                .weapon_flags
                .iter()
                .any(|flag| flag == "ForcedTwoHands")
                && opposite.is_some()
            {
                return Err(GearError::HandConflict);
            }
        }
    }
    if let Some(left_weapon) = equipment[5] {
        let item = facts.item(left_weapon).ok_or(GearError::UnknownItem)?;
        if !item.weapon_flags.iter().any(|flag| flag == "TwoSwords")
            || (support != 477 && !innates.contains(&477))
        {
            return Err(GearError::UnverifiedRestriction);
        }
    }
    Ok(())
}

/// Apply manual held-quantity edits as a baseline, then all equipment deltas
/// together. This permits an explicitly returned item to be equipped elsewhere
/// in the same draft, while rejecting final underflow and overflow.
pub fn project_held_counts(
    initial: &[u8; 261],
    manual: &[(u16, u8)],
    plans: &[GearPlan],
) -> Result<[u8; 261], GearError> {
    let mut counts = initial.map(i32::from);
    let mut seen = [false; 261];
    for (item_id, quantity) in manual {
        let index = usize::from(*item_id);
        if !(1..261).contains(&index) || matches!(index, 254 | 255) || *quantity > 99 {
            return Err(GearError::UnknownItem);
        }
        if seen[index] {
            return Err(GearError::DuplicateField);
        }
        seen[index] = true;
        counts[index] = i32::from(*quantity);
    }
    for plan in plans {
        for (item_id, delta) in &plan.held_deltas {
            let index = usize::from(*item_id);
            if !(1..261).contains(&index) || matches!(index, 254 | 255) {
                return Err(GearError::UnknownItem);
            }
            seen[index] = true;
            counts[index] += i32::from(*delta);
        }
    }
    let mut result = *initial;
    for (index, count) in counts.into_iter().enumerate() {
        if !seen[index] {
            continue;
        }
        if count < 0 {
            return Err(GearError::StockUnderflow);
        }
        if count > 99 {
            return Err(GearError::StockOverflow);
        }
        result[index] = u8::try_from(count).map_err(|_| GearError::StockOverflow)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit() -> GearUnit {
        GearUnit {
            job_id: 2,
            support_ability: 0,
            equipment: [None, None, None, Some(1), None, None, Some(128)],
        }
    }

    #[test]
    fn held_swap_returns_old_and_consumes_new() -> Result<(), GearError> {
        let facts = EquipmentFacts::bundled().map_err(|_| GearError::UnknownItem)?;
        let plan = unit().plan(facts, GearSlot::RightHand, Some(23), Some(GearSource::Held))?;
        assert_eq!(plan.equipment[3], Some(23));
        assert_eq!(plan.equipment[4], None);
        let mut initial = [0; 261];
        initial[23] = 1;
        let final_counts = project_held_counts(&initial, &[], &[plan])?;
        assert_eq!(final_counts[1], 1);
        assert_eq!(final_counts[23], 0);
        Ok(())
    }

    #[test]
    fn creation_leaves_one_held_copy_and_rejects_overflow() -> Result<(), GearError> {
        let facts = EquipmentFacts::bundled().map_err(|_| GearError::UnknownItem)?;
        let plan = unit().plan(
            facts,
            GearSlot::RightHand,
            Some(23),
            Some(GearSource::CreateAndEquip),
        )?;
        let initial = [0; 261];
        let final_counts = project_held_counts(&initial, &[], std::slice::from_ref(&plan))?;
        assert_eq!(final_counts[1], 1);
        assert_eq!(final_counts[23], 1);
        let mut full = initial;
        full[23] = 99;
        assert_eq!(
            project_held_counts(&full, &[], &[plan]),
            Err(GearError::StockOverflow)
        );
        Ok(())
    }

    #[test]
    fn stock_and_unverified_restrictions_fail_closed() -> Result<(), GearError> {
        let facts = EquipmentFacts::bundled().map_err(|_| GearError::UnknownItem)?;
        let plan = unit().plan(facts, GearSlot::RightHand, Some(23), Some(GearSource::Held))?;
        assert_eq!(
            project_held_counts(&[0; 261], &[], &[plan]),
            Err(GearError::StockUnderflow)
        );
        assert_eq!(
            unit().plan(facts, GearSlot::Head, Some(23), Some(GearSource::Held)),
            Err(GearError::WrongSlot)
        );
        assert_eq!(
            unit().plan(
                facts,
                GearSlot::RightHand,
                Some(255),
                Some(GearSource::Held)
            ),
            Err(GearError::UnknownItem)
        );
        Ok(())
    }

    #[test]
    fn enhanced_ids_and_category_family_mismatch() -> Result<(), GearError> {
        let facts = EquipmentFacts::bundled().map_err(|_| GearError::UnknownItem)?;
        assert!(unit()
            .plan(
                facts,
                GearSlot::RightHand,
                Some(256),
                Some(GearSource::Held)
            )
            .is_ok());
        assert!(unit()
            .plan(facts, GearSlot::Body, Some(259), Some(GearSource::Held))
            .is_ok());
        assert!(unit()
            .plan(
                facts,
                GearSlot::Accessory,
                Some(260),
                Some(GearSource::Held)
            )
            .is_ok());
        // The pinned row marks 258 as Armor while its category is Hat.
        assert_eq!(
            unit().plan(facts, GearSlot::Body, Some(258), Some(GearSource::Held)),
            Err(GearError::WrongSlot)
        );
        Ok(())
    }

    #[test]
    fn full_draft_nets_returns_transfers_and_manual_quantities() -> Result<(), GearError> {
        let facts = EquipmentFacts::bundled().map_err(|_| GearError::UnknownItem)?;
        let removing = unit().plan(facts, GearSlot::RightHand, None, None)?;
        let other = GearUnit {
            equipment: [None; 7],
            ..unit()
        };
        let equipping = other.plan(facts, GearSlot::RightHand, Some(1), Some(GearSource::Held))?;
        let counts = project_held_counts(&[0; 261], &[], &[removing, equipping])?;
        assert_eq!(counts[1], 0);
        let plan = other.plan(facts, GearSlot::RightHand, Some(23), Some(GearSource::Held))?;
        let counts = project_held_counts(&[0; 261], &[(23, 1)], &[plan])?;
        assert_eq!(counts[23], 0);
        Ok(())
    }
}
