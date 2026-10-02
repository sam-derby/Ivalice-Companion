//! Read-only local resource for the pinned named ability map.

use std::{fs, path::Path};

use ivalice_domain::ability_flags::ValidatedAbilityFlags;

use crate::windows_fs;

pub struct AbilityFlagsLoader;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityFlagsLoadError {
    Unavailable,
}

impl AbilityFlagsLoader {
    pub fn load(resource_root: &Path) -> Result<ValidatedAbilityFlags, AbilityFlagsLoadError> {
        let path = resource_root
            .join("resources")
            .join("ability-flags-v1.json");
        windows_fs::validate_regular_file(&path).map_err(|_| AbilityFlagsLoadError::Unavailable)?;
        let size = fs::metadata(&path)
            .map_err(|_| AbilityFlagsLoadError::Unavailable)?
            .len();
        if size == 0 || size > 256 * 1024 {
            return Err(AbilityFlagsLoadError::Unavailable);
        }
        let bytes = fs::read(path).map_err(|_| AbilityFlagsLoadError::Unavailable)?;
        ValidatedAbilityFlags::from_json(&bytes).map_err(|_| AbilityFlagsLoadError::Unavailable)
    }
}
