use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::platform_fs::{self, FileIdentity, PathFailure};

#[cfg(windows)]
type LastWriteTime = u64;
#[cfg(target_os = "linux")]
type LastWriteTime = (i64, i64, i64, i64);
#[cfg(target_os = "linux")]
#[path = "snapshot_linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
use linux::{observe_handle, open_source, SourceHandle};

pub const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;
pub const SNAPSHOT_ATTEMPTS: u8 = 3;
const READ_CHUNK_BYTES: usize = 1024 * 1024;
const OBSERVATION_INTERVAL: Duration = Duration::from_millis(100);
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(100), Duration::from_millis(200)];
const ELAPSED_BUDGET: Duration = Duration::from_secs(2);
#[cfg(windows)]
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
#[cfg(windows)]
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
#[cfg(windows)]
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
#[cfg(windows)]
const UNSUPPORTED_ATTRIBUTES: u32 = FILE_ATTRIBUTE_REPARSE_POINT
    | FILE_ATTRIBUTE_OFFLINE
    | FILE_ATTRIBUTE_RECALL_ON_OPEN
    | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotErrorCode {
    InvalidPath,
    NotRegularFile,
    UnsupportedLocation,
    IdentityUnavailable,
    NotFound,
    AccessDenied,
    SourceBusy,
    SourceChanged,
    EmptyInput,
    TooLarge,
    ResourceLimit,
    IoFailure,
    RetryExhausted,
    TimedOut,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotTransientCode {
    SourceBusy,
    SourceChanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotError {
    pub code: SnapshotErrorCode,
    pub attempts: u8,
    pub os_code: Option<u32>,
    pub last_transient: Option<SnapshotTransientCode>,
}

impl SnapshotError {
    fn terminal(code: SnapshotErrorCode, attempts: u8, os_code: Option<u32>) -> Self {
        Self {
            code,
            attempts,
            os_code,
            last_transient: None,
        }
    }
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.code {
            SnapshotErrorCode::InvalidPath => "snapshot path is invalid",
            SnapshotErrorCode::NotRegularFile => "snapshot source is not a regular file",
            SnapshotErrorCode::UnsupportedLocation => "snapshot source location is unsupported",
            SnapshotErrorCode::IdentityUnavailable => "snapshot source identity is unavailable",
            SnapshotErrorCode::NotFound => "snapshot source was not found",
            SnapshotErrorCode::AccessDenied => "snapshot source access was denied",
            SnapshotErrorCode::SourceBusy => "snapshot source is busy",
            SnapshotErrorCode::SourceChanged => "snapshot source changed while reading",
            SnapshotErrorCode::EmptyInput => "snapshot source is empty",
            SnapshotErrorCode::TooLarge => "snapshot source exceeds the size limit",
            SnapshotErrorCode::ResourceLimit => "snapshot resource limit was reached",
            SnapshotErrorCode::IoFailure => "snapshot input/output operation failed",
            SnapshotErrorCode::RetryExhausted => "snapshot retries were exhausted",
            SnapshotErrorCode::TimedOut => "snapshot acquisition timed out",
            SnapshotErrorCode::Cancelled => "snapshot acquisition was cancelled",
        })
    }
}

impl std::error::Error for SnapshotError {}

pub struct Snapshot {
    bytes: Arc<[u8]>,
    sha256: [u8; 32],
    observed: Observation,
}

impl Clone for Snapshot {
    fn clone(&self) -> Self {
        Self {
            bytes: Arc::clone(&self.bytes),
            sha256: self.sha256,
            observed: self.observed,
        }
    }
}

impl Snapshot {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn sha256(&self) -> [u8; 32] {
        self.sha256
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[derive(Clone, Copy)]
struct Observation {
    identity: FileIdentity,
    length: u64,
    last_write_time: LastWriteTime,
}

impl Observation {
    #[cfg(windows)]
    fn from_information(
        information: &winsafe::BY_HANDLE_FILE_INFORMATION,
    ) -> Result<Self, AttemptFailure> {
        let attributes = information.dwFileAttributes.raw();
        if information.nNumberOfLinks == 0 {
            return Err(AttemptFailure::changed());
        }
        if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return Err(AttemptFailure::terminal(
                SnapshotErrorCode::NotRegularFile,
                None,
            ));
        }
        if attributes & UNSUPPORTED_ATTRIBUTES != 0 {
            return Err(AttemptFailure::terminal(
                SnapshotErrorCode::UnsupportedLocation,
                None,
            ));
        }
        Ok(Self {
            identity: FileIdentity {
                volume: u64::from(information.dwVolumeSerialNumber),
                index: information.nFileIndex(),
            },
            length: information.nFileSize(),
            last_write_time: u64::from(information.ftLastWriteTime.dwLowDateTime)
                | (u64::from(information.ftLastWriteTime.dwHighDateTime) << 32),
        })
    }

