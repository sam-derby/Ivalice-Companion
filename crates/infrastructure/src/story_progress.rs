//! Read-only local resource for main story chapter and objective labels.

use std::{fs, path::Path};

use ivalice_domain::story_progress::ValidatedStoryProgress;

use crate::platform_fs;

pub struct StoryProgressLoader;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryProgressLoadError {
    Unavailable,
}

impl StoryProgressLoader {
    pub fn load(resource_root: &Path) -> Result<ValidatedStoryProgress, StoryProgressLoadError> {
        let path = resource_root
            .join("resources")
            .join("story-progress-v3.json");
        platform_fs::validate_regular_file(&path)
            .map_err(|_| StoryProgressLoadError::Unavailable)?;
        let size = fs::metadata(&path)
            .map_err(|_| StoryProgressLoadError::Unavailable)?
            .len();
        if size == 0 || size > 64 * 1024 {
            return Err(StoryProgressLoadError::Unavailable);
        }
        let bytes = fs::read(path).map_err(|_| StoryProgressLoadError::Unavailable)?;
        ValidatedStoryProgress::from_json(&bytes).map_err(|_| StoryProgressLoadError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{StoryProgressLoadError, StoryProgressLoader};

    #[test]
    fn missing_and_invalid_resources_are_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let root =
            std::env::temp_dir().join(format!("ivalice-story-progress-{}", std::process::id()));
        fs::create_dir_all(root.join("resources"))?;
        assert_eq!(
            StoryProgressLoader::load(&root).err(),
            Some(StoryProgressLoadError::Unavailable)
        );
        let path = root.join("resources/story-progress-v3.json");
        fs::write(&path, b"{}")?;
        assert_eq!(
            StoryProgressLoader::load(&root).err(),
            Some(StoryProgressLoadError::Unavailable)
        );
        fs::remove_file(path)?;
        fs::remove_dir(root.join("resources"))?;
        fs::remove_dir(root)?;
        Ok(())
    }
}
