use super::*;
fn identity(name: &str) -> Identity {
    Identity {
        worktree: Some(name.into()),
        git_dir: format!("{name}/.git").into(),
        common_dir: format!("{name}/.git").into(),
    }
}
#[test]
fn switches_in_place_rejects_prior_outcomes_and_retains_only_submodule_parent_stack() {
    let mut navigation = Navigation::new(identity("/fixture/one"));
    let generation = navigation.generation();
    navigation.enter_submodule(identity("/fixture/one/sub"));
    assert!(!navigation.accepts(generation));
    assert!(navigation.return_parent());
    assert_eq!(navigation.current(), &identity("/fixture/one"));
    assert!(!navigation.return_parent());
    navigation.enter_submodule(identity("/fixture/one/sub"));
    navigation.switch_in_place(identity("/fixture/two"));
    assert_eq!(navigation.current(), &identity("/fixture/two"));
    assert!(!navigation.return_parent());
    assert!(navigation.accepts(navigation.generation()));
}