    fn same_source(self, other: Self) -> bool {
        self.identity == other.identity
            && self.length == other.length
            && self.last_write_time == other.last_write_time
    }
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    state: Arc<CancellationState>,
}

#[derive(Debug, Default)]
struct CancellationState {
    cancelled: AtomicBool,
    lock: Mutex<()>,
    wake: Condvar,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Release);
        self.state.wake.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }

    fn wait(&self, duration: Duration) {
        if self.is_cancelled() {
            return;
        }
        let guard = lock_recover(&self.state.lock);
        let _unused = match self.state.wake.wait_timeout(guard, duration) {
            Ok(result) => result,
            Err(poisoned) => poisoned.into_inner(),
        };
    }
}

#[derive(Clone, Default)]
pub struct SnapshotReader {
    state: Arc<ReaderState>,
}

#[derive(Default)]
struct ReaderState {
    generation: AtomicU64,
    acquisition: Mutex<()>,
}

impl SnapshotReader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel_current(&self) {
        self.state.generation.fetch_add(1, Ordering::AcqRel);
    }

    pub fn acquire(
        &self,
        path: &Path,
        cancellation: &CancellationToken,
    ) -> Result<Snapshot, SnapshotError> {
        self.acquire_limited(path, cancellation, MAX_SNAPSHOT_BYTES)
    }

    /// Apply a caller's stricter byte cap before allocating or reading source data.
    pub fn acquire_limited(
        &self,
        path: &Path,
        cancellation: &CancellationToken,
        max_bytes: u64,
    ) -> Result<Snapshot, SnapshotError> {
        let generation = self.state.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let started = Instant::now();
        let _guard = lock_recover(&self.state.acquisition);
        let runtime = NativeRuntime { started };
        self.acquire_inner(
            path,
            cancellation,
            generation,
            &runtime,
            max_bytes.min(MAX_SNAPSHOT_BYTES),
        )
    }

    fn acquire_inner(
        &self,
        path: &Path,
        cancellation: &CancellationToken,
        generation: u64,
        runtime: &dyn Runtime,
        max_bytes: u64,
    ) -> Result<Snapshot, SnapshotError> {
        let mut last_transient = None;
        let mut ever_observed = false;

        for attempt in 1..=SNAPSHOT_ATTEMPTS {
            check_stop(self, cancellation, generation, runtime, attempt - 1)?;
            let context = AttemptContext {
                reader: self,
                cancellation,
                generation,
                runtime,
                attempt,
            };
            match acquire_attempt(path, &context, &mut ever_observed, max_bytes) {
                Ok(snapshot) => {
                    check_stop(self, cancellation, generation, runtime, attempt)?;
                    runtime.checkpoint(Checkpoint::BeforePublish, attempt);
                    check_stop(self, cancellation, generation, runtime, attempt)?;
                    return Ok(snapshot);
                }
                Err(failure) if failure.transient.is_some() => {
                    last_transient = failure.transient;
                    if attempt == SNAPSHOT_ATTEMPTS {
                        return Err(SnapshotError {
                            code: SnapshotErrorCode::RetryExhausted,
                            attempts: attempt,
                            os_code: failure.os_code,
                            last_transient,
                        });
                    }
                    runtime.checkpoint(Checkpoint::BeforeRetry, attempt);
                    let delay = RETRY_DELAYS[usize::from(attempt - 1)];
                    interruptible_wait(self, cancellation, generation, runtime, delay, attempt)?;
                }
                Err(failure) => {
                    return Err(SnapshotError {
                        code: failure.code,
                        attempts: attempt,
                        os_code: failure.os_code,
                        last_transient,
                    });
                }
            }
        }

        Err(SnapshotError {
            code: SnapshotErrorCode::RetryExhausted,
            attempts: SNAPSHOT_ATTEMPTS,
            os_code: None,
            last_transient,
        })
    }
}

struct AttemptContext<'a> {
    reader: &'a SnapshotReader,
    cancellation: &'a CancellationToken,
    generation: u64,
    runtime: &'a dyn Runtime,
    attempt: u8,
}

fn acquire_attempt(
    path: &Path,
    context: &AttemptContext<'_>,
    ever_observed: &mut bool,
    max_bytes: u64,
) -> Result<Snapshot, AttemptFailure> {
    let expected_path_identity = validate_source_path(path, *ever_observed)?;
    let mut first = open_source(path, *ever_observed)?;
    let observation = observe_handle(&first)?;
    if observation.identity != expected_path_identity {
        return Err(AttemptFailure::changed());
    }
    *ever_observed = true;
    enforce_length(observation.length, max_bytes)?;
    check_stop_attempt(context)?;

    let length = usize::try_from(observation.length)
        .map_err(|_| AttemptFailure::terminal(SnapshotErrorCode::ResourceLimit, None))?;
    if !context.runtime.allow_allocation(length) {
        return Err(AttemptFailure::terminal(
            SnapshotErrorCode::ResourceLimit,
            None,
        ));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| AttemptFailure::terminal(SnapshotErrorCode::ResourceLimit, None))?;

    read_first_pass(&mut first, &mut bytes, length, context)?;
    context
        .runtime
        .checkpoint(Checkpoint::AfterFirstPass, context.attempt);
    check_stop_attempt(context)?;
    require_unchanged_handle(&first, observation)?;
    require_current_path(path, observation)?;
    drop(first);

    interruptible_wait_attempt(context, OBSERVATION_INTERVAL)?;
    context
        .runtime
        .checkpoint(Checkpoint::BeforeSecondOpen, context.attempt);
    check_stop_attempt(context)?;

    let expected_second_identity = validate_source_path(path, true)?;
    if expected_second_identity != observation.identity {
        return Err(AttemptFailure::changed());
    }
    let mut second = open_source(path, true)?;
    let second_observation = observe_handle(&second)?;
    if !observation.same_source(second_observation) {
        return Err(AttemptFailure::changed());
    }
    compare_second_pass(&mut second, &bytes, context)?;
    context
        .runtime
        .checkpoint(Checkpoint::AfterSecondPass, context.attempt);
    check_stop_attempt(context)?;
    require_unchanged_handle(&second, observation)?;
    require_current_path(path, observation)?;
    drop(second);

    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    Ok(Snapshot {
        bytes: Arc::from(bytes.into_boxed_slice()),
        sha256: digest,
        observed: observation,
    })
}

