//! TICSaveEditor.Core/Operations/SlotOperations.cs CopyOrDuplicate at 07ea857:
//! capture an intact 600-byte donor, preserving every opaque byte.

use super::{
    occupancy_marker, unit_record, ManualParseError, ManualPayload, BATTLE_OFFSET, BATTLE_SIZE,
};
use crate::{DecodedContainer, StoredAdlerStatus, UnitRecord};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitImageError {
    Manual(ManualParseError),
    StoredChecksum,
    EmptySource,
}

impl From<ManualParseError> for UnitImageError {
    fn from(error: ManualParseError) -> Self {
        Self::Manual(error)
    }
}

impl std::fmt::Display for UnitImageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for UnitImageError {}

/// An intact saved donor, not a fresh-character template or a recruitment
/// operation. Insertion must separately verify destination bookkeeping.
/// No Debug implementation: diagnostics must not expose personal save bytes.
#[derive(Clone, Eq, PartialEq)]
pub struct UnitRecordImage {
    bytes: [u8; unit_record::SIZE],
    record: UnitRecord,
}

impl UnitRecordImage {
    #[must_use]
    pub fn bytes(&self) -> &[u8; unit_record::SIZE] {
        &self.bytes
    }

    #[must_use]
    pub fn record(&self) -> &UnitRecord {
        &self.record
    }
}

impl DecodedContainer {
    /// Read a nonempty donor without normalizing padding, names, flags or
    /// combat sets. Like upstream copying, inactive and guest records can be
    /// captured; this does not establish their suitability for recruitment.
    pub fn unit_record_image(
        &self,
        manual_slot: u8,
        unit_position: u8,
    ) -> Result<UnitRecordImage, UnitImageError> {
        if self.stored_adler_status() != StoredAdlerStatus::Matched {
            return Err(UnitImageError::StoredChecksum);
        }
        let payload = ManualPayload::parse(self.payload())?;
        let slot = payload.slot_record(usize::from(manual_slot))?;
        if occupancy_marker(slot)? == 0 {
            return Err(ManualParseError::SlotEmpty.into());
        }
        let source = unit_record(
            slot,
            usize::from(unit_position),
            BATTLE_OFFSET + BATTLE_SIZE,
        )?;
        let record = UnitRecord::parse(source)?;
        if record.is_empty() {
            return Err(UnitImageError::EmptySource);
        }
        let mut bytes = [0; unit_record::SIZE];
        bytes.copy_from_slice(source);
        Ok(UnitRecordImage { bytes, record })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{MANUAL_SLOT_SIZE, PAYLOAD_HEADER_SIZE};
    use super::*;
    use crate::{
        SUPPORTED_FORMAT_DISCRIMINATOR, SUPPORTED_PAYLOAD_LENGTH, SUPPORTED_PAYLOAD_VERSION,
    };

    fn fixture() -> DecodedContainer {
        let mut payload = vec![0; SUPPORTED_PAYLOAD_LENGTH];
        payload[..4].copy_from_slice(&SUPPORTED_PAYLOAD_VERSION.to_le_bytes());
        payload[8..16].copy_from_slice(&SUPPORTED_FORMAT_DISCRIMINATOR.to_le_bytes());
        payload[PAYLOAD_HEADER_SIZE..PAYLOAD_HEADER_SIZE + 2].copy_from_slice(&1_u16.to_le_bytes());
        let start = PAYLOAD_HEADER_SIZE + BATTLE_OFFSET;
        for (index, byte) in payload[start..start + unit_record::SIZE]
            .iter_mut()
            .enumerate()
        {
            *byte = index.to_le_bytes()[0].wrapping_add(1);
        }
        // A noncanonical true byte must survive intact instead of becoming 1.
        payload[start + 0x126 + 0x57] = 0x80;
        DecodedContainer {
            payload: payload.into_boxed_slice(),
            stored_adler_status: StoredAdlerStatus::Matched,
        }
    }

    #[test]
    fn donor_preserves_every_byte_and_does_not_rebind_identity() -> Result<(), UnitImageError> {
        let decoded = fixture();
        let before = decoded.payload().to_vec();
        let image = decoded.unit_record_image(0, 0)?;
        let start = PAYLOAD_HEADER_SIZE + BATTLE_OFFSET;
        assert!(image.bytes().as_slice() == &before[start..start + unit_record::SIZE]);
        assert!(image.record().combat_sets[0].is_double_hand);
        assert_eq!(image.bytes()[0x126 + 0x57], 0x80);
        assert_eq!(image.record().unit_index, 2);
        assert!(!image.record().is_active(0));
        assert!(image.clone() == image);
        assert!(decoded.payload() == before);
        Ok(())
    }

    #[test]
    fn source_checks_fail_closed() {
        let mut decoded = fixture();
        assert!(matches!(
            decoded.unit_record_image(0, 1),
            Err(UnitImageError::EmptySource)
        ));
        assert!(matches!(
            decoded.unit_record_image(1, 0),
            Err(UnitImageError::Manual(ManualParseError::SlotEmpty))
        ));
        assert!(matches!(
            decoded.unit_record_image(50, 0),
            Err(UnitImageError::Manual(ManualParseError::SlotIndex))
        ));
        assert!(matches!(
            decoded.unit_record_image(0, 54),
            Err(UnitImageError::Manual(ManualParseError::UnitBounds))
        ));
        decoded.stored_adler_status = StoredAdlerStatus::Mismatched;
        assert!(matches!(
            decoded.unit_record_image(0, 0),
            Err(UnitImageError::StoredChecksum)
        ));
        decoded.stored_adler_status = StoredAdlerStatus::Matched;
        decoded.payload[0] = 0;
        assert!(matches!(
            decoded.unit_record_image(0, 0),
            Err(UnitImageError::Manual(ManualParseError::UnsupportedVersion))
        ));
        decoded.payload = vec![0; 10].into_boxed_slice();
        assert!(matches!(
            decoded.unit_record_image(0, 0),
            Err(UnitImageError::Manual(ManualParseError::PayloadLength))
        ));
    }

    #[test]
    fn last_guest_in_last_slot_is_captured_without_mutating_either() -> Result<(), UnitImageError> {
        let mut decoded = fixture();
        let slot_start = PAYLOAD_HEADER_SIZE + 49 * MANUAL_SLOT_SIZE;
        decoded.payload[slot_start..slot_start + 2].copy_from_slice(&1_u16.to_le_bytes());
        let source = decoded.unit_record_image(0, 0)?;
        let start = slot_start + BATTLE_OFFSET + 53 * unit_record::SIZE;
        decoded.payload[start..start + unit_record::SIZE].copy_from_slice(source.bytes());
        let before = decoded.payload().to_vec();
        let guest = decoded.unit_record_image(49, 53)?;
        assert!(guest == source);
        assert!(decoded.payload() == before);
        Ok(())
    }
}
