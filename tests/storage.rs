use aipicker::{
    domain::{Preferences, Snapshot},
    storage::Store,
};

#[test]
fn settings_roundtrip_does_not_contain_a_credential() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().to_path_buf()).unwrap();
    let mut p = Preferences {
        selected: Some("a-model".into()),
        ..Default::default()
    };
    p.disabled.insert("hidden-model".into());
    store.save_preferences(&p).unwrap();
    let read = store.load_preferences().unwrap();
    assert_eq!(read.selected.as_deref(), Some("a-model"));
    assert!(read.disabled.contains("hidden-model"));
    assert!(
        !std::fs::read_to_string(dir.path().join("preferences.json"))
            .unwrap()
            .contains("key")
    );
}

#[test]
fn invalid_snapshot_cannot_replace_a_good_cache() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().to_path_buf()).unwrap();
    let mut snapshot = Snapshot::demo();
    snapshot.demo = false;
    store.save_snapshot(&snapshot).unwrap();
    let before = std::fs::read(dir.path().join("benchmarks.json")).unwrap();
    snapshot.models[0].input_price = Some(-2.0);
    assert!(store.save_snapshot(&snapshot).is_err());
    assert_eq!(
        std::fs::read(dir.path().join("benchmarks.json")).unwrap(),
        before
    );
    assert!(store.load_snapshot().unwrap().is_some());
}

#[test]
fn demo_cannot_be_saved_as_live_and_corrupt_cache_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().to_path_buf()).unwrap();
    assert!(store.save_snapshot(&Snapshot::demo()).is_err());
    std::fs::write(dir.path().join("benchmarks.json"), b"broken").unwrap();
    assert!(store.load_snapshot().is_err());
}

#[cfg(windows)]
#[test]
fn credential_roundtrip_is_encrypted_on_disk_and_can_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().to_path_buf()).unwrap();
    store.save_key("synthetic-test-secret").unwrap();
    assert_eq!(
        store.load_key().unwrap().as_deref(),
        Some("synthetic-test-secret")
    );
    let bytes = std::fs::read(dir.path().join("credential.dpapi")).unwrap();
    assert!(
        !bytes
            .windows(21)
            .any(|part| part == b"synthetic-test-secret")
    );
    store.save_key("").unwrap();
    assert!(store.load_key().unwrap().is_none());
}
