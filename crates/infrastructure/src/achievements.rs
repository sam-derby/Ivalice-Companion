//! Read-only local resource for in-game achievement descriptions.

use std::{fs, path::Path};

use ivalice_domain::achievements::ValidatedAchievements;

use crate::platform_fs;

pub struct AchievementsLoader;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AchievementsLoadError {
    Unavailable,
}

impl AchievementsLoader {
    pub fn load(resource_root: &Path) -> Result<ValidatedAchievements, AchievementsLoadError> {
        let path = resource_root.join("resources").join("achievements-v1.json");
        platform_fs::validate_regular_file(&path)
            .map_err(|_| AchievementsLoadError::Unavailable)?;
        let size = fs::metadata(&path)
            .map_err(|_| AchievementsLoadError::Unavailable)?
            .len();
        if size == 0 || size > 32 * 1024 {
            return Err(AchievementsLoadError::Unavailable);
        }
        let bytes = fs::read(path).map_err(|_| AchievementsLoadError::Unavailable)?;
        ValidatedAchievements::from_json(&bytes).map_err(|_| AchievementsLoadError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{AchievementsLoadError, AchievementsLoader};

    #[test]
    fn missing_and_invalid_resources_are_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let root =
            std::env::temp_dir().join(format!("ivalice-achievements-{}", std::process::id()));
        fs::create_dir_all(root.join("resources"))?;
        assert_eq!(
            AchievementsLoader::load(&root).err(),
            Some(AchievementsLoadError::Unavailable)
        );
        let path = root.join("resources/achievements-v1.json");
        fs::write(&path, b"{}")?;
        assert_eq!(
            AchievementsLoader::load(&root).err(),
            Some(AchievementsLoadError::Unavailable)
        );
        fs::remove_file(path)?;
        fs::remove_dir(root.join("resources"))?;
        fs::remove_dir(root)?;
        Ok(())
    }
}
