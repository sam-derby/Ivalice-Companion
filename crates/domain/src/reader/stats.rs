//! Status-screen values from normalized upstream job and item mechanics.

use super::{
    CurrentEquipment, EffectiveStats, Evasion, EvasionSource, Fact, ReaderDocument, ReaderUnit,
};
use crate::{
    game_data::reader_catalogue::{
        ItemMechanics, ItemMechanicsKind, JobMechanics, ReaderMechanics,
    },
    ValueState,
};

/// AeroStar's Battle Mechanics Guide, §7.1: floor(raw × job multiplier / 1,638,400),
/// minimum one, then additive equipment. Job/item inputs come from
/// fftivc.utility.modloader/TableData XML at d3123d2. Out-of-range or missing
/// mechanics remain unknown rather than inventing TIC caps or special cases.
pub fn apply_effective(document: &mut ReaderDocument, mechanics: Option<&ReaderMechanics>) {
    if let Some(mechanics) = mechanics {
        document.item_details = mechanics
            .items
            .iter()
            .map(|item| (item.id.clone(), item.details.clone()))
            .collect();
    }
    let ValueState::Known(units) = &mut document.roster.value else {
        return;
    };
    for unit in units {
        // fftivc.utility.modloader/TableData/JobData.xml at d3123d2:
        // growth belongs to the current job, independently of equipped items.
        unit.growth = mechanics
            .and_then(|data| reference_id(&unit.current_job).and_then(|id| find_job(data, id)))
            .and_then(|job| job.growth)
            .map_or_else(unknown, known);
        if let Some(mechanics) = mechanics {
            unit.effective = calculate(unit, mechanics);
        }
    }
}

fn calculate(unit: &ReaderUnit, data: &ReaderMechanics) -> EffectiveStats {
    let job = reference_id(&unit.current_job).and_then(|id| find_job(data, id));
    let bonuses = equipment_bonuses(&unit.equipment, data);
    let hp = stat(
        &unit.stored.bases.hp,
        job.map(|value| value.hp_multiplier),
        bonuses.map(|value| value.hp),
        999,
    );
    let mp = stat(
        &unit.stored.bases.mp,
        job.map(|value| value.mp_multiplier),
        bonuses.map(|value| value.mp),
        999,
    );
    let speed = stat(
        &unit.stored.bases.speed,
        job.map(|value| value.speed_multiplier),
        bonuses.map(|value| value.speed),
        50,
    );
    let physical_attack = stat(
        &unit.stored.bases.physical_attack,
        job.map(|value| value.pa_multiplier),
        bonuses.map(|value| value.pa),
        99,
    );
    let magical_attack = stat(
        &unit.stored.bases.magical_attack,
        job.map(|value| value.ma_multiplier),
        bonuses.map(|value| value.ma),
        99,
    );
    let movement = movement_bonus(&unit.abilities.movement);
    let movement_tiles = tiles(
        job.map(|value| value.movement_tiles),
        bonuses.map(|value| value.movement),
        movement.map(|value| value.0),
    );
    let jump_tiles = tiles(
        job.map(|value| value.jump_tiles),
        bonuses.map(|value| value.jump),
        movement.map(|value| value.1),
    );
    EffectiveStats {
        hp,
        mp,
        speed,
        physical_attack,
        magical_attack,
        movement_tiles,
        jump_tiles,
        evasion: evasion(job, &unit.equipment, data),
    }
}

fn find_job<'a>(data: &'a ReaderMechanics, id: &str) -> Option<&'a JobMechanics> {
    data.jobs.iter().find(|job| job.id == id)
}

fn find_item<'a>(data: &'a ReaderMechanics, id: &str) -> Option<&'a ItemMechanics> {
    data.items.iter().find(|item| item.id == id)
}

fn reference_id(fact: &Fact<super::CatalogueRef>) -> Option<&str> {
    match &fact.value {
        ValueState::Known(reference) => Some(&reference.id),
        _ => None,
    }
}

#[derive(Clone, Copy, Default)]
struct Bonuses {
    hp: i32,
    mp: i32,
    speed: i32,
    pa: i32,
    ma: i32,
    movement: i32,
    jump: i32,
}

fn equipment_bonuses(equipment: &CurrentEquipment, data: &ReaderMechanics) -> Option<Bonuses> {
    let mut total = Bonuses::default();
    for fact in [
        &equipment.head,
        &equipment.body,
        &equipment.accessory,
        &equipment.right_weapon,
        &equipment.right_shield,
        &equipment.left_weapon,
        &equipment.left_shield,
    ] {
        let item = match &fact.value {
            ValueState::Absent => continue,
            ValueState::Known(reference) => find_item(data, &reference.id)?,
            ValueState::Unknown | ValueState::Unsupported => return None,
        };
        if item.kind == ItemMechanicsKind::Other {
            return None;
        }
        total.hp += i32::from(item.hp_bonus);
        total.mp += i32::from(item.mp_bonus);
        total.speed += i32::from(item.speed_bonus);
        total.pa += i32::from(item.pa_bonus);
        total.ma += i32::from(item.ma_bonus);
        total.movement += i32::from(item.move_bonus);
        total.jump += i32::from(item.jump_bonus);
    }
    Some(total)
}

