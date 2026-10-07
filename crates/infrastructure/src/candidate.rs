use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::platform_fs::{self, PathFailure};
use crate::MAX_NOMINATIONS;

const DISCOVERED_FILENAME: &str = "enhanced.png";

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NominationKind {
    File,
    ContainingFolder,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NominatedPath {
    pub kind: NominationKind,
    pub path: PathBuf,
}

impl NominatedPath {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: NominationKind::File,
            path: path.into(),
        }
    }

    pub fn containing_folder(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: NominationKind::ContainingFolder,
            path: path.into(),
        }
    }

    fn candidate_path(&self) -> PathBuf {
        match self.kind {
            NominationKind::File => self.path.clone(),
            NominationKind::ContainingFolder => self.path.join(DISCOVERED_FILENAME),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CandidateIdentity {
    pub volume: u64,
    pub file_index: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    pub path: PathBuf,
    pub identity: CandidateIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateErrorCode {
    InvalidPath,
    NotFound,
    AccessDenied,
    NotRegularFile,
    UnsupportedLocation,
    IdentityUnavailable,
    IoFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateEntryError {
    pub entry_index: usize,
    pub code: CandidateErrorCode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateSet {
    NoCandidates,
    SingleCandidate(Candidate),
    AmbiguousCandidates(Vec<Candidate>),
    SelectionUnavailable(Vec<Candidate>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Discovery {
    pub candidates: CandidateSet,
    pub entry_errors: Vec<CandidateEntryError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    TooManyNominations,
}

pub trait CandidateValidator {
    fn validate(&self, path: &Path) -> Result<CandidateIdentity, CandidateErrorCode>;
}

#[derive(Clone, Debug, Default)]
pub struct NativeCandidateValidator {
    settings_root: Option<PathBuf>,
}

impl NativeCandidateValidator {
    pub fn new(settings_root: Option<PathBuf>) -> Self {
        Self { settings_root }
    }
}

impl CandidateValidator for NativeCandidateValidator {
    fn validate(&self, path: &Path) -> Result<CandidateIdentity, CandidateErrorCode> {
        let lexical_overlap = self
            .settings_root
            .as_deref()
            .is_some_and(|root| platform_fs::paths_overlap(path, root));
        let resolved_overlap = self.settings_root.as_deref().is_some_and(|root| {
            fs::canonicalize(path)
                .ok()
                .zip(fs::canonicalize(root).ok())
                .is_some_and(|(path, root)| platform_fs::paths_overlap(&path, &root))
        });
        if lexical_overlap || resolved_overlap {
            return Err(CandidateErrorCode::UnsupportedLocation);
        }
        platform_fs::validate_regular_file(path)
            .map(|identity| CandidateIdentity {
                volume: identity.volume,
                file_index: identity.index,
            })
            .map_err(map_path_failure)
    }
}

pub fn automatic_nominations() -> Vec<NominatedPath> {
    Vec::new()
}

pub fn discover_candidates(
    nominations: &[NominatedPath],
    selected_file: Option<&Path>,
    validator: &impl CandidateValidator,
) -> Result<Discovery, DiscoveryError> {
    if nominations.len() > MAX_NOMINATIONS {
        return Err(DiscoveryError::TooManyNominations);
    }

    let mut valid = Vec::new();
    let mut errors = Vec::new();
    for (entry_index, nomination) in nominations.iter().enumerate() {
        let path = nomination.candidate_path();
        match validator.validate(&path) {
            Ok(identity) => valid.push(Candidate { path, identity }),
            Err(code) => errors.push(CandidateEntryError { entry_index, code }),
        }
    }

    valid.sort_by_key(|candidate| ordinal_path_key(&candidate.path));
    let mut identities = HashSet::new();
    valid.retain(|candidate| identities.insert(candidate.identity));

    let candidates = if let Some(selected_path) = selected_file {
        match validator.validate(selected_path) {
            Ok(selected_identity)
                if valid.iter().any(|item| item.identity == selected_identity) =>
            {
                classify(valid)
            }
            _ => CandidateSet::SelectionUnavailable(valid),
        }
    } else {
        classify(valid)
    };

    Ok(Discovery {
        candidates,
        entry_errors: errors,
    })
}

fn classify(mut candidates: Vec<Candidate>) -> CandidateSet {
    match candidates.len() {
        0 => CandidateSet::NoCandidates,
        1 => CandidateSet::SingleCandidate(candidates.remove(0)),
        _ => CandidateSet::AmbiguousCandidates(candidates),
    }
}

#[cfg(windows)]
fn ordinal_path_key(path: &Path) -> (Vec<u16>, Vec<u16>) {
    let exact: Vec<u16> = path.as_os_str().encode_wide().collect();
    let folded: Vec<u16> = path
        .to_string_lossy()
        .to_uppercase()
        .encode_utf16()
        .collect();
    (folded, exact)
}

fn map_path_failure(failure: PathFailure) -> CandidateErrorCode {
    match failure {
        PathFailure::Invalid => CandidateErrorCode::InvalidPath,
        PathFailure::NotFound => CandidateErrorCode::NotFound,
        PathFailure::AccessDenied => CandidateErrorCode::AccessDenied,
        PathFailure::NotRegularFile => CandidateErrorCode::NotRegularFile,
        PathFailure::UnsupportedLocation => CandidateErrorCode::UnsupportedLocation,
        PathFailure::IdentityUnavailable => CandidateErrorCode::IdentityUnavailable,
        PathFailure::Busy => CandidateErrorCode::IoFailure,
        PathFailure::Io => CandidateErrorCode::IoFailure,
    }
}

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(target_os = "linux")]
fn ordinal_path_key(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}
