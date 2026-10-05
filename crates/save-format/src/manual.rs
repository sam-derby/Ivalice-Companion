use ivalice_domain::{
    ContainerMetadata, ManualSlot, ManualSlotId, NormalizedSave, SaveProvenance,
    SnapshotByteLength, SnapshotProvenance, ValueState,
};

use crate::{
    DecodedContainer, SUPPORTED_FORMAT_DISCRIMINATOR, SUPPORTED_PAYLOAD_LENGTH,
    SUPPORTED_PAYLOAD_VERSION,
};

pub mod inventory;
pub mod slot_metadata;
pub(super) mod unit_image;
pub(super) mod unit_record;
mod upstream_reader;

const PAYLOAD_HEADER_SIZE: usize = 0x10;
const MANUAL_SLOT_COUNT: usize = 50;
const MANUAL_SLOT_SIZE: usize = 0x9ce4;
const OCCUPANCY_MARKER_SIZE: usize = 2;

const BATTLE_OFFSET: usize = 0x0518;
const BATTLE_SIZE: usize = 0x8f48;
const UNIT_SIZE: usize = 600;
const UNIT_COUNT: usize = 54;

/// A bounded failure while adapting an integrity-checked payload into manual
/// slot and regular-roster domain values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManualParseError {
    PayloadLength,
    UnsupportedVersion,
    UnsupportedKind,
    SlotIndex,
    SlotEmpty,
    SlotBounds,
    BattleBounds,
    UnitBounds,
    DomainInvariant,
}

impl ManualParseError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::PayloadLength => "manual_payload_length",
            Self::UnsupportedVersion => "manual_unsupported_version",
            Self::UnsupportedKind => "manual_unsupported_kind",
            Self::SlotIndex => "manual_slot_index",
            Self::SlotEmpty => "manual_slot_empty",
            Self::SlotBounds => "manual_slot_bounds",
            Self::BattleBounds => "manual_battle_bounds",
            Self::UnitBounds => "manual_unit_bounds",
            Self::DomainInvariant => "manual_domain_invariant",
        }
    }
}

impl std::fmt::Display for ManualParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ManualParseError {}

impl DecodedContainer {
    /// Decode public slot metadata and progress storage for one occupied slot.
    pub fn slot_metadata(
        &self,
        selected_slot_index: u8,
    ) -> Result<Option<slot_metadata::SlotMetadata>, ManualParseError> {
        let payload = ManualPayload::parse(self.payload())?;
        let slot = payload.slot_record(usize::from(selected_slot_index))?;
        if occupancy_marker(slot)? == 0 {
            return Ok(None);
        }
        Ok(Some(slot_metadata::SlotMetadata::parse(slot)?))
    }

    /// Read distinct party, shop and found-item storage counts from one slot.
    pub fn battle_stores(
        &self,
        selected_slot_index: u8,
    ) -> Result<Option<inventory::BattleStores>, ManualParseError> {
        let payload = ManualPayload::parse(self.payload())?;
        let slot = payload.slot_record(usize::from(selected_slot_index))?;
        if occupancy_marker(slot)? == 0 {
            return Ok(None);
        }
        Ok(Some(inventory::BattleStores::parse(slot)?))
    }

    /// Decode all 54 upstream unit positions in one occupied manual slot.
    /// Empty and inactive records remain present so callers can compare them
    /// with the upstream reference without changing the save.
    pub fn unit_records(
        &self,
        selected_slot_index: u8,
    ) -> Result<Option<Vec<unit_record::UnitRecord>>, ManualParseError> {
        let payload = ManualPayload::parse(self.payload())?;
        let slot = payload.slot_record(usize::from(selected_slot_index))?;
        if occupancy_marker(slot)? == 0 {
            return Ok(None);
        }
        let battle_end = BATTLE_OFFSET
            .checked_add(BATTLE_SIZE)
            .ok_or(ManualParseError::BattleBounds)?;
        let mut units = Vec::with_capacity(UNIT_COUNT);
        for index in 0..UNIT_COUNT {
            units.push(unit_record::UnitRecord::parse(unit_record(
                slot, index, battle_end,
            )?)?);
        }
        Ok(Some(units))
    }

