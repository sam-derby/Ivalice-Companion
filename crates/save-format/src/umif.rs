use flate2::{Decompress, FlushDecompress, Status};

use crate::png::crc32;
use crate::{
    ContainerError, StoredAdlerStatus, MAX_CONTAINER_BYTES, MAX_DECODED_BYTES,
    SUPPORTED_FORMAT_DISCRIMINATOR, SUPPORTED_PAYLOAD_LENGTH, SUPPORTED_PAYLOAD_VERSION,
};

const MAIN_HEADER_SIZE: usize = 0x10;
const FILE_ENTRY_SIZE: usize = 0x20;
const TABLE_SIZE: usize = MAIN_HEADER_SIZE + FILE_ENTRY_SIZE;
const UMIF_MAGIC: u32 = 0x4649_4d55;
const XOR_KEY: u64 = 0x0f3f_80fe_5f1f_c4f3;
const EXPECTED_NAME: &[u8; 12] = b"fftsave.bin\0";

pub(crate) fn decode_supported_umif(
    input: &[u8],
    dictionary: &[u8],
) -> Result<(Vec<u8>, StoredAdlerStatus), ContainerError> {
    if input.len() > MAX_CONTAINER_BYTES {
        return Err(ContainerError::InputLimit);
    }
    if input.len() < TABLE_SIZE {
        return Err(ContainerError::UmifHeaderTruncated);
    }
    if read_u32(input, 0)? != u32::try_from(TABLE_SIZE).map_err(|_| ContainerError::UmifRange)? {
        return Err(ContainerError::UmifHeaderSize);
    }
    if read_u32(input, 4)? != 0 {
        return Err(ContainerError::UmifReservedWord);
    }
    if read_u32(input, 8)? != UMIF_MAGIC {
        return Err(ContainerError::UmifMagic);
    }
    if read_u32(input, 12)? != 1 {
        return Err(ContainerError::UmifFileCount);
    }

    let name_length =
        usize::try_from(read_u32(input, 0x10)?).map_err(|_| ContainerError::UmifRange)?;
    if name_length != EXPECTED_NAME.len() {
        return Err(ContainerError::UmifNameLength);
    }
    let stored_length =
        usize::try_from(read_u32(input, 0x14)?).map_err(|_| ContainerError::UmifRange)?;
    if stored_length < 8 {
        return Err(ContainerError::UmifRange);
    }
    let name_offset = read_nonnegative_usize(input, 0x18)?;
    let decoded_length = read_nonnegative_usize(input, 0x20)?;
    let data_offset = read_nonnegative_usize(input, 0x28)?;

    if decoded_length > MAX_DECODED_BYTES {
        return Err(ContainerError::OutputLimit);
    }
    if decoded_length != SUPPORTED_PAYLOAD_LENGTH {
        return Err(ContainerError::UnsupportedLength);
    }
    if name_offset != TABLE_SIZE {
        return Err(ContainerError::UmifNameOffset);
    }
    let name_end = name_offset
        .checked_add(name_length)
        .ok_or(ContainerError::UmifRange)?;
    let expected_data_offset = align_up(name_end, 4).ok_or(ContainerError::UmifRange)?;
    if data_offset != expected_data_offset {
        return Err(ContainerError::UmifDataOffset);
    }
    let data_end = data_offset
        .checked_add(stored_length)
        .ok_or(ContainerError::UmifRange)?;
    let expected_end = align_up(data_end, 4).ok_or(ContainerError::UmifRange)?;
    if expected_end != input.len() {
        return Err(ContainerError::UmifDataEnd);
    }

    let encrypted_name = input
        .get(name_offset..name_end)
        .ok_or(ContainerError::UmifRange)?;
    let mut filename = [0_u8; EXPECTED_NAME.len()];
    filename.copy_from_slice(encrypted_name);
    crypt(&mut filename);
    if filename != *EXPECTED_NAME {
        return Err(ContainerError::UmifFilename);
    }
    if nonzero(input.get(name_end..data_offset)) || nonzero(input.get(data_end..)) {
        return Err(ContainerError::UmifPadding);
    }

    let encrypted_stored = input
        .get(data_offset..data_end)
        .ok_or(ContainerError::UmifRange)?;
    let mut stored = Vec::new();
    stored
        .try_reserve_exact(stored_length)
        .map_err(|_| ContainerError::OutputLimit)?;
    stored.extend_from_slice(encrypted_stored);
    crypt(&mut stored);

    let dictionary_id = read_be_u32(&stored, 0)?;
    if dictionary_id != adler32(dictionary) {
        return Err(ContainerError::DictionaryId);
    }
    let deflate_end = stored
        .len()
        .checked_sub(4)
        .ok_or(ContainerError::UmifRange)?;
    let deflate = stored
        .get(4..deflate_end)
        .ok_or(ContainerError::UmifRange)?;
    let expected_adler = read_be_u32(&stored, deflate_end)?;
    let payload = decode_raw_stream(deflate, dictionary, decoded_length)?;
    let stored_adler_status = if adler32(&payload) == expected_adler {
        StoredAdlerStatus::Matched
    } else {
        StoredAdlerStatus::Mismatched
    };
    if payload.len() < 0x10 {
        return Err(ContainerError::PayloadTruncated);
    }
    if read_u32(&payload, 4)? != crc32(&payload[0x10..]) {
        return Err(ContainerError::PayloadChecksum);
    }

    let payload_version = read_u32(&payload, 0)?;
    let discriminator = read_u64(&payload, 8)?;
    if payload_version != SUPPORTED_PAYLOAD_VERSION {
        return Err(ContainerError::UnsupportedVersion);
    }
    if discriminator != SUPPORTED_FORMAT_DISCRIMINATOR {
        return Err(ContainerError::UnsupportedDiscriminator);
    }
    Ok((payload, stored_adler_status))
}