fn validate_source_path(path: &Path, was_observed: bool) -> Result<FileIdentity, AttemptFailure> {
    platform_fs::validate_regular_file(path).map_err(|failure| {
        let mapped = map_path_failure(failure);
        if was_observed && mapped.code == SnapshotErrorCode::NotFound {
            AttemptFailure::changed()
        } else {
            mapped
        }
    })
}

#[cfg(windows)]
fn open_source(path: &Path, was_observed: bool) -> Result<SourceHandle, AttemptFailure> {
    let path = path
        .to_str()
        .ok_or_else(|| AttemptFailure::terminal(SnapshotErrorCode::InvalidPath, None))?;
    let share = winsafe::co::FILE_SHARE::READ
        | winsafe::co::FILE_SHARE::WRITE
        | winsafe::co::FILE_SHARE::DELETE;
    winsafe::HFILE::CreateFile(
        path,
        winsafe::co::GENERIC::READ,
        Some(share),
        None,
        winsafe::co::DISPOSITION::OPEN_EXISTING,
        winsafe::co::FILE_ATTRIBUTE::NORMAL,
        Some(winsafe::co::FILE_FLAG::OPEN_REPARSE_POINT),
        None,
        None,
    )
    .map(|(handle, _)| SourceHandle { handle })
    .map_err(|error| map_windows_failure(error, was_observed))
}

#[cfg(windows)]
struct SourceHandle {
    handle: winsafe::guard::CloseHandleGuard<winsafe::HFILE>,
}

#[cfg(windows)]
impl SourceHandle {
    fn read_chunk(&self, buffer: &mut [u8]) -> Result<usize, AttemptFailure> {
        self.handle
            .ReadFile(buffer)
            .map(|count| count as usize)
            .map_err(|error| map_windows_failure(error, true))
    }
}

trait ChunkSource {
    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize, AttemptFailure>;
}

impl ChunkSource for SourceHandle {
    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize, AttemptFailure> {
        SourceHandle::read_chunk(self, buffer)
    }
}

#[cfg(windows)]
fn observe_handle(file: &SourceHandle) -> Result<Observation, AttemptFailure> {
    if file
        .handle
        .GetFileType()
        .map_err(|error| map_windows_failure(error, true))?
        != winsafe::co::FILE_TYPE::DISK
    {
        return Err(AttemptFailure::terminal(
            SnapshotErrorCode::NotRegularFile,
            None,
        ));
    }
    file.handle
        .GetFileInformationByHandle()
        .map_err(|error| map_windows_failure(error, true))
        .and_then(|information| Observation::from_information(&information))
}

fn enforce_length(length: u64, max_bytes: u64) -> Result<(), AttemptFailure> {
    if length == 0 {
        Err(AttemptFailure::terminal(
            SnapshotErrorCode::EmptyInput,
            None,
        ))
    } else if length > max_bytes {
        Err(AttemptFailure::terminal(SnapshotErrorCode::TooLarge, None))
    } else {
        Ok(())
    }
}

fn read_first_pass<S: ChunkSource>(
    file: &mut S,
    output: &mut Vec<u8>,
    length: usize,
    context: &AttemptContext<'_>,
) -> Result<(), AttemptFailure> {
    let mut chunk = [0_u8; READ_CHUNK_BYTES];
    while output.len() < length {
        check_stop_attempt(context)?;
        let remaining = length - output.len();
        let count = file.read_chunk(&mut chunk[..remaining.min(READ_CHUNK_BYTES)])?;
        if count == 0 {
            return Err(AttemptFailure::changed());
        }
        output.extend_from_slice(&chunk[..count]);
        context
            .runtime
            .checkpoint(Checkpoint::AfterFirstChunk, context.attempt);
    }
    probe_eof(file)?;
    Ok(())
}

fn compare_second_pass<S: ChunkSource>(
    file: &mut S,
    expected: &[u8],
    context: &AttemptContext<'_>,
) -> Result<(), AttemptFailure> {
    let mut chunk = [0_u8; READ_CHUNK_BYTES];
    let mut offset = 0_usize;
    while offset < expected.len() {
        check_stop_attempt(context)?;
        let remaining = expected.len() - offset;
        let count = file.read_chunk(&mut chunk[..remaining.min(READ_CHUNK_BYTES)])?;
        if count == 0 || chunk[..count] != expected[offset..offset + count] {
            return Err(AttemptFailure::changed());
        }
        offset += count;
        context
            .runtime
            .checkpoint(Checkpoint::AfterSecondChunk, context.attempt);
    }
    probe_eof(file)?;
    Ok(())
}

