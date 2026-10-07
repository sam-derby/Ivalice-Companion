use std::ffi::OsStr;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::platform_fs::{identity, DirectoryGuard, FileIdentity};
use crate::MAX_SNAPSHOT_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveEditError {
    UnsafePath,
    Changed,
    TooLarge,
    BackupFailed,
    RecoveryRequired,
    Io,
    Exists,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveBackup {
    path: std::path::PathBuf,
    original_sha256: [u8; 32],
    edited_sha256: [u8; 32],
}

impl SaveBackup {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn edited_sha256(&self) -> [u8; 32] {
        self.edited_sha256
    }
}

fn sibling_path(path: &Path, label: &str) -> Result<PathBuf, SaveEditError> {
    let mut name = path
        .file_stem()
        .ok_or(SaveEditError::UnsafePath)?
        .to_os_string();
    name.push(format!(" - {label}"));
    if let Some(extension) = path.extension() {
        name.push(".");
        name.push(extension);
    }
    Ok(path.with_file_name(name))
}

struct Verified {
    bytes: Vec<u8>,
    identity: FileIdentity,
    mode: u32,
}

fn read_verified(guard: &DirectoryGuard, name: &OsStr) -> Result<Verified, SaveEditError> {
    let file = guard
        .open_file(name)
        .map_err(|_| SaveEditError::UnsafePath)?;
    let before = file.metadata().map_err(|_| SaveEditError::Io)?;
    if before.nlink() != 1 {
        return Err(SaveEditError::UnsafePath);
    }
    if before.len() > MAX_SNAPSHOT_BYTES {
        return Err(SaveEditError::TooLarge);
    }
    let mut bytes = Vec::new();
    file.take(MAX_SNAPSHOT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SaveEditError::Io)?;
    let current = guard.open_file(name).map_err(|_| SaveEditError::Changed)?;
    let after = current.metadata().map_err(|_| SaveEditError::Io)?;
    if identity(&before) != identity(&after)
        || before.len() != after.len()
        || (
            before.mtime(),
            before.mtime_nsec(),
            before.ctime(),
            before.ctime_nsec(),
        ) != (
            after.mtime(),
            after.mtime_nsec(),
            after.ctime(),
            after.ctime_nsec(),
        )
        || u64::try_from(bytes.len()).map_err(|_| SaveEditError::TooLarge)? != before.len()
        || after.nlink() != 1
    {
        return Err(SaveEditError::Changed);
    }
    Ok(Verified {
        bytes,
        identity: identity(&before),
        mode: before.mode() & 0o777,
    })
}

fn require_expected(
    guard: &DirectoryGuard,
    name: &OsStr,
    expected: &Verified,
    digest: [u8; 32],
) -> Result<(), SaveEditError> {
    let current = read_verified(guard, name)?;
    if current.identity != expected.identity
        || <[u8; 32]>::from(Sha256::digest(&current.bytes)) != digest
    {
        return Err(SaveEditError::Changed);
    }
    Ok(())
}

fn write_copy(
    guard: &DirectoryGuard,
    name: &OsStr,
    bytes: &[u8],
    mode: u32,
) -> Result<FileIdentity, SaveEditError> {
    let mut file = guard
        .create_file(name, mode)
        .map_err(|_| SaveEditError::Io)?;
    let id = identity(&file.metadata().map_err(|_| SaveEditError::Io)?);
    let result = (|| {
        file.write_all(bytes).map_err(|_| SaveEditError::Io)?;
        file.set_permissions(std::fs::Permissions::from_mode(mode))
            .map_err(|_| SaveEditError::Io)?;
        file.sync_all().map_err(|_| SaveEditError::Io)?;
        guard.sync().map_err(|_| SaveEditError::Io)?;
        Ok(())
    })();
    if result.is_err() {
        remove_owned(guard, name, id);
    }
    result.map(|()| id)
}

fn remove_owned(guard: &DirectoryGuard, name: &OsStr, expected: FileIdentity) {
    if guard
        .open_file(name)
        .ok()
        .and_then(|file| file.metadata().ok())
        .is_some_and(|meta| identity(&meta) == expected && meta.nlink() == 1)
    {
        let _ = guard.remove(name);
    }
}

pub fn replace_save_with_backup_if_unchanged(
    path: &Path,
    expected_sha256: [u8; 32],
    original: &[u8],
    replacement: &[u8],
) -> Result<SaveBackup, SaveEditError> {
    rotate_save(
        path,
        expected_sha256,
        original,
        replacement,
        |guard, from, to| guard.rename(from, to, false).map_err(|_| SaveEditError::Io),
    )
}

fn rotate_save(
    path: &Path,
    expected_sha256: [u8; 32],
    original: &[u8],
    replacement: &[u8],
    promote: impl FnOnce(&DirectoryGuard, &OsStr, &OsStr) -> Result<(), SaveEditError>,
) -> Result<SaveBackup, SaveEditError> {
    check_bytes(original)?;
    check_bytes(replacement)?;
    if <[u8; 32]>::from(Sha256::digest(original)) != expected_sha256 {
        return Err(SaveEditError::Changed);
    }
    let guard = DirectoryGuard::pin(path.parent().ok_or(SaveEditError::UnsafePath)?)
        .map_err(|_| SaveEditError::UnsafePath)?;
    let name = path.file_name().ok_or(SaveEditError::UnsafePath)?;
    let source = read_verified(&guard, name)?;
    if source.bytes != original {
        return Err(SaveEditError::Changed);
    }
    let backup_path = sibling_path(path, "backup")?;
    let backup = backup_path.file_name().ok_or(SaveEditError::UnsafePath)?;
    let copy_path = sibling_path(path, "copy")?;
    let copy = copy_path.file_name().ok_or(SaveEditError::UnsafePath)?;
    let backup_copy_path = sibling_path(path, "backup copy")?;
    let backup_copy = backup_copy_path
        .file_name()
        .ok_or(SaveEditError::UnsafePath)?;
    let retired_path = sibling_path(path, "retired")?;
    let retired = retired_path.file_name().ok_or(SaveEditError::UnsafePath)?;
    if guard.open_file(retired).err() != Some(crate::platform_fs::PathFailure::NotFound) {
        return Err(SaveEditError::UnsafePath);
    }
    let mut backup_exists = false;
    match guard.open_file(backup) {
        Ok(file)
            if file
                .metadata()
                .map_err(|_| SaveEditError::BackupFailed)?
                .nlink()
                == 1 =>
        {
            backup_exists = true;
        }
        Err(crate::platform_fs::PathFailure::NotFound) => {}
        _ => return Err(SaveEditError::BackupFailed),
    }
    let copy_id = write_copy(&guard, copy, replacement, source.mode)?;
    let backup_copy_id = match write_copy(&guard, backup_copy, original, source.mode) {
        Ok(id) => id,
        Err(_) => {
            remove_owned(&guard, copy, copy_id);
            return Err(SaveEditError::BackupFailed);
        }
    };
    let result = (|| {
        require_expected(&guard, name, &source, expected_sha256)?;
        let written = read_verified(&guard, copy)?;
        if written.identity != copy_id || written.bytes != replacement {
            return Err(SaveEditError::Io);
        }
        let recovery = read_verified(&guard, backup_copy)?;
        if recovery.identity != backup_copy_id || recovery.bytes != original {
            return Err(SaveEditError::BackupFailed);
        }
        // Copy the backup: a game with the old file open could still write to
        // its inode after a rename.
        guard
            .rename(backup_copy, backup, backup_exists)
            .map_err(|_| SaveEditError::BackupFailed)?;
        require_expected(&guard, name, &source, expected_sha256)?;
        guard
            .rename(name, retired, false)
            .map_err(|_| SaveEditError::RecoveryRequired)?;
        let promoted = require_expected(&guard, retired, &source, expected_sha256)
            .and_then(|()| promote(&guard, copy, name));
        if let Err(error) = promoted {
            if require_expected(&guard, retired, &source, expected_sha256).is_err()
                || guard.rename(retired, name, false).is_err()
            {
                return Err(SaveEditError::RecoveryRequired);
            }
            return Err(error);
        }
        remove_owned(&guard, retired, source.identity);
        Ok(SaveBackup {
            path: backup_path.clone(),
            original_sha256: expected_sha256,
            edited_sha256: Sha256::digest(replacement).into(),
        })
    })();
    if result.is_err() {
        remove_owned(&guard, copy, copy_id);
        remove_owned(&guard, backup_copy, backup_copy_id);
    }
    result
}

fn check_bytes(bytes: &[u8]) -> Result<(), SaveEditError> {
    if bytes.is_empty()
        || u64::try_from(bytes.len()).map_err(|_| SaveEditError::TooLarge)? > MAX_SNAPSHOT_BYTES
    {
        return Err(SaveEditError::TooLarge);
    }
    Ok(())
}

pub fn write_new_save_file(
    path: &Path,
    bytes: &[u8],
    overwrite: bool,
) -> Result<(), SaveEditError> {
    check_bytes(bytes)?;
    let guard = DirectoryGuard::pin(path.parent().ok_or(SaveEditError::UnsafePath)?)
        .map_err(|_| SaveEditError::UnsafePath)?;
    let name = path.file_name().ok_or(SaveEditError::UnsafePath)?;
    let previous = match guard.open_file(name) {
        Ok(_) if !overwrite => return Err(SaveEditError::Exists),
        Ok(_) => Some(read_verified(&guard, name)?),
        Err(crate::platform_fs::PathFailure::NotFound) => None,
        _ => return Err(SaveEditError::UnsafePath),
    };
    let copy_path = sibling_path(path, "copy")?;
    let copy = copy_path.file_name().ok_or(SaveEditError::UnsafePath)?;
    let copy_id = write_copy(
        &guard,
        copy,
        bytes,
        previous.as_ref().map_or(0o600, |old| old.mode),
    )?;
    let result = (|| {
        if let Some(previous) = &previous {
            require_expected(
                &guard,
                name,
                previous,
                Sha256::digest(&previous.bytes).into(),
            )?;
        }
        guard
            .rename(copy, name, previous.is_some())
            .map_err(|_| SaveEditError::Io)
    })();
    if result.is_err() {
        remove_owned(&guard, copy, copy_id);
    }
    result
}

pub fn restore_save_from_backup_if_unchanged(
    path: &Path,
    edited_sha256: [u8; 32],
    backup: &SaveBackup,
) -> Result<(), SaveEditError> {
    if backup.path.parent() != path.parent() {
        return Err(SaveEditError::UnsafePath);
    }
    let guard = DirectoryGuard::pin(path.parent().ok_or(SaveEditError::UnsafePath)?)
        .map_err(|_| SaveEditError::UnsafePath)?;
    let original = read_verified(
        &guard,
        backup.path.file_name().ok_or(SaveEditError::UnsafePath)?,
    )
    .map_err(|error| {
        if error == SaveEditError::UnsafePath {
            error
        } else {
            SaveEditError::BackupFailed
        }
    })?;
    check_bytes(&original.bytes).map_err(|_| SaveEditError::BackupFailed)?;
    if <[u8; 32]>::from(Sha256::digest(&original.bytes)) != backup.original_sha256 {
        return Err(SaveEditError::BackupFailed);
    }
    replace_save_if_unchanged(path, edited_sha256, &original.bytes)
}

pub fn replace_save_if_unchanged(
    path: &Path,
    expected_sha256: [u8; 32],
    replacement: &[u8],
) -> Result<(), SaveEditError> {
    check_bytes(replacement)?;
    let guard = DirectoryGuard::pin(path.parent().ok_or(SaveEditError::UnsafePath)?)
        .map_err(|_| SaveEditError::UnsafePath)?;
    let name = path.file_name().ok_or(SaveEditError::UnsafePath)?;
    let source = read_verified(&guard, name)?;
    require_expected(&guard, name, &source, expected_sha256)?;
    let copy_path = sibling_path(path, "copy")?;
    let copy = copy_path.file_name().ok_or(SaveEditError::UnsafePath)?;
    let copy_id = write_copy(&guard, copy, replacement, source.mode)?;
    let result = (|| {
        require_expected(&guard, name, &source, expected_sha256)?;
        guard
            .rename(copy, name, true)
            .map_err(|_| SaveEditError::Io)
    })();
    if result.is_err() {
        remove_owned(&guard, copy, copy_id);
    }
    result
}

#[cfg(test)]
#[path = "save_edit_tests.rs"]
mod tests;

#[cfg(test)]
mod linux_tests {
    use super::*;

    #[test]
    fn failed_promotion_restores_original_and_concurrent_file_keeps_backup(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root =
            std::env::temp_dir().join(format!("ivalice-linux-rollback-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        let path = root.join("enhanced.png");
        let backup = root.join("enhanced - backup.png");
        let copy = root.join("enhanced - copy.png");
        std::fs::write(&path, b"original")?;
        let digest = Sha256::digest(b"original").into();
        let failed = rotate_save(&path, digest, b"original", b"edited", |_, _, _| {
            Err(SaveEditError::Io)
        });
        assert_eq!(failed, Err(SaveEditError::Io));
        assert_eq!(std::fs::read(&path)?, b"original");
        assert_eq!(std::fs::read(&backup)?, b"original");
        assert!(!copy.exists());
        let failed = rotate_save(&path, digest, b"original", b"edited", |guard, from, to| {
            std::fs::write(&path, b"concurrent").map_err(|_| SaveEditError::Io)?;
            guard.rename(from, to, false).map_err(|_| SaveEditError::Io)
        });
        assert_eq!(failed, Err(SaveEditError::RecoveryRequired));
        assert_eq!(std::fs::read(&path)?, b"concurrent");
        assert_eq!(std::fs::read(&backup)?, b"original");
        assert!(!copy.exists());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
