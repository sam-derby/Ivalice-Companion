//! Small, read-only arithmetic request using normalized reader mechanics.
use super::*;
use ivalice_domain::reader::stat_edit::{preview_base, BaseStatKind, BaseStatPreview};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BaseStatRequest {
    stat: BaseStatKind,
    previous_base: u32,
    value: Option<u32>,
    job_multiplier: u16,
    equipment_bonus: Option<i32>,
}

#[tauri::command]
pub fn preview_base_stat(request: BaseStatRequest) -> Result<BaseStatPreview, IpcError> {
    preview_base(
        request.previous_base,
        request.value,
        request.job_multiplier,
        request.equipment_bonus,
        request.stat,
    )
    .map_err(|_| IpcError::simple(IpcErrorCategory::Input, "invalid_stat_request", false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ivalice_domain::ValueState;

    #[test]
    fn arithmetic_contract_is_explicit_and_does_not_require_a_save_session() {
        let value = serde_json::json!({"stat":"magical_attack","previousBase":100000,
            "value":94,"jobMultiplier":100,"equipmentBonus":4});
        let request: BaseStatRequest =
            serde_json::from_value(value.clone()).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(
            serde_json::to_value(request).unwrap_or_else(|error| panic!("{error:?}")),
            value
        );
        let response = preview_base_stat(request).unwrap_or_else(|error| panic!("{error:?}"));
        let response = serde_json::to_value(response).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(response["base"]["value"]["value"], 94);
        assert_eq!(response["total"]["value"]["value"], 98);
        assert!(response.get("storedBase").is_some());
        let capped: BaseStatRequest = serde_json::from_value(serde_json::json!({
            "stat":"magical_attack","previousBase":100000,"value":99,
            "jobMultiplier":100,"equipmentBonus":4
        }))
        .unwrap_or_else(|error| panic!("{error:?}"));
        let capped = preview_base_stat(capped).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(capped.base.value, ValueState::Known(99));
        assert_eq!(capped.total.value, ValueState::Known(99));
        for invalid in [
            serde_json::json!({"stat":"raw","previousBase":0,"value":14,"jobMultiplier":100}),
            serde_json::json!({"stat":"hp","previousBase":0,"value":-1,"jobMultiplier":100}),
            serde_json::json!({"stat":"hp","previousBase":0,"value":14,"jobMultiplier":100,"extra":true}),
        ] {
            assert!(serde_json::from_value::<BaseStatRequest>(invalid).is_err());
        }
    }
}
