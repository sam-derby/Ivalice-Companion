//! Fixed offline reader catalogue intake with a validated bundled read fallback.

use std::path::PathBuf;

use ivalice_domain::{
    game_data::{
        reader_catalogue::{
            ReaderCatalogueError, ReaderMechanics, ValidatedReaderCatalogue,
            MAX_READER_CATALOGUE_BYTES,
        },
        SpoilerLevel,
    },
    reader::CatalogueRef,
    ValueState,
};

use crate::{CancellationToken, SnapshotErrorCode, SnapshotReader};

const RESOURCE_DIRECTORY: &str = "local.ivalice.companion";
const RESOURCE_FILE: &str = "reader-catalogue-v1.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReaderCatalogueLoadErrorCode {
    KnownFolderUnavailable,
    Snapshot(SnapshotErrorCode),
    Validation(ReaderCatalogueError),
    NonCanonical,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReaderCatalogueLoadError {
    pub code: ReaderCatalogueLoadErrorCode,
}

impl std::fmt::Display for ReaderCatalogueLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "reader catalogue resource load failed: {:?}",
            self.code
        )
    }
}

impl std::error::Error for ReaderCatalogueLoadError {}

/// One immutable validated resource. Hosts bind its digest to their resource generation.
pub struct ReaderCatalogueResource {
    data: ValidatedReaderCatalogue,
    sha256: [u8; 32],
}

impl ReaderCatalogueResource {
    /// Visibility is applied before any source label or description is returned.
    #[must_use]
    pub fn lookup(&self, id: &str, ceiling: SpoilerLevel) -> Option<CatalogueRef> {
        self.data.lookup(id, ceiling)
    }

    #[must_use]
    pub fn mechanics(&self) -> Option<&ReaderMechanics> {
        self.data.mechanics()
    }

    #[must_use]
    pub fn command_for_job(&self, job: &str) -> ValueState<CatalogueRef> {
        self.data.command_for_job(job)
    }

    /// Digest of the exact canonical bytes, never a saved-unit or file-path identity.
    #[must_use]
    pub fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
}

pub struct ReaderCatalogueLoader {
    path: PathBuf,
    bundled_path: Option<PathBuf>,
    reader: SnapshotReader,
}

impl ReaderCatalogueLoader {
    pub fn for_current_user() -> Result<Self, ReaderCatalogueLoadError> {
        let root = dirs::data_local_dir().ok_or(ReaderCatalogueLoadError {
            code: ReaderCatalogueLoadErrorCode::KnownFolderUnavailable,
        })?;
        Ok(Self::for_local_app_data(root))
    }

    /// Tests may substitute a temporary LocalAppData root; the child path stays fixed.
    #[must_use]
    pub fn for_local_app_data(root: PathBuf) -> Self {
        Self {
            path: root.join(RESOURCE_DIRECTORY).join(RESOURCE_FILE),
            bundled_path: None,
            reader: SnapshotReader::new(),
        }
    }

    /// A missing profile resource may use the local build's bundled catalogue.
    /// A valid older profile may use a bundled mechanics extension with exactly
    /// matching source text. A present but invalid profile remains an error.
    #[must_use]
    pub fn with_bundled_path(mut self, path: PathBuf) -> Self {
        self.bundled_path = Some(path);
        self
    }

