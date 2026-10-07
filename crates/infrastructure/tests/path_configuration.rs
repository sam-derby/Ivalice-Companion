use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ivalice_infrastructure::{
    automatic_nominations, discover_candidates, CandidateErrorCode, CandidateIdentity,
    CandidateSet, CandidateValidator, DiscoveryError, NativeCandidateValidator, NominatedPath,
    Settings, SettingsError, SettingsStore, MAX_NOMINATIONS, MAX_SETTINGS_BYTES,
};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn synthetic_root(label: &str) -> Result<PathBuf, Box<dyn Error>> {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ivalice-d001-{}-{nonce}-{sequence}-{label}",
        std::process::id()
    ));
    fs::create_dir(&root)?;
    Ok(root)
}

#[test]
fn explicit_files_and_direct_children_are_ordered_and_deduplicated() -> Result<(), Box<dyn Error>> {
    let root = synthetic_root("candidates")?;
    let account_z = root.join("Z Account");
    let account_unicode = root.join("Á Account");
    fs::create_dir_all(&account_z)?;
    fs::create_dir_all(&account_unicode)?;
    let z_save = account_z.join("enhanced.png");
    let unicode_save = account_unicode.join("enhanced.png");
    fs::write(&z_save, b"candidate-z")?;
    fs::write(&unicode_save, b"candidate-unicode")?;
    let alias = root.join("renamed save.bin");
    fs::hard_link(&unicode_save, &alias)?;

    let nominations = vec![
        NominatedPath::containing_folder(&account_z),
        NominatedPath::file(&alias),
        NominatedPath::containing_folder(&account_unicode),
        NominatedPath::file(root.join("missing.png")),
    ];
    let validator = NativeCandidateValidator::default();
    let discovery = discover_candidates(&nominations, None, &validator)
        .map_err(|_| "candidate count unexpectedly exceeded the limit")?;

    let CandidateSet::AmbiguousCandidates(candidates) = discovery.candidates else {
        return Err("expected distinct account candidates".into());
    };
    assert_eq!(candidates.len(), 2);
    #[cfg(windows)]
    let expected = [alias, z_save];
    #[cfg(target_os = "linux")]
    let expected = [z_save, alias];
    assert_eq!(candidates[0].path, expected[0]);
    assert_eq!(candidates[1].path, expected[1]);
    assert_eq!(discovery.entry_errors.len(), 1);
    assert_eq!(discovery.entry_errors[0].code, CandidateErrorCode::NotFound);
    Ok(())
}

#[test]
fn empty_rules_and_removed_selection_are_explicit() -> Result<(), Box<dyn Error>> {
    assert!(automatic_nominations().is_empty());
    let root = synthetic_root("selection")?;
    let existing = root.join("existing.bin");
    fs::write(&existing, b"candidate")?;
    let missing = root.join("removed.bin");
    let validator = NativeCandidateValidator::default();
    let discovery =
        discover_candidates(&[NominatedPath::file(existing)], Some(&missing), &validator)
            .map_err(|_| "candidate count unexpectedly exceeded the limit")?;
    assert!(matches!(
        discovery.candidates,
        CandidateSet::SelectionUnavailable(ref candidates) if candidates.len() == 1
    ));

    let too_many = vec![NominatedPath::file(&missing); MAX_NOMINATIONS + 1];
    assert_eq!(
        discover_candidates(&too_many, None, &validator),
        Err(DiscoveryError::TooManyNominations)
    );
    Ok(())
}

#[test]
fn validator_failures_remain_distinct_per_entry() -> Result<(), Box<dyn Error>> {
    struct FailingValidator;
    impl CandidateValidator for FailingValidator {
        fn validate(&self, path: &Path) -> Result<CandidateIdentity, CandidateErrorCode> {
            match path.file_name().and_then(|name| name.to_str()) {
                Some("denied") => Err(CandidateErrorCode::AccessDenied),
                Some("identity") => Err(CandidateErrorCode::IdentityUnavailable),
                Some("directory") => Err(CandidateErrorCode::NotRegularFile),
                _ => Err(CandidateErrorCode::NotFound),
            }
        }
    }

    let root = synthetic_root("errors")?;
    let nominations = vec![
        NominatedPath::file(root.join("denied")),
        NominatedPath::file(root.join("identity")),
        NominatedPath::file(root.join("directory")),
        NominatedPath::file(root.join("missing")),
    ];
    let discovery = discover_candidates(&nominations, None, &FailingValidator)
        .map_err(|_| "candidate count unexpectedly exceeded the limit")?;
    let codes: Vec<_> = discovery
        .entry_errors
        .iter()
        .map(|error| error.code)
        .collect();
    assert_eq!(
        codes,
        vec![
            CandidateErrorCode::AccessDenied,
            CandidateErrorCode::IdentityUnavailable,
            CandidateErrorCode::NotRegularFile,
            CandidateErrorCode::NotFound,
        ]
    );
    Ok(())
}

