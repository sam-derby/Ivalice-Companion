//! Read-only local resource for errand postings and the artefact and wonder collection.

use std::{fs, path::Path};

use ivalice_domain::errands::ValidatedErrands;

use crate::windows_fs;

pub struct ErrandsLoader;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrandsLoadError {
    Unavailable,
}

impl ErrandsLoader {
    pub fn load(resource_root: &Path) -> Result<ValidatedErrands, ErrandsLoadError> {
        let path = resource_root.join("resources").join("errands-v1.json");
        windows_fs::validate_regular_file(&path).map_err(|_| ErrandsLoadError::Unavailable)?;
        let size = fs::metadata(&path)
            .map_err(|_| ErrandsLoadError::Unavailable)?
            .len();
        if size == 0 || size > 256 * 1024 {
            return Err(ErrandsLoadError::Unavailable);
        }
        let bytes = fs::read(path).map_err(|_| ErrandsLoadError::Unavailable)?;
        ValidatedErrands::from_json(&bytes).map_err(|_| ErrandsLoadError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{ErrandsLoadError, ErrandsLoader};

    #[test]
    fn missing_and_invalid_resources_are_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("ivalice-errands-{}", std::process::id()));
        fs::create_dir_all(root.join("resources"))?;
        assert_eq!(
            ErrandsLoader::load(&root).err(),
            Some(ErrandsLoadError::Unavailable)
        );
        let path = root.join("resources/errands-v1.json");
        fs::write(&path, b"{}")?;
        assert_eq!(
            ErrandsLoader::load(&root).err(),
            Some(ErrandsLoadError::Unavailable)
        );
        fs::remove_file(path)?;
        fs::remove_dir(root.join("resources"))?;
        fs::remove_dir(root)?;
        Ok(())
    }
}