    /// Return source-rule occupied manual positions in stable payload order.
    ///
    /// The two-byte marker establishes occupancy only. It does not validate or
    /// assign meaning to the record's remaining opaque fields.
    pub fn occupied_manual_slots(&self) -> Result<Vec<ManualSlotId>, ManualParseError> {
        let payload = ManualPayload::parse(self.payload())?;
        payload.occupied_slots()
    }

    /// Normalize one manual position's structural metadata.
    ///
    /// An empty position is `Absent`. The writer build remains `Unknown`; the
    /// embedded tuple is structural evidence and does not identify a writer.
    pub fn normalized_manual_save(
        &self,
        snapshot_byte_length: SnapshotByteLength,
        selected_slot_index: u8,
    ) -> Result<NormalizedSave, ManualParseError> {
        let payload = ManualPayload::parse(self.payload())?;
        let selected_manual_slot = payload.selected_slot(selected_slot_index)?;
        Ok(NormalizedSave::supported(
            SaveProvenance {
                snapshot: SnapshotProvenance {
                    byte_length: snapshot_byte_length,
                },
                writer_build: ValueState::Unknown,
            },
            ContainerMetadata {
                embedded_payload_version: SUPPORTED_PAYLOAD_VERSION,
                format_discriminator: SUPPORTED_FORMAT_DISCRIMINATOR,
                payload_byte_length: u32::try_from(SUPPORTED_PAYLOAD_LENGTH)
                    .map_err(|_| ManualParseError::DomainInvariant)?,
                stored_adler_status: self.stored_adler_status(),
            },
            selected_manual_slot,
        ))
    }
}

pub(crate) fn gil_payload_offset(
    payload: &[u8],
    selected_slot_index: u8,
) -> Result<usize, ManualParseError> {
    let parsed = ManualPayload::parse(payload)?;
    let record = parsed.slot_record(usize::from(selected_slot_index))?;
    if occupancy_marker(record)? == 0 {
        return Err(ManualParseError::SlotEmpty);
    }
    PAYLOAD_HEADER_SIZE
        .checked_add(usize::from(selected_slot_index) * MANUAL_SLOT_SIZE)
        .and_then(|start| start.checked_add(slot_metadata::GIL_OFFSET))
        .ok_or(ManualParseError::SlotBounds)
}

/// Payload byte range of manual slot `index`, occupied or empty.
pub(crate) fn slot_payload_range(
    payload: &[u8],
    index: u8,
) -> Result<std::ops::Range<usize>, ManualParseError> {
    let parsed = ManualPayload::parse(payload)?;
    parsed.slot_record(usize::from(index))?;
    let start = PAYLOAD_HEADER_SIZE + usize::from(index) * MANUAL_SLOT_SIZE;
    Ok(start..start + MANUAL_SLOT_SIZE)
}

pub(crate) const SLOT_RECORD_SIZE: usize = MANUAL_SLOT_SIZE;
pub(crate) const SLOT_COUNT: u8 = 50;

pub(crate) fn record_is_occupied(record: &[u8]) -> Result<bool, ManualParseError> {
    Ok(occupancy_marker(record)? != 0)
}

pub(crate) fn occupied_slot_payload_base(
    payload: &[u8],
    selected_slot_index: u8,
) -> Result<usize, ManualParseError> {
    let parsed = ManualPayload::parse(payload)?;
    let record = parsed.slot_record(usize::from(selected_slot_index))?;
    if occupancy_marker(record)? == 0 {
        return Err(ManualParseError::SlotEmpty);
    }
    PAYLOAD_HEADER_SIZE
        .checked_add(usize::from(selected_slot_index) * MANUAL_SLOT_SIZE)
        .ok_or(ManualParseError::SlotBounds)
}

/// All 54 unit records: party positions 0..49 and story guests 50..53.
pub(crate) fn unit_block_payload_range(
    payload: &[u8],
    selected_slot_index: u8,
) -> Result<std::ops::Range<usize>, ManualParseError> {
    let start = occupied_slot_payload_base(payload, selected_slot_index)?
        .checked_add(BATTLE_OFFSET)
        .ok_or(ManualParseError::UnitBounds)?;
    let end = start + UNIT_COUNT * UNIT_SIZE;
    payload
        .get(start..end)
        .ok_or(ManualParseError::UnitBounds)?;
    Ok(start..end)
}