fn decode_raw_stream(
    input: &[u8],
    dictionary: &[u8],
    declared_length: usize,
) -> Result<Vec<u8>, ContainerError> {
    let capacity = declared_length
        .checked_add(1)
        .ok_or(ContainerError::OutputLimit)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| ContainerError::OutputLimit)?;
    output.resize(capacity, 0);

    let mut decoder = Decompress::new(false);
    decoder
        .set_dictionary(dictionary)
        .map_err(|_| ContainerError::CodecDictionary)?;
    loop {
        let consumed =
            usize::try_from(decoder.total_in()).map_err(|_| ContainerError::UmifRange)?;
        let produced =
            usize::try_from(decoder.total_out()).map_err(|_| ContainerError::UmifRange)?;
        if produced > declared_length {
            return Err(ContainerError::OutputLimit);
        }
        let remaining_input = input.get(consumed..).ok_or(ContainerError::Codec)?;
        let remaining_output = output
            .get_mut(produced..)
            .ok_or(ContainerError::OutputLimit)?;
        let flush = if remaining_input.is_empty() {
            FlushDecompress::Finish
        } else {
            FlushDecompress::None
        };
        let status = decoder
            .decompress(remaining_input, remaining_output, flush)
            .map_err(|_| ContainerError::Codec)?;
        let next_consumed =
            usize::try_from(decoder.total_in()).map_err(|_| ContainerError::UmifRange)?;
        let next_produced =
            usize::try_from(decoder.total_out()).map_err(|_| ContainerError::UmifRange)?;
        if next_produced > declared_length {
            return Err(ContainerError::OutputLimit);
        }
        if status == Status::StreamEnd {
            if next_consumed != input.len() {
                return Err(ContainerError::StreamTrailing);
            }
            if next_produced != declared_length {
                return Err(ContainerError::DecodedLength);
            }
            output.truncate(next_produced);
            return Ok(output);
        }
        if next_consumed == consumed && next_produced == produced {
            return Err(ContainerError::StreamIncomplete);
        }
    }
}

