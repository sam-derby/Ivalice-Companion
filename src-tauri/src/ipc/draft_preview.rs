//! Read-only projection of a complete transaction. No edit-session rebinding.
use super::*;
use ivalice_domain::{
    reader::{
        stat_edit::{solve_unit, BaseStatKind},
        ReaderDocument, ValidatedReader,
    },
    ValueState,
};
use ivalice_save_format::EditOperation;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DraftPreviewRequest {
    transaction: serde_json::Value,
    solve: Option<VisibleStatRequest>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VisibleStatRequest {
    unit_position: u8,
    stat: BaseStatKind,
    value: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftPreviewResponse {
    reader: ReaderDocument,
    solved_base: Option<u32>,
}

#[tauri::command]
pub async fn preview_draft(
    request: serde_json::Value,
    state: State<'_, DesktopState>,
) -> Result<DraftPreviewResponse, IpcError> {
    let request: DraftPreviewRequest = serde_json::from_value(request).map_err(|_| invalid())?;
    let (transaction, operations) = save_edit::parse_transaction_request(request.transaction)?;
    if request.solve.is_some_and(|solve| {
        solve.unit_position >= 50 || solve.value == 0 || solve.value > solve.stat.display_limit()
    }) {
        return Err(invalid());
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        project_draft(
            &state,
            transaction.snapshot_generation,
            transaction.manual_slot_id,
            operations,
            request.solve,
        )
    })
    .await
    .map_err(|_| worker_error())?
}

fn invalid() -> IpcError {
    IpcError::simple(IpcErrorCategory::Input, "invalid_stat_request", false)
}

fn project_draft(
    state: &DesktopState,
    generation: u64,
    slot: u8,
    mut operations: Vec<EditOperation>,
    solve: Option<VisibleStatRequest>,
) -> Result<DraftPreviewResponse, IpcError> {
    let _serial = lock_recover(&state.load_serial);
    ensure_current(state, generation)?;
    let loaded = lock_recover(&state.loaded_edit)
        .clone()
        .ok_or_else(IpcError::stale)?;
    if loaded.generation != generation || loaded.slot != slot {
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
    let decoded = save_edit::prepare_preview(state, &loaded, snapshot.bytes(), &operations)?;
    let mut solved_base = None;
    let projected = reader::with_catalogue(state, generation, slot, |resource, identity| {
        let token = reader::token(resource.sha256());
        if loaded.catalogue_token.as_ref() != Some(&token) {
            return Err(IpcError::stale());
        }
        let story = ivalice_infrastructure::StoryProgressLoader::load(&state.resource_root).ok();
        let project =
            |decoded: &ivalice_save_format::DecodedContainer| -> Result<ReaderDocument, IpcError> {
                let reader = decoded
                    .reader_manual_save_v2(identity.clone(), |id, ceiling| {
                        resource.lookup(id, ceiling)
                    })
                    .map_err(map_manual_error)?;
                let mut document = reader.document().clone();
                ivalice_domain::reader::stats::apply_effective(&mut document, resource.mechanics());
                ivalice_domain::reader::commands::apply_primary_commands(&mut document, |job| {
                    resource.command_for_job(job)
                });
                if let Some(story) = &story {
                    story.apply(&mut document.progress);
                }
                Ok(document)
            };
        let mut document = project(&decoded)?;
        if let Some(solve) = solve {
            let ValueState::Known(units) = &document.roster.value else {
                return Err(invalid());
            };
            let unit = units
                .iter()
                .find(|unit| unit.key == u16::from(solve.unit_position))
                .ok_or_else(invalid)?;
            let mechanics = resource.mechanics().ok_or_else(invalid)?;
            let base =
                solve_unit(unit, mechanics, solve.stat, solve.value).map_err(|_| invalid())?;
            operations.retain(|operation| !matches!(operation, EditOperation::BaseStat { unit_position, stat, .. } if *unit_position == solve.unit_position && *stat == solve.stat));
            operations.push(EditOperation::BaseStat {
                unit_position: solve.unit_position,
                stat: solve.stat,
                value: base,
            });
            let decoded =
                save_edit::prepare_preview(state, &loaded, snapshot.bytes(), &operations)?;
            document = project(&decoded)?;
            solved_base = Some(base);
        }
        ValidatedReader::validate(document).map_err(|_| worker_error())
    })??;
    Ok(DraftPreviewResponse {
        reader: projected,
        solved_base,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    #[ignore = "requires ignored example save and staged private runtime resources"]
    fn private_preview_and_write_share_exact_stats_without_rebinding(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("workspace")?;
        let original = fs::read(workspace.join(".local/saves/example/enhanced.png"))?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = workspace
            .join(".local/stat-edit-check")
            .join(format!("{}-{stamp}", std::process::id()));
        fs::create_dir_all(root.join("resources"))?;
        fs::write(root.join("enhanced.png"), &original)?;
        for (source, filename) in [
            (
                ".local/research-inputs/resources/ticsaveeditor-07ea857/CompressDict.bin",
                "CompressDict.bin",
            ),
            (
                ".local/reader-catalogue/reader-catalogue-with-growth-v3.json",
                "reader-catalogue-v1.json",
            ),
            (
                ".local/job-eligibility/job-requirements-v1.json",
                "job-requirements-v1.json",
            ),
            (
                ".local/ability-flags/ability-flags-v1.json",
                "ability-flags-v1.json",
            ),
        ] {
            fs::copy(
                workspace.join(source),
                root.join("resources").join(filename),
            )?;
        }
        let state = DesktopState::new(SettingsStore::for_local_app_data(&root)?, root.clone());
        set_selection(
            &state,
            root.join("enhanced.png").to_string_lossy().into_owned(),
        )?;
        let dictionary = read_dictionary(&state.dictionary_path())?;
        let decoded = decode_enhanced_png(&original, Some(&dictionary))?;
        let slot = decoded
            .occupied_manual_slots()?
            .last()
            .ok_or("slot")?
            .index();
        let generation = state.begin_load();
        let snapshot = state
            .reader
            .acquire(&root.join("enhanced.png"), &CancellationToken::default())?;
        let response = reader::project_selected_reader(
            &state,
            generation,
            reader::LoadReaderRequest {
                request_id: generation,
                manual_slot_id: Some(slot),
            },
            &snapshot,
            &root.join("enhanced.png"),
            &decoded,
        )?;
        let document = response.reader.ok_or("reader")?;
        let ValueState::Known(units) = &document.roster.value else {
            return Err("roster".into());
        };
        let unit = units
            .iter()
            .find(|unit| {
                unit.key < 50
                    && matches!(unit.effective.hp.value, ValueState::Known(value) if value < 999)
            })
            .ok_or("editable unit")?;
        let ValueState::Known(hp) = unit.effective.hp.value else {
            return Err("HP".into());
        };
        let position = u8::try_from(unit.key)?;
        let before_session = lock_recover(&state.loaded_edit)
            .clone()
            .ok_or("edit session")?;
        let detail = unit
            .effective
            .breakdown
            .get(&BaseStatKind::Hp)
            .ok_or("HP inputs")?;
        let (
            ValueState::Known(previous),
            ValueState::Known(base_hp),
            ValueState::Known(multiplier),
            ValueState::Known(bonus),
        ) = (
            &unit.stored.bases.hp.value,
            &detail.base.value,
            &detail.job_multiplier.value,
            &detail.equipment_bonus.value,
        )
        else {
            return Err("HP mechanics".into());
        };
        let started = std::time::Instant::now();
        let arithmetic = ivalice_domain::reader::stat_edit::preview_base(
            *previous,
            Some(base_hp + 1),
            *multiplier,
            Some(*bonus),
            BaseStatKind::Hp,
        )
        .map_err(|error| format!("{error:?}"))?;
        eprintln!("Base-stat arithmetic: {} ms", started.elapsed().as_millis());
        let base = arithmetic.stored_base;
        let projected = project_draft(
            &state,
            generation,
            slot,
            vec![EditOperation::BaseStat {
                unit_position: position,
                stat: BaseStatKind::Hp,
                value: base,
            }],
            None,
        )?;
        let ValueState::Known(projected_units) = &projected.reader.roster.value else {
            return Err("projected roster".into());
        };
        let projected_unit = projected_units
            .iter()
            .find(|unit| unit.key == u16::from(position))
            .ok_or("projected unit")?;
        assert_eq!(projected_unit.effective.hp.value, ValueState::Known(hp + 1));
        assert_eq!(projected_unit.effective.hp, arithmetic.total);
        assert_eq!(
            projected_unit
                .effective
                .breakdown
                .get(&BaseStatKind::Hp)
                .ok_or("HP breakdown")?
                .base,
            arithmetic.base
        );
        let after_session = lock_recover(&state.loaded_edit)
            .clone()
            .ok_or("edit session")?;
        assert_eq!(after_session.units, before_session.units);
        assert_eq!(after_session.sha256, before_session.sha256);
        assert!(lock_recover(&state.last_backup).is_none());
        assert_eq!(fs::read(root.join("enhanced.png"))?, original);
        let operations = vec![
            EditOperation::BaseStat {
                unit_position: position,
                stat: BaseStatKind::Hp,
                value: base,
            },
            EditOperation::CharacterLevel {
                unit_position: position,
                value: 10,
            },
            EditOperation::Experience {
                unit_position: position,
                value: 64,
            },
        ];
        let preview = project_draft(&state, generation, slot, operations.clone(), None)?;
        let edited =
            save_edit::prepare_replacement(&state, &before_session, &original, &operations)?;
        let decoded_edit = decode_enhanced_png(&edited, Some(&dictionary))?;
        let unencoded =
            save_edit::prepare_preview(&state, &before_session, &original, &operations)?;
        assert_eq!(unencoded.payload(), decoded_edit.payload());
        let duplicate = vec![operations[0], operations[0]];
        assert!(
            save_edit::prepare_preview(&state, &before_session, &original, &duplicate).is_err()
        );
        let before_records = decoded.unit_records(slot)?.ok_or("records")?;
        let after_records = decoded_edit.unit_records(slot)?.ok_or("records")?;
        let index = usize::from(position);
        assert_eq!(after_records[index].level, 10);
        assert_eq!(after_records[index].exp, 64);
        assert_eq!(after_records[index].hp_max_base, base);
        assert_eq!(
            after_records[index].job_levels,
            before_records[index].job_levels
        );
        assert_eq!(
            after_records[index].job_points,
            before_records[index].job_points
        );
        assert_eq!(
            after_records[index].total_job_points,
            before_records[index].total_job_points
        );
        let ValueState::Known(preview_units) = preview.reader.roster.value else {
            return Err("preview roster".into());
        };
        assert_eq!(
            preview_units
                .iter()
                .find(|unit| unit.key == u16::from(position))
                .ok_or("unit")?
                .effective,
            projected_unit.effective
        );
        let result = save_edit::save_selected_operations(&state, generation, slot, &operations)?;
        assert!(result.backup_created);
        assert_eq!(fs::read(root.join("enhanced - backup.png"))?, original);
        assert_eq!(fs::read(root.join("enhanced.png"))?, edited);
        assert_eq!(
            fs::read(workspace.join(".local/saves/example/enhanced.png"))?,
            original
        );
        assert!(project_draft(&state, generation, slot, operations, None).is_err());
        assert!(root.starts_with(workspace.join(".local/stat-edit-check")));
        fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn visible_request_serialization_is_explicit_and_strict() {
        let value = serde_json::json!({"unitPosition":3,"stat":"physical_attack","value":14});
        let request: VisibleStatRequest =
            serde_json::from_value(value.clone()).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(
            serde_json::to_value(request).unwrap_or_else(|error| panic!("{error:?}")),
            value
        );
        for invalid in [
            serde_json::json!({"unitPosition":3,"stat":"raw","value":14}),
            serde_json::json!({"unitPosition":3,"stat":"hp","value":-1}),
            serde_json::json!({"unitPosition":3,"stat":"hp","value":14,"extra":true}),
        ] {
            assert!(serde_json::from_value::<VisibleStatRequest>(invalid).is_err());
        }
    }
}
