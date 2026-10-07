use ivalice_domain::{
    game_data::reader_catalogue::{
        ItemMechanics, ItemMechanicsKind, JobMechanics, ReaderMechanics,
    },
    reader::{stats::apply_effective, CatalogueRef, Fact, GrowthCoefficients, ReaderDocument},
    ValueState,
};

fn known<T>(value: T) -> Fact<T> {
    Fact {
        value: ValueState::Known(value),
    }
}
fn reference(id: &str) -> Fact<CatalogueRef> {
    known(CatalogueRef {
        id: id.into(),
        label: ValueState::Known("Synthetic".into()),
        description: ValueState::Unknown,
        asset_key: ValueState::Unknown,
    })
}
fn job() -> JobMechanics {
    JobMechanics {
        id: "job:7".into(),
        growth: None,
        hp_multiplier: 100,
        mp_multiplier: 75,
        speed_multiplier: 100,
        pa_multiplier: 90,
        ma_multiplier: 80,
        movement_tiles: 4,
        jump_tiles: 3,
        character_evasion_percent: 10,
    }
}
fn item(id: &str, kind: ItemMechanicsKind) -> ItemMechanics {
    ItemMechanics {
        id: id.into(),
        kind,
        details: vec![],
        hp_bonus: 0,
        mp_bonus: 0,
        speed_bonus: 0,
        pa_bonus: 0,
        ma_bonus: 0,
        move_bonus: 0,
        jump_bonus: 0,
        physical_evasion_percent: 0,
        magical_evasion_percent: 0,
    }
}
fn mechanics() -> ReaderMechanics {
    ReaderMechanics {
        revision: "synthetic".into(),
        sources: vec![],
        jobs: vec![job()],
        items: vec![],
    }
}
fn document() -> ReaderDocument {
    let mut document: ReaderDocument = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/reader-v2.json"
    ))
    .unwrap_or_else(|error| panic!("{error}"));
    let ValueState::Known(units) = &mut document.roster.value else {
        panic!("fixture roster")
    };
    let unit = &mut units[0];
    unit.current_job = reference("job:7");
    unit.stored.bases.hp = known(458_752);
    unit.stored.bases.mp = known(245_760);
    unit.stored.bases.speed = known(98_304);
    unit.stored.bases.physical_attack = known(81_920);
    unit.stored.bases.magical_attack = known(65_536);
    unit.equipment.head = Fact {
        value: ValueState::Absent,
    };
    unit.equipment.body = Fact {
        value: ValueState::Absent,
    };
    unit.equipment.accessory = Fact {
        value: ValueState::Absent,
    };
    unit.equipment.right_weapon = Fact {
        value: ValueState::Absent,
    };
    unit.equipment.right_shield = Fact {
        value: ValueState::Absent,
    };
    unit.equipment.left_weapon = Fact {
        value: ValueState::Absent,
    };
    unit.equipment.left_shield = Fact {
        value: ValueState::Absent,
    };
    unit.abilities.movement = Fact {
        value: ValueState::Absent,
    };
    document
}
fn first(document: &ReaderDocument) -> &ivalice_domain::reader::ReaderUnit {
    let ValueState::Known(units) = &document.roster.value else {
        panic!("fixture roster")
    };
    &units[0]
}

#[test]
fn growth_follows_current_job_and_does_not_depend_on_equipment_or_bases() {
    let mut document = document();
    let mut data = mechanics();
    let growth = GrowthCoefficients {
        hp: 0,
        mp: 255,
        speed: 100,
        physical_attack: 40,
        magical_attack: 50,
    };
    data.jobs[0].growth = Some(growth);
    let ValueState::Known(units) = &mut document.roster.value else {
        panic!("fixture roster")
    };
    units[0].equipment.head = Fact {
        value: ValueState::Unknown,
    };
    units[0].stored.bases.hp = Fact {
        value: ValueState::Unknown,
    };
    apply_effective(&mut document, Some(&data));
    assert_eq!(first(&document).growth.value, ValueState::Known(growth));
    assert_eq!(first(&document).effective.hp.value, ValueState::Unknown);
    let ValueState::Known(units) = &mut document.roster.value else {
        panic!("fixture roster")
    };
    // No generic-job allowlist: an arbitrary named-job reference is handled identically.
    units[0].current_job = reference("job:57");
    let mut named_job = job();
    named_job.id = "job:57".into();
    named_job.growth = Some(GrowthCoefficients { hp: 11, ..growth });
    data.jobs.push(named_job);
    apply_effective(&mut document, Some(&data));
    assert_eq!(
        first(&document).growth.value,
        ValueState::Known(GrowthCoefficients { hp: 11, ..growth })
    );
    data.jobs[1].growth = None;
    apply_effective(&mut document, Some(&data));
    assert_eq!(first(&document).growth.value, ValueState::Unknown);
    data.jobs[1].growth = Some(growth);
    apply_effective(&mut document, Some(&data));
    apply_effective(&mut document, None);
    assert_eq!(first(&document).growth.value, ValueState::Unknown);
    data.jobs.clear();
    apply_effective(&mut document, Some(&data));
    assert_eq!(first(&document).growth.value, ValueState::Unknown);
}

