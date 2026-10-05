//! Chapter 4 side quests read from event flags. Each scene's flags are the ones
//! its optional event script (ScenarioId keys 5000-5130 in 0004.pac, scripts
//! from 0005.pac script/enhanced) is alone in setting; no main-story script
//! writes them. Quest and episode names follow the Zerobandwidth walkthrough's
//! quest list, matched to each event's dialogue in 0002.en.pac
//! fftpack/text/eventNNN.en.mes. Worked out statically and not yet checked
//! against a Chapter 4 save.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SideQuestScene {
    pub label: &'static str,
    /// The scene has been seen when any of these flags is set.
    pub flags: &'static [u16],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SideQuestCounter {
    pub label: &'static str,
    pub variable: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SideQuest {
    pub name: &'static str,
    pub scenes: &'static [SideQuestScene],
    pub counter: Option<SideQuestCounter>,
}

const fn scene(label: &'static str, flags: &'static [u16]) -> SideQuestScene {
    SideQuestScene { label, flags }
}

// Ordering evidence: events 467 and 483 add 1 to flags 0x3D6 and 0x3D8, and
// event 489 adds 2 to both, so The Curse follows Beowulf the Hunter and The Holy Dragon.
pub const SIDE_QUESTS: &[SideQuest] = &[
    SideQuest {
        name: "Lost Technology",
        scenes: &[
            // Event 211 (scenario 5020).
            scene("The Metallic Sphere", &[0x99]),
            // Events 466 and 467 (scenario 5080).
            scene("Beowulf the Hunter", &[0xa7]),
            // Event 483 (scenario 5100).
            scene("The Holy Dragon", &[0xa9]),
            // Event 213 (scenario 5025).
            scene("The Automaton", &[0x9a]),
            // Event 215 (scenario 5030).
            scene("The Orrery", &[0x9b]),
            // Events 487 and 491 (scenarios 5125, 5126).
            scene("Nelveska's Guardian", &[0xab]),
            // Event 489 (scenario 5130).
            scene("The Curse", &[0xac]),
            // Event 217 (scenario 5040).
            scene("Cloud", &[0x9c]),
            // Event 219 (scenario 5050).
            scene("Aerith", &[0x3ee]),
        ],
        counter: None,
    },
    SideQuest {
        name: "Flower peddler",
        // Events 460 and 461 (scenario 5060): a flower declined or bought.
        scenes: &[scene("Offered a flower", &[0xa2])],
        counter: None,
    },
    SideQuest {
        name: "Into the Dark",
        scenes: &[
            // Event 463 (scenario 5070).
            scene("Midlight's Deep rumour", &[0xa5]),
            // Event 113 (scenario 5010).
            scene("Terminus", &[0x95]),
        ],
        // Event 93 (scenario 5000, "Found a passage leading deeper") adds 1.
        counter: Some(SideQuestCounter {
            label: "Deeper passages found",
            variable: 0x65,
        }),
    },
];

/// Whether each scene of `quest` has been seen; `None` when a flag is unreadable.
pub fn scenes_seen(quest: &SideQuest, flag: impl Fn(u16) -> Option<bool>) -> Option<Vec<bool>> {
    quest
        .scenes
        .iter()
        .map(|scene| {
            scene
                .flags
                .iter()
                .map(|id| flag(*id))
                .try_fold(false, |seen, value| Some(seen || value?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flag_belongs_to_one_scene_and_is_a_flag() {
        let mut flags: Vec<u16> = SIDE_QUESTS
            .iter()
            .flat_map(|quest| quest.scenes.iter())
            .flat_map(|scene| scene.flags.iter().copied())
            .collect();
        assert!(flags.iter().all(|id| (0x80..0x400).contains(id)));
        let count = flags.len();
        flags.sort_unstable();
        flags.dedup();
        assert_eq!(flags.len(), count);
    }

    #[test]
    fn a_scene_is_seen_when_any_of_its_flags_is_set() {
        const SCENES: &[SideQuestScene] =
            &[scene("Either", &[0x9b, 0x9c]), scene("Other", &[0xa2])];
        let quest = SideQuest {
            name: "Test",
            scenes: SCENES,
            counter: None,
        };
        let seen = scenes_seen(&quest, |id| Some(id == 0x9c));
        assert_eq!(seen, Some(vec![true, false]));
        assert_eq!(scenes_seen(&quest, |_| None), None);
    }

    #[test]
    fn counters_are_event_variables() {
        assert!(SIDE_QUESTS
            .iter()
            .filter_map(|quest| quest.counter)
            .all(|counter| counter.variable < 0x80));
    }
}