pub(crate) fn party_count_payload_offset(
    payload: &[u8],
    selected_slot_index: u8,
    item_position: u16,
) -> Result<usize, ManualParseError> {
    let position = usize::from(item_position);
    if position >= inventory::PARTY_CAPACITY || matches!(position, 0 | 254 | 255) {
        return Err(ManualParseError::DomainInvariant);
    }
    let parsed = ManualPayload::parse(payload)?;
    let record = parsed.slot_record(usize::from(selected_slot_index))?;
    if occupancy_marker(record)? == 0 {
        return Err(ManualParseError::SlotEmpty);
    }
    let offset = PAYLOAD_HEADER_SIZE
        .checked_add(usize::from(selected_slot_index) * MANUAL_SLOT_SIZE)
        .and_then(|start| start.checked_add(inventory::party_counts_offset()))
        .and_then(|start| start.checked_add(position))
        .ok_or(ManualParseError::SlotBounds)?;
    payload.get(offset).ok_or(ManualParseError::SlotBounds)?;
    Ok(offset)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UnitStatField {
    Bravery,
    Faith,
}

pub(crate) fn unit_stat_payload_offset(
    payload: &[u8],
    selected_slot_index: u8,
    unit_position: u8,
    field: UnitStatField,
) -> Result<usize, ManualParseError> {
    let base = editable_unit_payload_offset(payload, selected_slot_index, unit_position)?;
    let stat_offset = match field {
        UnitStatField::Bravery => 0x1e,
        UnitStatField::Faith => 0x1f,
    };
    base.checked_add(stat_offset)
        .ok_or(ManualParseError::UnitBounds)
}

/// Identity fields are encoded on all 54 active unit records, including guests
/// and monsters. Other field families retain their separate context checks.
pub(crate) fn identity_unit_payload_offset(
    payload: &[u8],
    slot: u8,
    position: u8,
) -> Result<usize, ManualParseError> {
    let position = usize::from(position);
    if position >= UNIT_COUNT {
        return Err(ManualParseError::UnitBounds);
    }
    let parsed = ManualPayload::parse(payload)?;
    let record = parsed.slot_record(usize::from(slot))?;
    if occupancy_marker(record)? == 0 {
        return Err(ManualParseError::SlotEmpty);
    }
    let unit = unit_record::UnitRecord::parse(unit_record(
        record,
        position,
        BATTLE_OFFSET + BATTLE_SIZE,
    )?)?;
    if !unit.is_active(position) {
        return Err(ManualParseError::DomainInvariant);
    }
    Ok(PAYLOAD_HEADER_SIZE
        + usize::from(slot) * MANUAL_SLOT_SIZE
        + BATTLE_OFFSET
        + position * UNIT_SIZE)
}

pub(crate) fn editable_unit_payload_offset(
    payload: &[u8],
    selected_slot_index: u8,
    unit_position: u8,
) -> Result<usize, ManualParseError> {
    let position = usize::from(unit_position);
    if position >= 50 {
        return Err(ManualParseError::UnitBounds);
    }
    let parsed = ManualPayload::parse(payload)?;
    let record = parsed.slot_record(usize::from(selected_slot_index))?;
    if occupancy_marker(record)? == 0 {
        return Err(ManualParseError::SlotEmpty);
    }
    let battle_end = BATTLE_OFFSET + BATTLE_SIZE;
    let unit = unit_record::UnitRecord::parse(unit_record(record, position, battle_end)?)?;
    if !unit.is_active(position) || (0x5e..=0x8d).contains(&unit.job) {
        return Err(ManualParseError::DomainInvariant);
    }
    let offset = PAYLOAD_HEADER_SIZE
        .checked_add(usize::from(selected_slot_index) * MANUAL_SLOT_SIZE)
        .and_then(|start| start.checked_add(BATTLE_OFFSET))
        .and_then(|start| start.checked_add(position * UNIT_SIZE))
        .ok_or(ManualParseError::UnitBounds)?;
    payload
        .get(offset..offset + UNIT_SIZE)
        .ok_or(ManualParseError::UnitBounds)?;
    Ok(offset)
}

/// Candidate for a same-position donor copy. Inactive populated records are
/// eligible because the controlled Guild recruitment reused one such record.
pub(crate) fn inactive_party_unit_payload_offset(
    payload: &[u8],
    selected_slot_index: u8,
    unit_position: u8,
) -> Result<usize, ManualParseError> {
    let position = usize::from(unit_position);
    if !(1..50).contains(&position) {
        return Err(ManualParseError::UnitBounds);
    }
    let parsed = ManualPayload::parse(payload)?;
    let slot = parsed.slot_record(usize::from(selected_slot_index))?;
    if occupancy_marker(slot)? == 0 {
        return Err(ManualParseError::SlotEmpty);
    }
    let candidate =
        unit_record::UnitRecord::parse(unit_record(slot, position, BATTLE_OFFSET + BATTLE_SIZE)?)?;
    if candidate.is_active(position) {
        return Err(ManualParseError::DomainInvariant);
    }
    let offset = PAYLOAD_HEADER_SIZE
        .checked_add(usize::from(selected_slot_index) * MANUAL_SLOT_SIZE)
        .and_then(|start| start.checked_add(BATTLE_OFFSET))
        .and_then(|start| start.checked_add(position * UNIT_SIZE))
        .ok_or(ManualParseError::UnitBounds)?;
    payload
        .get(offset..offset + UNIT_SIZE)
        .ok_or(ManualParseError::UnitBounds)?;
    Ok(offset)
}

struct ManualPayload<'a> {
    bytes: &'a [u8],
}

