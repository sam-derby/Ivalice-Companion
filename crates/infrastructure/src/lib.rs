//! Windows filesystem infrastructure for explicit save candidates and local settings.

#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(not(windows))]
compile_error!("ivalice-infrastructure currently supports the Windows target only");

mod ability_flags;
mod candidate;
mod job_eligibility;
mod reader_catalogue;
mod save_edit;
mod settings;
mod snapshot;
mod windows_fs;

pub use ability_flags::AbilityFlagsLoader;
pub use candidate::{
    automatic_nominations, discover_candidates, Candidate, CandidateEntryError, CandidateErrorCode,
    CandidateIdentity, CandidateSet, CandidateValidator, Discovery, DiscoveryError,
    NativeCandidateValidator, NominatedPath, NominationKind,
};
pub use job_eligibility::{JobRequirementsLoadError, JobRequirementsLoader};
pub use reader_catalogue::{
    ReaderCatalogueLoadError, ReaderCatalogueLoadErrorCode, ReaderCatalogueLoader,
    ReaderCatalogueResource,
};
pub use save_edit::{
    replace_save_if_unchanged, replace_save_with_backup_if_unchanged,
    restore_save_from_backup_if_unchanged, SaveBackup, SaveEditError,
};
pub use settings::{Settings, SettingsError, SettingsStore};
pub use snapshot::{
    CancellationToken, Snapshot, SnapshotError, SnapshotErrorCode, SnapshotReader,
    SnapshotTransientCode, MAX_SNAPSHOT_BYTES, SNAPSHOT_ATTEMPTS,
};

pub const MAX_NOMINATIONS: usize = 16;
pub const SETTINGS_SCHEMA: u32 = 1;
pub const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