#[test]
fn native_validation_rejects_invalid_and_non_file_inputs() -> Result<(), Box<dyn Error>> {
    let root = synthetic_root("invalid")?;
    let validator = NativeCandidateValidator::default();
    assert_eq!(
        validator.validate(Path::new("relative.png")),
        Err(CandidateErrorCode::InvalidPath)
    );
    #[cfg(windows)]
    assert_eq!(
        validator.validate(Path::new(r"\\server\share\enhanced.png")),
        Err(CandidateErrorCode::UnsupportedLocation)
    );
    assert_eq!(
        validator.validate(&root),
        Err(CandidateErrorCode::NotRegularFile)
    );
    Ok(())
}

#[test]
fn redirected_settings_root_round_trips_without_touching_sources() -> Result<(), Box<dyn Error>> {
    let root = synthetic_root("redirected")?;
    let redirected_local_app_data = root.join("Redirected Local App Data");
    fs::create_dir_all(&redirected_local_app_data)?;
    let source = root.join("account save.bin");
    fs::write(&source, b"source sentinel")?;
    let store = SettingsStore::for_local_app_data(&redirected_local_app_data)?;
    let settings = Settings {
        nominations: vec![NominatedPath::file(&source)],
        selected_file: Some(source.clone()),
    };

    store.save(&settings)?;
    assert_eq!(store.load()?, Some(settings.clone()));
    store.save(&settings)?;
    assert_eq!(fs::read(&source)?, b"source sentinel");
    assert_eq!(
        store.root(),
        redirected_local_app_data.join("local.ivalice.companion")
    );
    let entries: Vec<_> = fs::read_dir(store.root())?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file_name(), "settings.json");
    Ok(())
}

#[test]
fn malformed_unknown_and_oversized_settings_are_not_overwritten() -> Result<(), Box<dyn Error>> {
    let root = synthetic_root("malformed")?;
    let local_app_data = root.join("Local");
    fs::create_dir_all(&local_app_data)?;
    let store = SettingsStore::for_local_app_data(local_app_data)?;
    store.save(&Settings::default())?;
    let settings_path = store.root().join("settings.json");

    let malformed = b"{not-json";
    fs::write(&settings_path, malformed)?;
    assert_eq!(store.load(), Err(SettingsError::Malformed));
    assert_eq!(
        store.save(&Settings::default()),
        Err(SettingsError::Malformed)
    );
    assert_eq!(fs::read(&settings_path)?, malformed);

    let unknown_schema = br#"{"schema":2,"nominations":[],"selected_file":null}"#;
    fs::write(&settings_path, unknown_schema)?;
    assert_eq!(store.load(), Err(SettingsError::UnsupportedSchema));

    let oversized_len = usize::try_from(MAX_SETTINGS_BYTES)? + 1;
    fs::write(&settings_path, vec![b' '; oversized_len])?;
    assert_eq!(store.load(), Err(SettingsError::TooLarge));
    Ok(())
}

#[test]
fn hard_linked_settings_and_destination_overlap_are_rejected() -> Result<(), Box<dyn Error>> {
    let root = synthetic_root("isolation")?;
    let local_app_data = root.join("Local");
    fs::create_dir_all(&local_app_data)?;
    let store = SettingsStore::for_local_app_data(local_app_data)?;
    let inside_settings = store.root().join("source.bin");
    let overlapping = Settings {
        nominations: vec![NominatedPath::file(inside_settings)],
        selected_file: None,
    };
    assert_eq!(
        store.save(&overlapping),
        Err(SettingsError::DestinationOverlap)
    );

    store.save(&Settings::default())?;
    let settings_path = store.root().join("settings.json");
    fs::hard_link(&settings_path, root.join("settings-alias.json"))?;
    assert_eq!(store.load(), Err(SettingsError::UnsafeDestination));
    assert_eq!(
        store.save(&Settings::default()),
        Err(SettingsError::UnsafeDestination)
    );
    Ok(())
}

#[test]
#[cfg(windows)]
fn reparse_settings_root_is_rejected() -> Result<(), Box<dyn Error>> {
    let root = synthetic_root("junction")?;
    let local_app_data = root.join("Local");
    let junction_target = root.join("Elsewhere");
    fs::create_dir_all(&local_app_data)?;
    fs::create_dir_all(&junction_target)?;
    let store = SettingsStore::for_local_app_data(local_app_data)?;
    let script = "New-Item -ItemType Junction -Path $env:IVALICE_D001_LINK -Target $env:IVALICE_D001_TARGET | Out-Null";
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", script])
        .env_remove("PSModulePath")
        .env("IVALICE_D001_LINK", store.root())
        .env("IVALICE_D001_TARGET", &junction_target)
        .status()?;
    assert!(status.success());
    assert_eq!(
        store.save(&Settings::default()),
        Err(SettingsError::UnsupportedLocation)
    );
    Ok(())
}
