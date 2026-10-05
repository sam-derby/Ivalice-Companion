//! Story guests, permanent members and Ramza's chapter form rebuilt from the
//! game's battle entries. scripts/import-story-roster.py joins ScenarioId events
//! to their battle entries and DismissUnit instructions; this module turns each
//! unit into a deterministic record plan.

use serde::{Deserialize, Serialize};

use crate::{
    calendar::day_of_year,
    game_data::inventory_category::inventory_category,
    job_eligibility::JOB_LEVEL_TOTAL_JP,
    reader::{level_growth, Fact, GrowthCoefficients, StoredBases},
    ValueState,
};

const STORY_SOURCE_SHA256: &str =
    "68e926c75e6bc151c8fe66b94cea6b1d3a610d13da4f5824f6a596448777c76a";
const SCRIPTS_SHA256: &str = "5afc5268cbac2df9fb8e1093dd8e6a71b62e4688c7c764e415111899bbf179a4";
const BATTLE_SHA256: &str = "e790abe545695d271417c3817e1570bbc452a20cb48c4c54aaad71696d544637";
const MAX_BYTES: usize = 64 * 1024;
const MAX_UNITS: usize = 128;
const MAX_STEPS: usize = 1024;
pub const GUEST_POSITIONS: u8 = 4;
/// ENTD level 0xFE takes the party's level; only permanent members use it here.
pub const PARTY_LEVEL: u8 = 0xfe;
// ENTD equipment: 0xFE is a generated item, 0xFF is none; both stay empty.
const GENERATED_ITEM: u8 = 0xfe;
const EMPTY_ITEM: u16 = 0xff;
// ENTD abilities 0x1FE/0x1FF are generated or none; generated choices stay empty.
const GENERATED_ABILITY: u16 = 0x1fe;
const MALE: u8 = 0x80;
const FEMALE: u8 = 0x40;
const MONSTER: u8 = 0x20;
// ENTD flags 0x08 (load) and, once joined, 0x01 (save formation) are not kept.
const LOAD_FORMATION: u8 = 0x08;
const SAVE_FORMATION: u8 = 0x01;
const JOB_SLOTS: usize = 22;
// Unlocked-job bytes store slot 0 (Squire) and 1 (Chemist) in bit 7 and 6.
const STARTING_UNLOCKS: u32 = 0xc0;

/// Ramza's character, personal job and command per chapter form, from the
/// pinned catalogue's job_commands (job:1-3 use command:25-27).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RamzaForm {
    pub character: u8,
    pub job: u8,
    pub command: u8,
}

