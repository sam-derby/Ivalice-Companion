use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf, Prefix};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
const UNSUPPORTED_ATTRIBUTES: u32 = FILE_ATTRIBUTE_REPARSE_POINT
    | FILE_ATTRIBUTE_OFFLINE
    | FILE_ATTRIBUTE_RECALL_ON_OPEN
    | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FileIdentity {
    pub(crate) volume: u64,
    pub(crate) index: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathFailure {
    Invalid,
    NotFound,
    AccessDenied,
    NotRegularFile,
    UnsupportedLocation,
    IdentityUnavailable,
    Busy,
    Io,
}

pub(crate) fn validate_path_shape(path: &Path) -> Result<char, PathFailure> {
    if path.to_str().is_none() {
        return Err(PathFailure::Invalid);
    }

    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => char::from(letter),
            _ => return Err(PathFailure::UnsupportedLocation),
        },
        _ => return Err(PathFailure::Invalid),
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(PathFailure::Invalid);
    }

    for component in components {
        match component {
            Component::Normal(value) if !contains_colon(value) => {}
            _ => return Err(PathFailure::Invalid),
        }
    }
    Ok(drive)
}

fn contains_colon(value: &OsStr) -> bool {
    value.encode_wide().any(|unit| unit == u16::from(b':'))
}

pub(crate) fn validate_supported_volume(path: &Path) -> Result<(), PathFailure> {
    let drive = validate_path_shape(path)?;
    let root = format!("{}:\\", drive.to_ascii_uppercase());
    if winsafe::GetDriveType(Some(&root)) != winsafe::co::DRIVE::FIXED {
        return Err(PathFailure::UnsupportedLocation);
    }

    let mut file_system = String::new();
    winsafe::GetVolumeInformation(Some(&root), None, None, None, None, Some(&mut file_system))
        .map_err(|_| PathFailure::UnsupportedLocation)?;
    if !file_system.eq_ignore_ascii_case("NTFS") {
        return Err(PathFailure::UnsupportedLocation);
    }
    Ok(())
}

pub(crate) fn validate_components(path: &Path) -> Result<(), PathFailure> {
    for current in absolute_ancestors(path) {
        let metadata = fs::symlink_metadata(current).map_err(map_io_error)?;
        if has_unsupported_attributes(&metadata) {
            return Err(PathFailure::UnsupportedLocation);
        }
    }
    Ok(())
}

pub(crate) fn validate_regular_file(path: &Path) -> Result<FileIdentity, PathFailure> {
    validate_path_shape(path)?;
    validate_supported_volume(path)?;
    validate_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(map_io_error)?;
    if !metadata.is_file() {
        return Err(PathFailure::NotRegularFile);
    }
    let (identity, links, attributes) = query_path_info(path, true)?;
    if links == 0 || attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(PathFailure::UnsupportedLocation);
    }
    Ok(identity)
}

pub(crate) fn safe_single_link_file(path: &Path) -> Result<FileIdentity, PathFailure> {
    let metadata = fs::symlink_metadata(path).map_err(map_io_error)?;
    if !metadata.is_file() || has_unsupported_attributes(&metadata) {
        return Err(PathFailure::UnsupportedLocation);
    }
    let (identity, links, attributes) = query_path_info(path, true)?;
    if links != 1 || attributes & UNSUPPORTED_ATTRIBUTES != 0 {
        return Err(PathFailure::UnsupportedLocation);
    }
    Ok(identity)
}

pub(crate) fn has_unsupported_attributes(metadata: &Metadata) -> bool {
    metadata.file_attributes() & UNSUPPORTED_ATTRIBUTES != 0
}

pub(crate) fn map_io_error(error: io::Error) -> PathFailure {
    if matches!(error.raw_os_error(), Some(32 | 33)) {
        return PathFailure::Busy;
    }
    match error.kind() {
        io::ErrorKind::NotFound => PathFailure::NotFound,
        io::ErrorKind::PermissionDenied => PathFailure::AccessDenied,
        _ => PathFailure::Io,
    }
}

pub(crate) struct DirectoryGuard {
    entries: Vec<(
        PathBuf,
        FileIdentity,
        winsafe::guard::CloseHandleGuard<winsafe::HFILE>,
    )>,
}

impl DirectoryGuard {
    pub(crate) fn pin(path: &Path) -> Result<Self, PathFailure> {
        validate_path_shape(path)?;
        validate_supported_volume(path)?;
        let mut entries = Vec::new();

        for current in absolute_ancestors(path) {
            let (file, _) = open_path_handle(current, false, true)?;
            let information = file
                .GetFileInformationByHandle()
                .map_err(map_windows_error)?;
            if information.dwFileAttributes.raw() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                || information.dwFileAttributes.raw() & 0x10 == 0
            {
                return Err(PathFailure::UnsupportedLocation);
            }
            let file_identity = identity_from_information(&information);
            entries.push((current.to_path_buf(), file_identity, file));
        }
        Ok(Self { entries })
    }

