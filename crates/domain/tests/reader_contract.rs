use ivalice_domain::reader::{ReaderDocument, ReaderError, ValidatedReader, MAX_READER_BYTES};
use serde_json::{json, Value};

const EXAMPLE: &[u8] = include_bytes!("../../../tests/fixtures/reader-v2.json");

fn example() -> Value {
    serde_json::from_slice(EXAMPLE).unwrap_or_else(|error| panic!("{error}"))
}

fn check(value: &Value) -> Result<ValidatedReader, ReaderError> {
    ValidatedReader::from_json(&serde_json::to_vec(value).unwrap_or_else(|error| panic!("{error}")))
}

fn known(value: Value) -> Value {
    json!({"value":{"state":"known","value":value}})
}

#[test]
fn documented_example_round_trips_and_preserves_zero_absent_unknown() {
    let reader = ValidatedReader::from_json(EXAMPLE).unwrap_or_else(|error| panic!("{error:?}"));
    let bytes = reader.to_json().unwrap_or_else(|error| panic!("{error:?}"));
    let second = ValidatedReader::from_json(&bytes).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(reader.document(), second.document());
    let value: Value = serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("{error}"));
    let unit = &value["roster"]["value"]["value"][0];
    assert_eq!(
        unit["stored"]["experience"]["value"],
        json!({"state":"known","value":0})
    );
    assert_eq!(
        unit["equipment"]["head"]["value"],
        json!({"state":"absent"})
    );
    assert_eq!(unit["effective"]["hp"]["value"], json!({"state":"unknown"}));
}

#[test]
fn unknown_fields_schema_and_placeholder_values_fail_closed() {
    let original = example();
    let mut changed = original.clone();
    changed["schema"] = json!("v5");
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidJson));
    changed = original.clone();
    changed["write_path"] = json!("not permitted");
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidJson));
    changed = original;
    changed["gil"]["value"]["value"] = json!(0);
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidJson));
}

#[test]
fn per_value_claim_and_spoiler_metadata_are_rejected() {
    let mut changed = example();
    changed["gil"] = known(json!(0));
    assert!(check(&changed).is_ok());
    changed["gil"]["claims"] = json!(["obsolete"]);
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidJson));
    changed = example();
    changed["gil"]["spoiler"] = json!("full");
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidJson));
}

#[test]
fn difficulty_is_explicitly_unknown_and_cannot_be_omitted_or_defaulted() {
    let reader = ValidatedReader::from_json(EXAMPLE).unwrap_or_else(|error| panic!("{error:?}"));
    assert!(matches!(
        &reader.document().progress.difficulty.value,
        ivalice_domain::ValueState::Unknown
    ));
    let encoded: Value =
        serde_json::from_slice(&reader.to_json().unwrap_or_else(|error| panic!("{error:?}")))
            .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        encoded["progress"]["difficulty"]["value"],
        json!({"state":"unknown"})
    );
    let mut missing = example();
    missing["progress"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("progress object"))
        .remove("difficulty");
    assert_eq!(check(&missing).err(), Some(ReaderError::InvalidJson));
    let mut numeric = example();
    numeric["progress"]["difficulty"] = known(json!(0));
    assert_eq!(check(&numeric).err(), Some(ReaderError::InvalidJson));
}

#[test]
fn known_difficulty_requires_a_valid_catalogue_reference() {
    let mut changed = example();
    changed["progress"]["difficulty"] = known(json!({
        "id":"difficulty:synthetic",
        "label":{"state":"known","value":"Synthetic difficulty"},
        "description":{"state":"unknown"},"asset_key":{"state":"unknown"}
    }));
    let reader = check(&changed).unwrap_or_else(|error| panic!("{error:?}"));
    let second =
        ValidatedReader::from_json(&reader.to_json().unwrap_or_else(|error| panic!("{error:?}")))
            .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(
        reader.document().progress.difficulty,
        second.document().progress.difficulty
    );
    changed["progress"]["difficulty"]["value"]["value"]["id"] = json!("difficulty:");
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidValue));
}

