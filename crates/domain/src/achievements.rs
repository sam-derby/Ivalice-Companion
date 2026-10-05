//! In-game achievement descriptions joined to the save's FftoAchievement bytes.
//! scripts/import-achievements.py normalizes the game's Achievement table.

use serde::{Deserialize, Serialize};

const SOURCE_SHA256: &str = "e4a57d259417f85b95bfd88f467be317b850d2df4e3ce45074db2511cbdf83db";
const MAX_BYTES: usize = 32 * 1024;
const MAX_TEXT_CHARS: usize = 512;
/// The save holds one unlocked byte and one progress counter per achievement.
pub const ACHIEVEMENT_COUNT: u8 = 50;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Achievement {
    pub index: u8,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AchievementsDocument {
    pub schema: String,
    pub profile: String,
    pub source_sha256: String,
    pub achievements: Vec<Achievement>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AchievementsError {
    InvalidJson,
    TooLarge,
    SourceMismatch,
    InvalidTable,
}

#[derive(Clone, Debug)]
pub struct ValidatedAchievements(AchievementsDocument);

impl ValidatedAchievements {
    pub fn from_json(bytes: &[u8]) -> Result<Self, AchievementsError> {
        if bytes.len() > MAX_BYTES {
            return Err(AchievementsError::TooLarge);
        }
        let document = serde_json::from_slice(bytes).map_err(|_| AchievementsError::InvalidJson)?;
        Self::validate(document)
    }

    pub fn validate(document: AchievementsDocument) -> Result<Self, AchievementsError> {
        if document.schema != "achievements_v1"
            || document.profile != "english_steam_enhanced_manual"
            || document.source_sha256 != SOURCE_SHA256
        {
            return Err(AchievementsError::SourceMismatch);
        }
        let valid = document.achievements.len() == usize::from(ACHIEVEMENT_COUNT)
            && document
                .achievements
                .iter()
                .enumerate()
                .all(|(position, achievement)| {
                    usize::from(achievement.index) == position
                        && !achievement.description.trim().is_empty()
                        && achievement.description.chars().count() <= MAX_TEXT_CHARS
                });
        if !valid {
            return Err(AchievementsError::InvalidTable);
        }
        Ok(Self(document))
    }

    pub fn achievements(&self) -> &[Achievement] {
        &self.0.achievements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> AchievementsDocument {
        AchievementsDocument {
            schema: "achievements_v1".into(),
            profile: "english_steam_enhanced_manual".into(),
            source_sha256: SOURCE_SHA256.into(),
            achievements: (0..ACHIEVEMENT_COUNT)
                .map(|index| Achievement {
                    index,
                    description: format!("Award {index}."),
                })
                .collect(),
        }
    }

    #[test]
    fn every_saved_achievement_has_one_description_in_order() {
        let achievements =
            ValidatedAchievements::validate(document()).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(achievements.achievements()[49].description, "Award 49.");
        let mut source = document();
        source.source_sha256 = "0".repeat(64);
        assert_eq!(
            ValidatedAchievements::validate(source).err(),
            Some(AchievementsError::SourceMismatch)
        );
        let mut short = document();
        short.achievements.pop();
        let mut unordered = document();
        unordered.achievements.swap(0, 1);
        let mut blank = document();
        blank.achievements[3].description = " ".into();
        for invalid in [short, unordered, blank] {
            assert_eq!(
                ValidatedAchievements::validate(invalid).err(),
                Some(AchievementsError::InvalidTable)
            );
        }
        assert_eq!(
            ValidatedAchievements::from_json(b"{}").err(),
            Some(AchievementsError::InvalidJson)
        );
    }
}