impl<'a> ManualPayload<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, ManualParseError> {
        if bytes.len() != SUPPORTED_PAYLOAD_LENGTH {
            return Err(ManualParseError::PayloadLength);
        }
        if read_u32(bytes, 0)? != SUPPORTED_PAYLOAD_VERSION {
            return Err(ManualParseError::UnsupportedVersion);
        }
        if read_u64(bytes, 8)? != SUPPORTED_FORMAT_DISCRIMINATOR {
            return Err(ManualParseError::UnsupportedKind);
        }

        let expected_length = MANUAL_SLOT_SIZE
            .checked_mul(MANUAL_SLOT_COUNT)
            .and_then(|length| length.checked_add(PAYLOAD_HEADER_SIZE))
            .ok_or(ManualParseError::SlotBounds)?;
        if expected_length != bytes.len() {
            return Err(ManualParseError::PayloadLength);
        }
        Ok(Self { bytes })
    }

    fn occupied_slots(&self) -> Result<Vec<ManualSlotId>, ManualParseError> {
        let mut occupied = Vec::with_capacity(MANUAL_SLOT_COUNT);
        for index in 0..MANUAL_SLOT_COUNT {
            let record = self.slot_record(index)?;
            if occupancy_marker(record)? != 0 {
                let index = u8::try_from(index).map_err(|_| ManualParseError::DomainInvariant)?;
                occupied
                    .push(ManualSlotId::new(index).map_err(|_| ManualParseError::DomainInvariant)?);
            }
        }
        Ok(occupied)
    }

    fn selected_slot(
        &self,
        selected_slot_index: u8,
    ) -> Result<ValueState<ManualSlot>, ManualParseError> {
        let slot_id =
            ManualSlotId::new(selected_slot_index).map_err(|_| ManualParseError::SlotIndex)?;
        let record = self.slot_record(usize::from(selected_slot_index))?;
        if occupancy_marker(record)? == 0 {
            return Ok(ValueState::Absent);
        }

        Ok(ValueState::Known(ManualSlot { id: slot_id }))
    }

    fn slot_record(&self, index: usize) -> Result<&'a [u8], ManualParseError> {
        if index >= MANUAL_SLOT_COUNT {
            return Err(ManualParseError::SlotIndex);
        }
        let start = index
            .checked_mul(MANUAL_SLOT_SIZE)
            .and_then(|offset| PAYLOAD_HEADER_SIZE.checked_add(offset))
            .ok_or(ManualParseError::SlotBounds)?;
        let end = start
            .checked_add(MANUAL_SLOT_SIZE)
            .ok_or(ManualParseError::SlotBounds)?;
        self.bytes
            .get(start..end)
            .ok_or(ManualParseError::SlotBounds)
    }
}

