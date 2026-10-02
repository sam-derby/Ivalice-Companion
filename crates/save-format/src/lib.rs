//! Bounded save-container decoding and scoped manual-slot edits.
//!
//! The UMIF read path adapts TICSaveEditor behavior derived from FF16Tools.
//! See `NOTICE.md`. The public API accepts only immutable bytes and
//! exposes no filesystem operation. Edits return a new encoded container.

mod creature;
mod edit;
mod error;
mod manual;
mod png;
mod resource;
mod umif;

pub use creature::{creature_form, creature_forms, CreatureCategory, CreatureForm};
pub use edit::{
    edit_enhanced_png, edit_enhanced_png_with_abilities, edit_enhanced_png_with_jobs,
    edit_gil_enhanced_png, named_character_ids, named_character_sex,
    named_human_initialization_supported, EditOperation, EquippedSlot, GearSlot, GearSource,
    GenericCreationDonor, GilEditError, GuestAdditionDonor, NamedCreationBase, StoryAdditionDonor,
};
pub use error::{ContainerError, ErrorKind};
pub use ivalice_domain::StoredAdlerStatus;
pub use manual::inventory::BattleStores;
pub use manual::slot_metadata::{RawSlotField, SlotMetadata};
pub use manual::unit_image::{UnitImageError, UnitRecordImage};
pub use manual::unit_record::{CombatSetRecord, UnitRecord};
pub use manual::ManualParseError;
pub use resource::{
    PINNED_DICTIONARY_LENGTH, PINNED_DICTIONARY_SHA256, PINNED_DICTIONARY_SHA256_HEX,
};

/// Maximum accepted PNG/container input size.
pub const MAX_CONTAINER_BYTES: usize = 64 * 1024 * 1024;

/// Maximum decoded payload size, including the one-byte bomb sentinel.
pub const MAX_DECODED_BYTES: usize = 16 * 1024 * 1024;

/// The only embedded container structure reproduced by this project so far.
pub const SUPPORTED_PAYLOAD_VERSION: u32 = 16;
pub const SUPPORTED_FORMAT_DISCRIMINATOR: u64 = 14;
pub const SUPPORTED_PAYLOAD_LENGTH: usize = 2_008_216;

/// A decoded, integrity-checked payload for the exact supported structure.
///
/// Callers may inspect the payload; scoped edit APIs validate and repack changes.
#[derive(Debug, Eq, PartialEq)]
pub struct DecodedContainer {
    payload: Box<[u8]>,
    stored_adler_status: StoredAdlerStatus,
}

impl DecodedContainer {
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    #[must_use]
    pub const fn payload_version(&self) -> u32 {
        SUPPORTED_PAYLOAD_VERSION
    }

    #[must_use]
    pub const fn format_discriminator(&self) -> u64 {
        SUPPORTED_FORMAT_DISCRIMINATOR
    }

    #[must_use]
    pub const fn stored_adler_status(&self) -> StoredAdlerStatus {
        self.stored_adler_status
    }
}

/// Decode a complete Enhanced PNG container using the separately provisioned
/// pinned dictionary.
///
/// `dictionary` is explicit so this format crate remains free of paths and
/// filesystem access. A provisioning layer may load the pinned resource from
/// local storage; absence and identity mismatch remain distinct errors.
pub fn decode_enhanced_png(
    input: &[u8],
    dictionary: Option<&[u8]>,
) -> Result<DecodedContainer, ContainerError> {
    if input.is_empty() {
        return Err(ContainerError::InputEmpty);
    }
    if input.len() > MAX_CONTAINER_BYTES {
        return Err(ContainerError::InputLimit);
    }

    let ffto = png::extract_single_ffto(input)?;
    let dictionary = resource::validate_dictionary(dictionary)?;
    let (payload, stored_adler_status) = umif::decode_supported_umif(ffto, dictionary)?;
    Ok(DecodedContainer {
        payload: payload.into_boxed_slice(),
        stored_adler_status,
    })
}