pub(crate) fn crypt(data: &mut [u8]) {
    // TICSaveEditor.Core/Save/UmifContainer.cs Crypt at 07ea857 restarts the key for each tail word.
    let key = XOR_KEY.to_le_bytes();
    let mut offset = 0;
    while data.len() - offset >= 8 {
        for index in 0..8 {
            data[offset + index] ^= key[index];
        }
        offset += 8;
    }
    for width in [4, 2, 1] {
        if data.len() - offset >= width {
            for index in 0..width {
                data[offset + index] ^= key[index];
            }
            offset += width;
        }
    }
}

fn adler32(input: &[u8]) -> u32 {
    const MODULUS: u32 = 65_521;
    let mut low = 1_u32;
    let mut high = 0_u32;
    for byte in input {
        low = (low + u32::from(*byte)) % MODULUS;
        high = (high + low) % MODULUS;
    }
    (high << 16) | low
}

fn read_u32(input: &[u8], offset: usize) -> Result<u32, ContainerError> {
    let end = offset.checked_add(4).ok_or(ContainerError::UmifRange)?;
    Ok(u32::from_le_bytes(
        input
            .get(offset..end)
            .ok_or(ContainerError::UmifHeaderTruncated)?
            .try_into()
            .map_err(|_| ContainerError::UmifHeaderTruncated)?,
    ))
}

fn read_be_u32(input: &[u8], offset: usize) -> Result<u32, ContainerError> {
    let end = offset.checked_add(4).ok_or(ContainerError::UmifRange)?;
    Ok(u32::from_be_bytes(
        input
            .get(offset..end)
            .ok_or(ContainerError::UmifRange)?
            .try_into()
            .map_err(|_| ContainerError::UmifRange)?,
    ))
}

fn read_u64(input: &[u8], offset: usize) -> Result<u64, ContainerError> {
    let end = offset.checked_add(8).ok_or(ContainerError::UmifRange)?;
    Ok(u64::from_le_bytes(
        input
            .get(offset..end)
            .ok_or(ContainerError::UmifHeaderTruncated)?
            .try_into()
            .map_err(|_| ContainerError::UmifHeaderTruncated)?,
    ))
}

fn read_nonnegative_usize(input: &[u8], offset: usize) -> Result<usize, ContainerError> {
    let signed = i64::from_le_bytes(read_u64(input, offset)?.to_le_bytes());
    if signed < 0 {
        return Err(ContainerError::UmifNegativeValue);
    }
    usize::try_from(signed).map_err(|_| ContainerError::UmifRange)
}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value
        .checked_add(alignment.checked_sub(1)?)
        .map(|sum| sum & !(alignment - 1))
}

fn nonzero(value: Option<&[u8]>) -> bool {
    value.is_none_or(|bytes| bytes.iter().any(|byte| *byte != 0))
}

#[cfg(test)]
mod tests {
    use flate2::{Compress, Compression, FlushCompress, Status};

    use super::{adler32, crypt, decode_supported_umif};
    use crate::png::crc32;
    use crate::{
        ContainerError, StoredAdlerStatus, MAX_DECODED_BYTES, SUPPORTED_FORMAT_DISCRIMINATOR,
        SUPPORTED_PAYLOAD_LENGTH, SUPPORTED_PAYLOAD_VERSION,
    };

    fn dictionary(seed: u8) -> Vec<u8> {
        (0..32 * 1024)
            .map(|index| (index % 251) as u8 ^ seed)
            .collect()
    }

    fn payload(version: u32, discriminator: u64, length: usize) -> Vec<u8> {
        let mut result: Vec<u8> = (0..length).map(|index| (index % 239) as u8).collect();
        if length >= 16 {
            result[0..4].copy_from_slice(&version.to_le_bytes());
            result[8..16].copy_from_slice(&discriminator.to_le_bytes());
            let checksum = crc32(&result[0x10..]);
            result[4..8].copy_from_slice(&checksum.to_le_bytes());
        }
        result
    }