#[test]
fn duplicate_names_are_valid_but_unit_and_persistent_keys_cannot_collide() {
    let mut changed = example();
    let mut second = changed["roster"]["value"]["value"][0].clone();
    second["key"] = json!(2);
    changed["roster"]["value"]["value"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("roster array"))
        .push(second);
    assert!(check(&changed).is_ok());
    changed["roster"]["value"]["value"][1]["key"] = json!(1);
    assert_eq!(check(&changed).err(), Some(ReaderError::DuplicateIdentity));
    changed["roster"]["value"]["value"][1]["key"] = json!(2);
    for index in 0..2 {
        changed["roster"]["value"]["value"][index]["persistent_identity"] =
            known(json!("unit:one"));
    }
    assert_eq!(check(&changed).err(), Some(ReaderError::DuplicateIdentity));
}

#[test]
fn unknown_kind_and_guest_monster_remain_representable() {
    let mut changed = example();
    changed["roster"]["value"]["value"][0]["kind"]["value"] = json!({"state":"unsupported"});
    assert!(check(&changed).is_ok());
    changed["roster"]["value"]["value"][0]["kind"] = known(json!("monster"));
    changed["roster"]["value"]["value"][0]["membership"] = known(json!("guest"));
    assert!(check(&changed).is_ok());
}

#[test]
fn snapshot_resource_session_slot_and_resource_token_each_reject_stale_results() {
    let reader = ValidatedReader::from_json(EXAMPLE).unwrap_or_else(|error| panic!("{error:?}"));
    let identity = reader.document().identity.clone();
    assert!(reader.for_identity(&identity).is_ok());
    for index in 0..5 {
        let mut different = identity.clone();
        match index {
            0 => different.snapshot_generation += 1,
            1 => different.resource_generation += 1,
            2 => different.session = "another-session".into(),
            3 => different.manual_slot = 1,
            _ => {
                different.resource_token = ivalice_domain::ValueState::Known("resource:two".into())
            }
        }
        assert_eq!(
            reader.for_identity(&different).err(),
            Some(ReaderError::Stale)
        );
    }
}

#[test]
fn numeric_width_and_array_limits_reject_invalid_inputs() {
    let mut changed = example();
    changed["identity"]["snapshot_generation"] = json!(9_007_199_254_740_992_u64);
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidIdentity));
    changed = example();
    changed["gil"] = known(json!(9_007_199_254_740_992_u64));
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidValue));
    changed = example();
    changed["roster"]["value"]["value"][0]["stored"]["bases"]["hp"] = known(json!(0x100_0000));
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidValue));
    changed = example();
    let unit = changed["roster"]["value"]["value"][0].clone();
    changed["roster"]["value"]["value"] = Value::Array(vec![unit; 55]);
    assert_eq!(check(&changed).err(), Some(ReaderError::TooLarge));
    assert_eq!(
        ValidatedReader::from_json(&vec![b' '; MAX_READER_BYTES + 1]).err(),
        Some(ReaderError::TooLarge)
    );
}

#[test]
fn total_jp_never_becomes_current_jp_or_eligibility() {
    let mut changed = example();
    let reference = json!({"id":"job:synthetic","label":{"state":"unknown"},"description":{"state":"unknown"},"asset_key":{"state":"unknown"}});
    changed["roster"]["value"]["value"][0]["jobs"] = known(json!([{
        "slot":0,"job":known(reference), "level":known(json!(1)),
        "current_jp":{"value":{"state":"unknown"}},
        "total_jp":known(json!(0))
    }]));
    let reader = check(&changed).unwrap_or_else(|error| panic!("{error:?}"));
    let encoded: Value =
        serde_json::from_slice(&reader.to_json().unwrap_or_else(|error| panic!("{error:?}")))
            .unwrap_or_else(|error| panic!("{error}"));
    let unit = &encoded["roster"]["value"]["value"][0];
    assert_eq!(
        unit["jobs"]["value"]["value"][0]["current_jp"]["value"]["state"],
        "unknown"
    );
    assert_eq!(
        unit["guidance"]["job_eligible"]["value"]["state"],
        "unknown"
    );
}