pub const RAMZA_FORMS: [RamzaForm; 3] = [
    RamzaForm {
        character: 1,
        job: 1,
        command: 25,
    },
    RamzaForm {
        character: 2,
        job: 2,
        command: 26,
    },
    RamzaForm {
        character: 3,
        job: 3,
        command: 27,
    },
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RosterUnit {
    pub entry: u16,
    pub unit: u8,
    pub sprite: u8,
    pub name: u8,
    pub flags: u8,
    pub level: u8,
    pub birth_month: u8,
    pub birth_day: u8,
    pub brave: u8,
    pub faith: u8,
    pub job: u8,
    pub unlock_job: u8,
    pub unlock_level: u8,
    pub reaction: u16,
    pub support: u16,
    pub movement: u16,
    pub equipment: [u8; 5],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestStep {
    pub progress: i32,
    pub units: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryMember {
    pub unit: usize,
    pub joins: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RamzaFormStep {
    pub progress: i32,
    pub character: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryRosterDocument {
    pub schema: String,
    pub profile: String,
    pub story_source_sha256: String,
    pub scripts_sha256: String,
    pub battle_sha256: String,
    pub units: Vec<RosterUnit>,
    pub guest_steps: Vec<GuestStep>,
    pub members: Vec<StoryMember>,
    pub ramza_forms: Vec<RamzaFormStep>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryRosterError {
    InvalidJson,
    TooLarge,
    SourceMismatch,
    InvalidTable,
    InvalidGrowth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RosterRole {
    Guest,
    Member,
}

/// Every field of one constructed unit record that is not left at its empty value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitRecordPlan {
    pub character: u8,
    pub name_key: u16,
    pub sex: u8,
    pub job: u8,
    pub birthday: u8,
    pub zodiac: u8,
    pub level: u8,
    pub brave: u8,
    pub faith: u8,
    pub reaction: u16,
    pub support: u16,
    pub movement: u16,
    /// Head, body, accessory, right weapon, right shield, left weapon, left shield.
    pub equipment: [u16; 7],
    /// HP, MP, Speed, PA, MA stored bases.
    pub bases: [u32; 5],
    pub unlocked_jobs: u32,
    pub job_levels: [u8; JOB_SLOTS],
}

impl UnitRecordPlan {
    /// Total JP at the threshold of each job level; current JP stays zero.
    #[must_use]
    pub fn total_job_points(&self) -> [u16; JOB_SLOTS] {
        self.job_levels
            .map(|level| JOB_LEVEL_TOTAL_JP[usize::from(level.min(8))])
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedStoryRoster(StoryRosterDocument);

impl ValidatedStoryRoster {
    pub fn from_json(bytes: &[u8]) -> Result<Self, StoryRosterError> {
        if bytes.len() > MAX_BYTES {
            return Err(StoryRosterError::TooLarge);
        }
        let document = serde_json::from_slice(bytes).map_err(|_| StoryRosterError::InvalidJson)?;
        Self::validate(document)
    }

    pub fn validate(document: StoryRosterDocument) -> Result<Self, StoryRosterError> {
        if document.schema != "story_roster_v1"
            || document.profile != "english_steam_enhanced_manual"
            || document.story_source_sha256 != STORY_SOURCE_SHA256
            || document.scripts_sha256 != SCRIPTS_SHA256
            || document.battle_sha256 != BATTLE_SHA256
        {
            return Err(StoryRosterError::SourceMismatch);
        }
        let units = &document.units;
        let units_valid = units.len() <= MAX_UNITS && units.iter().all(valid_unit);
        let guests_valid = document.guest_steps.len() <= MAX_STEPS
            && document
                .guest_steps
                .windows(2)
                .all(|pair| pair[0].progress < pair[1].progress)
            && document.guest_steps.iter().all(|step| {
                step.units.len() <= usize::from(GUEST_POSITIONS)
                    && step.units.iter().enumerate().all(|(position, index)| {
                        units.get(*index).is_some_and(|unit| {
                            unit.level != PARTY_LEVEL
                                && step.units[..position]
                                    .iter()
                                    .all(|earlier| units[*earlier].name != unit.name)
                        })
                    })
            });
        let members_valid = document.members.len() <= MAX_UNITS
            && document
                .members
                .windows(2)
                .all(|pair| pair[0].joins <= pair[1].joins)
            && document.members.iter().enumerate().all(|(index, member)| {
                units.get(member.unit).is_some_and(|unit| {
                    document.members[..index].iter().all(|earlier| {
                        let other = &units[earlier.unit];
                        (other.sprite, other.name) != (unit.sprite, unit.name)
                    })
                })
            });
        let forms_valid = document.ramza_forms.len() <= RAMZA_FORMS.len()
            && document
                .ramza_forms
                .windows(2)
                .all(|pair| pair[0].progress < pair[1].progress)
            && document
                .ramza_forms
                .iter()
                .all(|form| ramza_form(form.character).is_some());
        if !units_valid || !guests_valid || !members_valid || !forms_valid {
            return Err(StoryRosterError::InvalidTable);
        }
        Ok(Self(document))
    }

    /// Guests in position order once `progress` is the last completed step.
    pub fn guests_at(&self, progress: i32) -> impl Iterator<Item = &RosterUnit> {
        self.0
            .guest_steps
            .iter()
            .rev()
            .find(|step| step.progress <= progress)
            .map(|step| step.units.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|index| &self.0.units[*index])
    }

    /// Every story member with whether it belongs to the party at `progress`.
    pub fn members_at(&self, progress: i32) -> impl Iterator<Item = (&RosterUnit, bool)> {
        self.0
            .members
            .iter()
            .map(move |member| (&self.0.units[member.unit], member.joins <= progress))
    }

    /// Members a step edit adds and removes, given the active (character,
    /// name key) pairs in the saved party.
    pub fn member_changes(
        &self,
        progress: i32,
        active: &[(u8, u16)],
    ) -> (Vec<&RosterUnit>, Vec<&RosterUnit>) {
        let (mut joins, mut leaves) = (Vec::new(), Vec::new());
        for (unit, present) in self.members_at(progress) {
            let saved = active.contains(&(unit.sprite, u16::from(unit.name)));
            if present && !saved {
                joins.push(unit);
            } else if !present && saved {
                leaves.push(unit);
            }
        }
        (joins, leaves)
    }

    /// Ramza's chapter form; the new-game form precedes the first change.
    #[must_use]
    pub fn ramza_form_at(&self, progress: i32) -> RamzaForm {
        self.0
            .ramza_forms
            .iter()
            .rev()
            .find(|form| form.progress <= progress)
            .and_then(|form| ramza_form(form.character))
            .unwrap_or(RAMZA_FORMS[0])
    }
}

#[must_use]
pub fn ramza_form(character: u8) -> Option<RamzaForm> {
    RAMZA_FORMS
        .into_iter()
        .find(|form| form.character == character)
}

fn valid_unit(unit: &RosterUnit) -> bool {
    let equipment_valid = unit
        .equipment
        .iter()
        .zip([
            &["Headgear"][..],
            &["Armor"],
            &["Accessories"],
            &["Weapons", "Shields"],
            &["Weapons", "Shields"],
        ])
        .all(|(item, families)| {
            *item == 0
                || *item >= GENERATED_ITEM
                || inventory_category(u16::from(*item))
                    .is_some_and(|family| families.contains(&family))
        });
    matches!(
        unit.flags & (MALE | FEMALE | MONSTER),
        MALE | FEMALE | MONSTER
    ) && unit.name != 0xff
        && ramza_form(unit.sprite).is_none()
        && ((1..=99).contains(&unit.level) || unit.level == PARTY_LEVEL)
        && unit.brave <= 100
        && unit.faith <= 100
        && day_of_year(unit.birth_month, unit.birth_day).is_some()
        && usize::from(unit.unlock_job) < JOB_SLOTS
        && unit.unlock_level <= 8
        && equipment_valid
}

/// Zodiac index (Aries = 0) for a one-based day of a 365-day year.
fn zodiac_index(day: u16) -> u8 {
    // Western sign starts; every saved unit in the example and m4b dumps fits them.
    const STARTS: [(u16, u8); 12] = [
        (20, 10),
        (50, 11),
        (80, 0),
        (110, 1),
        (141, 2),
        (173, 3),
        (204, 4),
        (235, 5),
        (266, 6),
        (297, 7),
        (327, 8),
        (356, 9),
    ];
    STARTS
        .iter()
        .rev()
        .find(|(start, _)| day >= *start)
        .map_or(9, |(_, sign)| *sign)
}

/// Builds the plan with SpawnData profile bases grown to the entry level, or to
/// `party_level` for members whose entry asks for the party's level.
pub fn plan_unit(
    unit: &RosterUnit,
    role: RosterRole,
    growth: GrowthCoefficients,
    party_level: u8,
) -> Result<UnitRecordPlan, StoryRosterError> {
    let level = if unit.level == PARTY_LEVEL && role == RosterRole::Member {
        party_level
    } else {
        unit.level
    };
    if !valid_unit(unit) || !(1..=99).contains(&level) {
        return Err(StoryRosterError::InvalidTable);
    }
    let day =
        day_of_year(unit.birth_month, unit.birth_day).ok_or(StoryRosterError::InvalidTable)?;
    let [day_low, day_high] = day.to_le_bytes();
    let monster = unit.flags & MONSTER != 0;
    let female = unit.flags & MALE == 0;
    // fftivc.utility.modloader TableData/SpawnData.xml at ba92f91, ids 0, 1 and 3:
    // HP, MP, Speed, PA, MA in 1/16384 units; the random variance stays zero.
    let profile: [u32; 5] = if monster {
        [35, 8, 5, 5, 5]
    } else if female {
        [28, 15, 6, 4, 5]
    } else {
        [30, 14, 6, 5, 4]
    };
    let fact = |value: u32| Fact {
        value: ValueState::Known(value * 16_384),
    };
    let start = StoredBases {
        hp: fact(profile[0]),
        mp: fact(profile[1]),
        speed: fact(profile[2]),
        physical_attack: fact(profile[3]),
        magical_attack: fact(profile[4]),
    };
    let grown = level_growth::simulate(&start, 1, level, growth, &[])
        .map_err(|_| StoryRosterError::InvalidGrowth)?;
    let known = |fact: &Fact<u32>| match fact.value {
        ValueState::Known(value) => Ok(value),
        _ => Err(StoryRosterError::InvalidGrowth),
    };
    let mut equipment = [EMPTY_ITEM; 7];
    for (index, item) in unit.equipment.iter().enumerate() {
        if *item >= GENERATED_ITEM {
            continue;
        }
        let shield = inventory_category(u16::from(*item)) == Some("Shields");
        let slot = match (index, shield) {
            (3, false) => 3,
            (3, true) => 4,
            (4, false) => 5,
            (4, true) => 6,
            _ => index,
        };
        equipment[slot] = u16::from(*item);
    }
    let ability = |id: u16| if id >= GENERATED_ABILITY { 0 } else { id };
    // Example-save humans start every job at 1, except slot 20 and the other
    // sex's Bard (17) or Dancer (18) slot; monsters keep only slot 21.
    let mut job_levels = [if monster { 0 } else { 1 }; JOB_SLOTS];
    job_levels[20] = 0;
    job_levels[21] = 1;
    let mut unlocked_jobs = 0;
    if !monster {
        job_levels[if female { 17 } else { 18 }] = 0;
        job_levels[usize::from(unit.unlock_job)] = unit.unlock_level.max(1);
        let bit = (u32::from(unit.unlock_job) / 8) * 8 + 7 - u32::from(unit.unlock_job) % 8;
        unlocked_jobs = STARTING_UNLOCKS | 1 << bit;
    }
    let kept_flags = match role {
        RosterRole::Guest => !LOAD_FORMATION,
        RosterRole::Member => !(LOAD_FORMATION | SAVE_FORMATION),
    };
    Ok(UnitRecordPlan {
        character: unit.sprite,
        name_key: u16::from(unit.name),
        sex: unit.flags & kept_flags,
        job: unit.job,
        birthday: day_low,
        zodiac: (zodiac_index(day) << 4) | day_high,
        level,
        brave: unit.brave,
        faith: unit.faith,
        reaction: ability(unit.reaction),
        support: ability(unit.support),
        movement: ability(unit.movement),
        equipment,
        bases: [
            known(&grown.hp)?,
            known(&grown.mp)?,
            known(&grown.speed)?,
            known(&grown.physical_attack)?,
            known(&grown.magical_attack)?,
        ],
        unlocked_jobs,
        job_levels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gaffgarion() -> RosterUnit {
        // Battle entry 272 unit 1; the example save's step-465 guest at position 50.
        RosterUnit {
            entry: 272,
            unit: 1,
            sprite: 23,
            name: 23,
            flags: 0x91,
            level: 10,
            birth_month: 8,
            birth_day: 26,
            brave: 61,
            faith: 67,
            job: 23,
            unlock_job: 6,
            unlock_level: 2,
            reaction: 0,
            support: 0,
            movement: 0,
            equipment: [150, 177, 215, 22, 133],
        }
    }

    fn boco() -> RosterUnit {
        // Battle entry 404 unit 0; position 8 from example step 478.
        RosterUnit {
            entry: 404,
            unit: 0,
            sprite: 130,
            name: 118,
            flags: 0x30,
            level: 10,
            birth_month: 3,
            birth_day: 21,
            brave: 68,
            faith: 48,
            job: 94,
            unlock_job: 0,
            unlock_level: 0,
            reaction: 0x1fe,
            support: 0x1fe,
            movement: 0x1fe,
            equipment: [0; 5],
        }
    }

    fn growth(speed: u8, physical_attack: u8, magical_attack: u8) -> GrowthCoefficients {
        GrowthCoefficients {
            hp: 11,
            mp: 11,
            speed,
            physical_attack,
            magical_attack,
        }
    }

    fn document() -> StoryRosterDocument {
        StoryRosterDocument {
            schema: "story_roster_v1".into(),
            profile: "english_steam_enhanced_manual".into(),
            story_source_sha256: STORY_SOURCE_SHA256.into(),
            scripts_sha256: SCRIPTS_SHA256.into(),
            battle_sha256: BATTLE_SHA256.into(),
            units: vec![gaffgarion(), boco()],
            guest_steps: vec![
                GuestStep {
                    progress: 450,
                    units: vec![0],
                },
                GuestStep {
                    progress: 525,
                    units: vec![],
                },
            ],
            members: vec![StoryMember {
                unit: 1,
                joins: 470,
            }],
            ramza_forms: vec![RamzaFormStep {
                progress: 450,
                character: 2,
            }],
        }
    }

    #[test]
    fn game_created_units_match_the_planned_fixed_fields_and_bases() -> Result<(), StoryRosterError>
    {
        let plan = plan_unit(&gaffgarion(), RosterRole::Guest, growth(100, 50, 50), 1)?;
        assert_eq!((plan.birthday, plan.zodiac, plan.sex), (238, 0x50, 0x91));
        assert_eq!(
            (plan.level, plan.brave, plan.faith, plan.job, plan.name_key),
            (10, 61, 67, 23, 23)
        );
        assert_eq!(plan.equipment, [150, 177, 215, 22, 255, 255, 133]);
        // Saved Speed/PA/MA are exact; HP/MP add the game's random variance.
        assert_eq!(&plan.bases[2..], &[107_061, 96_374, 77_101]);
        assert!(plan.bases[0] <= 900_487 && plan.bases[1] <= 426_938);
        assert_eq!(plan.unlocked_jobs, 0xc2);
        assert_eq!(
            (plan.job_levels[6], plan.job_levels[18], plan.job_levels[17]),
            (2, 0, 1)
        );
        assert_eq!(plan.total_job_points()[6], 200);
        let member = plan_unit(&gaffgarion(), RosterRole::Member, growth(100, 50, 50), 1)?;
        assert_eq!(member.sex, 0x90);
        let agrias = RosterUnit {
            sprite: 52,
            name: 52,
            flags: 0x51,
            birth_month: 6,
            birth_day: 22,
            equipment: [149, 178, 255, 22, 132],
            ..gaffgarion()
        };
        let plan = plan_unit(&agrias, RosterRole::Guest, growth(100, 50, 46), 1)?;
        assert_eq!((plan.birthday, plan.zodiac), (173, 0x30));
        assert_eq!(&plan.bases[2..], &[107_061, 77_101, 97_598]);
        assert_eq!(plan.job_levels[17], 0);
        // Delita's 25 November (day 329) stores its high bit in the zodiac byte.
        let delita = RosterUnit {
            birth_month: 11,
            birth_day: 25,
            level: 1,
            reaction: 0x1fe,
            equipment: [254, 254, 254, 19, 254],
            ..gaffgarion()
        };
        let plan = plan_unit(&delita, RosterRole::Guest, growth(100, 50, 50), 1)?;
        assert_eq!((plan.birthday, plan.zodiac, plan.reaction), (73, 0x81, 0));
        assert_eq!(plan.bases, [491_520, 229_376, 98_304, 81_920, 65_536]);
        assert_eq!(plan.equipment, [255, 255, 255, 19, 255, 255, 255]);
        Ok(())
    }

    #[test]
    fn monster_members_use_the_monster_profile_and_saved_layout() -> Result<(), StoryRosterError> {
        let plan = plan_unit(&boco(), RosterRole::Member, growth(75, 35, 7), 1)?;
        assert_eq!((plan.character, plan.name_key, plan.sex), (130, 118, 0x30));
        assert_eq!(plan.equipment, [0, 0, 0, 0, 255, 0, 255]);
        assert_eq!((plan.unlocked_jobs, plan.reaction), (0, 0));
        assert_eq!(
            plan.job_levels.iter().filter(|level| **level != 0).count(),
            1
        );
        assert_eq!(plan.job_levels[21], 1);
        // Example Boco Speed 91,613 at level 10; PA/MA carry the game's variance.
        assert!((91_600..=91_625).contains(&plan.bases[2]));
        let party = RosterUnit {
            level: PARTY_LEVEL,
            ..boco()
        };
        assert_eq!(
            plan_unit(&party, RosterRole::Member, growth(75, 35, 7), 24)?.level,
            24
        );
        assert_eq!(
            plan_unit(&party, RosterRole::Guest, growth(75, 35, 7), 24).err(),
            Some(StoryRosterError::InvalidTable)
        );
        Ok(())
    }

    #[test]
    fn zodiac_starts_follow_the_saved_units() {
        for (month, day, sign) in [
            (1, 1, 9),
            (1, 20, 10),
            (2, 19, 11),
            (3, 21, 0),
            (5, 11, 1),
            (6, 22, 3),
            (7, 24, 4),
            (10, 15, 6),
            (11, 22, 7),
            (11, 23, 8),
            (12, 22, 9),
            (12, 31, 9),
        ] {
            let day = day_of_year(month, day).unwrap_or_default();
            assert_eq!(zodiac_index(day), sign, "{month}/{day}");
        }
        assert_eq!(day_of_year(2, 29), None);
        assert_eq!(day_of_year(13, 1), None);
    }

    #[test]
    fn steps_resolve_guests_members_and_ramza_forms() -> Result<(), StoryRosterError> {
        let roster = ValidatedStoryRoster::validate(document())?;
        assert_eq!(roster.guests_at(440).count(), 0);
        assert_eq!(
            roster
                .guests_at(478)
                .map(|unit| unit.name)
                .collect::<Vec<_>>(),
            [23]
        );
        assert_eq!(roster.guests_at(940).count(), 0);
        let members = |progress| {
            roster
                .members_at(progress)
                .map(|(unit, present)| (unit.name, present))
                .collect::<Vec<_>>()
        };
        assert_eq!(members(465), [(118, false)]);
        assert_eq!(members(470), [(118, true)]);
        let names =
            |units: Vec<&RosterUnit>| units.iter().map(|unit| unit.name).collect::<Vec<_>>();
        let (joins, leaves) = roster.member_changes(470, &[]);
        assert_eq!((names(joins), names(leaves)), (vec![118], vec![]));
        let (joins, leaves) = roster.member_changes(465, &[(130, 118)]);
        assert_eq!((names(joins), names(leaves)), (vec![], vec![118]));
        assert_eq!(roster.member_changes(470, &[(130, 118)]), (vec![], vec![]));
        assert_eq!(roster.ramza_form_at(40), RAMZA_FORMS[0]);
        assert_eq!(roster.ramza_form_at(450), RAMZA_FORMS[1]);
        Ok(())
    }

    #[test]
    fn invalid_documents_fail_closed() {
        let mut wrong_source = document();
        wrong_source.scripts_sha256 = "0".repeat(64);
        let mut duplicate = document();
        duplicate.guest_steps[0].units = vec![0, 0];
        let mut missing = document();
        missing.guest_steps[0].units = vec![2];
        let mut unordered = document();
        unordered.guest_steps.reverse();
        let mut generated = document();
        generated.units[0].level = 0xfd;
        let mut party_guest = document();
        party_guest.units[0].level = PARTY_LEVEL;
        let mut wrong_slot = document();
        wrong_slot.units[0].equipment[0] = 177;
        let mut twice = document();
        twice.members.push(StoryMember {
            unit: 1,
            joins: 480,
        });
        let mut form = document();
        form.ramza_forms[0].character = 4;
        assert_eq!(
            ValidatedStoryRoster::validate(wrong_source).err(),
            Some(StoryRosterError::SourceMismatch)
        );
        for invalid in [
            duplicate,
            missing,
            unordered,
            generated,
            party_guest,
            wrong_slot,
            twice,
            form,
        ] {
            assert_eq!(
                ValidatedStoryRoster::validate(invalid).err(),
                Some(StoryRosterError::InvalidTable)
            );
        }
        assert_eq!(
            ValidatedStoryRoster::from_json(b"{}").err(),
            Some(StoryRosterError::InvalidJson)
        );
    }
}
