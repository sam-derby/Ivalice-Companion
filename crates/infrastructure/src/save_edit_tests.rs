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
fn disposable_file_replacement_requires_matching_digest() -> Result<(), Box<dyn std::error::Error>>
{
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
fn named_rotation_reuses_backup_and_preserves_filename() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("ivalice-rotation-{}", std::process::id()));
    fs::create_dir_all(&root)?;
    let path = root.join("enhanced.png");
    let original = b"first";
    let first = b"second";
    let second = b"third";
    fs::write(&path, original)?;
    let original_identity =
        crate::platform_fs::safe_single_link_file(&path).map_err(|_| "identity unavailable")?;
    let backup = replace_save_with_backup_if_unchanged(
        &path,
        Sha256::digest(original).into(),
        original,
        first,
    )
    .map_err(|error| format!("first save failed: {error:?}"))?;
    assert_eq!(backup.path(), root.join("enhanced - backup.png"));
    let backup_identity = crate::platform_fs::safe_single_link_file(backup.path())
        .map_err(|_| "identity unavailable")?;
    #[cfg(windows)]
    assert_eq!(backup_identity, original_identity);
    #[cfg(target_os = "linux")]
    assert_ne!(backup_identity, original_identity);
    let backup =
        replace_save_with_backup_if_unchanged(&path, Sha256::digest(first).into(), first, second)
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
#[cfg(windows)]
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
