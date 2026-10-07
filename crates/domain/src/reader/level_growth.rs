//! Experimental level growth. Not wired into desktop IPC yet; game parity still needs checking.
use super::{
    stat_edit::{BaseStatKind, MAX_BASE},
    GrowthCoefficients, StoredBases,
};
use crate::ValueState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrowthError {
    InvalidLevel,
    UnknownBase,
    OutOfRange,
}

/// Recompute from the saved baseline, once per crossed level. Explicit overrides win.
pub fn simulate(
    saved: &StoredBases,
    from: u8,
    to: u8,
    growth: GrowthCoefficients,
    overrides: &[(BaseStatKind, u32)],
) -> Result<StoredBases, GrowthError> {
    if !(1..=99).contains(&from) || !(1..=99).contains(&to) {
        return Err(GrowthError::InvalidLevel);
    }
    let mut result = saved.clone();
    for stat in BaseStatKind::ALL {
        let ValueState::Known(mut base) = stat.base(saved).value else {
            return Err(GrowthError::UnknownBase);
        };
        if base > MAX_BASE {
            return Err(GrowthError::OutOfRange);
        }
        let coefficient = match stat {
            BaseStatKind::Hp => growth.hp,
            BaseStatKind::Mp => growth.mp,
            BaseStatKind::Speed => growth.speed,
            BaseStatKind::PhysicalAttack => growth.physical_attack,
            BaseStatKind::MagicalAttack => growth.magical_attack,
        };
        let mut level = from;
        while level != to {
            let denominator =
                u32::from(coefficient) + u32::from(if level < to { level } else { level - 1 });
            let amount = base / denominator;
            base = if level < to {
                base.checked_add(amount)
                    .filter(|value| *value <= MAX_BASE)
                    .ok_or(GrowthError::OutOfRange)?
            } else {
                base - amount
            };
            if level < to {
                level += 1;
            } else {
                level -= 1;
            }
        }
        stat.base_mut(&mut result).value = ValueState::Known(base);
    }
    for (stat, base) in overrides {
        if *base > MAX_BASE {
            return Err(GrowthError::OutOfRange);
        }
        stat.base_mut(&mut result).value = ValueState::Known(*base);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bases(value: u32) -> StoredBases {
        let fact = super::super::Fact {
            value: ValueState::Known(value),
        };
        StoredBases {
            hp: fact.clone(),
            mp: fact.clone(),
            speed: fact.clone(),
            physical_attack: fact.clone(),
            magical_attack: fact,
        }
    }
    fn growth() -> GrowthCoefficients {
        GrowthCoefficients {
            hp: 40,
            mp: 50,
            speed: 100,
            physical_attack: 40,
            magical_attack: 50,
        }
    }
    #[test]
    fn independent_steps_job_coefficients_and_explicit_overrides() {
        let saved = bases(10_000);
        let same =
            simulate(&saved, 10, 10, growth(), &[]).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(same, saved);
        let up =
            simulate(&saved, 10, 11, growth(), &[]).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(up.hp.value, ValueState::Known(10_200));
        let down =
            simulate(&saved, 10, 9, growth(), &[]).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(down.hp.value, ValueState::Known(9_796));
        let multi =
            simulate(&saved, 10, 12, growth(), &[]).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(multi.hp.value, ValueState::Known(10_400));
        assert_eq!(simulate(&up, 11, 12, growth(), &[]), Ok(multi));
        assert_ne!(
            simulate(
                &saved,
                10,
                11,
                GrowthCoefficients { hp: 10, ..growth() },
                &[]
            ),
            Ok(up)
        );
        let manual = simulate(&saved, 10, 12, growth(), &[(BaseStatKind::Hp, 12_345)])
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(manual.hp.value, ValueState::Known(12_345));
        assert_eq!(saved, bases(10_000));
    }
    #[test]
    fn invalid_missing_and_overflow_fail() {
        assert_eq!(
            simulate(&bases(10), 0, 10, growth(), &[]),
            Err(GrowthError::InvalidLevel)
        );
        assert_eq!(
            simulate(&bases(MAX_BASE), 10, 11, growth(), &[]),
            Err(GrowthError::OutOfRange)
        );
        let mut missing = bases(10);
        missing.mp.value = ValueState::Unknown;
        assert_eq!(
            simulate(&missing, 10, 11, growth(), &[]),
            Err(GrowthError::UnknownBase)
        );
        assert_eq!(
            simulate(
                &bases(10),
                1,
                2,
                GrowthCoefficients { hp: 0, ..growth() },
                &[]
            )
            .map(|value| value.hp.value),
            Ok(ValueState::Known(20))
        );
    }
}
