use super::*;
use crate::storage::Persistence;

#[test]
fn missing_config_defaults_but_corruption_is_not_silently_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    assert!(load(&path).unwrap().query.is_empty());
    std::fs::write(&path, "not JSON").unwrap();
    assert_eq!(
        load(&path).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
}
#[test]
fn ordered_worker_persists_only_public_preferences_in_an_explicit_profile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profile/preferences.json");
    let worker = Persistence::new(path.clone());
    let first = worker.save(Config {
        query: "first".into(),
    });
    let last = worker.save(Config {
        query: "last".into(),
    });
    assert_eq!(first.recv_blocking().unwrap(), Ok(()));
    assert_eq!(last.recv_blocking().unwrap(), Ok(()));
    assert_eq!(load(&path).unwrap().query, "last");
    assert!(!path.with_extension("tmp").exists());
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 1);
}
#[test]
fn persistence_reports_file_failures() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-a-directory");
    std::fs::write(&file, "").unwrap();
    let worker = Persistence::new(file.join("preferences.json"));
    assert!(
        worker
            .save(Config::default())
            .recv_blocking()
            .unwrap()
            .is_err()
    );
}
