//! Linux filesystem operations use open directory descriptors, never a reopened
//! ambient parent when creating, renaming or removing a save.
use std::fs::{self, File, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use rustix::fs::{self as native, Mode, OFlags, RenameFlags};

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

pub(crate) fn identity(metadata: &Metadata) -> FileIdentity {
    FileIdentity {
        volume: metadata.dev(),
        index: metadata.ino(),
    }
}

pub(crate) fn validate_path_shape(path: &Path) -> Result<(), PathFailure> {
    if !path.is_absolute() || path.to_str().is_none() {
        return Err(PathFailure::Invalid);
    }
    if path
        .components()
        .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return Err(PathFailure::Invalid);
    }
    Ok(())
}

pub(crate) fn map_io_error(error: io::Error) -> PathFailure {
    match error.kind() {
        io::ErrorKind::NotFound => PathFailure::NotFound,
        io::ErrorKind::PermissionDenied => PathFailure::AccessDenied,
        io::ErrorKind::WouldBlock => PathFailure::Busy,
        _ if error.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error()) => {
            PathFailure::UnsupportedLocation
        }
        _ => PathFailure::Io,
    }
}

pub(crate) struct DirectoryGuard {
    entries: Vec<(PathBuf, FileIdentity, File)>,
}

impl DirectoryGuard {
    pub(crate) fn pin(path: &Path) -> Result<Self, PathFailure> {
        Self::open(path, false)
    }

