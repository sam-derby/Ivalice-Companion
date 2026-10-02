use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::candidate::{NominatedPath, NominationKind};
use crate::windows_fs::{self, DirectoryGuard, PathFailure};
use crate::{MAX_NOMINATIONS, MAX_SETTINGS_BYTES, SETTINGS_SCHEMA};

const SETTINGS_DIRECTORY: &str = "local.ivalice.companion";
const SETTINGS_FILE: &str = "settings.json";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Settings {
    pub nominations: Vec<NominatedPath>,
    pub selected_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsError {
    KnownFolderUnavailable,
    InvalidPath,
    NotFound,
    AccessDenied,
    UnsupportedLocation,
    IdentityUnavailable,
    TooManyNominations,
    TooLarge,
    Malformed,
    UnsupportedSchema,
    DestinationOverlap,
    UnsafeDestination,
    IoFailure,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::KnownFolderUnavailable => "the local application-data folder is unavailable",
            Self::InvalidPath => "a configured path is invalid",
            Self::NotFound => "a required companion settings path is missing",
            Self::AccessDenied => "access to companion settings was denied",
            Self::UnsupportedLocation => "the settings location is unsupported",
            Self::IdentityUnavailable => "filesystem identity is unavailable",
            Self::TooManyNominations => "too many save locations were nominated",
            Self::TooLarge => "settings exceed the configured size limit",
            Self::Malformed => "settings are malformed",
            Self::UnsupportedSchema => "the settings schema is unsupported",
            Self::DestinationOverlap => "settings overlap a nominated source location",
            Self::UnsafeDestination => "the settings destination is not a safe single-link file",
            Self::IoFailure => "a companion settings operation failed",
        })
    }
}

impl std::error::Error for SettingsError {}

#[derive(Clone, Debug)]
pub struct SettingsStore {
    local_app_data: PathBuf,
    root: PathBuf,
}

impl SettingsStore {
    pub fn for_current_user() -> Result<Self, SettingsError> {
        let local_app_data = dirs::data_local_dir().ok_or(SettingsError::KnownFolderUnavailable)?;
        Self::for_local_app_data(local_app_data)
    }

