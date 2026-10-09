use super::gui::{GuiPreferences, OrderedStorage, StorageReply};
use super::*;
#[test]
fn gui_profile_is_separate_and_startup_read_is_noncreating() {
    let temp = tempfile::tempdir().unwrap();
    let profile =
        gui::profile_directory(Some(temp.path()), Some(&temp.path().join("xdg"))).unwrap();
    assert_eq!(
        profile,
        temp.path().join("xdg").join(env!("CARGO_PKG_NAME"))
    );
    let (writer, preferences, _) = OrderedStorage::open(profile.clone()).unwrap();
    assert_eq!(preferences, GuiPreferences::default());
    assert!(!profile.exists());
    assert!(matches!(
        writer.flush().recv_blocking().unwrap().unwrap(),
        StorageReply::Flushed
    ));
    assert!(!profile.exists());
}
#[test]
fn one_shared_writer_orders_preferences_trust_and_revocation_without_shared_writes() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared.yml");
    let yaml = "customCommands: [{command: 'echo fixture-only'}]";
    fs::write(&shared, yaml).unwrap();
    let source = Source {
        path: shared.clone(),
        explicitly_chosen_global: false,
        yaml: yaml.into(),
    };
    let settings = Settings::load_m1(&[source]).unwrap();
    let profile = temp.path().join("gui-only");
    let (writer, _, trust) = OrderedStorage::open(profile.clone()).unwrap();
    assert!(!trust.may_execute(&settings, &shared));
    let other_window = writer.clone();
    let first = GuiPreferences {
        window_size: Some([800, 600]),
        ..GuiPreferences::default()
    };
    // Canceling a waiter is not canceling an accepted write.
    drop(writer.save_preferences(first));
    let approval = writer.approve(&settings, &shared).unwrap();
    let second = GuiPreferences {
        window_size: Some([1200, 900]),
        recent_repositories: vec![b"/fixture/\xff".to_vec()],
        ..GuiPreferences::default()
    };
    let saved = other_window.save_preferences(second.clone());
    let StorageReply::Trust(trust) = approval.recv_blocking().unwrap().unwrap() else {
        panic!("expected trust snapshot")
    };
    assert!(trust.may_execute(&settings, &shared));
    assert!(matches!(
        saved.recv_blocking().unwrap().unwrap(),
        StorageReply::Saved
    ));
    writer.flush().recv_blocking().unwrap().unwrap();
    assert_eq!(gui::GuiPreferences::default().version, 1);
    let (loaded, restarted) = gui::load_profile(&profile).unwrap();
    assert_eq!(loaded, second);
    assert!(restarted.may_execute(&settings, &shared));
    let trust_bytes = fs::read_to_string(profile.join("trust.json")).unwrap();
    assert!(!trust_bytes.contains("fixture-only"));
    assert_eq!(fs::read_to_string(&shared).unwrap(), yaml);
    let StorageReply::Trust(trust) = writer
        .revoke(shared.clone())
        .recv_blocking()
        .unwrap()
        .unwrap()
    else {
        panic!("expected trust snapshot")
    };
    assert!(!trust.may_execute(&settings, &shared));
}
#[test]
fn invalid_preferences_and_file_errors_are_explicit_and_do_not_replace_good_data() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let (writer, _, _) = OrderedStorage::open(profile.clone()).unwrap();
    writer
        .save_preferences(GuiPreferences::default())
        .recv_blocking()
        .unwrap()
        .unwrap();
    let previous = fs::read(profile.join("preferences.json")).unwrap();
    let invalid = GuiPreferences {
        version: 99,
        ..GuiPreferences::default()
    };
    assert!(
        writer
            .save_preferences(invalid)
            .recv_blocking()
            .unwrap()
            .is_err()
    );
    assert_eq!(
        fs::read(profile.join("preferences.json")).unwrap(),
        previous
    );
    fs::write(profile.join("trust.json"), "broken").unwrap();
    assert!(OrderedStorage::open(profile).is_err());
    let blocked = temp.path().join("not-a-directory");
    fs::write(&blocked, "fixture").unwrap();
    assert!(OrderedStorage::open(blocked).is_err());
}
