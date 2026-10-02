use crate::ContainerError;
use std::ops::Range;

const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_CHUNKS: usize = 4_096;

pub(crate) fn extract_single_ffto(input: &[u8]) -> Result<&[u8], ContainerError> {
    let range = single_ffto_range(input)?;
    input.get(range).ok_or(ContainerError::PngChunkTruncated)
}

pub(crate) fn single_ffto_range(input: &[u8]) -> Result<Range<usize>, ContainerError> {
    if input.get(..SIGNATURE.len()) != Some(SIGNATURE) {
        return Err(ContainerError::PngSignature);
    }

    let mut position = SIGNATURE.len();
    let mut chunk_count = 0_usize;
    let mut seen_idat = false;
    let mut ended_idat = false;
    let mut seen_palette = false;
    let mut ffto: Option<Range<usize>> = None;

    while position < input.len() {
        if chunk_count == MAX_CHUNKS {
            return Err(ContainerError::PngChunkLimit);
        }
        let header_end = position
            .checked_add(8)
            .ok_or(ContainerError::PngChunkLength)?;
        let header = input
            .get(position..header_end)
            .ok_or(ContainerError::PngChunkTruncated)?;
        let length = usize::try_from(u32::from_be_bytes(
            header[0..4]
                .try_into()
                .map_err(|_| ContainerError::PngChunkTruncated)?,
        ))
        .map_err(|_| ContainerError::PngChunkLength)?;
        if length > i32::MAX as usize {
            return Err(ContainerError::PngChunkLength);
        }
        let data_start = header_end;
        let data_end = data_start
            .checked_add(length)
            .ok_or(ContainerError::PngChunkLength)?;
        let chunk_end = data_end
            .checked_add(4)
            .ok_or(ContainerError::PngChunkLength)?;
        let data = input
            .get(data_start..data_end)
            .ok_or(ContainerError::PngChunkTruncated)?;
        let expected_crc = u32::from_be_bytes(
            input
                .get(data_end..chunk_end)
                .ok_or(ContainerError::PngChunkTruncated)?
                .try_into()
                .map_err(|_| ContainerError::PngChunkTruncated)?,
        );
        let kind: [u8; 4] = header[4..8]
            .try_into()
            .map_err(|_| ContainerError::PngChunkTruncated)?;
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            return Err(ContainerError::PngChunkType);
        }
        if !kind[2].is_ascii_uppercase() {
            return Err(ContainerError::PngReservedBit);
        }
        if crc32_parts(&kind, data) != expected_crc {
            return Err(ContainerError::PngChunkChecksum);
        }

        match &kind {
            b"IHDR" => {
                if chunk_count != 0 || length != 13 {
                    return Err(ContainerError::PngHeader);
                }
            }
            b"PLTE" => {
                if chunk_count == 0 || seen_palette || seen_idat || length == 0 || length % 3 != 0 {
                    return Err(ContainerError::PngPalette);
                }
                seen_palette = true;
            }
            b"IDAT" => {
                if chunk_count == 0 || ended_idat {
                    return Err(ContainerError::PngImageDataOrder);
                }
                seen_idat = true;
            }
            b"IEND" => {
                if length != 0 || !seen_idat {
                    return Err(ContainerError::PngEnd);
                }
                if chunk_end != input.len() {
                    return Err(ContainerError::PngTrailing);
                }
                return match ffto {
                    Some(range) if range.is_empty() => Err(ContainerError::PngFftoEmpty),
                    Some(range) => Ok(range),
                    None => Err(ContainerError::PngFftoCount),
                };
            }
            b"ffTo" => {
                if ffto.replace(data_start..data_end).is_some() {
                    return Err(ContainerError::PngFftoCount);
                }
            }
            _ if kind[0].is_ascii_uppercase() => {
                return Err(ContainerError::PngUnknownCritical);
            }
            _ => {}
        }

        if seen_idat && kind != *b"IDAT" {
            ended_idat = true;
        }
        chunk_count = chunk_count
            .checked_add(1)
            .ok_or(ContainerError::PngChunkLimit)?;
        position = chunk_end;
    }

    Err(ContainerError::PngEnd)
}