    pub fn for_local_app_data(local_app_data: impl Into<PathBuf>) -> Result<Self, SettingsError> {
        let local_app_data = local_app_data.into();
        windows_fs::validate_path_shape(&local_app_data).map_err(map_path_failure)?;
        Ok(Self {
            root: local_app_data.join(SETTINGS_DIRECTORY),
            local_app_data,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load(&self) -> Result<Option<Settings>, SettingsError> {
        if !self.root.try_exists().map_err(map_io_error)? {
            return Ok(None);
        }
        let guard = DirectoryGuard::pin(&self.root).map_err(map_path_failure)?;
        guard.validate().map_err(map_path_failure)?;
        let settings_path = self.root.join(SETTINGS_FILE);
        if !settings_path.try_exists().map_err(map_io_error)? {
            return Ok(None);
        }
        validate_settings_target(&settings_path)?;

        let mut file = File::open(&settings_path).map_err(map_io_error)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_SETTINGS_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(map_io_error)?;
        if u64::try_from(bytes.len()).map_or(true, |length| length > MAX_SETTINGS_BYTES) {
            return Err(SettingsError::TooLarge);
        }
        guard.validate().map_err(map_path_failure)?;
        decode_settings(&bytes).map(Some)
    }

    pub fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        validate_settings(settings)?;
        let protected_sources = configured_sources(settings);
        self.reject_overlaps(&protected_sources)?;
        let wire = StoredSettings::from_settings(settings)?;
        let mut bytes = serde_json::to_vec_pretty(&wire).map_err(|_| SettingsError::Malformed)?;
        bytes.push(b'\n');
        if u64::try_from(bytes.len()).map_or(true, |length| length > MAX_SETTINGS_BYTES) {
            return Err(SettingsError::TooLarge);
        }

        let (guard, root_directory) = self.prepare_root()?;
        self.reject_overlaps(&protected_sources)?;
        let settings_path = self.root.join(SETTINGS_FILE);
        if settings_path.try_exists().map_err(map_io_error)? {
            // Never replace settings whose schema or destination safety was not established.
            self.load()?.ok_or(SettingsError::UnsafeDestination)?;
            validate_settings_target(&settings_path)?;
        }

        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp_name = format!("settings.{}.{}.tmp", std::process::id(), sequence);
        let temp_path = self.root.join(temp_name);
        let write_result = self.write_and_publish(
            &guard,
            &root_directory,
            &temp_path,
            &settings_path,
            &bytes,
            &protected_sources,
        );
        if write_result.is_err() {
            remove_created_temp(&root_directory, &temp_path);
        }
        write_result
    }

    fn prepare_root(&self) -> Result<(DirectoryGuard, cap_std::fs::Dir), SettingsError> {
        windows_fs::validate_supported_volume(&self.local_app_data).map_err(map_path_failure)?;
        windows_fs::validate_components(&self.local_app_data).map_err(map_path_failure)?;
        let base_guard = DirectoryGuard::pin(&self.local_app_data).map_err(map_path_failure)?;
        if !self.root.try_exists().map_err(map_io_error)? {
            fs::create_dir(&self.root).map_err(map_io_error)?;
        }
        let root_guard = DirectoryGuard::pin(&self.root).map_err(map_path_failure)?;
        let root_directory =
            cap_std::fs::Dir::open_ambient_dir(&self.root, cap_std::ambient_authority())
                .map_err(map_io_error)?;
        base_guard.validate().map_err(map_path_failure)?;
        root_guard.validate().map_err(map_path_failure)?;
        Ok((root_guard, root_directory))
    }

    fn write_and_publish(
        &self,
        guard: &DirectoryGuard,
        root_directory: &cap_std::fs::Dir,
        temp_path: &Path,
        settings_path: &Path,
        bytes: &[u8],
        protected_sources: &[PathBuf],
    ) -> Result<(), SettingsError> {
        let temp_name = temp_path
            .file_name()
            .ok_or(SettingsError::UnsafeDestination)?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut temporary = root_directory
            .open_with(temp_name, &options)
            .map_err(map_io_error)?;
        temporary.write_all(bytes).map_err(map_io_error)?;
        temporary.sync_all().map_err(map_io_error)?;
        drop(temporary);

        let expected_identity = windows_fs::safe_single_link_file(temp_path)
            .map_err(|_| SettingsError::UnsafeDestination)?;
        guard.validate().map_err(map_path_failure)?;
        self.reject_overlaps(protected_sources)?;
        if settings_path.try_exists().map_err(map_io_error)? {
            validate_settings_target(settings_path)?;
        }

        root_directory
            .rename(temp_name, root_directory, SETTINGS_FILE)
            .map_err(map_io_error)?;
        let published_identity = windows_fs::safe_single_link_file(settings_path)
            .map_err(|_| SettingsError::UnsafeDestination)?;
        if published_identity != expected_identity {
            return Err(SettingsError::UnsafeDestination);
        }
        guard.validate().map_err(map_path_failure)?;
        Ok(())
    }

    fn reject_overlaps(&self, protected_sources: &[PathBuf]) -> Result<(), SettingsError> {
        for source in protected_sources {
            windows_fs::validate_path_shape(source).map_err(map_path_failure)?;
            if windows_fs::paths_overlap(&self.root, source) {
                return Err(SettingsError::DestinationOverlap);
            }
            if let (Ok(root), Ok(source)) = (fs::canonicalize(&self.root), fs::canonicalize(source))
            {
                if windows_fs::paths_overlap(&root, &source) {
                    return Err(SettingsError::DestinationOverlap);
                }
            }
        }
        Ok(())
    }
}

fn configured_sources(settings: &Settings) -> Vec<PathBuf> {
    let mut sources: Vec<_> = settings
        .nominations
        .iter()
        .map(|nomination| nomination.path.clone())
        .collect();
    if let Some(selected) = &settings.selected_file {
        sources.push(selected.clone());
    }
    sources
}

fn validate_settings(settings: &Settings) -> Result<(), SettingsError> {
    if settings.nominations.len() > MAX_NOMINATIONS {
        return Err(SettingsError::TooManyNominations);
    }
    for nomination in &settings.nominations {
        windows_fs::validate_path_shape(&nomination.path).map_err(map_path_failure)?;
    }
    if let Some(selected) = &settings.selected_file {
        windows_fs::validate_path_shape(selected).map_err(map_path_failure)?;
    }
    Ok(())
}

fn validate_settings_target(path: &Path) -> Result<(), SettingsError> {
    windows_fs::safe_single_link_file(path).map_err(|_| SettingsError::UnsafeDestination)?;
    Ok(())
}

fn remove_created_temp(root_directory: &cap_std::fs::Dir, path: &Path) {
    if path.file_name().is_some_and(|name| {
        let name = name.to_string_lossy();
        name.starts_with("settings.") && name.ends_with(".tmp")
    }) {
        if let Some(name) = path.file_name() {
            let _ = root_directory.remove_file(name);
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredSettings {
    schema: u32,
    nominations: Vec<StoredNomination>,
    selected_file: Option<String>,
}

impl StoredSettings {
    fn from_settings(settings: &Settings) -> Result<Self, SettingsError> {
        let nominations = settings
            .nominations
            .iter()
            .map(|nomination| {
                Ok(StoredNomination {
                    kind: nomination.kind,
                    path: path_to_string(&nomination.path)?,
                })
            })
            .collect::<Result<Vec<_>, SettingsError>>()?;
        let selected_file = settings
            .selected_file
            .as_deref()
            .map(path_to_string)
            .transpose()?;
        Ok(Self {
            schema: SETTINGS_SCHEMA,
            nominations,
            selected_file,
        })
    }

    fn into_settings(self) -> Result<Settings, SettingsError> {
        if self.schema != SETTINGS_SCHEMA {
            return Err(SettingsError::UnsupportedSchema);
        }
        let settings = Settings {
            nominations: self
                .nominations
                .into_iter()
                .map(|item| NominatedPath {
                    kind: item.kind,
                    path: PathBuf::from(item.path),
                })
                .collect(),
            selected_file: self.selected_file.map(PathBuf::from),
        };
        validate_settings(&settings)?;
        Ok(settings)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredNomination {
    kind: NominationKind,
    path: String,
}

fn decode_settings(bytes: &[u8]) -> Result<Settings, SettingsError> {
    serde_json::from_slice::<StoredSettings>(bytes)
        .map_err(|_| SettingsError::Malformed)?
        .into_settings()
}

fn path_to_string(path: &Path) -> Result<String, SettingsError> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or(SettingsError::InvalidPath)
}

fn map_path_failure(failure: PathFailure) -> SettingsError {
    match failure {
        PathFailure::Invalid => SettingsError::InvalidPath,
        PathFailure::NotFound => SettingsError::NotFound,
        PathFailure::AccessDenied => SettingsError::AccessDenied,
        PathFailure::NotRegularFile => SettingsError::UnsafeDestination,
        PathFailure::UnsupportedLocation => SettingsError::UnsupportedLocation,
        PathFailure::IdentityUnavailable => SettingsError::IdentityUnavailable,
        PathFailure::Busy => SettingsError::IoFailure,
        PathFailure::Io => SettingsError::IoFailure,
    }
}

fn map_io_error(error: io::Error) -> SettingsError {
    map_path_failure(windows_fs::map_io_error(error))
}