#[test]
fn combat_set_selection_needs_a_matching_saved_set() {
    let mut changed = example();
    changed["roster"]["value"]["value"][0]["selected_combat_set"] = known(json!(0));
    assert_eq!(check(&changed).err(), Some(ReaderError::MissingReference));
    changed["roster"]["value"]["value"][0]["selected_combat_set"] = known(json!(3));
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidValue));
}

#[test]
fn private_paths_control_characters_and_unbounded_labels_are_rejected() {
    for name in [" ".to_owned(), "a\u{0000}b".to_owned(), "a".repeat(257)] {
        let mut changed = example();
        changed["roster"]["value"]["value"][0]["name"]["value"]["value"]["text"] = json!(name);
        assert_eq!(check(&changed).err(), Some(ReaderError::InvalidText));
    }
    let mut changed = example();
    changed["identity"]["session"] = json!("C:\\private\\save.png");
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidIdentity));
}

#[test]
fn direct_deserialization_does_not_bypass_publish_validation() {
    let mut changed = example();
    changed["identity"]["manual_slot"] = json!(50);
    let document: ReaderDocument =
        serde_json::from_value(changed).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        ValidatedReader::validate(document).err(),
        Some(ReaderError::InvalidIdentity)
    );
}

#[test]
fn absent_facts_and_catalogue_namespaces_must_be_complete() {
    let mut changed = example();
    changed["roster"]["value"]["value"][0]["equipment"]["head"]["claims"] = json!([]);
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidJson));
    for id in [":", ":item", "item:"] {
        let mut changed = example();
        changed["roster"]["value"]["value"][0]["current_job"] = known(json!({
            "id":id,"label":{"state":"unknown"},"description":{"state":"unknown"},"asset_key":{"state":"unknown"}
        }));
        assert_eq!(check(&changed).err(), Some(ReaderError::InvalidValue));
    }
}

#[test]
fn large_direct_document_is_rejected_during_bounded_serialization() {
    let mut changed = example();
    let units: Vec<Value> = (0..54).map(|key| {
        let mut unit = changed["roster"]["value"]["value"][0].clone();
        unit["key"] = json!(key);
        unit["learned_abilities"] = known(Value::Array((0..32).map(|id| json!({
            "ability":known(json!({"id":format!("ability:{id}"),"label":{"state":"unknown"},
                "description":{"state":"known","value":"x".repeat(4096)},"asset_key":{"state":"unknown"}})),
            "learned":known(json!(false)),"jp_cost":known(json!(0))
        })).collect()));
        unit
    }).collect();
    changed["roster"]["value"]["value"] = Value::Array(units);
    let document: ReaderDocument =
        serde_json::from_value(changed).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        ValidatedReader::validate(document).err(),
        Some(ReaderError::TooLarge)
    );
}

