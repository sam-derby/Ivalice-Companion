use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::windows_fs::{self, DirectoryGuard};
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
        windows_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?;
    guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
    if hash_file(path)? != expected_sha256 {
        return Err(SaveEditError::Changed);
    }
    let backup_path = sibling_path(path, "backup")?;
    let copy_path = sibling_path(path, "copy")?;
    // Reuse only a regular, single-link backup; never replace a link or directory.
    match fs::symlink_metadata(&backup_path) {
        Ok(_) => {
            windows_fs::safe_single_link_file(&backup_path)
                .map_err(|_| SaveEditError::BackupFailed)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(SaveEditError::BackupFailed),
    }
    write_copy(&copy_path, replacement)?;
    let copy_identity =
        windows_fs::safe_single_link_file(&copy_path).map_err(|_| SaveEditError::UnsafePath)?;
    let result = (|| {
        guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
        if windows_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?
            != identity
            || hash_file(path)? != expected_sha256
        {
            return Err(SaveEditError::Changed);
        }
        let edited_sha256 = Sha256::digest(replacement).into();
        if windows_fs::safe_single_link_file(&copy_path).map_err(|_| SaveEditError::UnsafePath)?
            != copy_identity
            || hash_file(&copy_path)? != edited_sha256
        {
            return Err(SaveEditError::Io);
        }
        fs::rename(path, &backup_path).map_err(|_| SaveEditError::BackupFailed)?;
        let promotion = (|| {
            guard.validate().map_err(|_| SaveEditError::UnsafePath)?;
            if windows_fs::safe_single_link_file(&backup_path)
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
                || windows_fs::safe_single_link_file(&backup_path).ok() != Some(identity)
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
    if result.is_err() && windows_fs::safe_single_link_file(&copy_path).ok() == Some(copy_identity)
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
            windows_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?;
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
    windows_fs::safe_single_link_file(&backup.path).map_err(|_| SaveEditError::UnsafePath)?;
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
        windows_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?;
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
        if windows_fs::safe_single_link_file(path).map_err(|_| SaveEditError::UnsafePath)?
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
mod tests {
    use sha2::{Digest, Sha256};
    use std::fs;

    use super::{
        replace_save_if_unchanged, replace_save_with_backup_if_unchanged,
        restore_save_from_backup_if_unchanged, write_new_save_file, SaveEditError,
    };

    #[test]
    fn new_files_never_replace_without_consent() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("ivalice-export-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let path = root.join("export.png");
        write_new_save_file(&path, b"first", false)
            .map_err(|error| format!("export failed: {error:?}"))?;
        assert_eq!(fs::read(&path)?, b"first");
        assert_eq!(
            write_new_save_file(&path, b"second", false),
            Err(SaveEditError::Exists)
        );
        assert_eq!(fs::read(&path)?, b"first");
        write_new_save_file(&path, b"second", true)
            .map_err(|error| format!("overwrite failed: {error:?}"))?;
        assert_eq!(fs::read(&path)?, b"second");
        assert_eq!(
            write_new_save_file(&path, b"", true),
            Err(SaveEditError::TooLarge)
        );
        assert_eq!(fs::read_dir(&root)?.count(), 1);
        fs::remove_file(path)?;
        fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn disposable_file_replacement_requires_matching_digest(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("ivalice-gil-edit-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let path = root.join("copy.png");
        fs::write(&path, b"original")?;
        let expected: [u8; 32] = Sha256::digest(b"original").into();
        assert_eq!(
            replace_save_if_unchanged(&path, [0; 32], b"changed"),
            Err(SaveEditError::Changed)
        );
        assert_eq!(fs::read(&path)?, b"original");
        replace_save_if_unchanged(&path, expected, b"changed")
            .map_err(|error| format!("replacement failed: {error:?}"))?;
        assert_eq!(fs::read(&path)?, b"changed");
        assert_eq!(
            replace_save_if_unchanged(&path, expected, b"again"),
            Err(SaveEditError::Changed)
        );
        fs::remove_file(path)?;
        fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn named_rotation_reuses_backup_and_preserves_filename(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("ivalice-rotation-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let path = root.join("enhanced.png");
        let original = b"first";
        let first = b"second";
        let second = b"third";
        fs::write(&path, original)?;
        let original_identity =
            crate::windows_fs::safe_single_link_file(&path).map_err(|_| "identity unavailable")?;
        let backup = replace_save_with_backup_if_unchanged(
            &path,
            Sha256::digest(original).into(),
            original,
            first,
        )
        .map_err(|error| format!("first save failed: {error:?}"))?;
        assert_eq!(backup.path(), root.join("enhanced - backup.png"));
        assert_eq!(
            crate::windows_fs::safe_single_link_file(backup.path())
                .map_err(|_| "identity unavailable")?,
            original_identity
        );
        let backup = replace_save_with_backup_if_unchanged(
            &path,
            Sha256::digest(first).into(),
            first,
            second,
        )
        .map_err(|error| format!("second save failed: {error:?}"))?;
        assert_eq!(fs::read(&path)?, second);
        assert_eq!(fs::read(backup.path())?, first);
        assert_eq!(fs::read_dir(&root)?.count(), 2);
        restore_save_from_backup_if_unchanged(&path, Sha256::digest(second).into(), &backup)
            .map_err(|error| format!("restore failed: {error:?}"))?;
        assert_eq!(fs::read(&path)?, first);
        assert_eq!(fs::read(backup.path())?, first);
        for filename in ["custom", "custom.Save.PNG", "custom name.png"] {
            let path = root.join(filename);
            let copy = super::sibling_path(&path, "copy").map_err(|_| "invalid filename")?;
            assert_eq!(copy.extension(), path.extension());
            assert!(copy
                .file_stem()
                .ok_or("missing stem")?
                .to_string_lossy()
                .ends_with(" - copy"));
        }
        fs::remove_file(backup.path())?;
        fs::remove_file(&path)?;
        fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn copy_and_unsafe_backup_collisions_leave_original_untouched(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("ivalice-collision-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let path = root.join("enhanced.png");
        let copy = root.join("enhanced - copy.png");
        let backup = root.join("enhanced - backup.png");
        fs::write(&path, b"original")?;
        fs::write(&copy, b"unrelated copy")?;
        let expected = Sha256::digest(b"original").into();
        assert_eq!(
            replace_save_with_backup_if_unchanged(&path, expected, b"original", b"edited"),
            Err(SaveEditError::Io)
        );
        assert_eq!(fs::read(&path)?, b"original");
        assert_eq!(fs::read(&copy)?, b"unrelated copy");
        assert!(!backup.exists());
        fs::remove_file(&copy)?;
        fs::create_dir(&backup)?;
        assert_eq!(
            replace_save_with_backup_if_unchanged(&path, expected, b"original", b"edited"),
            Err(SaveEditError::BackupFailed)
        );
        assert_eq!(fs::read(&path)?, b"original");
        assert!(!copy.exists());
        fs::remove_dir(&backup)?;
        fs::hard_link(&path, &backup)?;
        assert_eq!(
            replace_save_with_backup_if_unchanged(&path, expected, b"original", b"edited"),
            Err(SaveEditError::UnsafePath)
        );
        fs::remove_file(&backup)?;
        fs::remove_file(path)?;
        fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn promotion_failure_rolls_back_or_retains_original_backup(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("ivalice-rollback-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let path = root.join("enhanced.png");
        let copy = root.join("enhanced - copy.png");
        let backup = root.join("enhanced - backup.png");
        fs::write(&path, b"original")?;
        let expected = Sha256::digest(b"original").into();
        let failed = super::rotate_save(&path, expected, b"original", b"edited", |from, to| {
            assert_eq!(from, copy);
            assert_eq!(to, path);
            assert_eq!(
                fs::read(&backup).ok().as_deref(),
                Some(b"original".as_slice())
            );
            assert_eq!(fs::read(from).ok().as_deref(), Some(b"edited".as_slice()));
            Err(SaveEditError::Io)
        });
        assert_eq!(failed, Err(SaveEditError::Io));
        assert_eq!(fs::read(&path)?, b"original");
        assert!(!copy.exists());
        assert!(!backup.exists());
        let failed = super::rotate_save(&path, expected, b"original", b"edited", |from, to| {
            fs::write(to, b"concurrent file").map_err(|_| SaveEditError::Io)?;
            super::move_without_replace(from, to)
        });
        assert_eq!(failed, Err(SaveEditError::RecoveryRequired));
        assert_eq!(fs::read(&path)?, b"concurrent file");
        assert_eq!(fs::read(&backup)?, b"original");
        assert!(!copy.exists());
        fs::remove_file(backup)?;
        fs::remove_file(path)?;
        fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn backup_is_exact_and_restore_rejects_stale_or_tampered_files(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("ivalice-backup-edit-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let path = root.join("copy.png");
        let original = b"complete original save";
        let edited = b"edited save";
        let original_hash: [u8; 32] = Sha256::digest(original).into();
        let edited_hash: [u8; 32] = Sha256::digest(edited).into();
        fs::write(&path, original)?;
        assert_eq!(
            replace_save_with_backup_if_unchanged(&path, [0; 32], original, edited),
            Err(SaveEditError::Changed)
        );
        assert_eq!(fs::read(&path)?, original);
        let backup = replace_save_with_backup_if_unchanged(&path, original_hash, original, edited)
            .map_err(|error| format!("backed replacement failed: {error:?}"))?;
        assert_eq!(fs::read(backup.path())?, original);
        assert_eq!(fs::read(&path)?, edited);
        assert_eq!(backup.edited_sha256(), edited_hash);
        assert_eq!(
            restore_save_from_backup_if_unchanged(&path, original_hash, &backup),
            Err(SaveEditError::Changed)
        );
        fs::write(backup.path(), b"tampered")?;
        assert_eq!(
            restore_save_from_backup_if_unchanged(&path, edited_hash, &backup),
            Err(SaveEditError::BackupFailed)
        );
        assert_eq!(fs::read(&path)?, edited);
        fs::write(backup.path(), original)?;
        restore_save_from_backup_if_unchanged(&path, edited_hash, &backup)
            .map_err(|error| format!("restore failed: {error:?}"))?;
        assert_eq!(fs::read(&path)?, original);
        assert_eq!(fs::read(backup.path())?, original);
        fs::remove_file(backup.path())?;
        fs::remove_file(path)?;
        fs::remove_dir(root)?;
        Ok(())
    }
}