#[test]
fn old_job_mechanics_load_without_growth_and_bad_coefficients_fail() {
    let value = serde_json::to_value(job()).unwrap_or_else(|error| panic!("{error}"));
    assert!(value.get("growth").is_none());
    let restored: JobMechanics =
        serde_json::from_value(value.clone()).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(restored.growth, None);
    for invalid in [
        serde_json::json!(256),
        serde_json::json!(-1),
        serde_json::json!("40"),
    ] {
        let mut bad = value.clone();
        bad["growth"] = serde_json::json!({"hp": invalid, "mp": 50, "speed": 100, "physical_attack": 40, "magical_attack": 50});
        assert!(serde_json::from_value::<JobMechanics>(bad).is_err());
    }
}

#[test]
fn floor_minimum_and_additive_equipment_are_separate() {
    let mut document = document();
    let mut data = mechanics();
    let ValueState::Known(units) = &mut document.roster.value else {
        panic!("fixture roster")
    };
    units[0].equipment.head = reference("item:200");
    units[0].equipment.accessory = reference("item:201");
    units[0].equipment.right_shield = reference("item:202");
    units[0].abilities.movement = reference("ability:487");
    let mut head = item("item:200", ItemMechanicsKind::Armor);
    head.hp_bonus = 10;
    head.mp_bonus = 5;
    let mut accessory = item("item:201", ItemMechanicsKind::Accessory);
    accessory.speed_bonus = 1;
    accessory.move_bonus = 1;
    accessory.magical_evasion_percent = 15;
    let mut shield = item("item:202", ItemMechanicsKind::Shield);
    shield.physical_evasion_percent = 13;
    shield.magical_evasion_percent = 3;
    data.items = vec![head, accessory, shield];
    apply_effective(&mut document, Some(&data));
    let effective = &first(&document).effective;
    assert_eq!(effective.hp.value, ValueState::Known(38));
    assert_eq!(effective.mp.value, ValueState::Known(16));
    assert_eq!(effective.speed.value, ValueState::Known(7));
    assert_eq!(effective.physical_attack.value, ValueState::Known(4));
    assert_eq!(effective.magical_attack.value, ValueState::Known(3));
    assert_eq!(effective.movement_tiles.value, ValueState::Known(7));
    assert_eq!(effective.jump_tiles.value, ValueState::Known(3));
    let ValueState::Known(evasion) = &effective.evasion.value else {
        panic!("evasion")
    };
    assert_eq!(
        evasion[1].physical_basis_points.value,
        ValueState::Known(1300)
    );
    assert_eq!(
        evasion[2].magical_basis_points.value,
        ValueState::Known(1500)
    );
}

#[test]
fn empty_equipment_and_non_bonus_movement_skills_keep_move_and_jump_known() {
    for ability in [
        None,
        Some("ability:487"),
        Some("ability:509"),
        Some("ability:498"),
    ] {
        let mut document = document();
        let ValueState::Known(units) = &mut document.roster.value else {
            panic!("fixture roster")
        };
        if let Some(ability) = ability {
            units[0].abilities.movement = reference(ability);
        }
        apply_effective(&mut document, Some(&mechanics()));
        assert_eq!(
            first(&document).effective.movement_tiles.value,
            ValueState::Known(if ability == Some("ability:487") { 6 } else { 4 })
        );
        assert_eq!(
            first(&document).effective.jump_tiles.value,
            ValueState::Known(3)
        );
    }
}

