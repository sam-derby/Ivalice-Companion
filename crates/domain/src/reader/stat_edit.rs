//! Game-facing stat requests resolved through the reader's effective mechanics.
use super::{stats, Fact, ReaderUnit, StoredBases};
use crate::{game_data::reader_catalogue::ReaderMechanics, ValueState};
use serde::{Deserialize, Serialize};

pub const MAX_BASE: u32 = 0x00ff_ffff;
pub const STAT_SCALE: u64 = 1_638_400;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseStatKind {
    Hp,
    Mp,
    Speed,
    PhysicalAttack,
    MagicalAttack,
}

impl BaseStatKind {
    pub const ALL: [Self; 5] = [
        Self::Hp,
        Self::Mp,
        Self::Speed,
        Self::PhysicalAttack,
        Self::MagicalAttack,
    ];

    pub const fn display_limit(self) -> u32 {
        match self {
            Self::Hp | Self::Mp => 999,
            Self::Speed => 50,
            Self::PhysicalAttack | Self::MagicalAttack => 99,
        }
    }

    pub fn base(self, bases: &StoredBases) -> &Fact<u32> {
        match self {
            Self::Hp => &bases.hp,
            Self::Mp => &bases.mp,
            Self::Speed => &bases.speed,
            Self::PhysicalAttack => &bases.physical_attack,
            Self::MagicalAttack => &bases.magical_attack,
        }
    }

