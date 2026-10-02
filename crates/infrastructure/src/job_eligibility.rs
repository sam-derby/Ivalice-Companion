//! Read-only bundled TIC job requirement resource.

use std::fs;
use std::path::Path;

use ivalice_domain::job_eligibility::ValidatedJobRequirements;

use crate::windows_fs;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobRequirementsLoadError {
    Missing,
    UnsafePath,
    Invalid,
}

pub struct JobRequirementsLoader;

impl JobRequirementsLoader {
    pub fn load(
        resource_root: &Path,
    ) -> Result<ValidatedJobRequirements, JobRequirementsLoadError> {
        let path = resource_root
            .join("resources")
            .join("job-requirements-v1.json");
        if !path.exists() {
            return Err(JobRequirementsLoadError::Missing);
        }
        windows_fs::validate_regular_file(&path)
            .map_err(|_| JobRequirementsLoadError::UnsafePath)?;
        let size = fs::metadata(&path)
            .map_err(|_| JobRequirementsLoadError::UnsafePath)?
            .len();
        if size == 0 || size > 16 * 1024 {
            return Err(JobRequirementsLoadError::Invalid);
        }
        let bytes = fs::read(&path).map_err(|_| JobRequirementsLoadError::UnsafePath)?;
        ValidatedJobRequirements::from_json(&bytes).map_err(|_| JobRequirementsLoadError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{JobRequirementsLoadError, JobRequirementsLoader};

    #[test]
    fn missing_and_invalid_bundles_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        let root =
            std::env::temp_dir().join(format!("ivalice-job-requirements-{}", std::process::id()));
        fs::create_dir_all(root.join("resources"))?;
        assert!(matches!(
            JobRequirementsLoader::load(&root),
            Err(JobRequirementsLoadError::Missing)
        ));
        let path = root.join("resources/job-requirements-v1.json");
        fs::write(&path, b"{}")?;
        assert!(matches!(
            JobRequirementsLoader::load(&root),
            Err(JobRequirementsLoadError::Invalid)
        ));
        fs::remove_file(path)?;
        fs::remove_dir(root.join("resources"))?;
        fs::remove_dir(root)?;
        Ok(())
    }
}