#[test]
fn missing_mechanics_or_special_encoded_jump_stays_unknown() {
    let mut document = document();
    apply_effective(&mut document, None);
    assert_eq!(first(&document).effective.hp.value, ValueState::Unknown);
    let mut data = mechanics();
    data.jobs[0].jump_tiles = 131;
    apply_effective(&mut document, Some(&data));
    assert_eq!(first(&document).effective.hp.value, ValueState::Known(28));
    assert_eq!(
        first(&document).effective.jump_tiles.value,
        ValueState::Unknown
    );
    let ValueState::Known(units) = &mut document.roster.value else {
        panic!("fixture roster")
    };
    units[0].equipment.head = Fact {
        value: ValueState::Unknown,
    };
    apply_effective(&mut document, Some(&data));
    assert_eq!(first(&document).effective.hp.value, ValueState::Unknown);
}

#[test]
fn unverified_display_caps_and_zero_base_do_not_publish_guesses() {
    let mut document = document();
    let data = mechanics();
    let ValueState::Known(units) = &mut document.roster.value else {
        panic!("fixture roster")
    };
    units[0].stored.bases.hp = known(0);
    units[0].stored.bases.physical_attack = known(0x00ff_ffff);
    apply_effective(&mut document, Some(&data));
    assert_eq!(first(&document).effective.hp.value, ValueState::Known(1));
    assert_eq!(
        first(&document).effective.physical_attack.value,
        ValueState::Unknown
    );
}

#[test]
fn attack_equipment_stops_at_99_in_the_normal_reader() {
    let mut document = document();
    let mut data = mechanics();
    data.jobs[0].pa_multiplier = 100;
    data.jobs[0].ma_multiplier = 100;
    let mut accessory = item("item:200", ItemMechanicsKind::Accessory);
    accessory.pa_bonus = 4;
    accessory.ma_bonus = 4;
    data.items.push(accessory);
    let ValueState::Known(units) = &mut document.roster.value else {
        panic!("fixture roster")
    };
    units[0].stored.bases.physical_attack = known(1_622_016);
    units[0].stored.bases.magical_attack = known(1_622_016);
    units[0].equipment.accessory = reference("item:200");
    apply_effective(&mut document, Some(&data));
    let unit = first(&document);
    assert_eq!(unit.effective.physical_attack.value, ValueState::Known(99));
    assert_eq!(unit.effective.magical_attack.value, ValueState::Known(99));
    for kind in [
        ivalice_domain::reader::stat_edit::BaseStatKind::PhysicalAttack,
        ivalice_domain::reader::stat_edit::BaseStatKind::MagicalAttack,
    ] {
        let detail = &unit.effective.breakdown[&kind];
        assert_eq!(detail.base.value, ValueState::Known(99));
        assert_eq!(detail.equipment_bonus.value, ValueState::Known(4));
    }
}