#[test]
fn populated_inventory_abilities_sets_and_progress_round_trip_with_distinct_states() {
    let mut changed = example();
    let reference = |id: &str| {
        known(
            json!({"id":id,"label":{"state":"known","value":"Synthetic label"},
        "description":{"state":"unknown"},"asset_key":{"state":"unknown"}}),
        )
    };
    changed["inventory"] = known(json!([{"key":0,"item":reference("item:synthetic"),
            "category":{"value":{"state":"unknown"}},"quantity":known(json!(0))}]));
    changed["progress"]["events"] = known(
        json!([{"subject":reference("event:synthetic"),"saved_state":known(json!("observed"))}]),
    );
    let unit = &mut changed["roster"]["value"]["value"][0];
    unit["learned_abilities"] = known(
        json!([{"ability":reference("ability:synthetic"),"learned":known(json!(false)),"jp_cost":known(json!(0))}]),
    );
    unit["combat_sets"] = known(json!([{
        "key":0,"assigned":known(json!(true)),"name":known(json!("Synthetic set")),"job":reference("job:synthetic"),
        "head":reference("item:head"),"body":reference("item:body"),"accessory":reference("item:accessory"),
        "right_hand":reference("item:weapon"),"left_hand":unit["equipment"]["head"].clone(),
        "abilities":unit["abilities"].clone(),"double_hand":known(json!(false))
    }]));
    unit["selected_combat_set"] = known(json!(0));
    let reader = check(&changed).unwrap_or_else(|error| panic!("{error:?}"));
    let round_trip =
        ValidatedReader::from_json(&reader.to_json().unwrap_or_else(|error| panic!("{error:?}")))
            .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(reader.document(), round_trip.document());
    for path in [
        "/inventory/value/value",
        "/progress/events/value/value",
        "/roster/value/value/0/learned_abilities/value/value",
        "/roster/value/value/0/combat_sets/value/value",
    ] {
        let mut duplicate = changed.clone();
        let list = duplicate
            .pointer_mut(path)
            .and_then(Value::as_array_mut)
            .unwrap_or_else(|| panic!("fixture list"));
        list.push(list[0].clone());
        assert_eq!(
            check(&duplicate).err(),
            Some(ReaderError::DuplicateIdentity)
        );
    }
}

#[test]
fn reader_accepts_catalogue_multiline_descriptions_without_relaxing_labels() {
    use ivalice_domain::game_data::{
        reader_catalogue::{
            ReaderCatalogueDocument, ReaderCatalogueEntry, ReaderCatalogueSource, ReaderCategory,
            ValidatedReaderCatalogue, READER_CATALOGUE_PROFILE, READER_CATALOGUE_REVISION,
            READER_CATALOGUE_SCHEMA,
        },
        SpoilerLevel,
    };
    use ivalice_domain::ValueState;
    let catalogue = ValidatedReaderCatalogue::validate(ReaderCatalogueDocument {
        schema: READER_CATALOGUE_SCHEMA.into(),
        profile: READER_CATALOGUE_PROFILE.into(),
        revision: READER_CATALOGUE_REVISION.into(),
        locale: "en".into(),
        sources: [
            ReaderCategory::Job,
            ReaderCategory::Command,
            ReaderCategory::Ability,
            ReaderCategory::Item,
            ReaderCategory::CharacterName,
        ]
        .into_iter()
        .map(|category| ReaderCatalogueSource {
            category,
            input_sha256: category.input_sha256().into(),
        })
        .collect(),
        entries: vec![ReaderCatalogueEntry {
            category: ReaderCategory::Item,
            id: "item:1".into(),
            label: ValueState::Known("Synthetic item".into()),
            description: ValueState::Known("First line\nSecond line".into()),
            spoiler: SpoilerLevel::Minimal,
        }],
        job_commands: vec![],
        mechanics: None,
    })
    .unwrap_or_else(|error| panic!("{error:?}"));
    let reference = catalogue
        .lookup("item:1", SpoilerLevel::Minimal)
        .unwrap_or_else(|| panic!("visible synthetic reference"));
    let mut changed = example();
    changed["roster"]["value"]["value"][0]["equipment"]["body"] =
        known(serde_json::to_value(reference).unwrap_or_else(|error| panic!("{error}")));
    let accepted = check(&changed).unwrap_or_else(|error| panic!("{error:?}"));
    assert!(ValidatedReader::from_json(
        &accepted
            .to_json()
            .unwrap_or_else(|error| panic!("{error:?}"))
    )
    .is_ok());
    let field = &mut changed["roster"]["value"]["value"][0]["equipment"]["body"]["value"]["value"];
    field["label"] = json!({"state":"known","value":"Bad\nlabel"});
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidText));
    changed["roster"]["value"]["value"][0]["equipment"]["body"]["value"]["value"]["label"] =
        json!({"state":"known","value":"Synthetic item"});
    changed["roster"]["value"]["value"][0]["equipment"]["body"]["value"]["value"]["description"] =
        json!({"state":"known","value":"Bad\rdescription"});
    assert_eq!(check(&changed).err(), Some(ReaderError::InvalidText));
}