fn probe_eof<S: ChunkSource>(file: &mut S) -> Result<(), AttemptFailure> {
    let mut probe = [0_u8; 1];
    let count = file.read_chunk(&mut probe)?;
    if count == 0 {
        Ok(())
    } else {
        Err(AttemptFailure::changed())
    }
}

fn require_unchanged_handle(
    file: &SourceHandle,
    expected: Observation,
) -> Result<(), AttemptFailure> {
    if expected.same_source(observe_handle(file)?) {
        Ok(())
    } else {
        Err(AttemptFailure::changed())
    }
}

fn require_current_path(path: &Path, expected: Observation) -> Result<(), AttemptFailure> {
    let identity = validate_source_path(path, true)?;
    if identity != expected.identity {
        return Err(AttemptFailure::changed());
    }
    let fresh = open_source(path, true)?;
    if expected.same_source(observe_handle(&fresh)?) {
        Ok(())
    } else {
        Err(AttemptFailure::changed())
    }
}

fn check_stop(
    reader: &SnapshotReader,
    cancellation: &CancellationToken,
    generation: u64,
    runtime: &dyn Runtime,
    attempts: u8,
) -> Result<(), SnapshotError> {
    if cancellation.is_cancelled() || reader.state.generation.load(Ordering::Acquire) != generation
    {
        Err(SnapshotError::terminal(
            SnapshotErrorCode::Cancelled,
            attempts,
            None,
        ))
    } else if runtime.elapsed() >= ELAPSED_BUDGET {
        Err(SnapshotError::terminal(
            SnapshotErrorCode::TimedOut,
            attempts,
            None,
        ))
    } else {
        Ok(())
    }
}

fn check_stop_attempt(context: &AttemptContext<'_>) -> Result<(), AttemptFailure> {
    check_stop(
        context.reader,
        context.cancellation,
        context.generation,
        context.runtime,
        0,
    )
    .map_err(|error| AttemptFailure::terminal(error.code, error.os_code))
}

fn interruptible_wait(
    reader: &SnapshotReader,
    cancellation: &CancellationToken,
    generation: u64,
    runtime: &dyn Runtime,
    duration: Duration,
    attempts: u8,
) -> Result<(), SnapshotError> {
    runtime.wait(duration, cancellation);
    check_stop(reader, cancellation, generation, runtime, attempts)
}

fn interruptible_wait_attempt(
    context: &AttemptContext<'_>,
    duration: Duration,
) -> Result<(), AttemptFailure> {
    interruptible_wait(
        context.reader,
        context.cancellation,
        context.generation,
        context.runtime,
        duration,
        0,
    )
    .map_err(|error| AttemptFailure::terminal(error.code, error.os_code))
}

#[derive(Clone, Copy)]
enum Checkpoint {
    AfterFirstChunk,
    AfterFirstPass,
    BeforeSecondOpen,
    AfterSecondChunk,
    AfterSecondPass,
    BeforeRetry,
    BeforePublish,
}

trait Runtime: Send + Sync {
    fn elapsed(&self) -> Duration;
    fn wait(&self, duration: Duration, cancellation: &CancellationToken);
    fn checkpoint(&self, _checkpoint: Checkpoint, _attempt: u8) {}
    fn allow_allocation(&self, _length: usize) -> bool {
        true
    }
}

struct NativeRuntime {
    started: Instant,
}

impl Runtime for NativeRuntime {
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn wait(&self, duration: Duration, cancellation: &CancellationToken) {
        cancellation.wait(duration);
    }
}

#[derive(Clone, Copy)]
struct AttemptFailure {
    code: SnapshotErrorCode,
    os_code: Option<u32>,
    transient: Option<SnapshotTransientCode>,
}

impl AttemptFailure {
    fn terminal(code: SnapshotErrorCode, os_code: Option<u32>) -> Self {
        Self {
            code,
            os_code,
            transient: None,
        }
    }

    fn changed() -> Self {
        Self {
            code: SnapshotErrorCode::SourceChanged,
            os_code: None,
            transient: Some(SnapshotTransientCode::SourceChanged),
        }
    }

    fn busy(os_code: Option<u32>) -> Self {
        Self {
            code: SnapshotErrorCode::SourceBusy,
            os_code,
            transient: Some(SnapshotTransientCode::SourceBusy),
        }
    }
}

fn map_path_failure(failure: PathFailure) -> AttemptFailure {
    AttemptFailure::terminal(
        match failure {
            PathFailure::Invalid => SnapshotErrorCode::InvalidPath,
            PathFailure::NotFound => SnapshotErrorCode::NotFound,
            PathFailure::AccessDenied => SnapshotErrorCode::AccessDenied,
            PathFailure::NotRegularFile => SnapshotErrorCode::NotRegularFile,
            PathFailure::UnsupportedLocation => SnapshotErrorCode::UnsupportedLocation,
            PathFailure::IdentityUnavailable => SnapshotErrorCode::IdentityUnavailable,
            PathFailure::Busy => return AttemptFailure::busy(None),
            PathFailure::Io => SnapshotErrorCode::IoFailure,
        },
        None,
    )
}

