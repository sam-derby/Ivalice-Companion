use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use ivalice_domain::{ManualSlotId, NormalizedSave, SnapshotByteLength};
#[cfg(test)]
use ivalice_infrastructure::Snapshot;
use ivalice_infrastructure::{
    discover_candidates, CancellationToken, CandidateErrorCode, CandidateSet, CandidateValidator,
    NativeCandidateValidator, NominatedPath, Settings, SettingsError, SettingsStore, SnapshotError,
    SnapshotErrorCode, SnapshotReader, SnapshotTransientCode,
};
use ivalice_save_format::{
    decode_enhanced_png, ContainerError, ErrorKind, ManualParseError, PINNED_DICTIONARY_LENGTH,
};
use serde::{Deserialize, Serialize};
use tauri::State;

mod character_preview;
pub use character_preview::{
    __cmd__preview_character, __tauri_command_name_preview_character, preview_character,
};
mod job_preview;
mod reader;
pub use job_preview::{
    __cmd__preview_job_progress, __tauri_command_name_preview_job_progress, preview_job_progress,
};
pub use reader::{__cmd__load_reader, __tauri_command_name_load_reader, load_reader};
mod save_edit;
pub use save_edit::{
    __cmd__restore_last_backup, __cmd__save_transaction, __tauri_command_name_restore_last_backup,
    __tauri_command_name_save_transaction, restore_last_backup, save_transaction,
};

const MAX_SELECTION_UTF16: usize = 32_767;
const DICTIONARY_DIRECTORY: &str = "resources";
const DICTIONARY_FILENAME: &str = "CompressDict.bin";
const MAX_SAFE_GENERATION: u64 = 9_007_199_254_740_991;

#[derive(Clone)]
pub struct DesktopState {
    settings: Result<SettingsStore, SettingsError>,
    reader: SnapshotReader,
    load_generation: Arc<AtomicU64>,
    load_serial: Arc<Mutex<()>>,
    resource_root: PathBuf,
    catalogue: Result<
        Arc<ivalice_infrastructure::ReaderCatalogueLoader>,
        ivalice_infrastructure::ReaderCatalogueLoadError,
    >,
    reader_session: Arc<str>,
    resource_generation: Arc<AtomicU64>,
    loaded_edit: Arc<Mutex<Option<LoadedEdit>>>,
    last_backup: Arc<Mutex<Option<BackupReceipt>>>,
}

#[derive(Clone)]
struct BackupReceipt {
    path: PathBuf,
    slot: u8,
    edited_sha256: [u8; 32],
    backup: ivalice_infrastructure::SaveBackup,
}

#[derive(Clone)]
struct LoadedEdit {
    generation: u64,
    path: PathBuf,
    sha256: [u8; 32],
    slot: u8,
    gil: u32,
    catalogue_token: Option<String>,
    units: Vec<ivalice_save_format::UnitRecord>,
}

impl DesktopState {
    #[cfg(test)]
    pub fn new(settings: SettingsStore, resource_root: PathBuf) -> Self {
        Self::with_settings(Ok(settings), resource_root)
    }

    pub fn for_current_user(resource_root: PathBuf) -> Self {
        Self::with_settings(SettingsStore::for_current_user(), resource_root)
    }

    fn with_settings(
        settings: Result<SettingsStore, SettingsError>,
        resource_root: PathBuf,
    ) -> Self {
        let catalogue = settings
            .as_ref()
            .ok()
            .and_then(|store| store.root().parent())
            .map(|root| {
                Arc::new(
                    ivalice_infrastructure::ReaderCatalogueLoader::for_local_app_data(
                        root.to_path_buf(),
                    )
                    .with_bundled_path(
                        resource_root
                            .join(DICTIONARY_DIRECTORY)
                            .join("reader-catalogue-v1.json"),
                    ),
                )
            })
            .ok_or(ivalice_infrastructure::ReaderCatalogueLoadError {
                code: ivalice_infrastructure::ReaderCatalogueLoadErrorCode::KnownFolderUnavailable,
            });
        // An opaque in-memory session marker, not an account/path or a persistent unit identity.
        use std::hash::{BuildHasher, Hasher};
        let session = format!(
            "reader-{:016x}",
            std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish()
        );
        Self {
            settings,
            reader: SnapshotReader::new(),
            load_generation: Arc::new(AtomicU64::new(0)),
            load_serial: Arc::new(Mutex::new(())),
            resource_root,
            catalogue,
            reader_session: Arc::from(session),
            resource_generation: Arc::new(AtomicU64::new(0)),
            loaded_edit: Arc::new(Mutex::new(None)),
            last_backup: Arc::new(Mutex::new(None)),
        }
    }

