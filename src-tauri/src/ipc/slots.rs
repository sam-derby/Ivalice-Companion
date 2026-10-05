//! Whole-slot operations on the loaded save, export to a new file and
//! read-only inspection of another save for import.

use super::save_edit::{map_gil_edit_error, map_save_edit_error};
use super::*;
use ivalice_infrastructure::{replace_save_with_backup_if_unchanged, write_new_save_file};
use ivalice_save_format::{apply_slot_operation, export_slot, SlotOperation};

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum RequestedSlotOperation {
    Copy {
        from: u8,
        to: u8,
        replace: bool,
    },
    Move {
        from: u8,
        to: u8,
    },
    Swap {
        first: u8,
        second: u8,
    },
    Delete {
        slot: u8,
    },
    Import {
        source_path: String,
        source_slot: u8,
        slot: u8,
        replace: bool,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SlotOperationRequest {
    snapshot_generation: u64,
    operation: RequestedSlotOperation,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportSlotRequest {
    snapshot_generation: u64,
    slot: u8,
    path: String,
    overwrite: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectedSave {
    pub slots: Vec<InspectedSlot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectedSlot {
    pub slot: u8,
    pub title: Option<String>,
    pub saved_at_unix_seconds: Option<i64>,
}

fn invalid(code: &'static str) -> IpcError {
    IpcError::simple(IpcErrorCategory::Input, code, false)
}

fn slot_index(slot: u8) -> Result<u8, IpcError> {
    if slot < 50 {
        Ok(slot)
    } else {
        Err(invalid("manual_slot_out_of_range"))
    }
}

fn parse_operation_request(value: serde_json::Value) -> Result<SlotOperationRequest, IpcError> {
    let request: SlotOperationRequest =
        serde_json::from_value(value).map_err(|_| invalid("invalid_slot_request"))?;
    if !(1..=MAX_SAFE_GENERATION).contains(&request.snapshot_generation) {
        return Err(invalid("invalid_slot_request"));
    }
    match &request.operation {
        RequestedSlotOperation::Copy { from, to, .. }
        | RequestedSlotOperation::Move { from, to } => {
            slot_index(*from)?;
            slot_index(*to)?;
        }
        RequestedSlotOperation::Swap { first, second } => {
            slot_index(*first)?;
            slot_index(*second)?;
        }
        RequestedSlotOperation::Delete { slot } => {
            slot_index(*slot)?;
        }
        RequestedSlotOperation::Import {
            source_path,
            source_slot,
            slot,
            ..
        } => {
            validate_selection_text(source_path)?;
            slot_index(*source_slot)?;
            slot_index(*slot)?;
        }
    }
    Ok(request)
}

/// The loaded file, re-read and checked unchanged since load.
fn loaded_snapshot(
    state: &DesktopState,
    generation: u64,
) -> Result<(LoadedEdit, ivalice_infrastructure::Snapshot), IpcError> {
    ensure_current(state, generation)?;
    let loaded = lock_recover(&state.loaded_edit)
        .clone()
        .ok_or_else(IpcError::stale)?;
    if loaded.generation != generation {
        return Err(IpcError::stale());
    }
    let settings = state
        .settings()?
        .load()
        .map_err(map_settings_error)?
        .ok_or_else(no_selection)?;
    if selected_candidate(state, &settings)? != loaded.path {
        return Err(IpcError::stale());
    }
    let snapshot = state
        .reader
        .acquire(&loaded.path, &CancellationToken::default())
        .map_err(map_snapshot_error)?;
    if snapshot.sha256() != loaded.sha256 {
        return Err(IpcError::simple(
            IpcErrorCategory::Snapshot,
            "save_changed_since_load",
            false,
        ));
    }
    Ok((loaded, snapshot))
}

/// Another save file, validated as a candidate and decoded read-only.
fn other_save(
    state: &DesktopState,
    path: &str,
) -> Result<ivalice_save_format::DecodedContainer, IpcError> {
    validate_selection_text(path)?;
    let path = PathBuf::from(path);
    NativeCandidateValidator::new(Some(state.settings()?.root().to_path_buf()))
        .validate(&path)
        .map_err(map_candidate_error)?;
    let snapshot = SnapshotReader::new()
        .acquire(&path, &CancellationToken::default())
        .map_err(map_snapshot_error)?;
    let dictionary = read_dictionary(&state.dictionary_path())?;
    decode_enhanced_png(snapshot.bytes(), Some(&dictionary)).map_err(map_container_error)
}

fn slot_operation_for(
    state: &DesktopState,
    operation: &RequestedSlotOperation,
) -> Result<SlotOperation, IpcError> {
    Ok(match operation {
        RequestedSlotOperation::Copy { from, to, replace } => SlotOperation::Copy {
            from: *from,
            to: *to,
            replace: *replace,
        },
        RequestedSlotOperation::Move { from, to } => SlotOperation::Move {
            from: *from,
            to: *to,
        },
        RequestedSlotOperation::Swap { first, second } => SlotOperation::Swap {
            first: *first,
            second: *second,
        },
        RequestedSlotOperation::Delete { slot } => SlotOperation::Delete { slot: *slot },
        RequestedSlotOperation::Import {
            source_path,
            source_slot,
            slot,
            replace,
        } => SlotOperation::Insert {
            slot: *slot,
            record: other_save(state, source_path)?
                .slot_record(*source_slot)
                .map_err(map_gil_edit_error)?
                .ok_or_else(|| invalid("import_slot_empty"))?,
            replace: *replace,
        },
    })
}

#[tauri::command]
pub async fn slot_operation(
    request: serde_json::Value,
    state: State<'_, DesktopState>,
) -> Result<(), IpcError> {
    let request = parse_operation_request(request)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || apply_requested(&state, &request))
        .await
        .map_err(|_| worker_error())?
}

fn apply_requested(state: &DesktopState, request: &SlotOperationRequest) -> Result<(), IpcError> {
    let _serial = lock_recover(&state.load_serial);
    let (loaded, snapshot) = loaded_snapshot(state, request.snapshot_generation)?;
    let operation = slot_operation_for(state, &request.operation)?;
    let dictionary = read_dictionary(&state.dictionary_path())?;
    let replacement = apply_slot_operation(snapshot.bytes(), &dictionary, &operation)
        .map_err(map_gil_edit_error)?;
    ensure_current(state, request.snapshot_generation)?;
    let backup = replace_save_with_backup_if_unchanged(
        &loaded.path,
        loaded.sha256,
        snapshot.bytes(),
        &replacement,
    )
    .map_err(map_save_edit_error)?;
    *lock_recover(&state.last_backup) = Some(BackupReceipt {
        path: loaded.path,
        slot: loaded.slot,
        edited_sha256: backup.edited_sha256(),
        backup,
    });
    state.cancel_load();
    Ok(())
}

fn same_file(first: &Path, second: &Path) -> bool {
    let lexical = |path: &Path| path.to_string_lossy().to_lowercase();
    lexical(first) == lexical(second)
        || std::fs::canonicalize(first)
            .ok()
            .zip(std::fs::canonicalize(second).ok())
            .is_some_and(|(first, second)| first == second)
}

fn backup_of(path: &Path) -> Option<PathBuf> {
    let mut name = path.file_stem()?.to_os_string();
    name.push(" - backup");
    if let Some(extension) = path.extension() {
        name.push(".");
        name.push(extension);
    }
    Some(path.with_file_name(name))
}

#[tauri::command]
pub async fn export_save_slot(
    request: serde_json::Value,
    state: State<'_, DesktopState>,
) -> Result<(), IpcError> {
    let request: ExportSlotRequest =
        serde_json::from_value(request).map_err(|_| invalid("invalid_export_request"))?;
    if !(1..=MAX_SAFE_GENERATION).contains(&request.snapshot_generation) {
        return Err(invalid("invalid_export_request"));
    }
    slot_index(request.slot)?;
    validate_selection_text(&request.path)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || export_requested(&state, &request))
        .await
        .map_err(|_| worker_error())?
}

fn export_requested(state: &DesktopState, request: &ExportSlotRequest) -> Result<(), IpcError> {
    let target = PathBuf::from(&request.path);
    if !target
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        return Err(invalid("export_path_not_png"));
    }
    let _serial = lock_recover(&state.load_serial);
    let (loaded, snapshot) = loaded_snapshot(state, request.snapshot_generation)?;
    // Never let an export replace the loaded save or its backup.
    if same_file(&target, &loaded.path)
        || backup_of(&loaded.path).is_some_and(|backup| same_file(&target, &backup))
        || inside_settings(state, &target)?
    {
        return Err(invalid("export_path_protected"));
    }
    let dictionary = read_dictionary(&state.dictionary_path())?;
    let exported =
        export_slot(snapshot.bytes(), &dictionary, request.slot).map_err(map_gil_edit_error)?;
    write_new_save_file(&target, &exported, request.overwrite).map_err(map_save_edit_error)
}

/// Exports must not land inside the app's own settings directory.
fn inside_settings(state: &DesktopState, target: &Path) -> Result<bool, IpcError> {
    let root = state.settings()?.root().to_path_buf();
    let lexical = target
        .to_string_lossy()
        .to_lowercase()
        .starts_with(&root.to_string_lossy().to_lowercase());
    Ok(lexical)
}

#[tauri::command]
pub async fn inspect_save_file(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<InspectedSave, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || inspect(&state, &path))
        .await
        .map_err(|_| worker_error())?
}

fn inspect(state: &DesktopState, path: &str) -> Result<InspectedSave, IpcError> {
    let decoded = other_save(state, path)?;
    let slots = decoded
        .occupied_manual_slots()
        .map_err(map_manual_error)?
        .iter()
        .map(|slot| {
            let metadata = decoded
                .slot_metadata(slot.index())
                .map_err(map_manual_error)?
                .ok_or_else(|| {
                    IpcError::simple(IpcErrorCategory::Internal, "occupied_slot_missing", false)
                })?;
            Ok(InspectedSlot {
                slot: slot.index(),
                title: metadata.readable_title(),
                saved_at_unix_seconds: (metadata.saved_at_unix_seconds > 0)
                    .then(|| i64::from(metadata.saved_at_unix_seconds)),
            })
        })
        .collect::<Result<Vec<_>, IpcError>>()?;
    Ok(InspectedSave { slots })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn slot_requests_accept_only_bounded_known_operations() {
        for operation in [
            json!({"kind": "copy", "from": 0, "to": 5, "replace": false}),
            json!({"kind": "move", "from": 0, "to": 49}),
            json!({"kind": "swap", "first": 1, "second": 2}),
            json!({"kind": "delete", "slot": 3}),
            json!({"kind": "import", "sourcePath": "C:\\saves\\other.png", "sourceSlot": 0, "slot": 4, "replace": true}),
        ] {
            assert!(
                parse_operation_request(
                    json!({"snapshotGeneration": 1, "operation": operation.clone()})
                )
                .is_ok(),
                "{operation}"
            );
        }
        for request in [
            json!({"snapshotGeneration": 0, "operation": {"kind": "delete", "slot": 3}}),
            json!({"snapshotGeneration": 1, "operation": {"kind": "delete", "slot": 50}}),
            json!({"snapshotGeneration": 1, "operation": {"kind": "copy", "from": 50, "to": 1, "replace": false}}),
            json!({"snapshotGeneration": 1, "operation": {"kind": "move", "from": 1}}),
            json!({"snapshotGeneration": 1, "operation": {"kind": "format", "slot": 1}}),
            json!({"snapshotGeneration": 1, "operation": {"kind": "import", "sourcePath": "", "sourceSlot": 0, "slot": 1, "replace": false}}),
            json!({"snapshotGeneration": 1, "operation": {"kind": "delete", "slot": 1}, "path": "x"}),
        ] {
            assert!(
                parse_operation_request(request.clone()).is_err(),
                "{request}"
            );
        }
    }

    #[test]
    fn exports_never_target_the_loaded_save_or_its_backup() {
        let loaded = Path::new("C:\\Saves\\enhanced.png");
        assert!(same_file(Path::new("c:\\saves\\ENHANCED.png"), loaded));
        assert_eq!(
            backup_of(loaded),
            Some(PathBuf::from("C:\\Saves\\enhanced - backup.png"))
        );
        assert!(!same_file(Path::new("C:\\Saves\\export.png"), loaded));
    }
}