    fn stored_stream(input: &[u8], dictionary: &[u8]) -> Vec<u8> {
        let mut encoder = Compress::new(Compression::best(), true);
        assert!(encoder.set_dictionary(dictionary).is_ok());
        let capacity = input.len().saturating_add(65_536);
        let mut output = vec![0; capacity];
        assert!(matches!(
            encoder.compress(input, &mut output, FlushCompress::Finish),
            Ok(Status::StreamEnd)
        ));
        output.truncate(usize::try_from(encoder.total_out()).unwrap_or(0));
        assert!(output.len() >= 10);
        assert_eq!(
            u32::from_be_bytes(output[2..6].try_into().unwrap_or([0; 4])),
            adler32(dictionary)
        );
        output[2..].to_vec()
    }

    fn container_with_declared(input: &[u8], dictionary: &[u8], declared_length: i64) -> Vec<u8> {
        let stored = stored_stream(input, dictionary);
        container_from_stored(stored, declared_length)
    }

    fn stale_stored_stream(input: &[u8], dictionary: &[u8]) -> Vec<u8> {
        let mut stored = stored_stream(input, dictionary);
        let trailer_end = stored.len();
        stored[trailer_end - 1] ^= 1;
        stored
    }

    fn container_from_stored(mut stored: Vec<u8>, declared_length: i64) -> Vec<u8> {
        crypt(&mut stored);
        let mut name = *b"fftsave.bin\0";
        crypt(&mut name);
        let data_offset = 0x3c_usize;
        let data_end = data_offset.saturating_add(stored.len());
        let total = data_end.saturating_add(3) & !3;
        let mut result = vec![0; total];
        result[0..4].copy_from_slice(&0x30_u32.to_le_bytes());
        result[8..12].copy_from_slice(&0x4649_4d55_u32.to_le_bytes());
        result[12..16].copy_from_slice(&1_u32.to_le_bytes());
        result[0x10..0x14].copy_from_slice(&12_u32.to_le_bytes());
        result[0x14..0x18].copy_from_slice(
            &u32::try_from(stored.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        result[0x18..0x20].copy_from_slice(&0x30_i64.to_le_bytes());
        result[0x20..0x28].copy_from_slice(&declared_length.to_le_bytes());
        result[0x28..0x30].copy_from_slice(&i64::try_from(data_offset).unwrap_or(0).to_le_bytes());
        result[0x30..0x3c].copy_from_slice(&name);
        result[data_offset..data_end].copy_from_slice(&stored);
        result
    }

    fn valid_payload() -> Vec<u8> {
        payload(
            SUPPORTED_PAYLOAD_VERSION,
            SUPPORTED_FORMAT_DISCRIMINATOR,
            SUPPORTED_PAYLOAD_LENGTH,
        )
    }

    #[test]
    fn crypt_restarts_key_at_each_partial_word() {
        let mut bytes = [0_u8; 7];
        crypt(&mut bytes);
        assert_eq!(bytes, [0xf3, 0xc4, 0x1f, 0x5f, 0xf3, 0xc4, 0xf3]);
        crypt(&mut bytes);
        assert_eq!(bytes, [0; 7]);
    }

    #[test]
    fn attributed_path_decodes_a_valid_synthetic_container() {
        let dictionary = dictionary(0x5a);
        let payload = valid_payload();
        let container = container_with_declared(
            &payload,
            &dictionary,
            i64::try_from(payload.len()).unwrap_or(-1),
        );
        assert_eq!(
            decode_supported_umif(&container, &dictionary),
            Ok((payload, StoredAdlerStatus::Matched))
        );
    }

    #[test]
    fn empty_truncated_and_noncanonical_layouts_fail_closed() {
        let dictionary = dictionary(0x11);
        let payload = valid_payload();
        let original = container_with_declared(
            &payload,
            &dictionary,
            i64::try_from(payload.len()).unwrap_or(-1),
        );
        for cut in [0, 1, 15, 16, 47, 48, 59, 60, original.len() - 1] {
            assert!(decode_supported_umif(&original[..cut], &dictionary).is_err());
        }

        for (offset, bytes, expected) in [
            (
                0,
                0x31_u32.to_le_bytes().to_vec(),
                ContainerError::UmifHeaderSize,
            ),
            (
                4,
                1_u32.to_le_bytes().to_vec(),
                ContainerError::UmifReservedWord,
            ),
            (8, 0_u32.to_le_bytes().to_vec(), ContainerError::UmifMagic),
            (
                12,
                2_u32.to_le_bytes().to_vec(),
                ContainerError::UmifFileCount,
            ),
            (
                0x10,
                11_u32.to_le_bytes().to_vec(),
                ContainerError::UmifNameLength,
            ),
            (
                0x18,
                (-1_i64).to_le_bytes().to_vec(),
                ContainerError::UmifNegativeValue,
            ),
            (
                0x20,
                (-1_i64).to_le_bytes().to_vec(),
                ContainerError::UmifNegativeValue,
            ),
            (
                0x28,
                (-1_i64).to_le_bytes().to_vec(),
                ContainerError::UmifNegativeValue,
            ),
            (
                0x28,
                i64::MAX.to_le_bytes().to_vec(),
                ContainerError::UmifDataOffset,
            ),
        ] {
            let mut changed = original.clone();
            changed[offset..offset + bytes.len()].copy_from_slice(&bytes);
            assert_eq!(decode_supported_umif(&changed, &dictionary), Err(expected));
        }

        let mut wrong_name = original.clone();
        wrong_name[0x30] ^= 1;
        assert_eq!(
            decode_supported_umif(&wrong_name, &dictionary),
            Err(ContainerError::UmifFilename)
        );

        let mut wrong_padding = original;
        let stored_length = usize::try_from(u32::from_le_bytes(
            wrong_padding[0x14..0x18].try_into().unwrap_or([0; 4]),
        ))
        .unwrap_or(0);
        let data_end = 0x3c + stored_length;
        if data_end < wrong_padding.len() {
            wrong_padding[data_end] = 1;
            assert_eq!(
                decode_supported_umif(&wrong_padding, &dictionary),
                Err(ContainerError::UmifPadding)
            );
        }
    }

    #[test]
    fn resource_stream_payload_and_support_failures_are_exact() {
        let wrong_dictionary = dictionary(0x23);
        let dictionary = dictionary(0x22);
        let valid = valid_payload();
        let original = container_with_declared(
            &valid,
            &dictionary,
            i64::try_from(valid.len()).unwrap_or(-1),
        );
        assert_eq!(
            decode_supported_umif(&original, &wrong_dictionary),
            Err(ContainerError::DictionaryId)
        );

        let stale = container_from_stored(
            stale_stored_stream(&valid, &dictionary),
            i64::try_from(SUPPORTED_PAYLOAD_LENGTH).unwrap_or(-1),
        );
        assert_eq!(
            decode_supported_umif(&stale, &dictionary),
            Ok((valid.clone(), StoredAdlerStatus::Mismatched))
        );

        let mut bad_payload_checksum = payload(
            SUPPORTED_PAYLOAD_VERSION,
            SUPPORTED_FORMAT_DISCRIMINATOR,
            SUPPORTED_PAYLOAD_LENGTH,
        );
        bad_payload_checksum[4] ^= 1;
        assert_eq!(
            decode_supported_umif(
                &container_from_stored(
                    stale_stored_stream(&bad_payload_checksum, &dictionary),
                    i64::try_from(SUPPORTED_PAYLOAD_LENGTH).unwrap_or(-1),
                ),
                &dictionary,
            ),
            Err(ContainerError::PayloadChecksum)
        );

        let mut corrupt_stream = stored_stream(&valid, &dictionary);
        corrupt_stream[4] ^= 0xff;
        assert_eq!(
            decode_supported_umif(
                &container_from_stored(
                    corrupt_stream,
                    i64::try_from(SUPPORTED_PAYLOAD_LENGTH).unwrap_or(-1),
                ),
                &dictionary,
            ),
            Err(ContainerError::Codec)
        );

        for (version, discriminator, expected) in [
            (
                SUPPORTED_PAYLOAD_VERSION + 1,
                SUPPORTED_FORMAT_DISCRIMINATOR,
                ContainerError::UnsupportedVersion,
            ),
            (
                SUPPORTED_PAYLOAD_VERSION,
                SUPPORTED_FORMAT_DISCRIMINATOR + 1,
                ContainerError::UnsupportedDiscriminator,
            ),
        ] {
            let changed = payload(version, discriminator, SUPPORTED_PAYLOAD_LENGTH);
            assert_eq!(
                decode_supported_umif(
                    &container_with_declared(
                        &changed,
                        &dictionary,
                        i64::try_from(SUPPORTED_PAYLOAD_LENGTH).unwrap_or(-1),
                    ),
                    &dictionary,
                ),
                Err(expected)
            );
        }
    }

    #[test]
    fn declared_lengths_trailing_streams_and_output_bombs_are_rejected() {
        let dictionary = dictionary(0x33);
        let valid = valid_payload();
        assert_eq!(
            decode_supported_umif(
                &container_with_declared(
                    &valid,
                    &dictionary,
                    i64::try_from(SUPPORTED_PAYLOAD_LENGTH - 1).unwrap_or(-1),
                ),
                &dictionary,
            ),
            Err(ContainerError::UnsupportedLength)
        );
        assert_eq!(
            decode_supported_umif(
                &container_with_declared(
                    &valid,
                    &dictionary,
                    i64::try_from(MAX_DECODED_BYTES + 1).unwrap_or(-1),
                ),
                &dictionary,
            ),
            Err(ContainerError::OutputLimit)
        );

        let bomb = payload(
            SUPPORTED_PAYLOAD_VERSION,
            SUPPORTED_FORMAT_DISCRIMINATOR,
            SUPPORTED_PAYLOAD_LENGTH + 1,
        );
        assert_eq!(
            decode_supported_umif(
                &container_with_declared(
                    &bomb,
                    &dictionary,
                    i64::try_from(SUPPORTED_PAYLOAD_LENGTH).unwrap_or(-1),
                ),
                &dictionary,
            ),
            Err(ContainerError::OutputLimit)
        );

        let short = payload(
            SUPPORTED_PAYLOAD_VERSION,
            SUPPORTED_FORMAT_DISCRIMINATOR,
            SUPPORTED_PAYLOAD_LENGTH - 1,
        );
        assert_eq!(
            decode_supported_umif(
                &container_with_declared(
                    &short,
                    &dictionary,
                    i64::try_from(SUPPORTED_PAYLOAD_LENGTH).unwrap_or(-1),
                ),
                &dictionary,
            ),
            Err(ContainerError::DecodedLength)
        );

        let mut stored = stored_stream(&valid, &dictionary);
        let trailer = stored.split_off(stored.len() - 4);
        stored.push(0);
        stored.extend(trailer);
        assert_eq!(
            decode_supported_umif(
                &container_from_stored(
                    stored,
                    i64::try_from(SUPPORTED_PAYLOAD_LENGTH).unwrap_or(-1),
                ),
                &dictionary,
            ),
            Err(ContainerError::StreamTrailing)
        );

        let mut truncated = stored_stream(&valid, &dictionary);
        let deflate_tail = truncated.len() - 5;
        truncated.remove(deflate_tail);
        assert_eq!(
            decode_supported_umif(
                &container_from_stored(
                    truncated,
                    i64::try_from(SUPPORTED_PAYLOAD_LENGTH).unwrap_or(-1),
                ),
                &dictionary,
            ),
            Err(ContainerError::StreamIncomplete)
        );
    }

    #[test]
    fn checksum_vectors_match_the_container_standards() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }
}