    fn settings(&self) -> Result<&SettingsStore, IpcError> {
        self.settings
            .as_ref()
            .map_err(|error| map_settings_error(*error))
    }

    fn begin_load(&self) -> u64 {
        *lock_recover(&self.loaded_edit) = None;
        self.reader.cancel_current();
        next_generation(&self.load_generation)
    }

    fn cancel_load(&self) {
        *lock_recover(&self.loaded_edit) = None;
        self.reader.cancel_current();
        next_generation(&self.load_generation);
    }

    fn is_current(&self, generation: u64) -> bool {
        (1..=MAX_SAFE_GENERATION).contains(&generation)
            && self.load_generation.load(Ordering::Acquire) == generation
    }

    fn dictionary_path(&self) -> PathBuf {
        self.resource_root
            .join(DICTIONARY_DIRECTORY)
            .join(DICTIONARY_FILENAME)
    }
}

/// Exhaustion permanently invalidates the counter rather than wrapping back to an old handle.
fn next_generation(counter: &AtomicU64) -> u64 {
    match counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        Some(current.saturating_add(1).min(MAX_SAFE_GENERATION + 1))
    }) {
        Ok(previous) => previous.saturating_add(1).min(MAX_SAFE_GENERATION + 1),
        Err(_) => MAX_SAFE_GENERATION + 1,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCategory {
    Input,
    Selection,
    Settings,
    Snapshot,
    Resource,
    Unsupported,
    Corrupt,
    Limit,
    Cancelled,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IpcError {
    pub category: IpcErrorCategory,
    pub code: &'static str,
    pub retryable: bool,
    pub attempts: Option<u8>,
    pub last_transient: Option<&'static str>,
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for IpcError {}

impl IpcError {
    pub(crate) const fn simple(
        category: IpcErrorCategory,
        code: &'static str,
        retryable: bool,
    ) -> Self {
        Self {
            category,
            code,
            retryable,
            attempts: None,
            last_transient: None,
        }
    }

    pub(crate) const fn stale() -> Self {
        Self::simple(IpcErrorCategory::Cancelled, "stale_result", false)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SaveSelectionStatus {
    NoSelection,
    Selected,
    Unavailable { error: IpcError },
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadSaveRequest {
    pub request_id: u64,
    pub manual_slot_id: Option<u8>,
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadSaveResponse {
    pub request_id: u64,
    pub occupied_manual_slots: Vec<ManualSlotId>,
    pub save: Option<NormalizedSave>,
}

#[tauri::command]
pub async fn get_save_selection(
    state: State<'_, DesktopState>,
) -> Result<SaveSelectionStatus, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || selection_status(&state))
        .await
        .map_err(|_| worker_error())?
}

#[tauri::command]
pub async fn set_save_selection(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<SaveSelectionStatus, IpcError> {
    let state = state.inner().clone();
    state.cancel_load();
    tauri::async_runtime::spawn_blocking(move || {
        let _serial = lock_recover(&state.load_serial);
        let result = set_selection(&state, path);
        // Also invalidate loads queued after the first cancellation but before persistence.
        state.cancel_load();
        result
    })
    .await
    .map_err(|_| worker_error())?
}

fn selection_status(state: &DesktopState) -> Result<SaveSelectionStatus, IpcError> {
    let store = state.settings()?;
    let Some(settings) = store.load().map_err(map_settings_error)? else {
        return Ok(SaveSelectionStatus::NoSelection);
    };
    if settings.selected_file.is_none() {
        return Ok(SaveSelectionStatus::NoSelection);
    }
    match selected_candidate(state, &settings) {
        Ok(_) => Ok(SaveSelectionStatus::Selected),
        Err(error) => Ok(SaveSelectionStatus::Unavailable { error }),
    }
}

fn set_selection(state: &DesktopState, path: String) -> Result<SaveSelectionStatus, IpcError> {
    validate_selection_text(&path)?;
    let path = PathBuf::from(path);
    let store = state.settings()?;
    let validator = NativeCandidateValidator::new(Some(store.root().to_path_buf()));
    validator.validate(&path).map_err(map_candidate_error)?;

    let settings = Settings {
        nominations: vec![NominatedPath::file(path.clone())],
        selected_file: Some(path),
    };
    store.save(&settings).map_err(map_settings_error)?;
    Ok(SaveSelectionStatus::Selected)
}

fn validate_selection_text(path: &str) -> Result<(), IpcError> {
    if path.is_empty() || path.contains('\0') || path.encode_utf16().count() > MAX_SELECTION_UTF16 {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_selection_path",
            false,
        ));
    }
    Ok(())
}

fn selected_candidate(state: &DesktopState, settings: &Settings) -> Result<PathBuf, IpcError> {
    let validator = NativeCandidateValidator::new(Some(state.settings()?.root().to_path_buf()));
    let discovery = discover_candidates(
        &settings.nominations,
        settings.selected_file.as_deref(),
        &validator,
    )
    .map_err(|_| IpcError::simple(IpcErrorCategory::Settings, "too_many_nominations", false))?;

    match discovery.candidates {
        CandidateSet::SingleCandidate(candidate) => Ok(candidate.path),
        CandidateSet::NoCandidates | CandidateSet::SelectionUnavailable(_) => {
            let error = discovery
                .entry_errors
                .first()
                .map_or_else(selection_unavailable, |entry| {
                    map_candidate_error(entry.code)
                });
            Err(error)
        }
        CandidateSet::AmbiguousCandidates(_) => Err(IpcError::simple(
            IpcErrorCategory::Selection,
            "ambiguous_selection",
            false,
        )),
    }
}

#[cfg(test)]
fn load_selected(
    state: &DesktopState,
    generation: u64,
    request: LoadSaveRequest,
) -> Result<LoadSaveResponse, IpcError> {
    if request.manual_slot_id.is_some_and(|index| index >= 50) {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "manual_slot_out_of_range",
            false,
        ));
    }

    let settings = state.settings()?.load().map_err(map_settings_error)?;
    let settings = settings.ok_or_else(no_selection)?;
    let path = selected_candidate(state, &settings)?;
    load_snapshot_with(state, generation, &path, request, |snapshot, selected| {
        decode_snapshot(state, snapshot, selected)
    })
}

#[cfg(test)]
fn load_snapshot_with<F>(
    state: &DesktopState,
    generation: u64,
    path: &Path,
    request: LoadSaveRequest,
    decode: F,
) -> Result<LoadSaveResponse, IpcError>
where
    F: FnOnce(
        &Snapshot,
        Option<u8>,
    ) -> Result<(Vec<ManualSlotId>, Option<NormalizedSave>), IpcError>,
{
    let _serial = lock_recover(&state.load_serial);
    ensure_current(state, generation)?;
    let snapshot = state
        .reader
        .acquire(path, &CancellationToken::default())
        .map_err(map_snapshot_error)?;
    ensure_current(state, generation)?;
    let (occupied_manual_slots, save) = decode(&snapshot, request.manual_slot_id)?;
    ensure_current(state, generation)?;
    Ok(LoadSaveResponse {
        request_id: request.request_id,
        occupied_manual_slots,
        save,
    })
}

#[cfg(test)]
fn decode_snapshot(
    state: &DesktopState,
    snapshot: &Snapshot,
    selected_slot: Option<u8>,
) -> Result<(Vec<ManualSlotId>, Option<NormalizedSave>), IpcError> {
    let dictionary = read_dictionary(&state.dictionary_path())?;
    let decoded =
        decode_enhanced_png(snapshot.bytes(), Some(&dictionary)).map_err(map_container_error)?;
    let occupied = decoded.occupied_manual_slots().map_err(map_manual_error)?;
    let save = selected_slot
        .map(|slot| {
            let byte_length = u64::try_from(snapshot.len())
                .map_err(|_| IpcError::simple(IpcErrorCategory::Limit, "snapshot_length", false))?;
            let byte_length = SnapshotByteLength::new(byte_length).map_err(|_| {
                IpcError::simple(IpcErrorCategory::Internal, "snapshot_invariant", false)
            })?;
            decoded
                .normalized_manual_save(byte_length, slot)
                .map_err(map_manual_error)
        })
        .transpose()?;
    Ok((occupied, save))
}

fn read_dictionary(path: &Path) -> Result<Vec<u8>, IpcError> {
    let mut file = File::open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            IpcError::simple(IpcErrorCategory::Resource, "resource_missing", false)
        }
        std::io::ErrorKind::PermissionDenied => {
            IpcError::simple(IpcErrorCategory::Resource, "resource_access_denied", false)
        }
        _ => IpcError::simple(IpcErrorCategory::Resource, "resource_read", false),
    })?;
    let read_limit = u64::try_from(PINNED_DICTIONARY_LENGTH)
        .map_err(|_| IpcError::simple(IpcErrorCategory::Limit, "resource_length", false))?
        + 1;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|_| IpcError::simple(IpcErrorCategory::Resource, "resource_read", false))?;
    Ok(bytes)
}