    pub(crate) fn validate(&self) -> Result<(), PathFailure> {
        for (path, expected, handle) in &self.entries {
            let handle_information = handle
                .GetFileInformationByHandle()
                .map_err(map_windows_error)?;
            let (path_identity, _, attributes) = query_path_info(path, true)?;
            if attributes & UNSUPPORTED_ATTRIBUTES != 0
                || identity_from_information(&handle_information) != *expected
                || path_identity != *expected
            {
                return Err(PathFailure::UnsupportedLocation);
            }
        }
        Ok(())
    }
}

fn absolute_ancestors(path: &Path) -> Vec<&Path> {
    let mut ancestors: Vec<_> = path.ancestors().collect();
    ancestors.reverse();
    ancestors
}

fn query_path_info(
    path: &Path,
    allow_delete: bool,
) -> Result<(FileIdentity, u32, u32), PathFailure> {
    let (handle, _) = open_path_handle(path, allow_delete, true)?;
    let information = handle
        .GetFileInformationByHandle()
        .map_err(map_windows_error)?;
    Ok((
        identity_from_information(&information),
        information.nNumberOfLinks,
        information.dwFileAttributes.raw(),
    ))
}

fn identity_from_information(information: &winsafe::BY_HANDLE_FILE_INFORMATION) -> FileIdentity {
    FileIdentity {
        volume: u64::from(information.dwVolumeSerialNumber),
        index: information.nFileIndex(),
    }
}

fn open_path_handle(
    path: &Path,
    allow_delete: bool,
    directory: bool,
) -> Result<
    (
        winsafe::guard::CloseHandleGuard<winsafe::HFILE>,
        winsafe::co::ERROR,
    ),
    PathFailure,
> {
    let path = path.to_str().ok_or(PathFailure::Invalid)?;
    let mut share = winsafe::co::FILE_SHARE::READ | winsafe::co::FILE_SHARE::WRITE;
    if allow_delete {
        share |= winsafe::co::FILE_SHARE::DELETE;
    }
    let flags = if directory {
        winsafe::co::FILE_FLAG::BACKUP_SEMANTICS | winsafe::co::FILE_FLAG::OPEN_REPARSE_POINT
    } else {
        winsafe::co::FILE_FLAG::OPEN_REPARSE_POINT
    };
    winsafe::HFILE::CreateFile(
        path,
        winsafe::co::GENERIC::default(),
        Some(share),
        None,
        winsafe::co::DISPOSITION::OPEN_EXISTING,
        winsafe::co::FILE_ATTRIBUTE::NORMAL,
        Some(flags),
        None,
        None,
    )
    .map_err(map_windows_error)
}

fn map_windows_error(error: winsafe::co::ERROR) -> PathFailure {
    if error == winsafe::co::ERROR::FILE_NOT_FOUND || error == winsafe::co::ERROR::PATH_NOT_FOUND {
        PathFailure::NotFound
    } else if error == winsafe::co::ERROR::ACCESS_DENIED {
        PathFailure::AccessDenied
    } else if error == winsafe::co::ERROR::SHARING_VIOLATION
        || error == winsafe::co::ERROR::LOCK_VIOLATION
    {
        PathFailure::Busy
    } else if error == winsafe::co::ERROR::NOT_SUPPORTED
        || error == winsafe::co::ERROR::INVALID_FUNCTION
    {
        PathFailure::IdentityUnavailable
    } else {
        PathFailure::Io
    }
}

pub(crate) fn paths_overlap(left: &Path, right: &Path) -> bool {
    let left = ordinal_components(left);
    let right = ordinal_components(right);
    left.starts_with(&right) || right.starts_with(&left)
}

fn ordinal_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::DirectoryGuard;

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn pinned_directory_rejects_replacement() -> Result<(), Box<dyn Error>> {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let parent = std::env::temp_dir().join(format!(
            "ivalice-d001-pin-{}-{sequence}",
            std::process::id()
        ));
        let guarded = parent.join("guarded");
        let replacement_name = parent.join("moved");
        fs::create_dir_all(&guarded)?;
        let guard = DirectoryGuard::pin(&guarded).map_err(|_| "could not pin synthetic root")?;

        let replacement = fs::rename(&guarded, &replacement_name);
        if replacement.is_ok() {
            assert!(guard.validate().is_err());
        } else {
            guard
                .validate()
                .map_err(|_| "guard identity changed after rejected replacement")?;
        }
        Ok(())
    }

    #[test]
    fn capability_directory_does_not_follow_a_replacement_path() -> Result<(), Box<dyn Error>> {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let parent = std::env::temp_dir().join(format!(
            "ivalice-d001-capability-{}-{sequence}",
            std::process::id()
        ));
        let guarded = parent.join("guarded");
        let moved = parent.join("moved");
        fs::create_dir_all(&guarded)?;
        let directory = cap_std::fs::Dir::open_ambient_dir(&guarded, cap_std::ambient_authority())?;

        if fs::rename(&guarded, &moved).is_ok() {
            fs::create_dir(&guarded)?;
            let mut options = cap_std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            let mut probe = directory.open_with("probe.tmp", &options)?;
            probe.write_all(b"capability target")?;
            probe.sync_all()?;
            assert!(!guarded.join("probe.tmp").exists());
            assert_eq!(fs::read(moved.join("probe.tmp"))?, b"capability target");
        }
        Ok(())
    }
}