fn occupancy_marker(record: &[u8]) -> Result<u16, ManualParseError> {
    let bytes = record
        .get(..OCCUPANCY_MARKER_SIZE)
        .ok_or(ManualParseError::SlotBounds)?;
    Ok(u16::from_le_bytes(
        bytes.try_into().map_err(|_| ManualParseError::SlotBounds)?,
    ))
}

fn unit_record(
    record: &[u8],
    unit_index: usize,
    battle_end: usize,
) -> Result<&[u8], ManualParseError> {
    if unit_index >= UNIT_COUNT {
        return Err(ManualParseError::UnitBounds);
    }
    let start = unit_index
        .checked_mul(UNIT_SIZE)
        .and_then(|offset| BATTLE_OFFSET.checked_add(offset))
        .ok_or(ManualParseError::UnitBounds)?;
    let end = start
        .checked_add(UNIT_SIZE)
        .ok_or(ManualParseError::UnitBounds)?;
    if end > battle_end {
        return Err(ManualParseError::UnitBounds);
    }
    record.get(start..end).ok_or(ManualParseError::UnitBounds)
}

fn read_u32(input: &[u8], offset: usize) -> Result<u32, ManualParseError> {
    let end = offset
        .checked_add(4)
        .ok_or(ManualParseError::PayloadLength)?;
    let bytes = input
        .get(offset..end)
        .ok_or(ManualParseError::PayloadLength)?;
    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| ManualParseError::PayloadLength)?,
    ))
}