    pub fn base_mut(self, bases: &mut StoredBases) -> &mut Fact<u32> {
        match self {
            Self::Hp => &mut bases.hp,
            Self::Mp => &mut bases.mp,
            Self::Speed => &mut bases.speed,
            Self::PhysicalAttack => &mut bases.physical_attack,
            Self::MagicalAttack => &mut bases.magical_attack,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatEditError {
    UnknownMechanics,
    Unrepresentable,
}

/// A single edited game-visible base, with equipment added afterwards.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseStatPreview {
    pub stored_base: u32,
    pub base: Fact<u32>,
    pub total: Fact<u32>,
}

/// Pure arithmetic over normalized reader inputs. No save decode or full projection.
/// None restores an exact saved base rather than inverse-solving it again.
pub fn preview_base(
    previous: u32,
    requested: Option<u32>,
    multiplier: u16,
    bonus: Option<i32>,
    stat: BaseStatKind,
) -> Result<BaseStatPreview, StatEditError> {
    let stored_base = match requested {
        Some(value) => solve_base(previous, value, multiplier, 0, stat.display_limit())?,
        None => previous,
    };
    let base =
        visible(stored_base, multiplier, 0, u32::MAX).ok_or(StatEditError::Unrepresentable)?;
    let total = bonus
        .and_then(|bonus| total_with_equipment(stored_base, multiplier, bonus, stat))
        .map_or(ValueState::Unknown, ValueState::Known);
    Ok(BaseStatPreview {
        stored_base,
        base: Fact {
            value: ValueState::Known(base),
        },
        total: Fact { value: total },
    })
}

/// Shared forward calculation, including the reader's minimum and evidence limits.
pub fn visible(base: u32, multiplier: u16, bonus: i32, limit: u32) -> Option<u32> {
    if multiplier == 0 || base > MAX_BASE {
        return None;
    }
    let scaled = (u64::from(base) * u64::from(multiplier) / STAT_SCALE).max(1);
    let value = i64::try_from(scaled).ok()?.checked_add(i64::from(bonus))?;
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0 && *value <= limit)
}

/// PA/MA keep their 99 display cap when equipment adds to a valid base.
/// TIC/remaster/formulas/FORMULA_REFERENCE.md at c2dae2a records PA/MA's 99
/// battle cap; the owner's requested editor display uses that same cap.
/// The other stat families retain their existing evidence limits.
pub fn total_with_equipment(
    base: u32,
    multiplier: u16,
    bonus: i32,
    stat: BaseStatKind,
) -> Option<u32> {
    match stat {
        BaseStatKind::PhysicalAttack | BaseStatKind::MagicalAttack => {
            let visible_base = visible(base, multiplier, 0, stat.display_limit())?;
            let total = i64::from(visible_base).checked_add(i64::from(bonus))?;
            u32::try_from(total)
                .ok()
                .filter(|total| *total > 0)
                .map(|total| total.min(stat.display_limit()))
        }
        BaseStatKind::Hp | BaseStatKind::Mp | BaseStatKind::Speed => {
            visible(base, multiplier, bonus, stat.display_limit())
        }
    }
}

/// Clamp the pending base into the exact integer preimage of the visible request.
/// The minimum-one case also includes bases whose unbounded floor is zero.
pub fn solve_base(
    previous: u32,
    requested: u32,
    multiplier: u16,
    bonus: i32,
    limit: u32,
) -> Result<u32, StatEditError> {
    if multiplier == 0 {
        return Err(StatEditError::UnknownMechanics);
    }
    if previous > MAX_BASE || requested == 0 || requested > limit {
        return Err(StatEditError::Unrepresentable);
    }
    let target = i64::from(requested) - i64::from(bonus);
    let target = u64::try_from(target)
        .ok()
        .filter(|value| *value >= 1)
        .ok_or(StatEditError::Unrepresentable)?;
    let multiplier = u64::from(multiplier);
    let low = if target == 1 {
        0
    } else {
        (target * STAT_SCALE).div_ceil(multiplier)
    };
    let high = ((target + 1) * STAT_SCALE).div_ceil(multiplier) - 1;
    let high = high.min(u64::from(MAX_BASE));
    if low > high {
        return Err(StatEditError::Unrepresentable);
    }
    let base = u32::try_from(u64::from(previous).clamp(low, high))
        .map_err(|_| StatEditError::Unrepresentable)?;
    if visible(
        base,
        u16::try_from(multiplier).map_err(|_| StatEditError::Unrepresentable)?,
        bonus,
        limit,
    ) != Some(requested)
    {
        return Err(StatEditError::Unrepresentable);
    }
    Ok(base)
}

pub fn solve_unit(
    unit: &ReaderUnit,
    data: &ReaderMechanics,
    stat: BaseStatKind,
    requested: u32,
) -> Result<u32, StatEditError> {
    let ValueState::Known(previous) = stat.base(&unit.stored.bases).value else {
        return Err(StatEditError::UnknownMechanics);
    };
    let (multiplier, bonus) =
        stats::stat_inputs(unit, data, stat).ok_or(StatEditError::UnknownMechanics)?;
    solve_base(previous, requested, multiplier, bonus, stat.display_limit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edited_base_excludes_equipment_and_reset_preserves_exact_raw_value() {
        let edited = preview_base(100_000, Some(94), 100, Some(4), BaseStatKind::MagicalAttack)
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(edited.base.value, ValueState::Known(94));
        assert_eq!(edited.total.value, ValueState::Known(98));
        let changed_gear = preview_base(
            edited.stored_base,
            None,
            100,
            Some(1),
            BaseStatKind::MagicalAttack,
        )
        .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(changed_gear.stored_base, edited.stored_base);
        assert_eq!(changed_gear.base.value, ValueState::Known(94));
        assert_eq!(changed_gear.total.value, ValueState::Known(95));
        let reset = preview_base(100_001, None, 100, Some(4), BaseStatKind::MagicalAttack)
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(reset.stored_base, 100_001);
    }

    #[test]
    fn attack_equipment_bonus_stops_at_99_without_changing_the_base() {
        for stat in [BaseStatKind::PhysicalAttack, BaseStatKind::MagicalAttack] {
            let preview = preview_base(0, Some(99), 100, Some(4), stat)
                .unwrap_or_else(|error| panic!("{error:?}"));
            assert_eq!(preview.base.value, ValueState::Known(99));
            assert_eq!(preview.total.value, ValueState::Known(99));
            assert_eq!(
                total_with_equipment(preview.stored_base, 100, -4, stat),
                Some(95)
            );
            assert_eq!(
                total_with_equipment(preview.stored_base, 100, 0, stat),
                Some(99)
            );
        }
    }

    #[test]
    fn base_preview_keeps_unknown_or_unevidenced_totals_unknown() {
        let preview = preview_base(0, Some(94), 100, None, BaseStatKind::MagicalAttack)
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(preview.base.value, ValueState::Known(94));
        assert_eq!(preview.total.value, ValueState::Unknown);
        let hp = preview_base(0, Some(999), 100, Some(4), BaseStatKind::Hp)
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(hp.total.value, ValueState::Unknown);
        assert!(preview_base(0, Some(100), 100, Some(0), BaseStatKind::MagicalAttack).is_err());
        assert!(preview_base(0, Some(10), 0, Some(0), BaseStatKind::Hp).is_err());
    }

    #[test]
    fn every_stat_roundtrips_and_keeps_an_already_valid_base() {
        for stat in BaseStatKind::ALL {
            for multiplier in [1, 75, 100, 135, u16::MAX] {
                for base in [0, 1, 98_304, 458_752, MAX_BASE] {
                    for bonus in [-1, 0, 1, 120] {
                        if let Some(value) = visible(base, multiplier, bonus, stat.display_limit())
                        {
                            assert_eq!(
                                solve_base(base, value, multiplier, bonus, stat.display_limit()),
                                Ok(base)
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn nearest_interval_endpoints_and_minimum_one() {
        assert_eq!(solve_base(0, 10, 100, 0, 50), Ok(163_840));
        assert_eq!(solve_base(MAX_BASE, 10, 100, 0, 50), Ok(180_223));
        assert_eq!(solve_base(0, 1, 100, 0, 50), Ok(0));
        assert_eq!(solve_base(MAX_BASE, 1, 100, 0, 50), Ok(32_767));
        let base = solve_base(100_000, 12, 100, 0, 50).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(visible(base, 100, 1, 50), Some(13));
        assert_eq!(visible(base, 100, 0, 50), Some(12));
    }

    #[test]
    fn invalid_and_unattainable_values_fail_closed() {
        for (previous, requested, multiplier, bonus) in [
            (0, 0, 100, 0),
            (0, 1000, 100, 0),
            (0, 1, 0, 0),
            (MAX_BASE + 1, 10, 100, 0),
            (0, 120, 100, 120),
            (0, 999, 1, 0),
        ] {
            assert!(solve_base(previous, requested, multiplier, bonus, 999).is_err());
        }
        let value = visible(MAX_BASE, 1, 0, 999).unwrap_or_else(|| panic!("upper bound"));
        assert_eq!(solve_base(MAX_BASE, value, 1, 0, 999), Ok(MAX_BASE));
    }
}
