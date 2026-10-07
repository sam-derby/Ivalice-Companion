use std::fs::File;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::{map_path_failure, AttemptFailure, Observation, SnapshotErrorCode};
use crate::platform_fs::{identity, DirectoryGuard, PathFailure};

pub(super) struct SourceHandle {
    file: File,
    guard: DirectoryGuard,
}

pub(super) fn open_source(path: &Path, was_observed: bool) -> Result<SourceHandle, AttemptFailure> {
    let open = || {
        let guard = DirectoryGuard::pin(path.parent().ok_or(PathFailure::Invalid)?)?;
        let file = guard.open_file(path.file_name().ok_or(PathFailure::Invalid)?)?;
        Ok(SourceHandle { file, guard })
    };
    open().map_err(|failure| {
        if was_observed && failure == PathFailure::NotFound {
            AttemptFailure::changed()
        } else {
            map_path_failure(failure)
        }
    })
}

impl SourceHandle {
    pub(super) fn read_chunk(&self, buffer: &mut [u8]) -> Result<usize, AttemptFailure> {
        let mut file = &self.file;
        file.read(buffer)
            .map_err(|_| AttemptFailure::terminal(SnapshotErrorCode::IoFailure, None))
    }
}

pub(super) fn observe_handle(source: &SourceHandle) -> Result<Observation, AttemptFailure> {
    source.guard.validate().map_err(map_path_failure)?;
    let metadata = source
        .file
        .metadata()
        .map_err(|_| AttemptFailure::terminal(SnapshotErrorCode::IoFailure, None))?;
    if metadata.nlink() == 0 {
        return Err(AttemptFailure::changed());
    }
    Ok(Observation {
        identity: identity(&metadata),
        length: metadata.len(),
        last_write_time: (
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        ),
    })
}
