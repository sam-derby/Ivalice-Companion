use std::error::Error;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{paths_overlap, safe_single_link_file, validate_regular_file, DirectoryGuard};
use crate::{
    replace_save_with_backup_if_unchanged, restore_save_from_backup_if_unchanged, SaveEditError,
    Settings, SettingsError, SettingsStore,
};
use sha2::{Digest, Sha256};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = std::env::temp_dir().join(format!(
            "ivalice-linux-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root)?;
        Ok(Self(root))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn fuse_resource_permission_never_enables_save_access() {
    use super::supported_filesystem;
    let fuse = 0x6573_5546;
    assert!(supported_filesystem(fuse, true, true));
    assert!(!supported_filesystem(fuse, false, true));
    assert!(!supported_filesystem(fuse, true, false));
    assert!(!supported_filesystem(fuse, false, false));
    // NFS remains unsupported even for read-only resources.
    assert!(!supported_filesystem(0x6969, true, true));
}

#[test]
#[ignore = "requires the CI-created read-only FUSE image"]
fn read_only_fuse_resources_preserve_save_restrictions() -> Result<(), Box<dyn Error>> {
    use crate::{CancellationToken, SnapshotErrorCode, SnapshotReader};
    let path = PathBuf::from(
        std::env::var_os("IVALICE_TEST_RESOURCE").ok_or("missing synthetic FUSE resource")?,
    );
    let cancellation = CancellationToken::default();
    let snapshot = SnapshotReader::for_resources().acquire(&path, &cancellation)?;
    assert_eq!(snapshot.bytes(), b"synthetic bundled resource");
    let save_error = SnapshotReader::new()
        .acquire(&path, &cancellation)
        .err()
        .ok_or("FUSE save unexpectedly accepted")?;
    assert_eq!(save_error.code, SnapshotErrorCode::UnsupportedLocation);
    assert_eq!(
        replace_save_with_backup_if_unchanged(
            &path,
            Sha256::digest(snapshot.bytes()).into(),
            snapshot.bytes(),
            b"edited"
        ),
        Err(SaveEditError::UnsafePath)
    );
    assert_eq!(fs::read(&path)?, snapshot.bytes());
    assert_eq!(
        fs::read_dir(path.parent().ok_or("missing FUSE parent")?)?.count(),
        1
    );
    Ok(())
}

#[test]
fn denied_read_is_explicit_and_permissions_are_restored() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("denied.png");
    fs::write(&path, b"original")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000))?;
    let result = validate_regular_file(&path);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    assert_eq!(result, Err(super::PathFailure::AccessDenied));
    assert_eq!(fs::read(path)?, b"original");
    Ok(())
}

#[test]
fn device_inode_identity_and_case_are_preserved() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("Save.png");
    fs::write(&path, b"source")?;
    let alias = scratch.0.join("alias.png");
    fs::hard_link(&path, &alias)?;
    let id = validate_regular_file(&path).map_err(|_| "validation failed")?;
    assert_eq!(id.volume, fs::metadata(&path)?.dev());
    assert_eq!(id.index, fs::metadata(&path)?.ino());
    assert_eq!(validate_regular_file(&alias), Ok(id));
    assert!(safe_single_link_file(&path).is_err());
    assert!(!paths_overlap(
        &scratch.0.join("Save"),
        &scratch.0.join("save")
    ));
    Ok(())
}

#[test]
fn symlink_sources_ancestors_and_settings_are_rejected() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new()?;
    let real = scratch.0.join("real");
    fs::create_dir(&real)?;
    let path = real.join("enhanced.png");
    fs::write(&path, b"original")?;
    let leaf = scratch.0.join("leaf.png");
    let parent = scratch.0.join("alias");
    symlink(&path, &leaf)?;
    symlink(&real, &parent)?;
    for candidate in [leaf, parent.join("enhanced.png")] {
        assert!(validate_regular_file(&candidate).is_err());
        assert_eq!(
            replace_save_with_backup_if_unchanged(
                &candidate,
                Sha256::digest(b"original").into(),
                b"original",
                b"edited"
            ),
            Err(SaveEditError::UnsafePath)
        );
    }
    let local = scratch.0.join("data");
    fs::create_dir(&local)?;
    let store = SettingsStore::for_local_app_data(&local)?;
    symlink(&real, store.root())?;
    assert_eq!(
        store.save(&Settings::default()),
        Err(SettingsError::UnsupportedLocation)
    );
    assert_eq!(fs::read(path)?, b"original");
    Ok(())
}

#[test]
fn renamed_parent_does_not_redirect_descriptor_writes() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new()?;
    let parent = scratch.0.join("parent");
    let moved = scratch.0.join("moved");
    fs::create_dir(&parent)?;
    let guard = DirectoryGuard::pin(&parent).map_err(|_| "pin failed")?;
    fs::rename(&parent, &moved)?;
    fs::create_dir(&parent)?;
    assert!(guard.validate().is_err());
    assert!(guard
        .create_file(std::ffi::OsStr::new("copy.png"), 0o600)
        .is_err());
    assert!(!parent.join("copy.png").exists());
    assert!(!moved.join("copy.png").exists());
    Ok(())
}

#[test]
fn no_replace_rename_preserves_concurrent_file() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new()?;
    fs::write(scratch.0.join("copy"), b"edited")?;
    fs::write(scratch.0.join("save"), b"concurrent")?;
    let guard = DirectoryGuard::pin(&scratch.0).map_err(|_| "pin failed")?;
    assert!(guard
        .rename(
            std::ffi::OsStr::new("copy"),
            std::ffi::OsStr::new("save"),
            false
        )
        .is_err());
    assert_eq!(fs::read(scratch.0.join("save"))?, b"concurrent");
    assert_eq!(fs::read(scratch.0.join("copy"))?, b"edited");
    Ok(())
}

#[test]
fn edit_restore_preserves_permissions_and_exact_backup() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("enhanced.png");
    fs::write(&path, b"original")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;
    let original_inode = fs::metadata(&path)?.ino();
    let backup = replace_save_with_backup_if_unchanged(
        &path,
        Sha256::digest(b"original").into(),
        b"original",
        b"edited",
    )
    .map_err(|_| "save failed")?;
    assert_eq!(fs::read(backup.path())?, b"original");
    assert_eq!(fs::metadata(backup.path())?.ino(), original_inode);
    assert_eq!(fs::metadata(&path)?.mode() & 0o777, 0o640);
    restore_save_from_backup_if_unchanged(&path, Sha256::digest(b"edited").into(), &backup)
        .map_err(|_| "restore failed")?;
    assert_eq!(fs::read(&path)?, b"original");
    assert_eq!(fs::read(backup.path())?, b"original");
    assert_eq!(fs::metadata(&path)?.mode() & 0o777, 0o640);
    Ok(())
}