#[test]
fn draft_bases_gear_abilities_and_reverts_share_the_reader_projection() {
    use ivalice_domain::reader::stat_edit::{solve_unit, BaseStatKind};
    let original = document();
    let mut pending = original.clone();
    let mut data = mechanics();
    let mut armour = item("item:200", ItemMechanicsKind::Armor);
    armour.hp_bonus = 120;
    armour.mp_bonus = 20;
    let mut boots = item("item:201", ItemMechanicsKind::Accessory);
    boots.speed_bonus = 1;
    boots.move_bonus = 1;
    boots.jump_bonus = 1;
    boots.pa_bonus = 2;
    boots.ma_bonus = 2;
    boots.physical_evasion_percent = 10;
    data.items = vec![armour, boots];
    let hp_base = solve_unit(first(&pending), &data, BaseStatKind::Hp, 50)
        .unwrap_or_else(|error| panic!("{error:?}"));
    let ValueState::Known(units) = &mut pending.roster.value else {
        panic!("roster")
    };
    units[0].stored.bases.hp = known(hp_base);
    apply_effective(&mut pending, Some(&data));
    assert_eq!(first(&pending).effective.hp.value, ValueState::Known(50));
    let ValueState::Known(units) = &mut pending.roster.value else {
        panic!("roster")
    };
    units[0].equipment.body = reference("item:200");
    units[0].equipment.accessory = reference("item:201");
    units[0].abilities.movement = reference("ability:487");
    apply_effective(&mut pending, Some(&data));
    let unit = first(&pending);
    assert_eq!(unit.stored.bases.hp.value, ValueState::Known(hp_base));
    assert_eq!(unit.effective.hp.value, ValueState::Known(170));
    let detail = &unit.effective.breakdown[&BaseStatKind::Hp];
    assert_eq!(detail.base.value, ValueState::Known(50));
    assert_eq!(detail.equipment_bonus.value, ValueState::Known(120));
    assert_eq!(detail.maximum, 999);
    assert_eq!(unit.effective.mp.value, ValueState::Known(31));
    assert_eq!(unit.effective.speed.value, ValueState::Known(7));
    assert_eq!(unit.effective.physical_attack.value, ValueState::Known(6));
    assert_eq!(unit.effective.magical_attack.value, ValueState::Known(5));
    assert_eq!(unit.effective.movement_tiles.value, ValueState::Known(7));
    assert_eq!(unit.effective.jump_tiles.value, ValueState::Known(4));
    let ValueState::Known(evasion) = &unit.effective.evasion.value else {
        panic!("evasion")
    };
    assert!(evasion.iter().any(|entry| entry.source
        == ivalice_domain::reader::EvasionSource::Accessory
        && entry.physical_basis_points.value == ValueState::Known(1000)));
    let ValueState::Known(units) = &mut pending.roster.value else {
        panic!("roster")
    };
    units[0].equipment = first(&original).equipment.clone();
    units[0].abilities = first(&original).abilities.clone();
    apply_effective(&mut pending, Some(&data));
    assert_eq!(first(&pending).effective.hp.value, ValueState::Known(50));
    let ValueState::Known(units) = &mut pending.roster.value else {
        panic!("roster")
    };
    units[0].stored.bases = first(&original).stored.bases.clone();
    apply_effective(&mut pending, Some(&data));
    let mut baseline = original;
    apply_effective(&mut baseline, Some(&data));
    assert_eq!(first(&pending).effective, first(&baseline).effective);
}

#[test]
fn every_kind_solves_using_current_job_and_equipment() {
    use ivalice_domain::reader::stat_edit::{solve_unit, BaseStatKind, StatEditError};
    let mut document = document();
    let mut data = mechanics();
    let mut accessory = item("item:201", ItemMechanicsKind::Accessory);
    accessory.hp_bonus = 10;
    accessory.mp_bonus = 5;
    accessory.speed_bonus = 1;
    accessory.pa_bonus = 2;
    accessory.ma_bonus = 3;
    data.items = vec![accessory];
    let ValueState::Known(units) = &mut document.roster.value else {
        panic!("roster")
    };
    units[0].equipment.accessory = reference("item:201");
    for (kind, desired) in [
        (BaseStatKind::Hp, 50),
        (BaseStatKind::Mp, 30),
        (BaseStatKind::Speed, 12),
        (BaseStatKind::PhysicalAttack, 14),
        (BaseStatKind::MagicalAttack, 11),
    ] {
        let base = solve_unit(first(&document), &data, kind, desired)
            .unwrap_or_else(|error| panic!("{error:?}"));
        let ValueState::Known(units) = &mut document.roster.value else {
            panic!("roster")
        };
        kind.base_mut(&mut units[0].stored.bases).value = ValueState::Known(base);
        apply_effective(&mut document, Some(&data));
        let effective = &first(&document).effective;
        let value = match kind {
            BaseStatKind::Hp => &effective.hp,
            BaseStatKind::Mp => &effective.mp,
            BaseStatKind::Speed => &effective.speed,
            BaseStatKind::PhysicalAttack => &effective.physical_attack,
            BaseStatKind::MagicalAttack => &effective.magical_attack,
        };
        assert_eq!(value.value, ValueState::Known(desired));
    }
    data.jobs.clear();
    assert_eq!(
        solve_unit(first(&document), &data, BaseStatKind::Hp, 50),
        Err(StatEditError::UnknownMechanics)
    );
}
