use super::*;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn dictionary() -> std::io::Result<Vec<u8>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read(
        root.join(".local/research-inputs/resources/ticsaveeditor-07ea857/CompressDict.bin"),
    )
}
const FIRST: usize = 0x10;
const STRIDE: usize = 0x9ce4;
const UNITS: usize = 0x518;

fn unit_start(slot: usize, position: usize) -> usize {
    FIRST + slot * STRIDE + UNITS + position * manual::unit_record::SIZE
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = u32::try_from(data.len())?.to_be_bytes().to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(data);
    bytes.extend_from_slice(&png::crc32_parts(kind, data).to_be_bytes());
    Ok(bytes)
}

fn fixture(dictionary: &[u8]) -> TestResult<Vec<u8>> {
    let mut payload = vec![0; crate::SUPPORTED_PAYLOAD_LENGTH];
    payload[..4].copy_from_slice(&crate::SUPPORTED_PAYLOAD_VERSION.to_le_bytes());
    payload[8..16].copy_from_slice(&crate::SUPPORTED_FORMAT_DISCRIMINATOR.to_le_bytes());
    for slot in 0..2 {
        payload[FIRST + slot * STRIDE] = 1;
        for (position, character) in [(0, 2), (1, 0x81), (2, 22), (50, 23)] {
            let base = unit_start(slot, position);
            payload[base..base + manual::unit_record::SIZE].fill(0x5a);
            payload[base] = character;
            payload[base + 1] = u8::try_from(position)?;
            payload[base + 2] = 74;
            payload[base + 4] = if character == 0x81 { 0x40 } else { 0x80 };
            payload[base + manual::unit_record::IN_TRIP_OFFSET] = 1;
            payload[base + 0x230..base + 0x232]
                .copy_from_slice(&u16::from(character).to_le_bytes());
        }
        // An available generic donor has no unique name key.
        let base = unit_start(slot, 1);
        payload[base + 0x230..base + 0x232].fill(0);
    }
    // The target has no story or guest duplicate of the source donors.
    for position in [2, 50] {
        let base = unit_start(0, position);
        payload[base..base + manual::unit_record::SIZE].fill(0);
    }
    let checksum = png::crc32(&payload[FIRST..]);
    payload[4..8].copy_from_slice(&checksum.to_le_bytes());
    let mut header = vec![0; 0x3c];
    header[..4].copy_from_slice(&0x30_u32.to_le_bytes());
    header[8..12].copy_from_slice(&0x4649_4d55_u32.to_le_bytes());
    header[12..16].copy_from_slice(&1_u32.to_le_bytes());
    header[0x10..0x14].copy_from_slice(&12_u32.to_le_bytes());
    header[0x18..0x20].copy_from_slice(&0x30_i64.to_le_bytes());
    header[0x20..0x28].copy_from_slice(&i64::try_from(payload.len())?.to_le_bytes());
    header[0x28..0x30].copy_from_slice(&0x3c_i64.to_le_bytes());
    let mut name = *b"fftsave.bin\0";
    umif::crypt(&mut name);
    header[0x30..].copy_from_slice(&name);
    let umif = pack_umif(&header, &payload, dictionary)?;
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0])?);
    png.extend(chunk(b"ffTo", &umif)?);
    png.extend(chunk(b"IDAT", &[0])?);
    png.extend(chunk(b"IEND", &[])?);
    Ok(png)
}

#[test]
#[ignore = "requires private pinned dictionary; payload is synthetic"]
fn additions_never_inherit_errands_and_preserve_donors_and_other_slots() -> TestResult {
    let bytes = dictionary()?;
    let dictionary = bytes.as_slice();
    let input = fixture(dictionary)?;
    let before = crate::decode_enhanced_png(&input, Some(dictionary))?;
    let mut nickname = [0; 16];
    nickname[..4].copy_from_slice(b"Mira");
    for operation in [
        EditOperation::CreateGenericFromSlot {
            source_slot: 1,
            source_position: 1,
            unit_position: 3,
            nickname,
        },
        EditOperation::AddNamedFromUnit {
            source_slot: 1,
            source_position: 1,
            unit_position: 3,
            character: 45,
        },
        EditOperation::AddStoryFromSlot {
            source_slot: 1,
            source_position: 2,
            unit_position: 2,
        },
        EditOperation::AddGuestFromSlot {
            source_slot: 1,
            source_position: 50,
            unit_position: 2,
        },
    ] {
        let target = unit_position(&operation).ok_or("target")?;
        let output = edit_enhanced_png(&input, dictionary, 0, &[operation])?;
        let after = crate::decode_enhanced_png(&output, Some(dictionary))?;
        assert_eq!(after.unit_record_image(0, target)?.record().in_trip, 0);
        let range = unit_start(0, usize::from(target))..unit_start(0, usize::from(target)) + 600;
        for (index, (old, new)) in before.payload().iter().zip(after.payload()).enumerate() {
            assert!(
                old == new || (4..8).contains(&index) || range.contains(&index),
                "unrelated byte {index}"
            );
        }
        assert_eq!(after.unit_record_image(0, 1)?.record().in_trip, 1);
    }
    Ok(())
}