fn ensure_current(state: &DesktopState, generation: u64) -> Result<(), IpcError> {
    if state.is_current(generation) {
        Ok(())
    } else {
        Err(IpcError::stale())
    }
}

fn no_selection() -> IpcError {
    IpcError::simple(IpcErrorCategory::Selection, "no_selection", false)
}

fn selection_unavailable() -> IpcError {
    IpcError::simple(IpcErrorCategory::Selection, "selection_unavailable", false)
}

fn worker_error() -> IpcError {
    IpcError::simple(IpcErrorCategory::Internal, "worker_failed", false)
}

fn map_candidate_error(error: CandidateErrorCode) -> IpcError {
    let code = match error {
        CandidateErrorCode::InvalidPath => "invalid_path",
        CandidateErrorCode::NotFound => "not_found",
        CandidateErrorCode::AccessDenied => "access_denied",
        CandidateErrorCode::NotRegularFile => "not_regular_file",
        CandidateErrorCode::UnsupportedLocation => "unsupported_location",
        CandidateErrorCode::IdentityUnavailable => "identity_unavailable",
        CandidateErrorCode::IoFailure => "io_failure",
    };
    IpcError::simple(IpcErrorCategory::Selection, code, false)
}

fn map_settings_error(error: SettingsError) -> IpcError {
    let code = match error {
        SettingsError::KnownFolderUnavailable => "known_folder_unavailable",
        SettingsError::InvalidPath => "invalid_path",
        SettingsError::NotFound => "not_found",
        SettingsError::AccessDenied => "access_denied",
        SettingsError::UnsupportedLocation => "unsupported_location",
        SettingsError::IdentityUnavailable => "identity_unavailable",
        SettingsError::TooManyNominations => "too_many_nominations",
        SettingsError::TooLarge => "settings_too_large",
        SettingsError::Malformed => "settings_malformed",
        SettingsError::UnsupportedSchema => "settings_schema",
        SettingsError::DestinationOverlap => "destination_overlap",
        SettingsError::UnsafeDestination => "unsafe_destination",
        SettingsError::IoFailure => "io_failure",
    };
    IpcError::simple(IpcErrorCategory::Settings, code, false)
}

