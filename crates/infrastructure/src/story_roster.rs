//! Read-only local resource for story guests, members and Ramza's chapter form.

use std::{fs, path::Path};

use ivalice_domain::story_roster::ValidatedStoryRoster;

use crate::platform_fs;

pub struct StoryRosterLoader;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryRosterLoadError {
    Unavailable,
}

impl StoryRosterLoader {
    pub fn load(resource_root: &Path) -> Result<ValidatedStoryRoster, StoryRosterLoadError> {
        let path = resource_root.join("resources").join("story-roster-v1.json");
        platform_fs::validate_regular_file(&path).map_err(|_| StoryRosterLoadError::Unavailable)?;
        let size = fs::metadata(&path)
            .map_err(|_| StoryRosterLoadError::Unavailable)?
            .len();
        if size == 0 || size > 64 * 1024 {
            return Err(StoryRosterLoadError::Unavailable);
        }
        let bytes = fs::read(path).map_err(|_| StoryRosterLoadError::Unavailable)?;
        ValidatedStoryRoster::from_json(&bytes).map_err(|_| StoryRosterLoadError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{StoryRosterLoadError, StoryRosterLoader};

    #[test]
    fn missing_and_invalid_resources_are_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let root =
            std::env::temp_dir().join(format!("ivalice-story-roster-{}", std::process::id()));
        fs::create_dir_all(root.join("resources"))?;
        assert_eq!(
            StoryRosterLoader::load(&root).err(),
            Some(StoryRosterLoadError::Unavailable)
        );
        let path = root.join("resources/story-roster-v1.json");
        fs::write(&path, b"{}")?;
        assert_eq!(
            StoryRosterLoader::load(&root).err(),
            Some(StoryRosterLoadError::Unavailable)
        );
        fs::remove_file(path)?;
        fs::remove_dir(root.join("resources"))?;
        fs::remove_dir(root)?;
        Ok(())
    }
}