fn read_u64(input: &[u8], offset: usize) -> Result<u64, ManualParseError> {
    let end = offset
        .checked_add(8)
        .ok_or(ManualParseError::PayloadLength)?;
    let bytes = input
        .get(offset..end)
        .ok_or(ManualParseError::PayloadLength)?;
    Ok(u64::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| ManualParseError::PayloadLength)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoredAdlerStatus;

    const CHARACTER_OFFSET: usize = 0x00;
    const UNIT_INDEX_OFFSET: usize = 0x01;
    const CURRENT_JOB_OFFSET: usize = 0x02;
    const LEVEL_OFFSET: usize = 0x1d;

    fn synthetic_payload() -> Vec<u8> {
        let mut payload = vec![0; SUPPORTED_PAYLOAD_LENGTH];
        payload[0..4].copy_from_slice(&SUPPORTED_PAYLOAD_VERSION.to_le_bytes());
        payload[8..16].copy_from_slice(&SUPPORTED_FORMAT_DISCRIMINATOR.to_le_bytes());
        payload
    }

    fn slot_start(index: usize) -> usize {
        PAYLOAD_HEADER_SIZE + index * MANUAL_SLOT_SIZE
    }

    fn occupy(payload: &mut [u8], slot_index: usize) {
        let start = slot_start(slot_index);
        payload[start..start + 2].copy_from_slice(&1_u16.to_le_bytes());
    }

    fn set_unit(
        payload: &mut [u8],
        slot_index: usize,
        unit_index: usize,
        character: u8,
        stored_index: u8,
        current_job: u8,
        level: u8,
    ) {
        let start = slot_start(slot_index) + BATTLE_OFFSET + unit_index * UNIT_SIZE;
        payload[start + CHARACTER_OFFSET] = character;
        payload[start + UNIT_INDEX_OFFSET] = stored_index;
        payload[start + CURRENT_JOB_OFFSET] = current_job;
        payload[start + LEVEL_OFFSET] = level;
    }

    #[test]
    fn inactive_party_target_accepts_stored_departure_and_checks_boundaries() {
        let mut payload = synthetic_payload();
        occupy(&mut payload, 49);
        set_unit(&mut payload, 49, 1, 0x80, 1, 0x4a, 1);
        set_unit(&mut payload, 49, 2, 3, 0xff, 3, 20);
        let departed = inactive_party_unit_payload_offset(&payload, 49, 2)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(departed, slot_start(49) + BATTLE_OFFSET + 2 * UNIT_SIZE);
        assert_eq!(
            inactive_party_unit_payload_offset(&payload, 49, 1),
            Err(ManualParseError::DomainInvariant)
        );
        assert_eq!(
            inactive_party_unit_payload_offset(&payload, 49, 49),
            Ok(slot_start(49) + BATTLE_OFFSET + 49 * UNIT_SIZE)
        );
        for position in [0, 50, 54, u8::MAX] {
            assert_eq!(
                inactive_party_unit_payload_offset(&payload, 49, position),
                Err(ManualParseError::UnitBounds)
            );
        }
        assert_eq!(
            inactive_party_unit_payload_offset(&payload, 48, 2),
            Err(ManualParseError::SlotEmpty)
        );
    }

    #[test]
    fn decoded_unit_records_include_all_positions_and_ignore_empty_slots() {
        let mut payload = synthetic_payload();
        occupy(&mut payload, 49);
        set_unit(&mut payload, 49, 53, 2, 53, 88, 42);
        let original = payload.clone();
        let decoded = DecodedContainer {
            payload: payload.into_boxed_slice(),
            stored_adler_status: StoredAdlerStatus::Matched,
        };
        assert_eq!(decoded.unit_records(0), Ok(None));
        let records = decoded
            .unit_records(49)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("occupied slot"));
        assert_eq!(records.len(), 54);
        assert!(records[0].is_empty());
        assert_eq!(records[53].job, 88);
        assert!(records[53].is_active(53));
        assert_eq!(decoded.unit_records(50), Err(ManualParseError::SlotIndex));
        assert_eq!(decoded.payload(), original);
    }

    #[test]
    fn exact_layout_enumerates_multiple_occupied_slots_in_stable_order() {
        let mut bytes = synthetic_payload();
        occupy(&mut bytes, 49);
        occupy(&mut bytes, 2);
        let before = bytes.clone();
        let parsed = ManualPayload::parse(&bytes).and_then(|value| value.occupied_slots());
        assert_eq!(
            parsed.map(|slots| slots
                .into_iter()
                .map(ManualSlotId::index)
                .collect::<Vec<_>>()),
            Ok(vec![2, 49])
        );
        assert_eq!(bytes, before);
    }

    #[test]
    fn empty_selected_slot_is_absent_even_when_opaque_bytes_are_nonzero() {
        let mut bytes = synthetic_payload();
        bytes[slot_start(4) + 20] = 0x7f;
        let selected = ManualPayload::parse(&bytes).and_then(|value| value.selected_slot(4));
        assert_eq!(selected, Ok(ValueState::Absent));
    }

    #[test]
    fn all_unit_positions_preserve_active_flags_jobs_and_levels() {
        let mut bytes = synthetic_payload();
        occupy(&mut bytes, 7);
        set_unit(&mut bytes, 7, 0, 0x80, 0, 0x4b, 42);
        set_unit(&mut bytes, 7, 1, 0, 1, 2, 10);
        set_unit(&mut bytes, 7, 2, 0x80, 0xff, 3, 11);
        set_unit(&mut bytes, 7, 3, 0x80, 3, 0xff, 0);
        set_unit(&mut bytes, 7, 49, 0x80, 49, 4, 99);
        set_unit(&mut bytes, 7, 50, 0x80, 50, 5, 42);
        let decoded = DecodedContainer {
            payload: bytes.into_boxed_slice(),
            stored_adler_status: StoredAdlerStatus::Matched,
        };
        let units = decoded
            .unit_records(7)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("occupied slot"));
        assert_eq!(units.len(), 54);
        assert!(units[0].is_active(0));
        assert_eq!((units[0].job, units[0].level), (0x4b, 42));
        assert!(units[1].is_empty());
        assert!(!units[2].is_active(2));
        assert!(units[3].is_active(3));
        assert_eq!((units[3].job, units[3].level), (0xff, 0));
        assert!(units[49].is_active(49));
        assert!(units[50].is_active(50));
    }

    #[test]
    fn unsupported_and_truncated_payloads_fail_closed() {
        let mut bytes = synthetic_payload();
        bytes[0..4].copy_from_slice(&(SUPPORTED_PAYLOAD_VERSION + 1).to_le_bytes());
        assert_eq!(
            ManualPayload::parse(&bytes).err(),
            Some(ManualParseError::UnsupportedVersion)
        );

        let mut bytes = synthetic_payload();
        bytes[8..16].copy_from_slice(&(SUPPORTED_FORMAT_DISCRIMINATOR + 1).to_le_bytes());
        assert_eq!(
            ManualPayload::parse(&bytes).err(),
            Some(ManualParseError::UnsupportedKind)
        );

        let mut bytes = synthetic_payload();
        bytes.pop();
        assert_eq!(
            ManualPayload::parse(&bytes).err(),
            Some(ManualParseError::PayloadLength)
        );
        assert_eq!(
            ManualPayload::parse(&[]).err(),
            Some(ManualParseError::PayloadLength)
        );
    }

    #[test]
    fn every_slot_and_unit_boundary_is_checked() {
        let bytes = synthetic_payload();
        let parsed = ManualPayload::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(parsed.slot_record(0).map(<[u8]>::len), Ok(MANUAL_SLOT_SIZE));
        assert_eq!(
            parsed.slot_record(MANUAL_SLOT_COUNT - 1).map(<[u8]>::len),
            Ok(MANUAL_SLOT_SIZE)
        );
        assert_eq!(
            parsed.slot_record(MANUAL_SLOT_COUNT),
            Err(ManualParseError::SlotIndex)
        );

        let record = parsed
            .slot_record(0)
            .unwrap_or_else(|error| panic!("{error}"));
        let battle_end = BATTLE_OFFSET + BATTLE_SIZE;
        assert_eq!(
            unit_record(record, 0, battle_end).map(<[u8]>::len),
            Ok(UNIT_SIZE)
        );
        assert_eq!(
            unit_record(record, UNIT_COUNT - 1, battle_end).map(<[u8]>::len),
            Ok(UNIT_SIZE)
        );
        assert_eq!(
            unit_record(record, UNIT_COUNT, battle_end),
            Err(ManualParseError::UnitBounds)
        );
        assert_eq!(
            unit_record(
                record,
                UNIT_COUNT - 1,
                BATTLE_OFFSET + UNIT_COUNT * UNIT_SIZE - 1
            ),
            Err(ManualParseError::UnitBounds)
        );
    }

    #[test]
    fn selected_slot_index_is_bounded() {
        let bytes = synthetic_payload();
        let parsed = ManualPayload::parse(&bytes).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(parsed.selected_slot(50), Err(ManualParseError::SlotIndex));
    }

    #[test]
    fn persistent_unit_stat_offsets_require_active_party_humans() {
        let mut bytes = synthetic_payload();
        occupy(&mut bytes, 7);
        set_unit(&mut bytes, 7, 3, 0x80, 3, 2, 10);
        set_unit(&mut bytes, 7, 4, 0x80, 4, 0x5e, 10);
        let start = slot_start(7) + BATTLE_OFFSET + 3 * UNIT_SIZE;
        assert_eq!(
            unit_stat_payload_offset(&bytes, 7, 3, UnitStatField::Bravery),
            Ok(start + 0x1e)
        );
        assert_eq!(
            unit_stat_payload_offset(&bytes, 7, 3, UnitStatField::Faith),
            Ok(start + 0x1f)
        );
        for position in [0, 4, 50] {
            assert!(unit_stat_payload_offset(&bytes, 7, position, UnitStatField::Bravery).is_err());
        }
        assert_eq!(
            unit_stat_payload_offset(&bytes, 8, 3, UnitStatField::Faith),
            Err(ManualParseError::SlotEmpty)
        );
    }

    #[test]
    fn unit_flags_and_job_points_preserve_each_stored_bit_and_zero() {
        let mut bytes = synthetic_payload();
        occupy(&mut bytes, 26);
        set_unit(&mut bytes, 26, 0, 0x80, 0, 0x4e, 24);
        let start = slot_start(26) + BATTLE_OFFSET;
        bytes[start + 0x32] = 0x84;
        bytes[start + 0x80..start + 0x82].copy_from_slice(&218_u16.to_le_bytes());
        let before = bytes.clone();
        let decoded = DecodedContainer {
            payload: bytes.into_boxed_slice(),
            stored_adler_status: StoredAdlerStatus::Matched,
        };
        let units = decoded
            .unit_records(26)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("occupied slot"));
        assert_eq!(units[0].ability_flags[0][0], 0x84);
        assert_eq!(units[0].job_points[0], 218);
        assert_eq!(units[0].job_points[1], 0);
        assert_eq!(decoded.payload(), before);
    }

    #[test]
    fn occupied_slot_can_have_an_empty_first_unit_without_inventing_values() {
        let mut bytes = synthetic_payload();
        occupy(&mut bytes, 0);
        let decoded = DecodedContainer {
            payload: bytes.into_boxed_slice(),
            stored_adler_status: StoredAdlerStatus::Matched,
        };
        let units = decoded
            .unit_records(0)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("occupied slot"));
        assert!(units[0].is_empty());
        assert!(!units[0].is_active(0));
        let normalized = decoded
            .normalized_manual_save(
                SnapshotByteLength::new(512).unwrap_or_else(|error| panic!("{error}")),
                0,
            )
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(matches!(
            normalized.selected_manual_slot,
            ValueState::Known(_)
        ));
    }

    #[test]
    fn all_job_level_nibbles_keep_zero_high_values_and_neighbors_distinct() {
        let mut bytes = synthetic_payload();
        occupy(&mut bytes, 26);
        set_unit(&mut bytes, 26, 0, 0x80, 0, 0x4e, 24);
        let start = slot_start(26) + BATTLE_OFFSET + 0x74;
        bytes[start] = 0x6f;
        bytes[start + 1] = 0x34;
        bytes[start + 2] = 0x07;
        bytes[start + 11] = 0x90;
        let before = bytes.clone();
        let decoded = DecodedContainer {
            payload: bytes.into_boxed_slice(),
            stored_adler_status: StoredAdlerStatus::Matched,
        };
        let units = decoded
            .unit_records(26)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("occupied slot"));
        assert_eq!(&units[0].job_levels[..6], &[6, 15, 3, 4, 0, 7]);
        assert_eq!(&units[0].job_levels[22..], &[9, 0]);
        assert_eq!(decoded.payload(), before);
    }

    #[test]
    fn normalized_result_keeps_unknown_build_and_integrity_metadata() {
        let mut bytes = synthetic_payload();
        occupy(&mut bytes, 0);
        set_unit(&mut bytes, 0, 0, 0x80, 0, 9, 12);
        let decoded = DecodedContainer {
            payload: bytes.into_boxed_slice(),
            stored_adler_status: StoredAdlerStatus::Mismatched,
        };
        let payload_before = decoded.payload().to_vec();
        let snapshot_length =
            SnapshotByteLength::new(512).unwrap_or_else(|error| panic!("{error}"));
        let normalized = decoded
            .normalized_manual_save(snapshot_length, 0)
            .unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(normalized.provenance.writer_build, ValueState::Unknown);
        let ValueState::Known(metadata) = normalized.container else {
            panic!("supported fixture did not return container metadata");
        };
        assert_eq!(metadata.stored_adler_status, StoredAdlerStatus::Mismatched);
        let ValueState::Known(slot) = normalized.selected_manual_slot else {
            panic!("occupied fixture did not return a selected slot");
        };
        assert_eq!(slot.id.index(), 0);
        let units = decoded
            .unit_records(0)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("occupied slot"));
        assert_eq!(units[0].level, 12);
        assert_eq!(decoded.payload(), payload_before);
    }
    #[test]
    fn identity_coordinates_cover_active_humans_monsters_and_guests_only() {
        let mut bytes = synthetic_payload();
        occupy(&mut bytes, 0);
        for (position, character, job) in [
            (0, 1, 0),
            (1, 0x80, 74),
            (2, 0x82, 94),
            (50, 4, 4),
            (53, 30, 30),
        ] {
            let index = u8::try_from(position).unwrap_or(u8::MAX);
            set_unit(&mut bytes, 0, position, character, index, job, 1);
            assert!(identity_unit_payload_offset(&bytes, 0, index).is_ok());
        }
        assert!(identity_unit_payload_offset(&bytes, 0, 3).is_err());
        assert!(identity_unit_payload_offset(&bytes, 0, 54).is_err());
        assert!(identity_unit_payload_offset(&bytes, 1, 0).is_err());
    }
}