fn map_snapshot_error(error: SnapshotError) -> IpcError {
    let (category, code, retryable) = match error.code {
        SnapshotErrorCode::InvalidPath => (IpcErrorCategory::Snapshot, "invalid_path", false),
        SnapshotErrorCode::NotRegularFile => {
            (IpcErrorCategory::Snapshot, "not_regular_file", false)
        }
        SnapshotErrorCode::UnsupportedLocation => {
            (IpcErrorCategory::Snapshot, "unsupported_location", false)
        }
        SnapshotErrorCode::IdentityUnavailable => {
            (IpcErrorCategory::Snapshot, "identity_unavailable", false)
        }
        SnapshotErrorCode::NotFound => (IpcErrorCategory::Snapshot, "not_found", false),
        SnapshotErrorCode::AccessDenied => (IpcErrorCategory::Snapshot, "access_denied", false),
        SnapshotErrorCode::SourceBusy => (IpcErrorCategory::Snapshot, "source_busy", true),
        SnapshotErrorCode::SourceChanged => (IpcErrorCategory::Snapshot, "source_changed", true),
        SnapshotErrorCode::EmptyInput => (IpcErrorCategory::Snapshot, "empty_input", false),
        SnapshotErrorCode::TooLarge => (IpcErrorCategory::Limit, "snapshot_too_large", false),
        SnapshotErrorCode::ResourceLimit => (IpcErrorCategory::Limit, "resource_limit", false),
        SnapshotErrorCode::IoFailure => (IpcErrorCategory::Snapshot, "io_failure", false),
        SnapshotErrorCode::RetryExhausted => (IpcErrorCategory::Snapshot, "retry_exhausted", true),
        SnapshotErrorCode::TimedOut => (IpcErrorCategory::Snapshot, "timed_out", true),
        SnapshotErrorCode::Cancelled => (IpcErrorCategory::Cancelled, "cancelled", false),
    };
    IpcError {
        category,
        code,
        retryable,
        attempts: Some(error.attempts),
        last_transient: error.last_transient.map(|transient| match transient {
            SnapshotTransientCode::SourceBusy => "source_busy",
            SnapshotTransientCode::SourceChanged => "source_changed",
        }),
    }
}