    fn open(path: &Path, create: bool) -> Result<Self, PathFailure> {
        validate_path_shape(path)?;
        let root = File::from(
            native::open(
                "/",
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(io::Error::from)
            .map_err(map_io_error)?,
        );
        let root_identity = identity(&root.metadata().map_err(map_io_error)?);
        let mut entries = vec![(PathBuf::from("/"), root_identity, root)];
        let mut current = PathBuf::from("/");
        for component in path.components() {
            if let Component::Normal(name) = component {
                let parent = &entries.last().ok_or(PathFailure::Invalid)?.2;
                if create {
                    match native::mkdirat(parent, name, Mode::RWXU) {
                        Ok(()) => native::fsync(parent)
                            .map_err(io::Error::from)
                            .map_err(map_io_error)?,
                        Err(rustix::io::Errno::EXIST) => {}
                        Err(error) => return Err(map_io_error(error.into())),
                    }
                }
                let file = File::from(
                    native::openat(
                        parent,
                        name,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|error| {
                        if matches!(error, rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR) {
                            PathFailure::UnsupportedLocation
                        } else {
                            map_io_error(error.into())
                        }
                    })?,
                );
                current.push(name);
                let metadata = file.metadata().map_err(map_io_error)?;
                entries.push((current.clone(), identity(&metadata), file));
            }
        }
        let guard = Self { entries };
        guard.validate()?;
        Ok(guard)
    }

    pub(crate) fn directory(&self) -> Result<&File, PathFailure> {
        self.entries
            .last()
            .map(|entry| &entry.2)
            .ok_or(PathFailure::Invalid)
    }

    pub(crate) fn validate(&self) -> Result<(), PathFailure> {
        for (path, expected, file) in &self.entries {
            let metadata = fs::symlink_metadata(path).map_err(map_io_error)?;
            if !metadata.is_dir()
                || identity(&metadata) != *expected
                || identity(&file.metadata().map_err(map_io_error)?) != *expected
            {
                return Err(PathFailure::UnsupportedLocation);
            }
        }
        Ok(())
    }

    pub(crate) fn open_file(&self, name: &std::ffi::OsStr) -> Result<File, PathFailure> {
        self.open_with_policy(name, false)
    }

    pub(crate) fn open_resource_file(&self, name: &std::ffi::OsStr) -> Result<File, PathFailure> {
        self.open_with_policy(name, true)
    }

    fn open_with_policy(
        &self,
        name: &std::ffi::OsStr,
        resource: bool,
    ) -> Result<File, PathFailure> {
        self.validate()?;
        let file = File::from(
            native::openat(
                self.directory()?,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(io::Error::from)
            .map_err(map_io_error)?,
        );
        let metadata = file
            .metadata()
            .map_err(|_| PathFailure::IdentityUnavailable)?;
        if !metadata.is_file() {
            return Err(PathFailure::NotRegularFile);
        }
        if metadata.nlink() == 0 {
            return Err(PathFailure::UnsupportedLocation);
        }
        validate_volume_policy(&file, resource)?;
        self.validate()?;
        Ok(file)
    }

    pub(crate) fn create_file(
        &self,
        name: &std::ffi::OsStr,
        mode: u32,
    ) -> Result<File, PathFailure> {
        self.validate()?;
        validate_volume(self.directory()?)?;
        let file = File::from(
            native::openat(
                self.directory()?,
                name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_bits_truncate(mode),
            )
            .map_err(io::Error::from)
            .map_err(map_io_error)?,
        );
        Ok(file)
    }

    pub(crate) fn rename(
        &self,
        from: &std::ffi::OsStr,
        to: &std::ffi::OsStr,
        replace: bool,
    ) -> Result<(), PathFailure> {
        self.validate()?;
        native::renameat_with(
            self.directory()?,
            from,
            self.directory()?,
            to,
            if replace {
                RenameFlags::empty()
            } else {
                RenameFlags::NOREPLACE
            },
        )
        .map_err(io::Error::from)
        .map_err(map_io_error)?;
        self.sync()?;
        self.validate()
    }

    pub(crate) fn remove(&self, name: &std::ffi::OsStr) -> Result<(), PathFailure> {
        native::unlinkat(self.directory()?, name, native::AtFlags::empty())
            .map_err(io::Error::from)
            .map_err(map_io_error)?;
        self.sync()
    }

    pub(crate) fn sync(&self) -> Result<(), PathFailure> {
        native::fsync(self.directory()?)
            .map_err(io::Error::from)
            .map_err(map_io_error)
    }
}

// Linux v6.12 include/uapi/linux/magic.h. Restrict saves and companion data to
// local filesystems; squashfs also permits read-only AppImage resource loading.
// https://github.com/torvalds/linux/blob/v6.12/include/uapi/linux/magic.h
fn validate_volume(file: &File) -> Result<(), PathFailure> {
    validate_volume_policy(file, false)
}

fn validate_volume_policy(file: &File, resource: bool) -> Result<(), PathFailure> {
    let filesystem = native::fstatfs(file).map_err(|_| PathFailure::UnsupportedLocation)?;
    let kind = u64::try_from(filesystem.f_type).map_err(|_| PathFailure::UnsupportedLocation)?;
    let read_only = native::fstatvfs(file)
        .map_err(|_| PathFailure::UnsupportedLocation)?
        .f_flag
        .contains(native::StatVfsMountFlags::RDONLY);
    if !supported_filesystem(kind, read_only, resource) {
        return Err(PathFailure::UnsupportedLocation);
    }
    Ok(())
}

// FUSE's statfs reports FUSE_SUPER_MAGIC even for squashfuse/AppImage mounts
// (Linux v6.12 fs/fuse/inode.c). Permit it only for read-only resource reads.
fn supported_filesystem(kind: u64, read_only: bool, resource: bool) -> bool {
    matches!(
        kind,
        0xef53 | 0x9123_683e | 0x5846_5342 | 0x0102_1994 | 0x794c_7630 | 0x7371_7368
    ) || (kind == 0x6573_5546 && read_only && resource)
}

pub(crate) fn ensure_data_directory(path: &Path) -> Result<(), PathFailure> {
    DirectoryGuard::open(path, true).map(|_| ())
}

pub(crate) fn validate_supported_volume(path: &Path) -> Result<(), PathFailure> {
    let guard = DirectoryGuard::pin(path)?;
    validate_volume(guard.directory()?)
}

pub(crate) fn validate_components(path: &Path) -> Result<(), PathFailure> {
    DirectoryGuard::pin(path).map(|_| ())
}

pub(crate) fn validate_regular_file(path: &Path) -> Result<FileIdentity, PathFailure> {
    validate_path_shape(path)?;
    let guard = DirectoryGuard::pin(path.parent().ok_or(PathFailure::Invalid)?)?;
    let file = guard.open_file(path.file_name().ok_or(PathFailure::Invalid)?)?;
    Ok(identity(&file.metadata().map_err(map_io_error)?))
}

pub(crate) fn safe_single_link_file(path: &Path) -> Result<FileIdentity, PathFailure> {
    let guard = DirectoryGuard::pin(path.parent().ok_or(PathFailure::Invalid)?)?;
    let file = guard.open_file(path.file_name().ok_or(PathFailure::Invalid)?)?;
    let metadata = file.metadata().map_err(map_io_error)?;
    if metadata.nlink() != 1 {
        return Err(PathFailure::UnsupportedLocation);
    }
    Ok(identity(&metadata))
}

pub(crate) fn validate_regular_resource_file(path: &Path) -> Result<FileIdentity, PathFailure> {
    validate_path_shape(path)?;
    let guard = DirectoryGuard::pin(path.parent().ok_or(PathFailure::Invalid)?)?;
    let file = guard.open_resource_file(path.file_name().ok_or(PathFailure::Invalid)?)?;
    Ok(identity(&file.metadata().map_err(map_io_error)?))
}

pub(crate) fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

#[cfg(test)]
#[path = "linux_fs_tests.rs"]
mod tests;