#[cfg(windows)]
fn map_windows_failure(error: winsafe::co::ERROR, was_observed: bool) -> AttemptFailure {
    let os_code = Some(error.raw());
    match os_code {
        Some(2 | 3) if was_observed => AttemptFailure::changed(),
        Some(2 | 3) => AttemptFailure::terminal(SnapshotErrorCode::NotFound, os_code),
        Some(5) => AttemptFailure::terminal(SnapshotErrorCode::AccessDenied, os_code),
        Some(8 | 14) => AttemptFailure::terminal(SnapshotErrorCode::ResourceLimit, os_code),
        Some(32 | 33) => AttemptFailure::busy(os_code),
        _ => AttemptFailure::terminal(SnapshotErrorCode::IoFailure, os_code),
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    #[cfg(windows)]
    use std::fs::File;
    use std::fs::{self, FileTimes, OpenOptions};
    use std::io::{Cursor, Read};
    #[cfg(windows)]
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    #[cfg(windows)]
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    #[cfg(windows)]
    use std::thread;
    #[cfg(windows)]
    use std::time::Instant;

    use super::*;

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn synthetic_file(label: &str, bytes: &[u8]) -> Result<PathBuf, Box<dyn Error>> {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ivalice-d002-{}-{sequence}-{label}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        let path = root.join("source.bin");
        fs::write(&path, bytes)?;
        Ok(path)
    }

    fn acquire(path: &Path) -> Result<Snapshot, SnapshotError> {
        SnapshotReader::new().acquire(path, &CancellationToken::default())
    }

    #[test]
    fn stable_snapshot_is_immutable_after_source_changes() -> Result<(), Box<dyn Error>> {
        let path = synthetic_file("stable", b"stable synthetic bytes")?;
        let before = fs::read(&path)?;
        let metadata = fs::metadata(&path)?;
        let snapshot = acquire(&path)?;
        let after_acquisition = fs::metadata(&path)?;
        assert_eq!(fs::read(&path)?, before);
        assert_eq!(after_acquisition.len(), metadata.len());
        assert_eq!(after_acquisition.modified()?, metadata.modified()?);
        let expected_digest: [u8; 32] = Sha256::digest(&before).into();
        assert_eq!(snapshot.sha256, expected_digest);
        #[cfg(windows)]
        assert_eq!(
            snapshot.observed.last_write_time,
            metadata.last_write_time()
        );
        fs::write(&path, b"writer changed source")?;

        assert_eq!(snapshot.bytes(), before);
        assert_eq!(snapshot.len(), before.len());
        assert_ne!(snapshot.bytes(), fs::read(&path)?);
        Ok(())
    }

    #[test]
    fn empty_oversized_and_exact_limit_inputs_are_bounded() -> Result<(), Box<dyn Error>> {
        let empty = synthetic_file("empty", b"")?;
        let error = acquire(&empty)
            .err()
            .ok_or("empty input unexpectedly passed")?;
        assert_eq!(error.code, SnapshotErrorCode::EmptyInput);

        let oversized = synthetic_file("oversized", b"x")?;
        fs::OpenOptions::new()
            .write(true)
            .open(&oversized)?
            .set_len(MAX_SNAPSHOT_BYTES + 1)?;
        let error = acquire(&oversized)
            .err()
            .ok_or("oversized input unexpectedly passed")?;
        assert_eq!(error.code, SnapshotErrorCode::TooLarge);

        let exact = synthetic_file("exact", b"x")?;
        fs::OpenOptions::new()
            .write(true)
            .open(&exact)?
            .set_len(MAX_SNAPSHOT_BYTES)?;
        // This checks the byte boundary, not the host's disk throughput. Keep
        // the production deadline covered by the separate clock-driven tests.
        let snapshot = acquire_with_runtime(
            &SnapshotReader::new(),
            &exact,
            &CancellationToken::default(),
            &HookRuntime::new(|_, _| {}),
        )?;
        assert_eq!(u64::try_from(snapshot.len())?, MAX_SNAPSHOT_BYTES);
        Ok(())
    }

    #[test]
    fn bounded_read_loop_accepts_short_chunks_and_rejects_early_eof() -> Result<(), Box<dyn Error>>
    {
        struct ShortReader {
            inner: Cursor<Vec<u8>>,
            maximum: usize,
        }
        impl ChunkSource for ShortReader {
            fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize, AttemptFailure> {
                let maximum = buffer.len().min(self.maximum);
                self.inner
                    .read(&mut buffer[..maximum])
                    .map_err(|_| AttemptFailure::terminal(SnapshotErrorCode::IoFailure, None))
            }
        }

        let mut source = ShortReader {
            inner: Cursor::new(b"short chunks".to_vec()),
            maximum: 2,
        };
        let reader = SnapshotReader::new();
        let generation = reader.state.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let runtime = HookRuntime::new(|_, _| {});
        let cancellation = CancellationToken::default();
        let context = AttemptContext {
            reader: &reader,
            cancellation: &cancellation,
            generation,
            runtime: &runtime,
            attempt: 1,
        };
        let mut output = Vec::new();
        read_first_pass(&mut source, &mut output, b"short chunks".len(), &context)
            .map_err(|_| "short reads should be joined")?;
        assert_eq!(output, b"short chunks");

        let mut early = ShortReader {
            inner: Cursor::new(b"tiny".to_vec()),
            maximum: 2,
        };
        let mut target = Vec::new();
        let failure = read_first_pass(&mut early, &mut target, 8, &context)
            .err()
            .ok_or("early EOF unexpectedly passed")?;
        assert_eq!(failure.code, SnapshotErrorCode::SourceChanged);
        Ok(())
    }

    struct HookRuntime<F> {
        elapsed_millis: AtomicU64,
        allocation_allowed: bool,
        hook: F,
    }

    impl<F> HookRuntime<F>
    where
        F: Fn(Checkpoint, u8) + Send + Sync,
    {
        fn new(hook: F) -> Self {
            Self {
                elapsed_millis: AtomicU64::new(0),
                allocation_allowed: true,
                hook,
            }
        }
    }

    impl<F> Runtime for HookRuntime<F>
    where
        F: Fn(Checkpoint, u8) + Send + Sync,
    {
        fn elapsed(&self) -> Duration {
            Duration::from_millis(self.elapsed_millis.load(Ordering::Acquire))
        }

        fn wait(&self, duration: Duration, _cancellation: &CancellationToken) {
            let millis = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
            self.elapsed_millis.fetch_add(millis, Ordering::AcqRel);
        }

        fn checkpoint(&self, checkpoint: Checkpoint, attempt: u8) {
            (self.hook)(checkpoint, attempt);
        }

        fn allow_allocation(&self, _length: usize) -> bool {
            self.allocation_allowed
        }
    }

    fn acquire_with_runtime(
        reader: &SnapshotReader,
        path: &Path,
        cancellation: &CancellationToken,
        runtime: &dyn Runtime,
    ) -> Result<Snapshot, SnapshotError> {
        let generation = reader.state.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let _guard = lock_recover(&reader.state.acquisition);
        reader.acquire_inner(path, cancellation, generation, runtime, MAX_SNAPSHOT_BYTES)
    }

    #[test]
    fn same_length_change_with_restored_timestamp_exhausts_retries() -> Result<(), Box<dyn Error>> {
        let original = vec![b'A'; READ_CHUNK_BYTES + 1];
        let replacement = vec![b'B'; original.len()];
        let path = synthetic_file("same-length", &original)?;
        let modified = fs::metadata(&path)?.modified()?;
        let changes = AtomicU64::new(0);
        let runtime = HookRuntime::new(|checkpoint, _| {
            if matches!(checkpoint, Checkpoint::AfterFirstPass) {
                let count = changes.fetch_add(1, Ordering::AcqRel);
                let bytes = if count.is_multiple_of(2) {
                    &replacement
                } else {
                    &original
                };
                let _write = fs::write(&path, bytes);
                if let Ok(file) = OpenOptions::new().write(true).open(&path) {
                    let _times = file.set_times(FileTimes::new().set_modified(modified));
                }
            }
        });
        let reader = SnapshotReader::new();
        let error = acquire_with_runtime(&reader, &path, &CancellationToken::default(), &runtime)
            .err()
            .ok_or("changing source unexpectedly passed")?;
        assert_eq!(error.code, SnapshotErrorCode::RetryExhausted);
        assert_eq!(
            error.last_transient,
            Some(SnapshotTransientCode::SourceChanged)
        );
        assert_eq!(error.attempts, SNAPSHOT_ATTEMPTS);
        assert_eq!(fs::read(&path)?, replacement);
        Ok(())
    }

    #[test]
    fn append_truncation_and_replacement_are_detected() -> Result<(), Box<dyn Error>> {
        for mode in ["append", "truncate", "replace"] {
            let bytes = vec![b'A'; READ_CHUNK_BYTES + 1];
            let path = synthetic_file(mode, &bytes)?;
            let last_mutated_attempt = AtomicU64::new(0);
            let runtime = HookRuntime::new(|checkpoint, attempt| {
                if matches!(checkpoint, Checkpoint::AfterFirstChunk)
                    && last_mutated_attempt.swap(u64::from(attempt), Ordering::AcqRel)
                        != u64::from(attempt)
                {
                    match mode {
                        "append" => {
                            if let Ok(mut changed) = fs::read(&path) {
                                changed.push(b'B');
                                let _write = fs::write(&path, changed);
                            }
                        }
                        "truncate" => {
                            let reduction = usize::from(attempt) * 1024;
                            let new_length = bytes.len().saturating_sub(reduction).max(1);
                            let _write = fs::write(&path, &bytes[..new_length]);
                        }
                        _ => {
                            let replacement = path.with_extension("replacement");
                            let _write = fs::write(&replacement, vec![b'C'; bytes.len()]);
                            let _remove = fs::remove_file(&path);
                            let _rename = fs::rename(replacement, &path);
                        }
                    }
                }
            });
            let reader = SnapshotReader::new();
            let error =
                acquire_with_runtime(&reader, &path, &CancellationToken::default(), &runtime)
                    .err()
                    .ok_or("mutating source unexpectedly passed")?;
            assert!(
                matches!(
                    error.code,
                    SnapshotErrorCode::RetryExhausted | SnapshotErrorCode::TooLarge
                ),
                "{mode} returned {error:?}"
            );
            let actual = fs::read(&path)?;
            match mode {
                "append" => {
                    let mut expected = bytes.clone();
                    expected.extend(std::iter::repeat_n(b'B', usize::from(SNAPSHOT_ATTEMPTS)));
                    assert_eq!(actual, expected);
                }
                "truncate" => {
                    let reduction = usize::from(SNAPSHOT_ATTEMPTS) * 1024;
                    assert_eq!(actual, bytes[..bytes.len() - reduction]);
                }
                _ => assert_eq!(actual, vec![b'C'; bytes.len()]),
            }
        }
        Ok(())
    }

    #[test]
    fn replacement_between_passes_exhausts_retries() -> Result<(), Box<dyn Error>> {
        let original = vec![b'A'; 4096];
        let expected = vec![b'R'; original.len()];
        let path = synthetic_file("between-passes", &original)?;
        let last_mutated_attempt = AtomicU64::new(0);
        let runtime = HookRuntime::new(|checkpoint, attempt| {
            if matches!(checkpoint, Checkpoint::BeforeSecondOpen)
                && last_mutated_attempt.swap(u64::from(attempt), Ordering::AcqRel)
                    != u64::from(attempt)
            {
                let replacement = path.with_extension(format!("replacement-{attempt}"));
                let _write = fs::write(&replacement, &expected);
                let _remove = fs::remove_file(&path);
                let _rename = fs::rename(replacement, &path);
            }
        });
        let error = acquire_with_runtime(
            &SnapshotReader::new(),
            &path,
            &CancellationToken::default(),
            &runtime,
        )
        .err()
        .ok_or("between-pass replacement unexpectedly passed")?;
        assert_eq!(error.code, SnapshotErrorCode::RetryExhausted);
        assert_eq!(
            error.last_transient,
            Some(SnapshotTransientCode::SourceChanged)
        );
        assert_eq!(fs::read(&path)?, expected);
        Ok(())
    }

    #[test]
    fn cancellation_deadline_and_allocation_failure_are_distinct() -> Result<(), Box<dyn Error>> {
        let path = synthetic_file("stops", b"bounded")?;
        let token = CancellationToken::default();
        let cancel_runtime = HookRuntime::new(|checkpoint, _| {
            if matches!(checkpoint, Checkpoint::AfterFirstPass) {
                token.cancel();
            }
        });
        let reader = SnapshotReader::new();
        let error = acquire_with_runtime(&reader, &path, &token, &cancel_runtime)
            .err()
            .ok_or("cancelled read unexpectedly passed")?;
        assert_eq!(error.code, SnapshotErrorCode::Cancelled);

        let timeout_runtime = HookRuntime::new(|checkpoint, _| {
            if matches!(checkpoint, Checkpoint::AfterFirstChunk) {
                timeout_runtime_placeholder();
            }
        });
        timeout_runtime
            .elapsed_millis
            .store(ELAPSED_BUDGET.as_millis() as u64, Ordering::Release);
        let error = acquire_with_runtime(
            &SnapshotReader::new(),
            &path,
            &CancellationToken::default(),
            &timeout_runtime,
        )
        .err()
        .ok_or("timed out read unexpectedly passed")?;
        assert_eq!(error.code, SnapshotErrorCode::TimedOut);

        let mut allocation_runtime = HookRuntime::new(|_, _| {});
        allocation_runtime.allocation_allowed = false;
        let error = acquire_with_runtime(
            &SnapshotReader::new(),
            &path,
            &CancellationToken::default(),
            &allocation_runtime,
        )
        .err()
        .ok_or("denied allocation unexpectedly passed")?;
        assert_eq!(error.code, SnapshotErrorCode::ResourceLimit);
        Ok(())
    }

    fn timeout_runtime_placeholder() {}

    #[test]
    fn missing_path_is_terminal_and_error_text_is_private() -> Result<(), Box<dyn Error>> {
        let missing = std::env::temp_dir().join("ivalice-d002-private-missing-source.bin");
        let error = acquire(&missing)
            .err()
            .ok_or("missing path unexpectedly passed")?;
        assert_eq!(error.code, SnapshotErrorCode::NotFound);
        assert!(!error.to_string().contains("ivalice-d002-private"));
        assert_eq!(error.attempts, 1);
        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn sharing_violation_retries_three_times_without_changing_source() -> Result<(), Box<dyn Error>>
    {
        let bytes = b"busy synthetic source";
        let path = synthetic_file("busy", bytes)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).share_mode(0);
        let exclusive = options.open(&path)?;
        let error = acquire(&path)
            .err()
            .ok_or("exclusively opened source unexpectedly passed")?;
        drop(exclusive);

        assert_eq!(error.code, SnapshotErrorCode::RetryExhausted);
        assert_eq!(error.attempts, SNAPSHOT_ATTEMPTS);
        assert_eq!(
            error.last_transient,
            Some(SnapshotTransientCode::SourceBusy)
        );
        assert_eq!(fs::read(&path)?, bytes);
        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn read_data_denial_is_terminal_and_acl_is_restored() -> Result<(), Box<dyn Error>> {
        let bytes = b"denied synthetic source";
        let path = synthetic_file("denied", bytes)?;
        let root = path.parent().ok_or("synthetic source has no parent")?;
        let ready = root.join("acl-ready");
        let release = root.join("acl-release");
        let fault = root.join("acl-fault");
        let script = r#"
$ErrorActionPreference = 'Stop'
$path = $env:IVALICE_D002_PATH
$ready = $env:IVALICE_D002_READY
$release = $env:IVALICE_D002_RELEASE
$fault = $env:IVALICE_D002_FAULT
$stage = 'read-acl'
try {
$sections = [Security.AccessControl.AccessControlSections]::Access
$original = (Get-Acl -LiteralPath $path).GetSecurityDescriptorSddlForm($sections)
$acl = [Security.AccessControl.FileSecurity]::new()
$acl.SetSecurityDescriptorSddlForm($original, $sections)
$changed = [Security.AccessControl.FileSecurity]::new()
$changed.SetSecurityDescriptorSddlForm($original, $sections)
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
try {
  $stage = 'deny-read'
  $rule = [Security.AccessControl.FileSystemAccessRule]::new($identity.User, [Security.AccessControl.FileSystemRights]::ReadData, [Security.AccessControl.AccessControlType]::Deny)
  [void]$changed.AddAccessRule($rule)
  Set-Acl -LiteralPath $path -AclObject $changed
  [IO.File]::WriteAllText($ready, 'ready')
  $stage = 'wait-for-release'
  while (-not (Test-Path -LiteralPath $release)) { Start-Sleep -Milliseconds 10 }
} finally {
  try {
    $stage = 'restore-acl'
    Set-Acl -LiteralPath $path -AclObject $acl
    $stage = 'verify-restored-acl'
    if ((Get-Acl -LiteralPath $path).GetSecurityDescriptorSddlForm($sections) -ne $original) {
      throw 'ACL restoration differed'
    }
  } finally { $identity.Dispose() }
}
} catch {
  [IO.File]::WriteAllText($fault, ($stage + ':' + $_.Exception.HResult))
  exit 1
}
"#;
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("IVALICE_D002_PATH", &path)
            .env("IVALICE_D002_READY", &ready)
            .env("IVALICE_D002_RELEASE", &release)
            .env("IVALICE_D002_FAULT", &fault)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        // Allow process startup on a loaded CI host; this is independent of
        // snapshot acquisition's unchanged two-second production deadline.
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready.exists() && Instant::now() < deadline && child.try_wait()?.is_none() {
            thread::sleep(Duration::from_millis(10));
        }

        let (direct_os_code, result) = if ready.exists() {
            (
                File::open(&path)
                    .err()
                    .and_then(|error| error.raw_os_error()),
                acquire(&path),
            )
        } else {
            (
                None,
                Err(SnapshotError::terminal(
                    SnapshotErrorCode::IoFailure,
                    0,
                    None,
                )),
            )
        };
        fs::write(&release, b"release")?;
        let status = child.wait()?;
        let diagnostic = fs::read_to_string(&fault).unwrap_or_else(|_| "no fault status".into());
        assert!(status.success(), "ACL helper failed: {diagnostic}");
        assert_eq!(direct_os_code, Some(5), "ACL did not deny a direct read");
        let error = result
            .err()
            .ok_or("read-denied source unexpectedly passed")?;
        assert_eq!(error.code, SnapshotErrorCode::AccessDenied);
        assert_eq!(error.attempts, 1);
        assert_eq!(fs::read(&path)?, bytes);
        Ok(())
    }

    #[test]
    fn stale_generation_is_discarded_before_publication() -> Result<(), Box<dyn Error>> {
        let path = synthetic_file("late", b"late synthetic result")?;
        let reader = SnapshotReader::new();
        let runtime = HookRuntime::new(|checkpoint, _| {
            if matches!(checkpoint, Checkpoint::AfterSecondPass) {
                reader.cancel_current();
            }
        });
        let error = acquire_with_runtime(&reader, &path, &CancellationToken::default(), &runtime)
            .err()
            .ok_or("stale result unexpectedly published")?;
        assert_eq!(error.code, SnapshotErrorCode::Cancelled);
        Ok(())
    }

    #[test]
    fn memory_snapshot_creates_no_cleanup_targets() -> Result<(), Box<dyn Error>> {
        let path = synthetic_file("cleanup", b"memory only")?;
        let root = path.parent().ok_or("synthetic source has no parent")?;
        let sentinel = root.join("unrelated-sentinel");
        fs::write(&sentinel, b"keep")?;
        let before = directory_names(root)?;
        let _snapshot = acquire(&path)?;
        let after = directory_names(root)?;
        assert_eq!(after, before);
        assert_eq!(fs::read(sentinel)?, b"keep");
        Ok(())
    }

    fn directory_names(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
        let mut names = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<Result<Vec<_>, _>>()?;
        names.sort();
        Ok(names)
    }
}
