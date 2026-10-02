//! In-memory character initialization; never replaces a save or creates a backup.
use super::*;
use ivalice_domain::ValueState;
use ivalice_save_format::{edit_enhanced_png, EditOperation};

#[tauri::command]
pub async fn preview_character(
    request: serde_json::Value,
    state: State<'_, DesktopState>,
) -> Result<reader::LoadReaderResponse, IpcError> {
    let (request, operations) = save_edit::parse_transaction_request(request)?;
    if operations.is_empty()
        || operations.iter().any(|operation| {
            !matches!(
                operation,
                EditOperation::CreateGenericFromSlot { .. }
                    | EditOperation::AddNamedFromUnit { .. }
                    | EditOperation::CreateCreature { .. }
            )
        })
    {
        return Err(IpcError::simple(
            IpcErrorCategory::Input,
            "invalid_character_preview",
            false,
        ));
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _serial = lock_recover(&state.load_serial);
        ensure_current(&state, request.snapshot_generation)?;
        let loaded = lock_recover(&state.loaded_edit)
            .clone()
            .ok_or_else(IpcError::stale)?;
        if loaded.generation != request.snapshot_generation || loaded.slot != request.manual_slot_id
        {
            return Err(IpcError::stale());
        }
        let settings = state
            .settings()?
            .load()
            .map_err(map_settings_error)?
            .ok_or_else(no_selection)?;
        if selected_candidate(&state, &settings)? != loaded.path {
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
        let dictionary = read_dictionary(&state.dictionary_path())?;
        let initialized =
            edit_enhanced_png(snapshot.bytes(), &dictionary, loaded.slot, &operations)
                .map_err(save_edit::map_gil_edit_error)?;
        let decoded =
            decode_enhanced_png(&initialized, Some(&dictionary)).map_err(map_container_error)?;
        let projected = reader::project_selected_reader(
            &state,
            loaded.generation,
            reader::LoadReaderRequest {
                request_id: loaded.generation,
                manual_slot_id: Some(loaded.slot),
            },
            &snapshot,
            &loaded.path,
            &decoded,
        );
        // Bind job previews to the initialized units while retaining the original file hash.
        // A failed projection must leave the previous edit session intact.
        match projected {
            Ok(response)
                if response.reader.as_ref().is_some_and(|document| {
                    matches!(&document.identity.resource_token, ValueState::Known(token)
                    if Some(token) == loaded.catalogue_token.as_ref())
                }) && response.edit_context.is_some() =>
            {
                Ok(response)
            }
            result => {
                *lock_recover(&state.loaded_edit) = Some(loaded);
                match result {
                    Err(error) => Err(error),
                    Ok(response) => Err(response.reader_error.unwrap_or_else(IpcError::stale)),
                }
            }
        }
    })
    .await
    .map_err(|_| worker_error())?
}
