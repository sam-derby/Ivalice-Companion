//! Errand postings and the artefact and wonder collection from the game's
//! Profit and Collection tables, normalized by scripts/import-errands.py.
//! Their saved status (World Mouke* and *FindDay fields) is not yet decoded.

use serde::{Deserialize, Serialize};

const PROFIT_SHA256: &str = "ac6ea2a5b4d879304c002f78e03146cd6a0272be561a02f71de0b2a8edc6a595";
const COLLECTION_SHA256: &str = "48d0a9287d5aef6164aba419cb8af785c3649d72803fa176544c622ec9558c18";
const MAX_BYTES: usize = 256 * 1024;
const MAX_TEXT_CHARS: usize = 2048;
pub const ERRAND_COUNT: usize = 96;
pub const ARTEFACT_COUNT: usize = 27;
pub const WONDER_COUNT: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Errand {
    pub index: u8,
    pub title: String,
    pub client: String,
    pub posting: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionKind {
    Artefact,
    Wonder,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionEntry {
    pub index: u8,
    pub kind: CollectionKind,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrandsDocument {
    pub schema: String,
    pub profile: String,
    pub profit_sha256: String,
    pub collection_sha256: String,
    pub errands: Vec<Errand>,
    pub collection: Vec<CollectionEntry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrandsError {
    InvalidJson,
    TooLarge,
    SourceMismatch,
    InvalidTable,
}

#[derive(Clone, Debug)]
pub struct ValidatedErrands(ErrandsDocument);

fn text_ok(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= MAX_TEXT_CHARS
}

impl ValidatedErrands {
    pub fn from_json(bytes: &[u8]) -> Result<Self, ErrandsError> {
        if bytes.len() > MAX_BYTES {
            return Err(ErrandsError::TooLarge);
        }
        let document = serde_json::from_slice(bytes).map_err(|_| ErrandsError::InvalidJson)?;
        Self::validate(document)
    }

    pub fn validate(document: ErrandsDocument) -> Result<Self, ErrandsError> {
        if document.schema != "errands_v1"
            || document.profile != "english_steam_enhanced_manual"
            || document.profit_sha256 != PROFIT_SHA256
            || document.collection_sha256 != COLLECTION_SHA256
        {
            return Err(ErrandsError::SourceMismatch);
        }
        let errands_ok = document.errands.len() == ERRAND_COUNT
            && document
                .errands
                .iter()
                .enumerate()
                .all(|(position, errand)| {
                    usize::from(errand.index) == position
                        && text_ok(&errand.title)
                        && text_ok(&errand.client)
                        && text_ok(&errand.posting)
                });
        let count = |kind| {
            document
                .collection
                .iter()
                .filter(|entry| entry.kind == kind)
                .count()
        };
        let collection_ok = count(CollectionKind::Artefact) == ARTEFACT_COUNT
            && count(CollectionKind::Wonder) == WONDER_COUNT
            && document
                .collection
                .iter()
                .enumerate()
                .all(|(position, entry)| {
                    usize::from(entry.index) == position
                        && text_ok(&entry.name)
                        && text_ok(&entry.description)
                });
        if !errands_ok || !collection_ok {
            return Err(ErrandsError::InvalidTable);
        }
        Ok(Self(document))
    }

    pub fn errands(&self) -> &[Errand] {
        &self.0.errands
    }

    pub fn collection(&self) -> &[CollectionEntry] {
        &self.0.collection
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> ErrandsDocument {
        ErrandsDocument {
            schema: "errands_v1".into(),
            profile: "english_steam_enhanced_manual".into(),
            profit_sha256: PROFIT_SHA256.into(),
            collection_sha256: COLLECTION_SHA256.into(),
            errands: (0..ERRAND_COUNT)
                .map(|index| Errand {
                    index: u8::try_from(index).unwrap_or(u8::MAX),
                    title: format!("Errand {index}"),
                    client: "Client".into(),
                    posting: "Wanted.".into(),
                })
                .collect(),
            collection: (0..ARTEFACT_COUNT + WONDER_COUNT)
                .map(|index| CollectionEntry {
                    index: u8::try_from(index).unwrap_or(u8::MAX),
                    kind: if index < WONDER_COUNT {
                        CollectionKind::Wonder
                    } else {
                        CollectionKind::Artefact
                    },
                    name: format!("Find {index}"),
                    description: "Old.".into(),
                })
                .collect(),
        }
    }

    #[test]
    fn complete_ordered_tables_validate() {
        let valid =
            ValidatedErrands::validate(document()).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(valid.errands()[95].title, "Errand 95");
        assert_eq!(valid.collection()[0].kind, CollectionKind::Wonder);
    }

    #[test]
    fn changed_sources_and_tables_fail() {
        let mut source = document();
        source.profit_sha256 = "0".repeat(64);
        assert_eq!(
            ValidatedErrands::validate(source).err(),
            Some(ErrandsError::SourceMismatch)
        );
        let mut short = document();
        short.errands.pop();
        let mut unordered = document();
        unordered.collection.swap(0, 1);
        let mut kinds = document();
        kinds.collection[30].kind = CollectionKind::Wonder;
        let mut blank = document();
        blank.errands[4].posting = " ".into();
        for invalid in [short, unordered, kinds, blank] {
            assert_eq!(
                ValidatedErrands::validate(invalid).err(),
                Some(ErrandsError::InvalidTable)
            );
        }
        assert_eq!(
            ValidatedErrands::from_json(b"{}").err(),
            Some(ErrandsError::InvalidJson)
        );
    }
}
