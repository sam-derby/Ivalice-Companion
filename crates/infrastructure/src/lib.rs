//! Platform filesystem infrastructure for explicit save candidates and local settings.

#[cfg(not(any(windows, target_os = "linux")))]
compile_error!("ivalice-infrastructure supports Windows and Linux targets only");

mod ability_flags;
mod achievements;
mod candidate;
mod errands;
mod job_eligibility;
#[cfg(windows)]
#[path = "windows_fs.rs"]
mod platform_fs;
#[cfg(target_os = "linux")]
#[path = "linux_fs.rs"]
mod platform_fs;
mod reader_catalogue;
#[cfg(windows)]
mod save_edit;
#[cfg(target_os = "linux")]
#[path = "save_edit_linux.rs"]
mod save_edit;
mod settings;
mod snapshot;
mod story_progress;
mod story_roster;

pub use ability_flags::AbilityFlagsLoader;
pub use achievements::{AchievementsLoadError, AchievementsLoader};
pub use candidate::{
    automatic_nominations, discover_candidates, Candidate, CandidateEntryError, CandidateErrorCode,
    CandidateIdentity, CandidateSet, CandidateValidator, Discovery, DiscoveryError,
    NativeCandidateValidator, NominatedPath, NominationKind,
};
pub use errands::{ErrandsLoadError, ErrandsLoader};
pub use job_eligibility::{JobRequirementsLoadError, JobRequirementsLoader};
pub use reader_catalogue::{
    ReaderCatalogueLoadError, ReaderCatalogueLoadErrorCode, ReaderCatalogueLoader,
    ReaderCatalogueResource,
};
pub use save_edit::{
    replace_save_if_unchanged, replace_save_with_backup_if_unchanged,
    restore_save_from_backup_if_unchanged, write_new_save_file, SaveBackup, SaveEditError,
};
pub use settings::{Settings, SettingsError, SettingsStore};
pub use snapshot::{
    CancellationToken, Snapshot, SnapshotError, SnapshotErrorCode, SnapshotReader,
    SnapshotTransientCode, MAX_SNAPSHOT_BYTES, SNAPSHOT_ATTEMPTS,
};
pub use story_progress::{StoryProgressLoadError, StoryProgressLoader};
pub use story_roster::{StoryRosterLoadError, StoryRosterLoader};

pub const MAX_NOMINATIONS: usize = 16;
pub const SETTINGS_SCHEMA: u32 = 1;
pub const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
