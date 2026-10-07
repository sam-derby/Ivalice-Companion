use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::platform_fs::{self, DirectoryGuard};
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

/// Write a verified sibling copy, rotate the original into the reusable backup,
/// then promote the copy. Failed promotion restores the original when possible;
/// a failed rollback leaves the exact original at the backup path for recovery.
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
        move_without_replace,
    )
}

fn rotate_save(
    path: &Path,
    expected_sha256: [u8; 32],
    original: &[u8],
    replacement: &[u8],
    promote: impl FnOnce(&Path, &Path) -> Result<(), SaveEditError>,
) -> Result<SaveBackup, SaveEditError> {
    if original.is_empty()
        || original.len() as u64 > MAX_SNAPSHOT_BYTES
        || replacement.is_empty()
        || replacement.len() as u64 > MAX_SNAPSHOT_BYTES
    {
        return Err(SaveEditError::TooLarge);
    }
    if <[u8; 32]>::from(Sha256::digest(original)) != expected_sha256 {
        return Err(SaveEditError::Changed);
    }
    let parent = path.parent().ok_or(SaveEditError::UnsafePath)?;
    let guard = DirectoryGuard::pin(parent).map_err(|_| SaveEditError::UnsafePath)?;
    let identity =
        platform_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?;
    guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
    if hash_file(path)? != expected_sha256 {
        return Err(SaveEditError::Changed);
    }
    let backup_path = sibling_path(path, "backup")?;
    let copy_path = sibling_path(path, "copy")?;
    // Reuse only a regular, single-link backup; never replace a link or directory.
    match fs::symlink_metadata(&backup_path) {
        Ok(_) => {
            platform_fs::safe_single_link_file(&backup_path)
                .map_err(|_| SaveEditError::BackupFailed)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(SaveEditError::BackupFailed),
    }
    write_copy(&copy_path, replacement)?;
    let copy_identity =
        platform_fs::safe_single_link_file(&copy_path).map_err(|_| SaveEditError::UnsafePath)?;
    let result = (|| {
        guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
        if platform_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?
            != identity
            || hash_file(path)? != expected_sha256
        {
            return Err(SaveEditError::Changed);
        }
        let edited_sha256 = Sha256::digest(replacement).into();
        if platform_fs::safe_single_link_file(&copy_path).map_err(|_| SaveEditError::UnsafePath)?
            != copy_identity
            || hash_file(&copy_path)? != edited_sha256
        {
            return Err(SaveEditError::Io);
        }
        fs::rename(path, &backup_path).map_err(|_| SaveEditError::BackupFailed)?;
        let promotion = (|| {
            guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
            if platform_fs::safe_single_link_file(&backup_path)
                .map_err(|_| SaveEditError::UnsafePath)?
                != identity
                || hash_file(&backup_path)? != expected_sha256
            {
                return Err(SaveEditError::Changed);
            }
            promote(&copy_path, path)
        })();
        if let Err(error) = promotion {
            // Never overwrite a file that appeared at the original name during rotation.
            if guard.validate().is_err()
                || platform_fs::safe_single_link_file(&backup_path).ok() != Some(identity)
                || move_without_replace(&backup_path, path).is_err()
            {
                return Err(SaveEditError::RecoveryRequired);
            }
            return Err(error);
        }
        Ok(SaveBackup {
            path: backup_path,
            original_sha256: expected_sha256,
            edited_sha256,
        })
    })();
    if result.is_err() && platform_fs::safe_single_link_file(&copy_path).ok() == Some(copy_identity)
    {
        let _ = fs::remove_file(&copy_path);
    }
    result
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

fn write_copy(path: &Path, bytes: &[u8]) -> Result<(), SaveEditError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| SaveEditError::Io)?;
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result.map_err(|_| SaveEditError::Io)
}

fn move_without_replace(from: &Path, to: &Path) -> Result<(), SaveEditError> {
    winsafe::MoveFile(
        from.to_str().ok_or(SaveEditError::UnsafePath)?,
        to.to_str().ok_or(SaveEditError::UnsafePath)?,
    )
    .map_err(|_| SaveEditError::Io)
}

