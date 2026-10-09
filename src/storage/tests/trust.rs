use super::*;
#[test]
fn approvals_persist_separately_without_commands_and_expire_on_executable_changes() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join(".lazygit.yml");
    let yaml = "customCommands: [{command: 'echo disposable'}]";
    fs::write(&shared, yaml).unwrap();
    let source = Source {
        path: shared.clone(),
        explicitly_chosen_global: false,
        yaml: yaml.into(),
    };
    let settings = Settings::load("{}", std::slice::from_ref(&source)).unwrap();
    let path = temp.path().join("gui-only/trust.json");
    let mut trust = Trust::load(&path).unwrap();
    assert!(!trust.may_execute(&settings, &shared));
    assert!(!trust.may_execute(&settings, &temp.path().join("unencountered.yml")));
    trust.approve(&settings, &shared);
    trust.save(&path).unwrap();
    let trust = Trust::load(&path).unwrap();
    assert!(trust.may_execute(&settings, &shared));
    assert_eq!(fs::read_to_string(shared.clone()).unwrap(), yaml);
    assert!(
        !fs::read_to_string(path.clone())
            .unwrap()
            .contains("disposable")
    );
    let changed = Settings::load(
        "{}",
        &[Source {
            yaml: "customCommands: [{command: 'echo changed'}]".into(),
            ..source
        }],
    )
    .unwrap();
    assert!(!trust.may_execute(&changed, &shared));
    fs::write(&path, "invalid").unwrap();
    assert!(Trust::load(&path).is_err());
}
