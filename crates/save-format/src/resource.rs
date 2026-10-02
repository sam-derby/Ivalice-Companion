use sha2::{Digest, Sha256};

use crate::ContainerError;

pub const PINNED_DICTIONARY_LENGTH: usize = 32 * 1024;
pub const PINNED_DICTIONARY_SHA256_HEX: &str =
    "09cb9606666b9a218c4f3b26a0dd814e2f4de253041630f85dcaaa542a7e4ceb";
pub const PINNED_DICTIONARY_SHA256: [u8; 32] = [
    0x09, 0xcb, 0x96, 0x06, 0x66, 0x6b, 0x9a, 0x21, 0x8c, 0x4f, 0x3b, 0x26, 0xa0, 0xdd, 0x81, 0x4e,
    0x2f, 0x4d, 0xe2, 0x53, 0x04, 0x16, 0x30, 0xf8, 0x5d, 0xca, 0xaa, 0x54, 0x2a, 0x7e, 0x4c, 0xeb,
];

pub(crate) fn validate_dictionary(dictionary: Option<&[u8]>) -> Result<&[u8], ContainerError> {
    let dictionary = dictionary.ok_or(ContainerError::ResourceMissing)?;
    if dictionary.len() != PINNED_DICTIONARY_LENGTH {
        return Err(ContainerError::ResourceLength);
    }
    if Sha256::digest(dictionary).as_slice() != PINNED_DICTIONARY_SHA256 {
        return Err(ContainerError::ResourceMismatch);
    }
    Ok(dictionary)
}

#[cfg(test)]
mod tests {
    use super::{validate_dictionary, PINNED_DICTIONARY_LENGTH};
    use crate::ContainerError;

    #[test]
    fn missing_short_and_wrong_resources_are_distinct() {
        assert_eq!(
            validate_dictionary(None),
            Err(ContainerError::ResourceMissing)
        );
        assert_eq!(
            validate_dictionary(Some(&[])),
            Err(ContainerError::ResourceLength)
        );
        let wrong = vec![0; PINNED_DICTIONARY_LENGTH];
        assert_eq!(
            validate_dictionary(Some(&wrong)),
            Err(ContainerError::ResourceMismatch)
        );
    }
}
