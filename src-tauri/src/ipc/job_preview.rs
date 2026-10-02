//! Read-only eligibility projection from the immutable loaded records.
use super::*;
use ivalice_domain::job_eligibility::ValidatedJobRequirements;
use ivalice_infrastructure::JobRequirementsLoader;
use ivalice_save_format::UnitRecord;
use reader::{EditJobOptions, LockedJob, RequiredJobLevel};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreviewRequest {
    snapshot_generation: u64,
    manual_slot_id: u8,
    unit_position: u8,
    levels: Vec<ProposedLevel>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProposedLevel {
    job_slot: u8,
    level: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewResponse {
    job_options: EditJobOptions,
    invalidated_slots: Vec<u8>,
}

pub(super) fn job_options(
    position: u8,
    unit: &UnitRecord,
    levels: &[u8; 24],
    graph: &ValidatedJobRequirements,
) -> Option<EditJobOptions> {
    let unlocked = graph.saved_unlocks(unit.unlocked_jobs)?;
    let paths = graph.reachable_paths(levels);
    Some(EditJobOptions {
        unit_position: position,
        job_slots: (0..20)
            .filter(|index| paths[*index] || unlocked[*index])
            .filter_map(|index| u8::try_from(index).ok())
            .collect(),
        locked_jobs: graph
            .requirements()
            .iter()
            .filter(|row| !paths[usize::from(row.slot)] && !unlocked[usize::from(row.slot)])
            .map(|row| LockedJob {
                job_slot: row.slot,
                requires: row
                    .requires
                    .iter()
                    .map(|required| RequiredJobLevel {
                        job_slot: required.slot,
                        level: required.level,
                        current_level: levels[usize::from(required.slot)],
                    })
                    .collect(),
            })
            .collect(),
    })
}

#[tauri::command]
pub fn preview_job_progress(
    request: serde_json::Value,
    state: State<'_, DesktopState>,
) -> Result<PreviewResponse, IpcError> {
    let request: PreviewRequest = serde_json::from_value(request).map_err(|_| invalid_preview())?;
    ensure_current(&state, request.snapshot_generation)?;
    let loaded = lock_recover(&state.loaded_edit)
        .clone()
        .ok_or_else(IpcError::stale)?;
    if loaded.generation != request.snapshot_generation || loaded.slot != request.manual_slot_id {
        return Err(IpcError::stale());
    }
    let unit = loaded
        .units
        .get(usize::from(request.unit_position))
        .ok_or_else(invalid_preview)?;
    if !unit.is_active(usize::from(request.unit_position))
        || request.unit_position >= 50
        || (0x5e..=0x8d).contains(&unit.job)
        || request.levels.len() > 20
    {
        return Err(invalid_preview());
    }
    let mut levels = unit.job_levels;
    let mut seen = [false; 20];
    for row in request.levels {
        let index = usize::from(row.job_slot);
        if index >= 20 || row.level > 8 || seen[index] {
            return Err(invalid_preview());
        }
        seen[index] = true;
        levels[index] = row.level;
    }
    let graph = JobRequirementsLoader::load(&state.resource_root).map_err(|_| invalid_preview())?;
    Ok(PreviewResponse {
        job_options: job_options(request.unit_position, unit, &levels, &graph)
            .ok_or_else(invalid_preview)?,
        invalidated_slots: graph.invalidated_paths(&unit.job_levels, &levels),
    })
}

fn invalid_preview() -> IpcError {
    IpcError::simple(IpcErrorCategory::Input, "invalid_job_preview", false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_request_is_explicit_and_response_uses_camel_case() -> Result<(), serde_json::Error> {
        assert!(serde_json::from_value::<PreviewRequest>(serde_json::json!({
            "snapshotGeneration": 1, "manualSlotId": 35, "unitPosition": 0,
            "levels": [{"jobSlot": 7, "level": 3}], "path": "unexpected"
        }))
        .is_err());
        let response = PreviewResponse {
            job_options: EditJobOptions {
                unit_position: 0,
                job_slots: vec![7, 8],
                locked_jobs: vec![],
            },
            invalidated_slots: vec![8],
        };
        let value = serde_json::to_value(response)?;
        assert_eq!(value["invalidatedSlots"], serde_json::json!([8]));
        assert_eq!(value["jobOptions"]["jobSlots"], serde_json::json!([7, 8]));
        Ok(())
    }
}
