use super::*;
#[test]
fn context_specific_beats_global_custom_and_local_custom_overrides() {
    let key = Key::parse("<space>").unwrap().unwrap();
    let mut bindings = defaults();
    bindings.push(Binding {
        context: Context::Global,
        key: key.clone(),
        action: Action::Custom,
        custom: true,
    });
    assert_eq!(
        resolve(&bindings, Context::Files, &key),
        Some(Action::Stage)
    );
    assert_eq!(resolve(&bindings, Context::Subject, &key), None);
    assert_eq!(resolve(&bindings, Context::Menu, &key), None);
    bindings.push(Binding {
        context: Context::Files,
        key: key.clone(),
        action: Action::Custom,
        custom: true,
    });
    assert_eq!(
        resolve(&bindings, Context::Files, &key),
        Some(Action::Custom)
    );
}
#[test]
fn grammar_aliases_disabled_and_policies_are_explicit() {
    assert_eq!(Key::parse("<c-s>").unwrap(), Key::parse("ctrl-s").unwrap());
    assert_eq!(Key::parse("<esc>").unwrap(), Key::parse("escape").unwrap());
    assert_eq!(Key::parse("Q").unwrap(), Key::parse("<s-q>").unwrap());
    assert!(Key::parse("<disabled>").unwrap().is_none());
    assert!(Key::parse("<mystery-key>").is_err());
    for action in [
        "editConfig",
        "suspend",
        "cycleDiffRenderer",
        "update",
        "snake",
        "explode",
    ] {
        assert!(policy_action(action).is_some());
    }
    for (path, value) in [
        ("gui.language", "fr"),
        ("customCommands.output", "logWithPty"),
        ("os.editPreset", "vim"),
    ] {
        assert!(policy_setting(path, value).is_some());
    }
}