fn map_container_error(error: ContainerError) -> IpcError {
    let category = match error.kind() {
        ErrorKind::Resource => IpcErrorCategory::Resource,
        ErrorKind::Unsupported => IpcErrorCategory::Unsupported,
        ErrorKind::Corrupt => IpcErrorCategory::Corrupt,
        ErrorKind::Limit => IpcErrorCategory::Limit,
    };
    IpcError::simple(category, error.code(), false)
}

fn map_manual_error(error: ManualParseError) -> IpcError {
    let category = match error {
        ManualParseError::UnsupportedVersion | ManualParseError::UnsupportedKind => {
            IpcErrorCategory::Unsupported
        }
        ManualParseError::SlotIndex => IpcErrorCategory::Input,
        ManualParseError::SlotEmpty => IpcErrorCategory::Input,
        ManualParseError::PayloadLength
        | ManualParseError::SlotBounds
        | ManualParseError::BattleBounds
        | ManualParseError::UnitBounds => IpcErrorCategory::Corrupt,
        ManualParseError::DomainInvariant => IpcErrorCategory::Internal,
    };
    IpcError::simple(category, error.code(), false)
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use ivalice_domain::{
        ContainerMetadata, SaveProvenance, SnapshotProvenance, StoredAdlerStatus, ValueState,
    };

    fn test_root(label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let sequence = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ivalice-d006-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root)?;
        Ok(root)
    }

    fn state(root: &Path) -> Result<DesktopState, SettingsError> {
        Ok(DesktopState::new(
            SettingsStore::for_local_app_data(root)?,
            root.to_path_buf(),
        ))
    }

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    fn private_job_names(document: &serde_json::Value) -> Option<HashMap<u8, String>> {
        let columns = document.get("columns")?.as_array()?;
        let key_index = columns.iter().position(|column| {
            column.get("name").and_then(serde_json::Value::as_str) == Some("Key")
        })?;
        let name_index = columns.iter().position(|column| {
            column.get("name").and_then(serde_json::Value::as_str) == Some("Name")
        })?;
        let mut result = HashMap::new();
        for row in document.get("rows")?.as_array()? {
            let Some(values) = row.get("value").unwrap_or(row).as_array() else {
                continue;
            };
            let Some(key) = values
                .get(key_index)
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u8::try_from(value).ok())
            else {
                continue;
            };
            let Some(name) = values.get(name_index).and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !name.is_empty() {
                result.insert(key, name.to_owned());
            }
        }
        (!result.is_empty()).then_some(result)
    }

    fn private_expected_roster(observations: &serde_json::Value) -> Option<Vec<(String, u8)>> {
        if !observations.get("game_reload_succeeded")?.as_bool()? {
            return None;
        }
        let mut result = observations
            .get("active_roster")?
            .as_array()?
            .iter()
            .map(|row| {
                let job = row.get("current_job")?.as_str()?;
                let level = row
                    .get("level")?
                    .as_u64()
                    .and_then(|value| u8::try_from(value).ok())?;
                Some((job.to_owned(), level))
            })
            .collect::<Option<Vec<_>>>()?;
        result.sort_unstable();
        Some(result)
    }

    fn private_actual_roster(
        units: &[ivalice_save_format::UnitRecord],
        job_names: &HashMap<u8, String>,
    ) -> Option<Vec<(String, u8)>> {
        let mut result = units
            .iter()
            .enumerate()
            .filter(|(position, unit)| *position < 50 && unit.is_active(*position))
            .map(|(_, unit)| Some((job_names.get(&unit.job)?.clone(), unit.level)))
            .collect::<Option<Vec<_>>>()?;
        result.sort_unstable();
        Some(result)
    }

    fn synthetic_save(
        snapshot_length: usize,
    ) -> Result<NormalizedSave, Box<dyn std::error::Error>> {
        let byte_length = SnapshotByteLength::new(u64::try_from(snapshot_length)?)?;
        Ok(NormalizedSave::supported(
            SaveProvenance {
                snapshot: SnapshotProvenance { byte_length },
                writer_build: ValueState::Unknown,
            },
            ContainerMetadata {
                embedded_payload_version: 16,
                format_discriminator: 14,
                payload_byte_length: 2_008_216,
                stored_adler_status: StoredAdlerStatus::Matched,
            },
            ValueState::Known(ivalice_domain::ManualSlot {
                id: ManualSlotId::new(3)?,
            }),
        ))
    }

    #[tauri::command]
    fn test_unknown_contract() -> Result<LoadSaveResponse, IpcError> {
        Ok(LoadSaveResponse {
            request_id: 77,
            occupied_manual_slots: vec![ManualSlotId::new(3).map_err(|_| worker_error())?],
            save: Some(synthetic_save(512).map_err(|_| worker_error())?),
        })
    }

    #[test]
    fn response_serialization_matches_the_typescript_contract(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let response = LoadSaveResponse {
            request_id: 42,
            occupied_manual_slots: vec![ManualSlotId::new(3)?],
            save: Some(synthetic_save(512)?),
        };
        let value = serde_json::to_value(response)?;
        assert_eq!(value["requestId"], 42);
        assert_eq!(value["save"]["schema"], "v6");
        assert_eq!(value["save"]["selected_manual_slot"]["value"]["id"], 3);
        assert_eq!(value["occupiedManualSlots"][0], 3);
        assert_eq!(
            value["save"]["provenance"]["writer_build"]["state"],
            "unknown"
        );

        let error = map_snapshot_error(SnapshotError {
            code: SnapshotErrorCode::RetryExhausted,
            attempts: 3,
            os_code: Some(32),
            last_transient: Some(SnapshotTransientCode::SourceBusy),
        });
        let error_value = serde_json::to_value(error)?;
        assert_eq!(error_value["category"], "snapshot");
        assert_eq!(error_value["code"], "retry_exhausted");
        assert_eq!(error_value["attempts"], 3);
        assert_eq!(error_value["last_transient"], "source_busy");
        assert!(error_value.get("os_code").is_none());
        Ok(())
    }

    #[test]
    fn synthetic_snapshot_success_preserves_source_and_unknown_values(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("success")?;
        let source = root.join("enhanced.png");
        let original = b"original synthetic D006 snapshot".to_vec();
        fs::write(&source, &original)?;
        let state = state(&root)?;
        let generation = state.begin_load();
        let request = LoadSaveRequest {
            request_id: 9,
            manual_slot_id: Some(3),
        };
        let response =
            load_snapshot_with(&state, generation, &source, request, |snapshot, slot| {
                assert_eq!(slot, Some(3));
                Ok((
                    vec![ManualSlotId::new(3).map_err(|_| worker_error())?],
                    Some(synthetic_save(snapshot.len()).map_err(|_| worker_error())?),
                ))
            })?;

        assert_eq!(response.request_id, 9);
        assert!(response.save.is_some());
        assert_eq!(fs::read(&source)?, original);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn explicit_selection_is_validated_persisted_and_never_changes_the_source(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("selection")?;
        let source = root.join("enhanced.png");
        let original = b"selected synthetic source";
        fs::write(&source, original)?;
        let state = state(&root)?;

        let selected = set_selection(&state, source.to_string_lossy().into_owned())?;
        assert_eq!(selected, SaveSelectionStatus::Selected);
        assert_eq!(selection_status(&state)?, SaveSelectionStatus::Selected);
        assert_eq!(fs::read(&source)?, original);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn production_load_reports_missing_resource_and_removed_source_without_writes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let settings_root = test_root("native-errors-settings")?;
        let source_root = test_root("native-errors-source")?;
        let source = source_root.join("enhanced.png");
        let original = b"synthetic invalid container".to_vec();
        fs::write(&source, &original)?;
        let state = state(&settings_root)?;
        assert_eq!(
            set_selection(&state, source.to_string_lossy().into_owned())?,
            SaveSelectionStatus::Selected
        );

        let missing_resource = load_selected(
            &state,
            state.begin_load(),
            LoadSaveRequest {
                request_id: 10,
                manual_slot_id: None,
            },
        )
        .err()
        .unwrap_or_else(worker_error);
        assert_eq!(missing_resource.category, IpcErrorCategory::Resource);
        assert_eq!(missing_resource.code, "resource_missing");
        assert_eq!(fs::read(&source)?, original);

        fs::remove_file(&source)?;
        let removed_source = load_selected(
            &state,
            state.begin_load(),
            LoadSaveRequest {
                request_id: 11,
                manual_slot_id: None,
            },
        )
        .err()
        .unwrap_or_else(worker_error);
        assert_eq!(removed_source.category, IpcErrorCategory::Selection);
        assert_eq!(removed_source.code, "not_found");

        fs::remove_dir_all(source_root)?;
        fs::remove_dir_all(settings_root)?;
        Ok(())
    }

    #[test]
    #[ignore = "requires ignored R006 private input, observation, catalogue, and dictionary"]
    fn private_production_pipeline_matches_independent_roster_without_writes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workspace = workspace_root();
        let inputs = workspace.join(".local").join("research-inputs");
        let source = inputs.join("r006-new").join("enhanced.png");
        let dictionary = inputs
            .join("resources")
            .join("ticsaveeditor-07ea857")
            .join("CompressDict.bin");
        let observations = inputs.join("r006-new-observations.json");
        let catalogue = workspace
            .join(".local")
            .join("project-reuse-review")
            .join("upstream")
            .join("ticsaveeditor")
            .join("TICSaveEditor.Core")
            .join("Resources")
            .join("Nex")
            .join("en")
            .join("Job.json");
        let source_before = fs::read(&source)?;
        let dictionary_before = fs::read(&dictionary)?;
        let observations_before = fs::read(&observations)?;
        let catalogue_before = fs::read(&catalogue)?;
        assert!(!source_before.is_empty(), "private source unavailable");
        assert!(
            !dictionary_before.is_empty(),
            "private dictionary unavailable"
        );

        let root = test_root("private-native-pipeline")?;
        let resource_directory = root.join(DICTIONARY_DIRECTORY);
        fs::create_dir(&resource_directory)?;
        fs::write(
            resource_directory.join(DICTIONARY_FILENAME),
            &dictionary_before,
        )?;
        let state = state(&root)?;
        assert_eq!(
            set_selection(&state, source.to_string_lossy().into_owned())?,
            SaveSelectionStatus::Selected
        );

        let slots = load_selected(
            &state,
            state.begin_load(),
            LoadSaveRequest {
                request_id: 20,
                manual_slot_id: None,
            },
        )?;
        assert!(slots.save.is_none());
        assert!(slots
            .occupied_manual_slots
            .iter()
            .any(|slot| slot.index() == 7));

        let selected = load_selected(
            &state,
            state.begin_load(),
            LoadSaveRequest {
                request_id: 21,
                manual_slot_id: Some(7),
            },
        )?;
        let observation_json: serde_json::Value = serde_json::from_slice(&observations_before)?;
        let catalogue_json: serde_json::Value = serde_json::from_slice(&catalogue_before)?;
        let job_names = private_job_names(&catalogue_json).ok_or("private catalogue invalid")?;
        let expected =
            private_expected_roster(&observation_json).ok_or("private observation invalid")?;
        assert!(
            matches!(selected.save.as_ref().map(|save| &save.selected_manual_slot),
            Some(ValueState::Known(slot)) if slot.id.index() == 7)
        );
        let decoded = decode_enhanced_png(&source_before, Some(&dictionary_before))?;
        let units = decoded.unit_records(7)?.ok_or("private slot absent")?;
        assert_eq!(private_actual_roster(&units, &job_names), Some(expected));

        assert_eq!(fs::read(&source)?, source_before);
        assert_eq!(fs::read(&dictionary)?, dictionary_before);
        assert_eq!(fs::read(&observations)?, observations_before);
        assert_eq!(fs::read(&catalogue)?, catalogue_before);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn invalid_unsupported_corrupt_and_access_failures_are_structured() {
        let invalid = validate_selection_text("")
            .err()
            .unwrap_or_else(worker_error);
        assert_eq!(invalid.category, IpcErrorCategory::Input);
        assert_eq!(invalid.code, "invalid_selection_path");

        let unsupported = map_container_error(ContainerError::UnsupportedVersion);
        assert_eq!(unsupported.category, IpcErrorCategory::Unsupported);
        assert_eq!(unsupported.code, "unsupported_version");

        let corrupt = map_container_error(ContainerError::PngChunkChecksum);
        assert_eq!(corrupt.category, IpcErrorCategory::Corrupt);
        assert_eq!(corrupt.code, "png_chunk_checksum");

        let access = map_snapshot_error(SnapshotError {
            code: SnapshotErrorCode::AccessDenied,
            attempts: 1,
            os_code: Some(5),
            last_transient: None,
        });
        assert_eq!(access.category, IpcErrorCategory::Snapshot);
        assert_eq!(access.code, "access_denied");
        assert_eq!(access.attempts, Some(1));

        let unavailable_settings = DesktopState::with_settings(
            Err(SettingsError::KnownFolderUnavailable),
            PathBuf::from("unused-test-resource-root"),
        );
        let settings_error = selection_status(&unavailable_settings)
            .err()
            .unwrap_or_else(worker_error);
        assert_eq!(settings_error.category, IpcErrorCategory::Settings);
        assert_eq!(settings_error.code, "known_folder_unavailable");
    }

    #[test]
    fn newer_load_generation_discards_a_stale_result() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("stale")?;
        let source = root.join("enhanced.png");
        fs::write(&source, b"stale synthetic snapshot")?;
        let state = state(&root)?;
        let stale_generation = state.begin_load();
        let _current_generation = state.begin_load();
        let result = load_snapshot_with(
            &state,
            stale_generation,
            &source,
            LoadSaveRequest {
                request_id: 1,
                manual_slot_id: None,
            },
            |_, _| Ok((Vec::new(), None)),
        );
        assert_eq!(result, Err(IpcError::stale()));
        assert_eq!(fs::read(&source)?, b"stale synthetic snapshot");
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn actual_ipc_transports_success_unknown_values_and_structured_failure(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("ipc")?;
        let app = tauri::test::mock_builder()
            .manage(state(&root)?)
            .invoke_handler(tauri::generate_handler![
                get_save_selection,
                load_reader,
                test_unknown_contract
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build()?;
        let selection = tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "get_save_selection".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "http://tauri.localhost".parse()?,
                body: tauri::ipc::InvokeBody::default(),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .map_err(|error| std::io::Error::other(error.to_string()))?;
        let selection = selection.deserialize::<serde_json::Value>()?;
        assert_eq!(selection["state"], "no_selection");

        let success = tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "test_unknown_contract".into(),
                callback: tauri::ipc::CallbackFn(2),
                error: tauri::ipc::CallbackFn(3),
                url: "http://tauri.localhost".parse()?,
                body: tauri::ipc::InvokeBody::default(),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .map_err(|error| std::io::Error::other(error.to_string()))?;
        let success = success.deserialize::<serde_json::Value>()?;
        assert_eq!(success["requestId"], 77);
        assert_eq!(
            success["save"]["provenance"]["writer_build"]["state"],
            "unknown"
        );

        let failure = tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "load_reader".into(),
                callback: tauri::ipc::CallbackFn(4),
                error: tauri::ipc::CallbackFn(5),
                url: "http://tauri.localhost".parse()?,
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({
                    "request": { "requestId": 78, "manualSlotId": null }
                })),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .err()
        .ok_or("load without a selection unexpectedly passed")?;
        assert_eq!(failure["category"], "selection");
        assert_eq!(failure["code"], "no_selection");
        drop(webview);
        drop(app);
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
