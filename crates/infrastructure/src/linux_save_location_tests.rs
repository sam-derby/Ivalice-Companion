use std::error::Error;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    replace_save_snapshot_with_backup_if_unchanged, replace_save_with_backup_if_unchanged,
    resolve_save_path, restore_save_snapshot_from_backup_if_unchanged, save_write_restriction,
    CancellationToken, CandidateErrorCode, SaveEditError, SnapshotReader,
};
use sha2::{Digest, Sha256};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn in_directory(base: &Path) -> Result<Self, Box<dyn Error>> {
        let path = base.join(format!(
            "ivalice-location-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn new() -> Result<Self, Box<dyn Error>> {
        Self::in_directory(&std::env::temp_dir())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn links_resolve_once_and_a_retarget_cannot_redirect_a_saved_target() -> Result<(), Box<dyn Error>>
{
    let scratch = Scratch::new()?;
    let first = scratch.0.join("Steam library/Proton prefix/drive_c/Save");
    fs::create_dir_all(&first)?;
    let source = first.join("enhanced.png");
    fs::write(&source, b"original")?;
    let other = scratch.0.join("other.png");
    fs::write(&other, b"original")?;
    let alias = scratch.0.join("leaf.png");
    symlink(&source, &alias)?;
    let pinned = resolve_save_path(&alias).map_err(|_| "resolve failed")?;
    let snapshot = SnapshotReader::new().acquire(&pinned, &CancellationToken::default())?;
    fs::remove_file(&alias)?;
    symlink(&other, &alias)?;
    let backup = replace_save_snapshot_with_backup_if_unchanged(&pinned, &snapshot, b"edited")
        .map_err(|_| "edit failed")?;
    assert_eq!(fs::read(&source)?, b"edited");
    assert_eq!(fs::read(&alias)?, b"original");
    assert!(fs::symlink_metadata(&alias)?.is_symlink());
    assert_eq!(fs::read(backup.path())?, b"original");
    let dangling = scratch.0.join("dangling");
    symlink("missing", &dangling)?;
    assert_eq!(
        resolve_save_path(&dangling),
        Err(CandidateErrorCode::NotFound)
    );
    let looping = scratch.0.join("loop");
    symlink("loop", &looping)?;
    assert_eq!(
        resolve_save_path(&looping),
        Err(CandidateErrorCode::LinkResolution)
    );
    assert_eq!(fs::read_dir(&first)?.count(), 2);
    Ok(())
}

#[test]
fn equal_bytes_replacement_and_inserted_links_cannot_use_a_preview() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new()?;
    let source = scratch.0.join("enhanced.png");
    fs::write(&source, b"original")?;
    let snapshot = SnapshotReader::new().acquire(&source, &CancellationToken::default())?;
    fs::rename(&source, scratch.0.join("old"))?;
    fs::write(&source, b"original")?;
    assert_eq!(
        replace_save_snapshot_with_backup_if_unchanged(&source, &snapshot, b"edited"),
        Err(SaveEditError::Changed)
    );
    assert!(!scratch.0.join("enhanced - backup.png").exists());
    fs::remove_file(&source)?;
    symlink("old", &source)?;
    assert_eq!(
        replace_save_snapshot_with_backup_if_unchanged(&source, &snapshot, b"edited"),
        Err(SaveEditError::UnsafePath)
    );
    assert_eq!(fs::read(&source)?, b"original");
    assert!(fs::symlink_metadata(&source)?.is_symlink());
    Ok(())
}

fn publication_contract(base: &Path) -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::in_directory(base)?;
    let source = scratch.0.join("enhanced.png");
    fs::write(&source, b"original")?;
    assert_eq!(save_write_restriction(&source), Ok(None));
    let reader = SnapshotReader::new();
    let token = CancellationToken::default();
    let original = reader.acquire(&source, &token)?;
    assert_eq!(
        fs::read_dir(&scratch.0)?.count(),
        1,
        "snapshot must not write"
    );
    // An occupied staging filename must fail before touching the save.
    let blocked = scratch.0.join("enhanced - backup copy.png");
    fs::create_dir(&blocked)?;
    assert_eq!(
        replace_save_snapshot_with_backup_if_unchanged(&source, &original, b"edited"),
        Err(SaveEditError::BackupFailed)
    );
    assert_eq!(fs::read(&source)?, original.bytes());
    fs::remove_dir(&blocked)?;
    let backup = replace_save_snapshot_with_backup_if_unchanged(&source, &original, b"edited")
        .map_err(|e| format!("publication failed: {e:?}"))?;
    assert_eq!(fs::read(backup.path())?, original.bytes());
    let edited = reader.acquire(&source, &token)?;
    assert_ne!(original.file_identity(), edited.file_identity());
    // Changed bytes are rejected and the original backup remains recoverable.
    assert_eq!(
        replace_save_with_backup_if_unchanged(
            &source,
            Sha256::digest(b"original").into(),
            b"original",
            b"wrong"
        ),
        Err(SaveEditError::Changed)
    );
    assert_eq!(fs::read(backup.path())?, b"original");
    restore_save_snapshot_from_backup_if_unchanged(&source, &edited, &backup)
        .map_err(|e| format!("restore failed: {e:?}"))?;
    assert_eq!(fs::read(&source)?, original.bytes());
    assert_eq!(fs::read(backup.path())?, original.bytes());
    assert_eq!(
        fs::read_dir(&scratch.0)?.count(),
        2,
        "temporary files must be retired"
    );
    // A second publication exercises replacement of the previous backup.
    let reloaded = reader.acquire(&source, &token)?;
    replace_save_snapshot_with_backup_if_unchanged(&source, &reloaded, b"second")
        .map_err(|e| format!("second publication failed: {e:?}"))?;
    assert_eq!(fs::read(backup.path())?, original.bytes());
    Ok(())
}

#[test]
fn publication_contract_on_local_scratch() -> Result<(), Box<dyn Error>> {
    publication_contract(&std::env::temp_dir())
}

#[test]
#[ignore = "requires an explicit disposable NTFS/exFAT image mount"]
fn mounted_save_publication() -> Result<(), Box<dyn Error>> {
    let base = PathBuf::from(
        std::env::var_os("IVALICE_TEST_SAVE_DIRECTORY")
            .ok_or("missing disposable mount directory")?,
    );
    if std::env::var("IVALICE_TEST_DRIVER").ok().as_deref() == Some("ntfs-3g") {
        let scratch = Scratch::in_directory(&base)?;
        let source = scratch.0.join("enhanced.png");
        fs::write(&source, b"original")?;
        assert_eq!(
            save_write_restriction(&source),
            Ok(Some(crate::SaveWriteRestriction::NoReplaceRename))
        );
        let snapshot = SnapshotReader::new().acquire(&source, &CancellationToken::default())?;
        assert_eq!(snapshot.bytes(), b"original");
        assert_eq!(
            replace_save_snapshot_with_backup_if_unchanged(&source, &snapshot, b"edited"),
            Err(SaveEditError::BackupFailed)
        );
        assert_eq!(fs::read_dir(&scratch.0)?.count(), 1);
        assert_eq!(fs::read(&source)?, b"original");
        Ok(())
    } else {
        publication_contract(&base)
    }
}

#[test]
#[ignore = "requires an explicit disposable writable filesystem mount without verified publication"]
fn mounted_unverified_save_reads_and_attempts_guarded_publication() -> Result<(), Box<dyn Error>> {
    let base = PathBuf::from(
        std::env::var_os("IVALICE_TEST_SAVE_DIRECTORY")
            .ok_or("missing disposable mount directory")?,
    );
    let scratch = Scratch::in_directory(&base)?;
    let directory = scratch.0.join("Steam library/steamapps/compatdata/1004640/pfx/drive_c/users/steamuser/Documents/My Games/Test Game/Steam/test-profile");
    fs::create_dir_all(&directory)?;
    let source = directory.join("enhanced.png");
    fs::write(&source, b"original")?;
    let selected = resolve_save_path(&source).map_err(|_| "selection failed")?;
    let reader = SnapshotReader::new();
    let token = CancellationToken::default();
    let snapshot = reader.acquire(&selected, &token)?;
    assert_eq!(snapshot.bytes(), b"original");
    assert_eq!(
        save_write_restriction(&selected),
        Ok(Some(crate::SaveWriteRestriction::UnverifiedDriver))
    );
    let attempted = replace_save_snapshot_with_backup_if_unchanged(&selected, &snapshot, b"edited");
    if std::env::var("IVALICE_TEST_DRIVER").ok().as_deref() == Some("exfat-fuse")
        && attempted.is_err()
    {
        // FUSE driver versions differ in rename-without-replace support.
        // Unsupported publication must fail before changing the source,
        // without leaving a staging file or falling back to overwrite.
        assert_eq!(attempted, Err(SaveEditError::BackupFailed));
        assert_eq!(fs::read_dir(&directory)?.count(), 1);
        assert_eq!(fs::read(&selected)?, b"original");
        println!("Required publication operation unsupported; attempted edit left the original and folder unchanged");
    } else {
        let backup = attempted
            .map_err(|error| format!("unverified driver publication failed: {error:?}"))?;
        assert_eq!(fs::read(backup.path())?, b"original");
        assert_eq!(fs::read(&selected)?, b"edited");
        let edited = reader.acquire(&selected, &token)?;
        restore_save_snapshot_from_backup_if_unchanged(&selected, &edited, &backup)
            .map_err(|error| format!("unverified driver restore failed: {error:?}"))?;
        assert_eq!(fs::read(&selected)?, b"original");
        assert_eq!(fs::read(backup.path())?, b"original");
        assert_eq!(
            replace_save_snapshot_with_backup_if_unchanged(&selected, &snapshot, b"stale"),
            Err(SaveEditError::Changed)
        );
        println!("Unverified driver completed guarded publication, exact backup and restoration");
    }
    fs::write(&source, b"changed")?;
    let reloaded = reader.acquire(&selected, &token)?;
    assert_eq!(reloaded.bytes(), b"changed");
    assert_ne!(snapshot.sha256(), reloaded.sha256());
    assert_eq!(snapshot.bytes(), b"original");
    Ok(())
}

#[test]
#[ignore = "requires an explicit read-only disposable image mount"]
fn mounted_save_read_only() -> Result<(), Box<dyn Error>> {
    let source = PathBuf::from(
        std::env::var_os("IVALICE_TEST_READ_ONLY_SAVE")
            .ok_or("missing read-only synthetic source")?,
    );
    let parent = source.parent().ok_or("missing parent")?;
    let count = fs::read_dir(parent)?.count();
    let snapshot = SnapshotReader::new().acquire(&source, &CancellationToken::default())?;
    assert_eq!(snapshot.bytes(), b"synthetic read-only save");
    assert_eq!(
        save_write_restriction(&source),
        Ok(Some(crate::SaveWriteRestriction::ReadOnlyMount))
    );
    assert_eq!(
        replace_save_snapshot_with_backup_if_unchanged(&source, &snapshot, b"edited"),
        Err(SaveEditError::ReadOnlyLocation)
    );
    assert_eq!(
        crate::write_new_save_file(&parent.join("export.png"), b"export", false),
        Err(SaveEditError::ReadOnlyLocation)
    );
    assert_eq!(fs::read(&source)?, snapshot.bytes());
    assert_eq!(fs::read_dir(parent)?.count(), count);
    Ok(())
}