fn stat(
    base: &Fact<u32>,
    multiplier: Option<u16>,
    bonus: Option<i32>,
    unverified_cap: u32,
) -> Fact<u32> {
    let (ValueState::Known(base), Some(multiplier), Some(bonus)) = (&base.value, multiplier, bonus)
    else {
        return unknown();
    };
    if multiplier == 0 || *base > 0x00ff_ffff {
        return unknown();
    }
    let scaled = (u64::from(*base) * u64::from(multiplier) / 1_638_400).max(1);
    let result = i64::try_from(scaled)
        .ok()
        .and_then(|value| value.checked_add(i64::from(bonus)));
    // Classic display caps are only leads for TIC. Keep values beyond them unknown.
    match result.and_then(|value| u32::try_from(value).ok()) {
        Some(value) if value > 0 && value <= unverified_cap => known(value),
        _ => unknown(),
    }
}

fn tiles(base: Option<u16>, equipment: Option<i32>, ability: Option<i32>) -> Fact<u16> {
    let (Some(base), Some(equipment), Some(ability)) = (base, equipment, ability) else {
        return unknown();
    };
    let value = i32::from(base) + equipment + ability;
    match u16::try_from(value) {
        Ok(value) if value <= 99 => known(value),
        _ => unknown(),
    }
}

fn movement_bonus(fact: &Fact<super::CatalogueRef>) -> Option<(i32, i32)> {
    match &fact.value {
        ValueState::Absent => Some((0, 0)),
        // fftivc.utility.modloader/TableData/AbilityData.xml at d3123d2.
        ValueState::Known(reference) => match reference.id.as_str() {
            "ability:486" => Some((1, 0)),
            "ability:487" => Some((2, 0)),
            "ability:488" => Some((3, 0)),
            "ability:489" => Some((0, 1)),
            "ability:490" => Some((0, 2)),
            "ability:491" => Some((0, 3)),
            // Remaining pinned movement skills alter traversal or recovery,
            // without an additive Move/Jump bonus.
            "ability:492" | "ability:493" | "ability:494" | "ability:495" | "ability:496"
            | "ability:497" | "ability:498" | "ability:499" | "ability:500" | "ability:501"
            | "ability:502" | "ability:503" | "ability:504" | "ability:505" | "ability:506"
            | "ability:507" | "ability:508" | "ability:509" => Some((0, 0)),
            _ => None,
        },
        ValueState::Unknown | ValueState::Unsupported => None,
    }
}

fn evasion(
    job: Option<&JobMechanics>,
    equipment: &CurrentEquipment,
    data: &ReaderMechanics,
) -> Fact<Vec<Evasion>> {
    let Some(job) = job else { return unknown() };
    let mut entries = vec![Evasion {
        source: EvasionSource::Character,
        physical_basis_points: known(job.character_evasion_percent * 100),
        magical_basis_points: known(0),
    }];
    for (fact, kind, source) in [
        (
            &equipment.right_shield,
            ItemMechanicsKind::Shield,
            EvasionSource::RightShield,
        ),
        (
            &equipment.left_shield,
            ItemMechanicsKind::Shield,
            EvasionSource::LeftShield,
        ),
        (
            &equipment.accessory,
            ItemMechanicsKind::Accessory,
            EvasionSource::Accessory,
        ),
        (
            &equipment.right_weapon,
            ItemMechanicsKind::Weapon,
            EvasionSource::RightWeapon,
        ),
        (
            &equipment.left_weapon,
            ItemMechanicsKind::Weapon,
            EvasionSource::LeftWeapon,
        ),
    ] {
        if let Some(entry) = evade_item(&fact.value, data, kind, source) {
            entries.push(entry);
        }
    }
    known(entries)
}

fn evade_item(
    value: &ValueState<super::CatalogueRef>,
    data: &ReaderMechanics,
    kind: ItemMechanicsKind,
    source: EvasionSource,
) -> Option<Evasion> {
    if matches!(value, ValueState::Absent) {
        return None;
    }
    let item = match value {
        ValueState::Known(reference) => {
            find_item(data, &reference.id).filter(|item| item.kind == kind)
        }
        _ => None,
    };
    Some(Evasion {
        source,
        physical_basis_points: item
            .map_or_else(unknown, |value| known(value.physical_evasion_percent * 100)),
        magical_basis_points: item
            .map_or_else(unknown, |value| known(value.magical_evasion_percent * 100)),
    })
}

fn known<T>(value: T) -> Fact<T> {
    Fact {
        value: ValueState::Known(value),
    }
}
fn unknown<T>() -> Fact<T> {
    Fact {
        value: ValueState::Unknown,
    }
}
