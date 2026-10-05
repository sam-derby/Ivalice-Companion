//! Main story chapter, objective and world-map area labels joined from the game's
//! own tables. FF16Tools dd91fb4 Nex/Layouts/ffto/{Chapter,GameProgress,
//! WorldMapProgressInfo,PlaceName,ScenarioId}.layout define the source columns;
//! scripts/import-story-progress.py normalizes them and replays event scripts.

use serde::{Deserialize, Serialize};

use crate::{
    reader::{CatalogueRef, Fact, SavedProgress},
    ValueState,
};

const SOURCE_SHA256: &str = "68e926c75e6bc151c8fe66b94cea6b1d3a610d13da4f5824f6a596448777c76a";
const SCRIPTS_SHA256: &str = "5afc5268cbac2df9fb8e1093dd8e6a71b62e4688c7c764e415111899bbf179a4";
const ROUTES_SHA256: &str = "a4376bcd4d0f94481adce0e1cb198f6044b778dc693a6be07d2ebcec7d8ab4dc";
/// Event flags showing world-map areas (0x200-0x22B) and roads (0x22C-0x25E).
pub const MAP_FLAGS: std::ops::Range<u16> = 0x200..0x25f;
const ROAD_FLAGS: u16 = 0x22c;
// Areas 44-46 are shown by roads 48-50 rather than by an area flag.
const MAP_AREAS: u8 = 44;
const SUB_LOCATION_ROAD: u16 = 48;
/// User GameFlag bytes; GameProgress tracks 1-11 are side quests.
pub const GAME_FLAG_COUNT: u8 = 32;
pub const SIDE_TRACKS: std::ops::Range<u8> = 1..12;
/// Event variable holding the saved world-map area.
pub const CURRENT_AREA_VARIABLE: u16 = 0x31;
const MAX_ROUTES: usize = 128;
const MAX_BYTES: usize = 64 * 1024;
const MAX_CHAPTERS: usize = 16;
const MAX_STEPS: usize = 1024;
const MAX_TEXT_CHARS: usize = 256;
// WorldMapProgressInfo TownId 47 means no town.
const AREA_LIMIT: u8 = 47;
// Event work ids 0x00-0x7F are integers; 0x80-0x3FF are bit flags.
const FLAG_START: u16 = 0x80;
const VARIABLE_LIMIT: u16 = 0x400;
const GIL_VARIABLE: u16 = 0x2c;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChapterStart {
    pub start: i32,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStep {
    pub progress: i32,
    pub objective: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldArea {
    pub id: u8,
    pub name: String,
}

/// Absolute event-work values after one ScenarioId step's scripts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepEffect {
    pub progress: i32,
    pub writes: Vec<(u16, i32)>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapStep {
    pub progress: i32,
    pub flags: Vec<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameFlagStep {
    pub progress: i32,
    pub flags: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryProgressDocument {
    pub schema: String,
    pub profile: String,
    pub source_sha256: String,
    pub scripts_sha256: String,
    pub routes_sha256: String,
    pub chapters: Vec<ChapterStart>,
    pub main_objectives: Vec<StoryStep>,
    pub areas: Vec<WorldArea>,
    /// Route key and its two end areas.
    pub routes: Vec<(u16, u8, u8)>,
    pub managed_variables: Vec<u16>,
    pub step_effects: Vec<StepEffect>,
    /// Shown map flags after each step where they change.
    pub world_map: Vec<MapStep>,
    pub managed_game_flags: Vec<u8>,
    /// Set managed game flags after each step where they change.
    pub game_flags: Vec<GameFlagStep>,
    /// Side-quest tracks are reset below this main step.
    pub side_tracks_from: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryProgressError {
    InvalidJson,
    TooLarge,
    SourceMismatch,
    InvalidTable,
}

#[derive(Clone, Debug)]
pub struct ValidatedStoryProgress(StoryProgressDocument);

impl ValidatedStoryProgress {
    pub fn from_json(bytes: &[u8]) -> Result<Self, StoryProgressError> {
        if bytes.len() > MAX_BYTES {
            return Err(StoryProgressError::TooLarge);
        }
        let document =
            serde_json::from_slice(bytes).map_err(|_| StoryProgressError::InvalidJson)?;
        Self::validate(document)
    }

    pub fn validate(document: StoryProgressDocument) -> Result<Self, StoryProgressError> {
        if document.schema != "story_progress_v3"
            || document.profile != "english_steam_enhanced_manual"
            || document.source_sha256 != SOURCE_SHA256
            || document.scripts_sha256 != SCRIPTS_SHA256
            || document.routes_sha256 != ROUTES_SHA256
        {
            return Err(StoryProgressError::SourceMismatch);
        }
        let chapters_valid = !document.chapters.is_empty()
            && document.chapters.len() <= MAX_CHAPTERS
            && document.chapters[0].start == 0
            && document
                .chapters
                .windows(2)
                .all(|pair| pair[0].start < pair[1].start)
            && document
                .chapters
                .iter()
                .all(|chapter| valid_text(chapter.title.as_deref()));
        let steps_valid = !document.main_objectives.is_empty()
            && document.main_objectives.len() <= MAX_STEPS
            && document.main_objectives[0].progress >= 0
            && document
                .main_objectives
                .windows(2)
                .all(|pair| pair[0].progress < pair[1].progress)
            && document
                .main_objectives
                .iter()
                .all(|step| valid_text(step.objective.as_deref()));
        let areas_valid = document.areas.len() <= usize::from(AREA_LIMIT)
            && document
                .areas
                .windows(2)
                .all(|pair| pair[0].id < pair[1].id)
            && document
                .areas
                .iter()
                .all(|area| area.id < AREA_LIMIT && valid_text(Some(&area.name)));
        let managed = &document.managed_variables;
        let managed_valid = managed.windows(2).all(|pair| pair[0] < pair[1])
            && managed
                .iter()
                .all(|id| *id < VARIABLE_LIMIT && *id != GIL_VARIABLE && !MAP_FLAGS.contains(id));
        let map_valid = document.world_map.len() <= MAX_STEPS
            && document
                .world_map
                .windows(2)
                .all(|pair| pair[0].progress < pair[1].progress)
            && document.world_map.iter().all(|step| {
                step.flags.windows(2).all(|pair| pair[0] < pair[1])
                    && step.flags.iter().all(|flag| MAP_FLAGS.contains(flag))
            });
        let game_flags = &document.managed_game_flags;
        let game_flags_valid = game_flags.windows(2).all(|pair| pair[0] < pair[1])
            && game_flags.iter().all(|flag| *flag < GAME_FLAG_COUNT)
            && document.game_flags.len() <= MAX_STEPS
            && document
                .game_flags
                .windows(2)
                .all(|pair| pair[0].progress < pair[1].progress)
            && document.game_flags.iter().all(|step| {
                step.flags.windows(2).all(|pair| pair[0] < pair[1])
                    && step.flags.iter().all(|flag| game_flags.contains(flag))
            });
        let routes_valid = document.routes.len() <= MAX_ROUTES
            && document.routes.iter().all(|(key, first, second)| {
                MAP_FLAGS.contains(&(ROAD_FLAGS + key))
                    && *first < AREA_LIMIT
                    && *second < AREA_LIMIT
            });
        let effects_valid = document.step_effects.len() <= MAX_STEPS
            && document
                .step_effects
                .windows(2)
                .all(|pair| pair[0].progress < pair[1].progress)
            && document.step_effects.iter().all(|step| {
                step.writes.windows(2).all(|pair| pair[0].0 < pair[1].0)
                    && step.writes.iter().all(|(id, value)| {
                        managed.binary_search(id).is_ok()
                            && (*id < FLAG_START || matches!(value, 0 | 1))
                    })
            });
        if !chapters_valid
            || !steps_valid
            || !areas_valid
            || !managed_valid
            || !effects_valid
            || !map_valid
            || !game_flags_valid
            || !routes_valid
            || document.side_tracks_from < 0
        {
            return Err(StoryProgressError::InvalidTable);
        }
        Ok(Self(document))
    }

    /// The last chapter whose start the saved value has reached; untitled rows stay unknown.
    #[must_use]
    pub fn chapter(&self, progress: i32) -> Option<&str> {
        self.0
            .chapters
            .iter()
            .rev()
            .find(|chapter| chapter.start <= progress)
            .and_then(|chapter| chapter.title.as_deref())
    }

    /// Exact step join; the importer already resolved each step's displayed caption.
    #[must_use]
    pub fn objective(&self, progress: i32) -> Option<&str> {
        self.0
            .main_objectives
            .binary_search_by_key(&progress, |step| step.progress)
            .ok()
            .and_then(|index| self.0.main_objectives[index].objective.as_deref())
    }

    #[must_use]
    pub fn area(&self, id: u8) -> Option<&str> {
        self.0
            .areas
            .binary_search_by_key(&id, |area| area.id)
            .ok()
            .map(|index| self.0.areas[index].name.as_str())
    }

    /// Selectable main story steps in story order.
    pub fn steps(&self) -> impl Iterator<Item = i32> + '_ {
        self.0.main_objectives.iter().map(|step| step.progress)
    }

    /// Every managed variable's replayed value once `progress` is the last completed
    /// step; variables no earlier step wrote are zero. Unmanaged event work is untouched.
    #[must_use]
    pub fn variable_plan(&self, progress: i32) -> Option<Vec<(u16, i32)>> {
        self.0
            .main_objectives
            .binary_search_by_key(&progress, |step| step.progress)
            .ok()?;
        let mut values: Vec<(u16, i32)> =
            self.0.managed_variables.iter().map(|id| (*id, 0)).collect();
        for step in self
            .0
            .step_effects
            .iter()
            .take_while(|step| step.progress <= progress)
        {
            for (id, value) in &step.writes {
                if let Ok(index) = values.binary_search_by_key(id, |(managed, _)| *managed) {
                    values[index].1 = *value;
                }
            }
        }
        let shown = self
            .0
            .world_map
            .iter()
            .take_while(|step| step.progress <= progress)
            .last()
            .map_or(&[][..], |step| step.flags.as_slice());
        values.extend(MAP_FLAGS.map(|flag| (flag, i32::from(shown.binary_search(&flag).is_ok()))));
        Some(values)
    }

    fn shown_map(&self, progress: i32) -> &[u16] {
        self.0
            .world_map
            .iter()
            .take_while(|step| step.progress <= progress)
            .last()
            .map_or(&[][..], |step| step.flags.as_slice())
    }

    /// Every managed game flag's value once `progress` is the last completed step.
    #[must_use]
    pub fn game_flag_plan(&self, progress: i32) -> Option<Vec<(u8, bool)>> {
        self.0
            .main_objectives
            .binary_search_by_key(&progress, |step| step.progress)
            .ok()?;
        let set = self
            .0
            .game_flags
            .iter()
            .take_while(|step| step.progress <= progress)
            .last()
            .map_or(&[][..], |step| step.flags.as_slice());
        Some(
            self.0
                .managed_game_flags
                .iter()
                .map(|flag| (*flag, set.contains(flag)))
                .collect(),
        )
    }

    /// Side-quest tracks to clear: all of them start in a later chapter.
    #[must_use]
    pub fn side_track_plan(&self, progress: i32, saved: &[i32; 12]) -> Vec<(u8, i32)> {
        if progress >= self.0.side_tracks_from {
            return Vec::new();
        }
        SIDE_TRACKS
            .filter(|track| saved[usize::from(*track)] != 0)
            .map(|track| (track, 0))
            .collect()
    }

    /// The nearest area by road that the map shows at `progress`, when the saved
    /// area is hidden there; `None` keeps the saved area.
    #[must_use]
    pub fn area_plan(&self, progress: i32, saved: u8) -> Option<u8> {
        let shown = self.shown_map(progress);
        let visible = |area: u8| {
            let flag = if area < MAP_AREAS {
                MAP_FLAGS.start + u16::from(area)
            } else {
                ROAD_FLAGS + SUB_LOCATION_ROAD + u16::from(area - MAP_AREAS)
            };
            area < AREA_LIMIT && shown.binary_search(&flag).is_ok()
        };
        if saved >= AREA_LIMIT || visible(saved) {
            return None;
        }
        let mut seen = vec![saved];
        let mut frontier = vec![saved];
        while !frontier.is_empty() {
            let mut next: Vec<u8> = self
                .0
                .routes
                .iter()
                .filter_map(|(_, first, second)| {
                    if frontier.contains(first) {
                        Some(*second)
                    } else if frontier.contains(second) {
                        Some(*first)
                    } else {
                        None
                    }
                })
                .filter(|area| !seen.contains(area))
                .collect();
            next.sort_unstable();
            next.dedup();
            if let Some(area) = next.iter().copied().find(|area| visible(*area)) {
                return Some(area);
            }
            seen.extend(&next);
            frontier = next;
        }
        None
    }

    pub fn apply(&self, progress: &mut SavedProgress) {
        if let ValueState::Known(id) = progress.area_index.value {
            if let Some(name) = self.area(id) {
                progress.location = Fact {
                    value: ValueState::Known(CatalogueRef {
                        id: format!("area:{id}"),
                        label: ValueState::Known(name.to_owned()),
                        description: ValueState::Unknown,
                        asset_key: ValueState::Unknown,
                    }),
                };
            }
        }
        let ValueState::Known(value) = progress.story_progress.value else {
            return;
        };
        progress.chapter = label(self.chapter(value));
        progress.objective = label(self.objective(value));
    }
}

fn valid_text(text: Option<&str>) -> bool {
    text.is_none_or(|text| !text.trim().is_empty() && text.chars().count() <= MAX_TEXT_CHARS)
}

fn label(text: Option<&str>) -> Fact<String> {
    Fact {
        value: text.map_or(ValueState::Unknown, |text| {
            ValueState::Known(text.to_owned())
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> StoryProgressDocument {
        StoryProgressDocument {
            schema: "story_progress_v3".into(),
            profile: "english_steam_enhanced_manual".into(),
            source_sha256: SOURCE_SHA256.into(),
            scripts_sha256: SCRIPTS_SHA256.into(),
            routes_sha256: ROUTES_SHA256.into(),
            chapters: vec![
                ChapterStart {
                    start: 0,
                    title: Some("Opening".into()),
                },
                ChapterStart {
                    start: 40,
                    title: Some("Second".into()),
                },
                ChapterStart {
                    start: 2040,
                    title: None,
                },
            ],
            main_objectives: vec![
                StoryStep {
                    progress: 40,
                    objective: Some("Reach the gate.".into()),
                },
                StoryStep {
                    progress: 86,
                    objective: None,
                },
            ],
            areas: vec![
                WorldArea {
                    id: 2,
                    name: "North Gate".into(),
                },
                WorldArea {
                    id: 46,
                    name: "Last Road".into(),
                },
            ],
            managed_variables: vec![0x6e, 0x1a4, 0x1a5],
            routes: vec![(1, 24, 2), (3, 2, 6), (48, 6, 44)],
            step_effects: vec![
                StepEffect {
                    progress: 30,
                    writes: vec![(0x6e, 1), (0x1a4, 1)],
                },
                StepEffect {
                    progress: 86,
                    writes: vec![(0x6e, 2), (0x1a4, 0), (0x1a5, 1)],
                },
            ],
            world_map: vec![
                MapStep {
                    progress: 10,
                    flags: vec![0x206, 0x25c],
                },
                MapStep {
                    progress: 70,
                    flags: vec![0x202, 0x206],
                },
            ],
            managed_game_flags: vec![1, 9],
            game_flags: vec![
                GameFlagStep {
                    progress: 10,
                    flags: vec![1],
                },
                GameFlagStep {
                    progress: 70,
                    flags: vec![9],
                },
            ],
            side_tracks_from: 86,
        }
    }

    #[test]
    fn game_flags_side_tracks_and_area_follow_the_step() {
        let story = ValidatedStoryProgress::validate(document())
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(story.game_flag_plan(40), Some(vec![(1, true), (9, false)]));
        assert_eq!(story.game_flag_plan(86), Some(vec![(1, false), (9, true)]));
        assert_eq!(story.game_flag_plan(41), None);
        let mut tracks = [0; 12];
        tracks[0] = 940;
        tracks[2] = 60;
        assert_eq!(story.side_track_plan(40, &tracks), [(2, 0)]);
        assert_eq!(story.side_track_plan(86, &tracks), []);
        // Step 40 shows Gariland (6) and the Akademy road (48 -> area 44).
        assert_eq!(story.area_plan(40, 24), Some(6));
        assert_eq!(story.area_plan(40, 44), None);
        assert_eq!(story.area_plan(40, 6), None);
        assert_eq!(story.area_plan(86, 44), Some(6));
        assert_eq!(story.area_plan(40, 47), None);
        assert_eq!(story.area_plan(40, 30), None);
        let mut unmanaged = document();
        unmanaged.game_flags[0].flags.push(3);
        let mut wide = document();
        wide.managed_game_flags.push(32);
        let mut road = document();
        road.routes.push((0x40, 0, 1));
        for changed in [unmanaged, wide, road] {
            assert_eq!(
                ValidatedStoryProgress::validate(changed).err(),
                Some(StoryProgressError::InvalidTable)
            );
        }
    }

    fn map(plan: &[(u16, i32)]) -> Vec<u16> {
        plan.iter()
            .filter(|(id, value)| MAP_FLAGS.contains(id) && *value == 1)
            .map(|(id, _)| *id)
            .collect()
    }

    #[test]
    fn variable_plan_sets_every_map_flag_from_the_latest_change() {
        let story = ValidatedStoryProgress::validate(document())
            .unwrap_or_else(|error| panic!("{error:?}"));
        let plan = story.variable_plan(40).unwrap_or_default();
        assert_eq!(plan.len(), 3 + MAP_FLAGS.len());
        assert_eq!(map(&plan), [0x206, 0x25c]);
        assert_eq!(
            map(&story.variable_plan(86).unwrap_or_default()),
            [0x202, 0x206]
        );
        let mut managed_map = document();
        managed_map.managed_variables.push(0x206);
        let mut outside = document();
        outside.world_map[0].flags.push(0x25f);
        let mut unordered = document();
        unordered.world_map.reverse();
        for changed in [managed_map, outside, unordered] {
            assert_eq!(
                ValidatedStoryProgress::validate(changed).err(),
                Some(StoryProgressError::InvalidTable)
            );
        }
    }

    #[test]
    fn variable_plan_replays_steps_and_clears_later_writes() {
        let story = ValidatedStoryProgress::validate(document())
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(story.steps().collect::<Vec<_>>(), [40, 86]);
        assert_eq!(
            story.variable_plan(40).map(|plan| plan[..3].to_vec()),
            Some(vec![(0x6e, 1), (0x1a4, 1), (0x1a5, 0)])
        );
        assert_eq!(
            story.variable_plan(86).map(|plan| plan[..3].to_vec()),
            Some(vec![(0x6e, 2), (0x1a4, 0), (0x1a5, 1)])
        );
        assert_eq!(story.variable_plan(30), None);
    }

    #[test]
    fn step_effects_fail_closed() {
        let mut unmanaged = document();
        unmanaged.step_effects[0].writes.push((0x1a6, 1));
        let mut wide_flag = document();
        wide_flag.step_effects[0].writes[1].1 = 2;
        let mut gil = document();
        gil.managed_variables.insert(0, 0x2c);
        let mut unordered = document();
        unordered.step_effects.reverse();
        for changed in [unmanaged, wide_flag, gil, unordered] {
            assert_eq!(
                ValidatedStoryProgress::validate(changed).err(),
                Some(StoryProgressError::InvalidTable)
            );
        }
        let mut scripts = document();
        scripts.scripts_sha256 = "0".repeat(64);
        assert_eq!(
            ValidatedStoryProgress::validate(scripts).err(),
            Some(StoryProgressError::SourceMismatch)
        );
    }

    #[test]
    fn chapter_uses_reached_start_and_objective_uses_exact_step() {
        let story = ValidatedStoryProgress::validate(document())
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(story.chapter(39), Some("Opening"));
        assert_eq!(story.chapter(40), Some("Second"));
        assert_eq!(story.chapter(2039), Some("Second"));
        assert_eq!(story.chapter(2040), None);
        assert_eq!(story.chapter(-1), None);
        assert_eq!(story.objective(40), Some("Reach the gate."));
        assert_eq!(story.objective(41), None);
        assert_eq!(story.objective(86), None);
    }

    #[test]
    fn saved_area_joins_only_a_named_world_map_area() {
        let story = ValidatedStoryProgress::validate(document())
            .unwrap_or_else(|error| panic!("{error:?}"));
        fn unknown<T>() -> Fact<T> {
            Fact {
                value: ValueState::Unknown,
            }
        }
        let mut progress = SavedProgress {
            title: unknown(),
            saved_at_unix_seconds: unknown(),
            hero_name: unknown(),
            location: unknown(),
            difficulty: unknown(),
            difficulty_code: unknown(),
            chapter: unknown(),
            story_progress: unknown(),
            objective: unknown(),
            area_index: Fact {
                value: ValueState::Known(46),
            },
            ramza_level: unknown(),
            story: unknown(),
            play_time_seconds: unknown(),
            next_event_id: unknown(),
            unnamed_event_values: unknown(),
            errands: unknown(),
            events: unknown(),
            recruitment: unknown(),
        };
        story.apply(&mut progress);
        let ValueState::Known(area) = &progress.location.value else {
            panic!("area expected")
        };
        assert_eq!(area.id, "area:46");
        assert_eq!(area.label, ValueState::Known("Last Road".into()));
        assert_eq!(progress.chapter.value, ValueState::Unknown);
        for id in [3, 47, 255] {
            progress.location = unknown();
            progress.area_index = Fact {
                value: ValueState::Known(id),
            };
            story.apply(&mut progress);
            assert_eq!(progress.location.value, ValueState::Unknown);
        }
    }

    #[test]
    fn source_and_table_shape_fail_closed() {
        let mut changed = document();
        changed.source_sha256 = "0".repeat(64);
        assert_eq!(
            ValidatedStoryProgress::validate(changed).err(),
            Some(StoryProgressError::SourceMismatch)
        );
        let mut unordered = document();
        unordered.main_objectives.reverse();
        assert_eq!(
            ValidatedStoryProgress::validate(unordered).err(),
            Some(StoryProgressError::InvalidTable)
        );
        let mut late_start = document();
        late_start.chapters[0].start = 10;
        assert_eq!(
            ValidatedStoryProgress::validate(late_start).err(),
            Some(StoryProgressError::InvalidTable)
        );
        let mut blank = document();
        blank.chapters[1].title = Some(" ".into());
        assert_eq!(
            ValidatedStoryProgress::validate(blank).err(),
            Some(StoryProgressError::InvalidTable)
        );
        let mut no_town = document();
        no_town.areas[1].id = AREA_LIMIT;
        assert_eq!(
            ValidatedStoryProgress::validate(no_town).err(),
            Some(StoryProgressError::InvalidTable)
        );
        assert_eq!(
            ValidatedStoryProgress::from_json(b"{}").err(),
            Some(StoryProgressError::InvalidJson)
        );
    }
}