#[test]
#[ignore = "requires private pinned dictionary; payload is synthetic"]
fn removal_changes_only_membership_and_rejects_ramza_conflicts_and_invalid_targets() -> TestResult {
    let bytes = dictionary()?;
    let dictionary = bytes.as_slice();
    let input = fixture(dictionary)?;
    let before = crate::decode_enhanced_png(&input, Some(dictionary))?;
    let remove = EditOperation::RemoveUnit { unit_position: 1 };
    let output = edit_enhanced_png(&input, dictionary, 0, &[remove])?;
    let after = crate::decode_enhanced_png(&output, Some(dictionary))?;
    for (index, (old, new)) in before.payload().iter().zip(after.payload()).enumerate() {
        assert!(old == new || (4..8).contains(&index) || index == unit_start(0, 1) + 1);
    }
    assert_eq!(after.unit_record_image(0, 1)?.record().unit_index, 255);
    assert_eq!(
        edit_enhanced_png(&output, dictionary, 0, &[remove]),
        Err(GilEditError::InvalidField)
    );
    for position in [0, 2, 54, 255] {
        assert!(edit_enhanced_png(
            &input,
            dictionary,
            0,
            &[EditOperation::RemoveUnit {
                unit_position: position
            }]
        )
        .is_err());
    }
    assert_eq!(
        edit_enhanced_png(&input, dictionary, 0, &[remove, remove]),
        Err(GilEditError::DuplicateField)
    );
    for operations in [
        [
            remove,
            EditOperation::Faith {
                unit_position: 1,
                value: 50,
            },
        ],
        [
            EditOperation::Faith {
                unit_position: 1,
                value: 50,
            },
            remove,
        ],
    ] {
        assert!(edit_enhanced_png(&input, dictionary, 0, &operations).is_err());
    }
    let preview = preview_enhanced_png(&input, dictionary, 0, &[remove], None, None)?;
    assert_eq!(preview.payload(), after.payload());
    Ok(())
}

#[test]
fn all_ramza_forms_are_protected_even_outside_reserved_position() {
    for character in 1..=3 {
        let mut records = vec![[0; 600]; 54];
        records[1][0] = character;
        records[1][1] = 1;
        assert_eq!(
            roster::changes(&[EditOperation::RemoveUnit { unit_position: 1 }], &records),
            Err(GilEditError::InvalidField)
        );
    }
}

#[test]
fn donor_initialization_clears_only_membership_and_errand_metadata() {
    for flag in [0, 1, 255] {
        let mut donor = [0x5a; 600];
        donor[manual::unit_record::IN_TRIP_OFFSET] = flag;
        let added = roster::copy_for_addition(&donor, 7);
        assert_eq!(added[1], 7);
        assert_eq!(added[manual::unit_record::IN_TRIP_OFFSET], 0);
        for (index, (old, new)) in donor.iter().zip(added).enumerate() {
            assert!(index == 1 || index == manual::unit_record::IN_TRIP_OFFSET || *old == new);
        }
        assert_eq!(donor[manual::unit_record::IN_TRIP_OFFSET], flag);
    }
}

#[test]
fn removal_preserves_all_other_bytes_for_humans_creatures_and_guests() -> TestResult {
    for (position, character) in [(1, 0x80), (2, 45), (3, 130), (50, 23), (53, 52)] {
        let mut records = vec![[0x5a; 600]; 54];
        records[position][0] = character;
        records[position][1] = u8::try_from(position)?;
        records[position][manual::unit_record::IN_TRIP_OFFSET] = 1;
        let operation = EditOperation::RemoveUnit {
            unit_position: u8::try_from(position)?,
        };
        let mut expected = records[position];
        expected[1] = 255;
        assert_eq!(
            roster::changes(&[operation], &records),
            Ok(vec![(position, expected)])
        );
        assert_eq!(
            roster::changes(&[operation, operation], &records),
            Err(GilEditError::DuplicateField)
        );
        records[position] = expected;
        assert_eq!(
            roster::changes(&[operation], &records),
            Err(GilEditError::InvalidField)
        );
    }
    Ok(())
}