pub(crate) fn crc32(input: &[u8]) -> u32 {
    crc32_parts(&[], input)
}

pub(crate) fn crc32_parts(first: &[u8], second: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in first.iter().chain(second) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::{crc32_parts, extract_single_ffto, SIGNATURE};
    use crate::ContainerError;

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        result.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
        result.extend_from_slice(kind);
        result.extend_from_slice(data);
        result.extend_from_slice(&crc32_parts(kind, data).to_be_bytes());
        result
    }

    fn envelope(ffto: &[u8]) -> Vec<u8> {
        let mut result = SIGNATURE.to_vec();
        result.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]));
        result.extend(chunk(b"ffTo", ffto));
        result.extend(chunk(b"IDAT", &[0]));
        result.extend(chunk(b"IEND", &[]));
        result
    }

    #[test]
    fn extracts_one_crc_checked_ffto() {
        let input = envelope(b"synthetic container");
        assert_eq!(extract_single_ffto(&input), Ok(&b"synthetic container"[..]));
    }

    #[test]
    fn every_envelope_truncation_fails() {
        let input = envelope(b"synthetic container");
        for cut in 0..input.len() {
            assert!(extract_single_ffto(&input[..cut]).is_err());
        }
    }

    #[test]
    fn signatures_checksums_counts_order_and_tail_fail_closed() {
        let input = envelope(b"synthetic container");
        let mut wrong_signature = input.clone();
        wrong_signature[0] = 0;
        assert_eq!(
            extract_single_ffto(&wrong_signature),
            Err(ContainerError::PngSignature)
        );

        let mut corrupt = input.clone();
        let payload = input
            .windows(b"synthetic container".len())
            .position(|value| value == b"synthetic container")
            .unwrap_or(0);
        corrupt[payload] ^= 1;
        assert_eq!(
            extract_single_ffto(&corrupt),
            Err(ContainerError::PngChunkChecksum)
        );

        let duplicate_at = input
            .windows(4)
            .position(|value| value == b"IDAT")
            .and_then(|value| value.checked_sub(4))
            .unwrap_or(input.len());
        let mut duplicate = input[..duplicate_at].to_vec();
        duplicate.extend(chunk(b"ffTo", b"second"));
        duplicate.extend_from_slice(&input[duplicate_at..]);
        assert_eq!(
            extract_single_ffto(&duplicate),
            Err(ContainerError::PngFftoCount)
        );

        let mut trailing = input.clone();
        trailing.push(0);
        assert_eq!(
            extract_single_ffto(&trailing),
            Err(ContainerError::PngTrailing)
        );

        let mut bad_order = SIGNATURE.to_vec();
        bad_order.extend(chunk(b"IHDR", &[0; 13]));
        bad_order.extend(chunk(b"IDAT", &[0]));
        bad_order.extend(chunk(b"tEXt", b"end IDAT"));
        bad_order.extend(chunk(b"IDAT", &[0]));
        bad_order.extend(chunk(b"IEND", &[]));
        assert_eq!(
            extract_single_ffto(&bad_order),
            Err(ContainerError::PngImageDataOrder)
        );
    }

    #[test]
    fn chunk_limits_and_unknown_critical_chunks_fail_closed() {
        let mut oversized = SIGNATURE.to_vec();
        oversized.extend_from_slice(&0x8000_0000_u32.to_be_bytes());
        oversized.extend_from_slice(b"IHDR");
        assert_eq!(
            extract_single_ffto(&oversized),
            Err(ContainerError::PngChunkLength)
        );

        let mut unknown = SIGNATURE.to_vec();
        unknown.extend(chunk(b"IHDR", &[0; 13]));
        unknown.extend(chunk(b"ABCD", &[]));
        assert_eq!(
            extract_single_ffto(&unknown),
            Err(ContainerError::PngUnknownCritical)
        );

        let mut too_many = SIGNATURE.to_vec();
        too_many.extend(chunk(b"IHDR", &[0; 13]));
        for _ in 1..4_096 {
            too_many.extend(chunk(b"tEXt", &[]));
        }
        too_many.extend(chunk(b"IEND", &[]));
        assert_eq!(
            extract_single_ffto(&too_many),
            Err(ContainerError::PngChunkLimit)
        );
    }
}
