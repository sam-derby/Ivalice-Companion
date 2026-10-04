use ivalice_domain::game_data::reader_catalogue::ValidatedReaderCatalogue;

#[test]
fn bundled_catalogue_is_canonical_and_supports_stat_growth() {
    let bytes = include_bytes!("../../../src-tauri/installer-resources/reader-catalogue-v1.json");
    let catalogue =
        ValidatedReaderCatalogue::from_json(bytes).unwrap_or_else(|error| panic!("{error:?}"));
    let canonical = catalogue
        .canonical_json()
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(bytes.as_slice(), canonical);
    let mechanics = catalogue
        .mechanics()
        .unwrap_or_else(|| panic!("bundled mechanics missing"));
    assert!(mechanics.jobs.iter().all(|job| job.growth.is_some()));
}
