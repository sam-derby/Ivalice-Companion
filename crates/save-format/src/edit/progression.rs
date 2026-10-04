//! TICSaveEditor.Core/Records/Layouts/UnitSaveDataLayout.cs at 07ea857.
//! Progression edits affect the independent level/EXP bytes and 24-bit bases only.
use super::{EditOperation, GilEditError};
use crate::{
    manual,
    manual::unit_record::{
        EXP_OFFSET, HP_BASE_OFFSET, LEVEL_OFFSET, MA_BASE_OFFSET, MP_BASE_OFFSET, PA_BASE_OFFSET,
        SPEED_BASE_OFFSET,
    },
    UnitRecord,
};
pub use ivalice_domain::reader::stat_edit::BaseStatKind;
use ivalice_domain::reader::stat_edit::MAX_BASE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Field {
    Level,
    Experience,
    Base(BaseStatKind),
}

impl Field {
    fn offset(self) -> usize {
        match self {
            Self::Level => LEVEL_OFFSET,
            Self::Experience => EXP_OFFSET,
            Self::Base(BaseStatKind::Hp) => HP_BASE_OFFSET,
            Self::Base(BaseStatKind::Mp) => MP_BASE_OFFSET,
            Self::Base(BaseStatKind::Speed) => SPEED_BASE_OFFSET,
            Self::Base(BaseStatKind::PhysicalAttack) => PA_BASE_OFFSET,
            Self::Base(BaseStatKind::MagicalAttack) => MA_BASE_OFFSET,
        }
    }
    fn width(self) -> usize {
        if matches!(self, Self::Base(_)) {
            3
        } else {
            1
        }
    }
    fn value(self, record: &UnitRecord) -> u32 {
        match self {
            Self::Level => u32::from(record.level),
            Self::Experience => u32::from(record.exp),
            Self::Base(BaseStatKind::Hp) => record.hp_max_base,
            Self::Base(BaseStatKind::Mp) => record.mp_max_base,
            Self::Base(BaseStatKind::Speed) => record.wt_base,
            Self::Base(BaseStatKind::PhysicalAttack) => record.at_base,
            Self::Base(BaseStatKind::MagicalAttack) => record.mat_base,
        }
    }
}

pub(crate) struct StagedField {
    offset: usize,
    position: u8,
    field: Field,
    value: u32,
}

impl StagedField {
    pub fn contains(&self, index: usize) -> bool {
        (self.offset..self.offset + self.field.width()).contains(&index)
    }
    pub fn verify(&self, records: &[UnitRecord]) -> bool {
        records
            .get(usize::from(self.position))
            .is_some_and(|record| self.field.value(record) == self.value)
    }
}

pub(crate) fn apply(
    payload: &mut [u8],
    slot: u8,
    operation: &EditOperation,
    staged: &mut Vec<StagedField>,
) -> Result<(), GilEditError> {
    let (position, field, value) = match *operation {
        EditOperation::CharacterLevel {
            unit_position,
            value,
        } => (unit_position, Field::Level, u32::from(value)),
        EditOperation::Experience {
            unit_position,
            value,
        } => (unit_position, Field::Experience, u32::from(value)),
        EditOperation::BaseStat {
            unit_position,
            stat,
            value,
        } => (unit_position, Field::Base(stat), value),
        _ => return Err(GilEditError::InvalidField),
    };
    let valid = match field {
        Field::Level => (1..=99).contains(&value),
        Field::Experience => value <= 99,
        Field::Base(_) => value <= MAX_BASE,
    };
    if !valid || position >= 50 {
        return Err(GilEditError::InvalidField);
    }
    let base = manual::identity_unit_payload_offset(payload, slot, position)
        .map_err(|_| GilEditError::InvalidField)?;
    let offset = base + field.offset();
    if staged.iter().any(|prior| prior.offset == offset) {
        return Err(GilEditError::DuplicateField);
    }
    payload
        .get_mut(offset..offset + field.width())
        .ok_or(GilEditError::Encoding)?
        .copy_from_slice(&value.to_le_bytes()[..field.width()]);
    staged.push(StagedField {
        offset,
        position,
        field,
        value,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn payload() -> Vec<u8> {
        let mut payload = vec![0; crate::SUPPORTED_PAYLOAD_LENGTH];
        payload[..4].copy_from_slice(&crate::SUPPORTED_PAYLOAD_VERSION.to_le_bytes());
        payload[8..16].copy_from_slice(&crate::SUPPORTED_FORMAT_DISCRIMINATOR.to_le_bytes());
        payload[0x10] = 1;
        let base = 0x10 + 0x518;
        payload[base] = 1;
        payload[base + 0x1d] = 37;
        payload
    }
    #[test]
    fn each_field_changes_only_its_bytes_and_decodes_exactly() {
        let operations = [
            EditOperation::CharacterLevel {
                unit_position: 0,
                value: 99,
            },
            EditOperation::Experience {
                unit_position: 0,
                value: 73,
            },
        ]
        .into_iter()
        .chain(BaseStatKind::ALL.map(|stat| EditOperation::BaseStat {
            unit_position: 0,
            stat,
            value: 0xabcdef,
        }));
        for operation in operations {
            let original = payload();
            let mut edited = original.clone();
            let mut staged = vec![];
            apply(&mut edited, 0, &operation, &mut staged)
                .unwrap_or_else(|error| panic!("{error:?}"));
            let decoded = crate::DecodedContainer {
                payload: edited.clone().into_boxed_slice(),
                stored_adler_status: crate::StoredAdlerStatus::Matched,
            };
            let records = decoded
                .unit_records(0)
                .unwrap_or_else(|error| panic!("{error:?}"))
                .unwrap_or_else(|| panic!("occupied"));
            assert!(staged[0].verify(&records));
            for (index, (before, after)) in original.iter().zip(&edited).enumerate() {
                assert!(before == after || staged[0].contains(index));
            }
            if matches!(operation, EditOperation::BaseStat { .. }) {
                assert_eq!(
                    &edited[staged[0].offset..staged[0].offset + 3],
                    &[0xef, 0xcd, 0xab]
                );
            }
            assert_eq!(
                apply(&mut edited, 0, &operation, &mut staged),
                Err(GilEditError::DuplicateField)
            );
        }
    }
    #[test]
    fn bounds_occupancy_and_party_membership_fail_without_mutation() {
        for operation in [
            EditOperation::CharacterLevel {
                unit_position: 0,
                value: 0,
            },
            EditOperation::CharacterLevel {
                unit_position: 0,
                value: 100,
            },
            EditOperation::Experience {
                unit_position: 0,
                value: 100,
            },
            EditOperation::BaseStat {
                unit_position: 0,
                stat: BaseStatKind::Hp,
                value: MAX_BASE + 1,
            },
            EditOperation::CharacterLevel {
                unit_position: 1,
                value: 1,
            },
            EditOperation::CharacterLevel {
                unit_position: 50,
                value: 1,
            },
        ] {
            let original = payload();
            let mut edited = original.clone();
            assert!(apply(&mut edited, 0, &operation, &mut vec![]).is_err());
            assert_eq!(original, edited);
        }
        for value in [0, MAX_BASE] {
            apply(
                &mut payload(),
                0,
                &EditOperation::BaseStat {
                    unit_position: 0,
                    stat: BaseStatKind::Mp,
                    value,
                },
                &mut vec![],
            )
            .unwrap_or_else(|error| panic!("{error:?}"));
        }
    }
}
