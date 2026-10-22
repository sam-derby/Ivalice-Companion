//! Read support is separate from the guarantees required to publish an edit.
use std::fs::{self, File};
use std::os::fd::AsRawFd;

use rustix::fs as native;

use crate::candidate::SaveWriteRestriction;
use crate::platform_fs::PathFailure;

const FUSE: u64 = 0x6573_5546;
const NTFS: u64 = 0x5346_544e;
const NTFS3: u64 = 0x7366_746e;
const EXFAT: u64 = 0x2011_bab0;

fn volume(file: &File) -> Result<(u64, bool), PathFailure> {
    let stat = native::fstatfs(file).map_err(|_| PathFailure::UnsupportedLocation)?;
    let kind = u64::try_from(stat.f_type).map_err(|_| PathFailure::UnsupportedLocation)?;
    let readonly = native::fstatvfs(file)
        .map_err(|_| PathFailure::UnsupportedLocation)?
        .f_flag
        .contains(native::StatVfsMountFlags::RDONLY);
    Ok((kind, readonly))
}

// Kernel v6.12 include/uapi/linux/magic.h; NTFS3 is defined in
// fs/ntfs3/super.c (ntfs_fill_super). Legacy NTFS uses NTFS_SB_MAGIC.
// https://github.com/torvalds/linux/blob/v6.12/include/uapi/linux/magic.h
fn native_save_filesystem(kind: u64) -> bool {
    matches!(
        kind,
        0xef53
            | 0x9123_683e
            | 0x5846_5342
            | 0x0102_1994
            | 0x794c_7630
            | 0x7371_7368
            | NTFS
            | NTFS3
            | EXFAT
    )
}

// fuse/fuseblk alone do not identify NTFS. Require both the descriptor's kernel
// mount record and NTFS-3G's system attribute (not a user-settable marker).
// Interface: tuxera/ntfs-3g 2022.10.3 libntfs-3g/xattrs.c, XATTR_NTFS_ATTRIB.
fn ntfs_3g(file: &File) -> bool {
    let Ok(fdinfo) = fs::read_to_string(format!("/proc/self/fdinfo/{}", file.as_raw_fd())) else {
        return false;
    };
    let Some(mount_id) = fdinfo
        .lines()
        .find_map(|line| line.strip_prefix("mnt_id:\t"))
    else {
        return false;
    };
    let Ok(mounts) = fs::read_to_string("/proc/self/mountinfo") else {
        return false;
    };
    if !ntfs_mount_record(&mounts, mount_id) {
        return false;
    }
    let mut attribute = [0_u8; 4];
    native::fgetxattr(file, "system.ntfs_attrib", &mut attribute).ok() == Some(4)
}

fn ntfs_mount_record(mounts: &str, mount_id: &str) -> bool {
    mounts.lines().any(|line| {
        let Some((mount, filesystem)) = line.split_once(" - ") else {
            return false;
        };
        mount.split_whitespace().next() == Some(mount_id)
            && matches!(
                filesystem.split_whitespace().next(),
                Some("fuse" | "fuseblk" | "fuse.ntfs-3g" | "fuse.lowntfs-3g")
            )
    })
}

pub(crate) fn validate_read(file: &File, resource: bool) -> Result<(), PathFailure> {
    let (kind, readonly) = volume(file)?;
    if supported_filesystem(kind, readonly, resource) || (kind == FUSE && ntfs_3g(file)) {
        Ok(())
    } else {
        Err(PathFailure::UnsupportedLocation)
    }
}

pub(crate) fn supported_filesystem(kind: u64, readonly: bool, resource: bool) -> bool {
    native_save_filesystem(kind) || (kind == FUSE && resource && readonly)
}

pub(crate) fn write_restriction(file: &File) -> Result<Option<SaveWriteRestriction>, PathFailure> {
    validate_read(file, false)?;
    let (kind, readonly) = volume(file)?;
    Ok(if readonly {
        Some(SaveWriteRestriction::ReadOnlyMount)
    } else if matches!(kind, NTFS | NTFS3) {
        // Native ntfs3 publication/recovery has not been verified. Read safely.
        Some(SaveWriteRestriction::UnverifiedDriver)
    } else if kind == FUSE {
        // NTFS-3G 2022.10.3 rejects renameat2(RENAME_NOREPLACE) with EINVAL.
        // Never fall back to a check-then-overwrite rename.
        Some(SaveWriteRestriction::NoReplaceRename)
    } else {
        None
    })
}

pub(crate) fn validate_write(file: &File) -> Result<(), PathFailure> {
    if write_restriction(file)?.is_some() {
        Err(PathFailure::UnsupportedLocation)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_local_types_and_exact_mount_records_are_recognized() {
        assert!(native_save_filesystem(NTFS));
        assert!(native_save_filesystem(NTFS3));
        assert!(native_save_filesystem(EXFAT));
        for kind in [FUSE, 0x6969, 0xff53_4d42, 0xfe53_4d42] {
            assert!(!native_save_filesystem(kind));
        }
        let records = "21 1 0:9 / /a\\040b rw - fuseblk /dev/loop0 rw\n22 1 0:10 / /ssh rw - fuse.sshfs remote rw\n";
        assert!(ntfs_mount_record(records, "21"));
        assert!(!ntfs_mount_record(records, "22"));
        assert!(!ntfs_mount_record(records, "2"));
        assert!(!ntfs_mount_record("21 malformed", "21"));
    }
}
