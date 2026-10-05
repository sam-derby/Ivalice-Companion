//! Whole-slot operations on the 50 manual slots of one save container.
//! An empty slot is all zero bytes, as in game-written files.

use crate::edit::{encode_payload, GilEditError};
use crate::manual::{
    record_is_occupied, slot_metadata::SlotMetadata, slot_payload_range, SLOT_COUNT,
    SLOT_RECORD_SIZE,
};
use crate::{png, StoredAdlerStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SlotOperation {
    /// Copy an occupied slot; an occupied target needs `replace`.
    Copy {
        from: u8,
        to: u8,
        replace: bool,
    },
    /// Move an occupied slot into an empty slot.
    Move {
        from: u8,
        to: u8,
    },
    /// Exchange two occupied slots.
    Swap {
        first: u8,
        second: u8,
    },
    Delete {
        slot: u8,
    },
    /// Place a validated slot record; an occupied target needs `replace`.
    Insert {
        slot: u8,
        record: Vec<u8>,
        replace: bool,
    },
}

/// Check that `record` is one complete occupied slot this crate can read.
pub fn validate_slot_record(record: &[u8]) -> Result<SlotMetadata, GilEditError> {
    if record.len() != SLOT_RECORD_SIZE || !record_is_occupied(record)? {
        return Err(GilEditError::InvalidField);
    }
    Ok(SlotMetadata::parse(record)?)
}

impl crate::DecodedContainer {
    /// The raw bytes of an occupied slot, or `None` when it is empty.
    pub fn slot_record(&self, slot: u8) -> Result<Option<Vec<u8>>, GilEditError> {
        let range = slot_payload_range(self.payload(), slot)?;
        let record = &self.payload()[range];
        Ok(record_is_occupied(record)?.then(|| record.to_vec()))
    }
}

fn decoded(input: &[u8], dictionary: &[u8]) -> Result<crate::DecodedContainer, GilEditError> {
    let decoded = crate::decode_enhanced_png(input, Some(dictionary))?;
    if decoded.stored_adler_status() != StoredAdlerStatus::Matched {
        return Err(GilEditError::StoredChecksum);
    }
    Ok(decoded)
}

fn occupied(payload: &[u8], slot: u8) -> Result<bool, GilEditError> {
    Ok(record_is_occupied(
        &payload[slot_payload_range(payload, slot)?],
    )?)
}

fn write_slot(payload: &mut [u8], slot: u8, record: &[u8]) -> Result<(), GilEditError> {
    let range = slot_payload_range(payload, slot)?;
    payload
        .get_mut(range)
        .ok_or(GilEditError::Encoding)?
        .copy_from_slice(record);
    Ok(())
}

fn finish(
    input: &[u8],
    dictionary: &[u8],
    original: &[u8],
    mut payload: Vec<u8>,
    touched: &[u8],
) -> Result<Vec<u8>, GilEditError> {
    let checksum = png::crc32(&payload[0x10..]);
    payload[4..8].copy_from_slice(&checksum.to_le_bytes());
    let (output, check) = encode_payload(input, dictionary, &payload)?;
    let allowed = touched
        .iter()
        .map(|slot| slot_payload_range(original, *slot))
        .collect::<Result<Vec<_>, _>>()?;
    // Only the checksum word and the touched slots may differ from the input.
    if original
        .iter()
        .zip(check.payload())
        .enumerate()
        .any(|(index, (before, after))| {
            before != after
                && !(4..8).contains(&index)
                && !allowed.iter().any(|range| range.contains(&index))
        })
    {
        return Err(GilEditError::Encoding);
    }
    Ok(output)
}

/// Apply one slot operation and return the re-encoded save.
pub fn apply_slot_operation(
    input: &[u8],
    dictionary: &[u8],
    operation: &SlotOperation,
) -> Result<Vec<u8>, GilEditError> {
    let decoded = decoded(input, dictionary)?;
    let original = decoded.payload();
    let mut payload = original.to_vec();
    let record = |slot: u8| -> Result<Vec<u8>, GilEditError> {
        decoded.slot_record(slot)?.ok_or(GilEditError::InvalidField)
    };
    let touched = match operation {
        SlotOperation::Copy { from, to, replace } => {
            if from == to || (occupied(original, *to)? && !replace) {
                return Err(GilEditError::InvalidField);
            }
            write_slot(&mut payload, *to, &record(*from)?)?;
            vec![*to]
        }
        SlotOperation::Move { from, to } => {
            if from == to || occupied(original, *to)? {
                return Err(GilEditError::InvalidField);
            }
            write_slot(&mut payload, *to, &record(*from)?)?;
            write_slot(&mut payload, *from, &[0; SLOT_RECORD_SIZE])?;
            vec![*from, *to]
        }
        SlotOperation::Swap { first, second } => {
            if first == second {
                return Err(GilEditError::InvalidField);
            }
            let (a, b) = (record(*first)?, record(*second)?);
            write_slot(&mut payload, *first, &b)?;
            write_slot(&mut payload, *second, &a)?;
            vec![*first, *second]
        }
        SlotOperation::Delete { slot } => {
            record(*slot)?;
            write_slot(&mut payload, *slot, &[0; SLOT_RECORD_SIZE])?;
            vec![*slot]
        }
        SlotOperation::Insert {
            slot,
            record,
            replace,
        } => {
            validate_slot_record(record)?;
            if occupied(original, *slot)? && !replace {
                return Err(GilEditError::InvalidField);
            }
            write_slot(&mut payload, *slot, record)?;
            vec![*slot]
        }
    };
    finish(input, dictionary, original, payload, &touched)
}

/// A full save holding only `slot`, at the same position, with every other slot empty.
pub fn export_slot(input: &[u8], dictionary: &[u8], slot: u8) -> Result<Vec<u8>, GilEditError> {
    let decoded = decoded(input, dictionary)?;
    let record = decoded
        .slot_record(slot)?
        .ok_or(GilEditError::InvalidField)?;
    let original = decoded.payload();
    let mut payload = original.to_vec();
    let others = (0..SLOT_COUNT)
        .filter(|other| *other != slot)
        .collect::<Vec<_>>();
    for other in &others {
        write_slot(&mut payload, *other, &[0; SLOT_RECORD_SIZE])?;
    }
    write_slot(&mut payload, slot, &record)?;
    finish(input, dictionary, original, payload, &others)
}