    /// Every call acquires fresh bounded bytes. Failed calls publish no resource.
    pub fn load(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<ReaderCatalogueResource, ReaderCatalogueLoadError> {
        let (mut snapshot, from_profile) = match self.reader.acquire_limited(
            &self.path,
            cancellation,
            MAX_READER_CATALOGUE_BYTES as u64,
        ) {
            Ok(snapshot) => (snapshot, true),
            Err(error) if error.code == SnapshotErrorCode::NotFound => {
                let path = self.bundled_path.as_ref().unwrap_or(&self.path);
                (
                    self.reader
                        .acquire_limited(path, cancellation, MAX_READER_CATALOGUE_BYTES as u64)
                        .map_err(|error| ReaderCatalogueLoadError {
                            code: ReaderCatalogueLoadErrorCode::Snapshot(error.code),
                        })?,
                    false,
                )
            }
            Err(error) => {
                return Err(ReaderCatalogueLoadError {
                    code: ReaderCatalogueLoadErrorCode::Snapshot(error.code),
                });
            }
        };
        let mut data =
            ValidatedReaderCatalogue::from_json(snapshot.bytes()).map_err(validation_error)?;
        let canonical = data.canonical_json().map_err(validation_error)?;
        if snapshot.bytes() != canonical {
            return Err(ReaderCatalogueLoadError {
                code: ReaderCatalogueLoadErrorCode::NonCanonical,
            });
        }
        if from_profile
            && (data
                .mechanics()
                .is_none_or(|mechanics| mechanics.items.iter().all(|item| item.details.is_empty()))
                || data
                    .mechanics()
                    .is_none_or(|mechanics| mechanics.jobs.iter().any(|job| job.growth.is_none()))
                || !data.has_job_commands())
        {
            if let Some(path) = &self.bundled_path {
                if let Ok(candidate) = self.reader.acquire_limited(
                    path,
                    cancellation,
                    MAX_READER_CATALOGUE_BYTES as u64,
                ) {
                    if let Ok(enriched) = ValidatedReaderCatalogue::from_json(candidate.bytes()) {
                        if enriched.mechanics().is_some()
                            && enriched.has_job_commands()
                            && data.same_text_as(&enriched)
                            && enriched
                                .canonical_json()
                                .is_ok_and(|bytes| bytes == candidate.bytes())
                        {
                            snapshot = candidate;
                            data = enriched;
                        }
                    }
                }
            }
        }
        if cancellation.is_cancelled() {
            return Err(ReaderCatalogueLoadError {
                code: ReaderCatalogueLoadErrorCode::Snapshot(SnapshotErrorCode::Cancelled),
            });
        }
        Ok(ReaderCatalogueResource {
            data,
            sha256: snapshot.sha256(),
        })
    }

    /// Reacquire before using an earlier content identity. Changed canonical content is stale;
    /// byte-identical replacement retains the same content identity. The host owns generations.
    pub fn load_matching(
        &self,
        expected_sha: [u8; 32],
        cancellation: &CancellationToken,
    ) -> Result<ReaderCatalogueResource, ReaderCatalogueLoadError> {
        let resource = self.load(cancellation)?;
        if resource.sha256 != expected_sha {
            return Err(ReaderCatalogueLoadError {
                code: ReaderCatalogueLoadErrorCode::Stale,
            });
        }
        Ok(resource)
    }
}

fn validation_error(error: ReaderCatalogueError) -> ReaderCatalogueLoadError {
    ReaderCatalogueLoadError {
        code: ReaderCatalogueLoadErrorCode::Validation(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ivalice_domain::{
        game_data::reader_catalogue::{
            ReaderCatalogueDocument, ReaderCatalogueEntry, ReaderCatalogueSource, ReaderCategory,
            READER_CATALOGUE_PROFILE, READER_CATALOGUE_REVISION, READER_CATALOGUE_SCHEMA,
        },
        ValueState,
    };
    use sha2::{Digest, Sha256};
    use std::{
        error::Error,
        fs,
        path::Path,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    /// Original synthetic text with required provenance declarations; no imported game content.
    fn fixture(label: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let categories = [
            ReaderCategory::Job,
            ReaderCategory::Command,
            ReaderCategory::Ability,
            ReaderCategory::Item,
            ReaderCategory::CharacterName,
        ];
        let document = ReaderCatalogueDocument {
            schema: READER_CATALOGUE_SCHEMA.into(),
            profile: READER_CATALOGUE_PROFILE.into(),
            revision: READER_CATALOGUE_REVISION.into(),
            locale: "en".into(),
            sources: categories
                .iter()
                .map(|category| ReaderCatalogueSource {
                    category: *category,
                    input_sha256: category.input_sha256().into(),
                })
                .collect(),
            entries: vec![
                ReaderCatalogueEntry {
                    category: ReaderCategory::Job,
                    id: "job:1".into(),
                    label: ValueState::Known(label.into()),
                    description: ValueState::Known("Synthetic description\nsecond line".into()),
                    spoiler: SpoilerLevel::Minimal,
                },
                ReaderCatalogueEntry {
                    category: ReaderCategory::CharacterName,
                    id: "character_name:2".into(),
                    label: ValueState::Known("Synthetic hidden name".into()),
                    description: ValueState::Unknown,
                    spoiler: SpoilerLevel::Full,
                },
            ],
            job_commands: vec![],
            mechanics: None,
        };
        Ok(ValidatedReaderCatalogue::validate(document)?.canonical_json()?)
    }

    fn temporary_root() -> Result<PathBuf, Box<dyn Error>> {
        static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ivalice-reader-catalogue-{}-{nanos}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(RESOURCE_DIRECTORY))?;
        Ok(root)
    }

    fn target(root: &Path) -> PathBuf {
        root.join(RESOURCE_DIRECTORY).join(RESOURCE_FILE)
    }

    #[test]
    fn bundled_catalogue_loads_on_fresh_profile_without_masking_invalid_override(
    ) -> Result<(), Box<dyn Error>> {
        let root = temporary_root()?;
        let bundle = root.join("resources").join(RESOURCE_FILE);
        fs::create_dir_all(bundle.parent().ok_or("bundle parent")?)?;
        let bundled = fixture("Synthetic bundled")?;
        fs::write(&bundle, &bundled)?;
        let loader = ReaderCatalogueLoader::for_local_app_data(root.clone())
            .with_bundled_path(bundle.clone());
        let cancel = CancellationToken::default();
        let first = loader.load(&cancel)?;
        assert_eq!(
            first
                .lookup("job:1", SpoilerLevel::Full)
                .map(|row| row.label),
            Some(ValueState::Known("Synthetic bundled".into()))
        );
        assert!(!target(&root).exists());
        fs::write(target(&root), b"invalid profile override")?;
        assert_eq!(
            loader.load(&cancel).err().map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Validation(
                ReaderCatalogueError::InvalidJson
            ))
        );
        fs::remove_file(target(&root))?;
        fs::remove_file(bundle)?;
        assert_eq!(
            loader.load(&cancel).err().map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Snapshot(
                SnapshotErrorCode::NotFound
            ))
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires ignored enriched reader catalogue"]
    fn older_matching_profile_uses_bundled_mechanics_without_changing_profile(
    ) -> Result<(), Box<dyn Error>> {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .ok_or("workspace unavailable")?;
        let bundled = fs::read(
            workspace.join(".local/reader-catalogue/reader-catalogue-with-commands.json"),
        )?;
        let mut older: serde_json::Value = serde_json::from_slice(&bundled)?;
        older
            .as_object_mut()
            .ok_or("catalogue object missing")?
            .remove("mechanics");
        older
            .as_object_mut()
            .ok_or("catalogue object missing")?
            .remove("job_commands");
        let older =
            ValidatedReaderCatalogue::from_json(&serde_json::to_vec(&older)?)?.canonical_json()?;
        let root = temporary_root()?;
        let bundle_path = root.join("resources").join(RESOURCE_FILE);
        fs::create_dir_all(bundle_path.parent().ok_or("bundle parent")?)?;
        fs::write(&bundle_path, &bundled)?;
        fs::write(target(&root), &older)?;
        let loader =
            ReaderCatalogueLoader::for_local_app_data(root.clone()).with_bundled_path(bundle_path);
        let cancel = CancellationToken::default();
        let resource = loader.load(&cancel)?;
        assert!(resource.mechanics().is_some());
        assert!(resource.command_for_job("job:86") != ValueState::Unknown);
        assert_eq!(
            resource.sha256(),
            <[u8; 32]>::from(Sha256::digest(&bundled))
        );
        assert_eq!(fs::read(target(&root))?, older);

        let different = fixture("Synthetic user text")?;
        fs::write(target(&root), &different)?;
        let retained = loader.load(&cancel)?;
        assert!(retained.mechanics().is_none());
        assert_eq!(
            retained.sha256(),
            <[u8; 32]>::from(Sha256::digest(&different))
        );
        fs::write(target(&root), b"corrupt profile")?;
        assert_eq!(
            loader.load(&cancel).err().map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Validation(
                ReaderCatalogueError::InvalidJson
            ))
        );
        Ok(())
    }

    #[test]
    fn matching_profile_without_growth_uses_bundle_without_changing_profile(
    ) -> Result<(), Box<dyn Error>> {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .ok_or("workspace unavailable")?;
        let Ok(bundled) = fs::read(
            workspace.join(".local/reader-catalogue/reader-catalogue-with-growth-v3.json"),
        ) else {
            return Ok(());
        };
        let mut older: ReaderCatalogueDocument = serde_json::from_slice(&bundled)?;
        for job in &mut older.mechanics.as_mut().ok_or("mechanics missing")?.jobs {
            job.growth = None;
        }
        let older = ValidatedReaderCatalogue::validate(older)?.canonical_json()?;
        let root = temporary_root()?;
        let bundle_path = root.join("bundle.json");
        fs::write(&bundle_path, &bundled)?;
        fs::write(target(&root), &older)?;
        let loader =
            ReaderCatalogueLoader::for_local_app_data(root.clone()).with_bundled_path(bundle_path);
        let resource = loader.load(&CancellationToken::default())?;
        assert!(resource
            .mechanics()
            .is_some_and(|data| data.jobs.iter().all(|job| job.growth.is_some())));
        assert_eq!(
            resource.sha256(),
            <[u8; 32]>::from(Sha256::digest(&bundled))
        );
        assert_eq!(fs::read(target(&root))?, older);
        Ok(())
    }

    #[test]
    fn immutable_resource_hash_and_filtered_text_match_unchanged_source(
    ) -> Result<(), Box<dyn Error>> {
        let root = temporary_root()?;
        let bytes = fixture("Synthetic étiquette")?;
        fs::write(target(&root), &bytes)?;
        let loader = ReaderCatalogueLoader::for_local_app_data(root.clone());
        let cancel = CancellationToken::default();
        let resource = loader.load(&cancel)?;
        assert_eq!(resource.sha256(), <[u8; 32]>::from(Sha256::digest(&bytes)));
        let job = resource
            .lookup("job:1", SpoilerLevel::Minimal)
            .ok_or("missing synthetic job")?;
        assert_eq!(job.label, ValueState::Known("Synthetic étiquette".into()));
        assert_eq!(
            job.description,
            ValueState::Known("Synthetic description\nsecond line".into())
        );
        assert_eq!(job.asset_key, ValueState::Unknown);
        assert!(resource
            .lookup("character_name:2", SpoilerLevel::Gameplay)
            .is_none());
        assert!(resource
            .lookup("character_name:2", SpoilerLevel::Full)
            .is_some());
        assert!(resource.lookup("item:65535", SpoilerLevel::Full).is_none());
        assert_eq!(
            loader.load_matching(resource.sha256(), &cancel)?.sha256(),
            resource.sha256()
        );
        assert_eq!(fs::read(target(&root))?, bytes);
        Ok(())
    }

    #[test]
    fn missing_corrupt_noncanonical_wrong_profile_and_oversized_fail() -> Result<(), Box<dyn Error>>
    {
        let root = temporary_root()?;
        let loader = ReaderCatalogueLoader::for_local_app_data(root.clone());
        let cancel = CancellationToken::default();
        assert_eq!(
            loader.load(&cancel).err().map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Snapshot(
                SnapshotErrorCode::NotFound
            ))
        );
        fs::write(target(&root), b"corrupt synthetic name")?;
        assert_eq!(
            loader.load(&cancel).err().map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Validation(
                ReaderCatalogueError::InvalidJson
            ))
        );
        let value: serde_json::Value = serde_json::from_slice(&fixture("Synthetic label")?)?;
        fs::write(target(&root), serde_json::to_vec_pretty(&value)?)?;
        assert_eq!(
            loader.load(&cancel).err().map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::NonCanonical)
        );
        let mut wrong_profile = value;
        wrong_profile["profile"] = serde_json::json!("other");
        fs::write(target(&root), serde_json::to_vec(&wrong_profile)?)?;
        assert_eq!(
            loader.load(&cancel).err().map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Validation(
                ReaderCatalogueError::SourceMismatch
            ))
        );
        fs::write(target(&root), vec![b' '; MAX_READER_CATALOGUE_BYTES + 1])?;
        assert_eq!(
            loader.load(&cancel).err().map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Snapshot(
                SnapshotErrorCode::TooLarge
            ))
        );
        Ok(())
    }

    #[test]
    fn cancellation_prevents_load_and_matching_publication() -> Result<(), Box<dyn Error>> {
        let root = temporary_root()?;
        let bytes = fixture("Synthetic label")?;
        fs::write(target(&root), &bytes)?;
        let loader = ReaderCatalogueLoader::for_local_app_data(root.clone());
        let cancel = CancellationToken::default();
        cancel.cancel();
        assert_eq!(
            loader.load(&cancel).err().map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Snapshot(
                SnapshotErrorCode::Cancelled
            ))
        );
        assert_eq!(
            loader
                .load_matching([0; 32], &cancel)
                .err()
                .map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Snapshot(
                SnapshotErrorCode::Cancelled
            ))
        );
        assert_eq!(fs::read(target(&root))?, bytes);
        Ok(())
    }

    #[test]
    fn replacement_changes_digest_and_old_content_identity_fails_closed(
    ) -> Result<(), Box<dyn Error>> {
        let root = temporary_root()?;
        let first_bytes = fixture("Synthetic first")?;
        let second_bytes = fixture("Synthetic second")?;
        fs::write(target(&root), &first_bytes)?;
        let loader = ReaderCatalogueLoader::for_local_app_data(root.clone());
        let cancel = CancellationToken::default();
        let first = loader.load(&cancel)?;
        let replacement = root.join("synthetic-replacement.json");
        let retired = root.join("synthetic-retired.json");
        fs::write(&replacement, &second_bytes)?;
        fs::rename(target(&root), &retired)?;
        fs::rename(&replacement, target(&root))?;
        assert_eq!(
            loader
                .load_matching(first.sha256(), &cancel)
                .err()
                .map(|error| error.code),
            Some(ReaderCatalogueLoadErrorCode::Stale)
        );
        let second = loader.load(&cancel)?;
        assert_ne!(first.sha256(), second.sha256());
        assert_eq!(
            second.sha256(),
            <[u8; 32]>::from(Sha256::digest(&second_bytes))
        );
        assert_eq!(
            first
                .lookup("job:1", SpoilerLevel::Full)
                .map(|row| row.label),
            Some(ValueState::Known("Synthetic first".into()))
        );
        assert_eq!(
            second
                .lookup("job:1", SpoilerLevel::Full)
                .map(|row| row.label),
            Some(ValueState::Known("Synthetic second".into()))
        );
        assert_eq!(fs::read(&retired)?, first_bytes);
        assert_eq!(fs::read(target(&root))?, second_bytes);
        Ok(())
    }

    #[test]
    fn failed_reload_has_no_fallback_and_errors_expose_no_paths_or_text(
    ) -> Result<(), Box<dyn Error>> {
        let root = temporary_root()?;
        fs::write(target(&root), fixture("Synthetic preserved")?)?;
        let loader = ReaderCatalogueLoader::for_local_app_data(root.clone());
        let cancel = CancellationToken::default();
        let first = loader.load(&cancel)?;
        fs::write(target(&root), b"secret synthetic text")?;
        let error = loader
            .load_matching(first.sha256(), &cancel)
            .err()
            .ok_or("expected reload failure")?;
        assert_eq!(
            error.code,
            ReaderCatalogueLoadErrorCode::Validation(ReaderCatalogueError::InvalidJson)
        );
        assert_eq!(
            error.to_string(),
            "reader catalogue resource load failed: Validation(InvalidJson)"
        );
        assert!(!format!("{error:?}").contains("secret synthetic text"));
        assert!(!format!("{error:?}").contains(root.to_string_lossy().as_ref()));
        assert!(loader.load(&cancel).is_err());
        assert_eq!(
            first
                .lookup("job:1", SpoilerLevel::Minimal)
                .map(|row| row.label),
            Some(ValueState::Known("Synthetic preserved".into()))
        );
        assert_eq!(fs::read(target(&root))?, b"secret synthetic text");
        Ok(())
    }

    #[test]
    fn byte_identical_replacement_preserves_content_identity() -> Result<(), Box<dyn Error>> {
        let root = temporary_root()?;
        let bytes = fixture("Synthetic unchanged")?;
        fs::write(target(&root), &bytes)?;
        let loader = ReaderCatalogueLoader::for_local_app_data(root.clone());
        let cancel = CancellationToken::default();
        let first = loader.load(&cancel)?;
        let retired = root.join("synthetic-identical-retired.json");
        fs::rename(target(&root), &retired)?;
        fs::write(target(&root), &bytes)?;
        let replacement = loader.load_matching(first.sha256(), &cancel)?;
        assert_eq!(replacement.sha256(), first.sha256());
        assert_eq!(
            replacement.lookup("job:1", SpoilerLevel::Minimal),
            first.lookup("job:1", SpoilerLevel::Minimal)
        );
        assert_eq!(fs::read(retired)?, bytes);
        assert_eq!(fs::read(target(&root))?, bytes);
        Ok(())
    }

    #[test]
    #[ignore = "run only under D032's registered local source-artifact reproduction"]
    fn registered_offline_artifact_loads_without_changing_source() -> Result<(), Box<dyn Error>> {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .ok_or("missing repository ancestor")?;
        let source = repository.join(".local/d032-catalogue/run-1.json");
        let cancel = CancellationToken::default();
        let source_reader = SnapshotReader::new();
        let before =
            source_reader.acquire_limited(&source, &cancel, MAX_READER_CATALOGUE_BYTES as u64)?;
        assert_eq!(before.len(), 413_817);
        let digest: String = before
            .sha256()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            digest,
            "d9733e2573dc228cbbb8228cdeb90d2dee52a0a22383cd70a528408ffb31ea8c"
        );
        let root = temporary_root()?;
        fs::write(target(&root), before.bytes())?;
        let loader = ReaderCatalogueLoader::for_local_app_data(root.clone());
        let resource = loader.load_matching(before.sha256(), &cancel)?;
        assert_eq!(resource.sha256(), before.sha256());
        assert_eq!(
            resource
                .lookup("job:78", SpoilerLevel::Full)
                .map(|row| row.label),
            Some(ValueState::Known("Monk".into()))
        );
        assert!(resource.lookup("job:78", SpoilerLevel::Minimal).is_none());
        assert_eq!(fs::read(target(&root))?, before.bytes());
        let after =
            source_reader.acquire_limited(&source, &cancel, MAX_READER_CATALOGUE_BYTES as u64)?;
        assert_eq!(after.sha256(), before.sha256());
        assert_eq!(after.bytes(), before.bytes());
        Ok(())
    }
}