/// Write a verified sibling copy, then publish it at `path`. An existing file is
/// replaced only with `overwrite` and only when it is a regular, single-link file.
pub fn write_new_save_file(
    path: &Path,
    bytes: &[u8],
    overwrite: bool,
) -> Result<(), SaveEditError> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(SaveEditError::TooLarge);
    }
    let parent = path.parent().ok_or(SaveEditError::UnsafePath)?;
    let guard = DirectoryGuard::pin(parent).map_err(|_| SaveEditError::UnsafePath)?;
    let exists = match fs::symlink_metadata(path) {
        Ok(_) if !overwrite => return Err(SaveEditError::Exists),
        Ok(_) => {
            platform_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => return Err(SaveEditError::Io),
    };
    let copy_path = sibling_path(path, "copy")?;
    write_copy(&copy_path, bytes)?;
    let result = (|| {
        guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
        if hash_file(&copy_path)? != <[u8; 32]>::from(Sha256::digest(bytes)) {
            return Err(SaveEditError::Io);
        }
        if exists {
            fs::rename(&copy_path, path).map_err(|_| SaveEditError::Io)
        } else {
            move_without_replace(&copy_path, path)
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&copy_path);
    }
    result
}

/// Restore only the file that still has the expected edited digest. The
/// backup remains on disk after restoration for independent recovery.
pub fn restore_save_from_backup_if_unchanged(
    path: &Path,
    edited_sha256: [u8; 32],
    backup: &SaveBackup,
) -> Result<(), SaveEditError> {
    if backup.path.parent() != path.parent() {
        return Err(SaveEditError::UnsafePath);
    }
    platform_fs::safe_single_link_file(&backup.path).map_err(|_| SaveEditError::UnsafePath)?;
    if fs::metadata(&backup.path)
        .map_err(|_| SaveEditError::BackupFailed)?
        .len()
        > MAX_SNAPSHOT_BYTES
    {
        return Err(SaveEditError::BackupFailed);
    }
    let bytes = fs::read(&backup.path).map_err(|_| SaveEditError::BackupFailed)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(SaveEditError::BackupFailed);
    }
    if <[u8; 32]>::from(Sha256::digest(&bytes)) != backup.original_sha256 {
        return Err(SaveEditError::BackupFailed);
    }
    replace_save_if_unchanged(path, edited_sha256, &bytes)
}

/// Replace only an unchanged, local, single-link file. The caller must hold its
/// load/selection serial lock and verify the replacement by decoding it first.
pub fn replace_save_if_unchanged(
    path: &Path,
    expected_sha256: [u8; 32],
    replacement: &[u8],
) -> Result<(), SaveEditError> {
    if replacement.is_empty() || replacement.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(SaveEditError::TooLarge);
    }
    let parent = path.parent().ok_or(SaveEditError::UnsafePath)?;
    let guard = DirectoryGuard::pin(parent).map_err(|_| SaveEditError::UnsafePath)?;
    let expected_identity =
        platform_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?;
    guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
    if hash_file(path)? != expected_sha256 {
        return Err(SaveEditError::Changed);
    }

    let temporary = sibling_path(path, "copy")?;
    let mut created = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| SaveEditError::Io)?;
        created = true;
        file.write_all(replacement).map_err(|_| SaveEditError::Io)?;
        file.sync_all().map_err(|_| SaveEditError::Io)?;
        drop(file);
        guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
        if platform_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?
            != expected_identity
            || hash_file(path)? != expected_sha256
        {
            return Err(SaveEditError::Changed);
        }
        fs::rename(&temporary, path).map_err(|_| SaveEditError::Io)?;
        guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
        Ok(())
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn hash_file(path: &Path) -> Result<[u8; 32], SaveEditError> {
    let mut file = fs::File::open(path).map_err(|_| SaveEditError::Io)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|_| SaveEditError::Io)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or(SaveEditError::TooLarge)?;
        if total > MAX_SNAPSHOT_BYTES {
            return Err(SaveEditError::TooLarge);
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().into())
}

#[cfg(test)]
#[path = "save_edit_tests.rs"]
mod tests;
