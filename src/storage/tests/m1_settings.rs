use super::*;
fn supplied(yaml: &str) -> Source {
    Source {
        path: "/fixture/config.yml".into(),
        explicitly_chosen_global: true,
        yaml: yaml.into(),
    }
}
#[test]
fn m1_defaults_and_supported_warning_message_panel_diff_refresh_overrides() {
    let defaults = Settings::load_m1(&[]).unwrap();
    let m1 = defaults.m1();
    assert!(!m1.warnings.skip_no_staged_files_warning);
    assert!(!m1.message.sign_off);
    assert!(m1.message.auto_wrap_commit_message);
    assert_eq!(m1.message.auto_wrap_width, 72);
    assert_eq!(m1.skip_hook_prefix, "WIP");
    assert_eq!(m1.panels.side_panels.len(), 5);
    assert!(m1.refresh.auto_refresh && m1.refresh.auto_detect_external_changes);
    assert_eq!(m1.refresh.refresh_interval_seconds, 10);
    assert_eq!(m1.refresh.external_change_check_interval_seconds, 2);
    assert!(
        defaults
            .diagnostics
            .iter()
            .any(|d| d.path == "git.autoFetch" && d.reason.contains("M3"))
    );
    let settings = Settings::load_m1(&[supplied("gui:\n  skipNoStagedFilesWarning: true\n  showFileTree: false\n  mainPanelSplitMode: vertical\n  wrapLinesInDiffView: false\n  commitLength: {show: false}\ngit:\n  commit: {signOff: true, autoWrapWidth: 60}\n  skipHookPrefix: ''\n  diffContextSize: 0\n  renameSimilarityThreshold: 80\n  autoRefresh: false\nrefresher: {refreshInterval: 0}\nconfirmOnQuit: true")]).unwrap();
    let m1 = settings.m1();
    assert!(m1.warnings.skip_no_staged_files_warning);
    assert!(m1.message.sign_off);
    assert_eq!(m1.message.auto_wrap_width, 60);
    assert!(m1.skip_hook_prefix.is_empty());
    assert!(!m1.show_commit_length);
    assert!(!m1.panels.show_file_tree);
    assert_eq!(m1.panels.main_panel_split_mode, SplitMode::Vertical);
    assert_eq!(m1.diff.context_size, 0);
    assert_eq!(m1.diff.rename_similarity_threshold, 80);
    assert!(!m1.diff.wrap_lines);
    assert!(!m1.refresh.auto_refresh);
    assert_eq!(m1.refresh.refresh_interval_seconds, 0);
    assert!(m1.confirm_on_quit);
    assert!(
        settings
            .diagnostics
            .iter()
            .any(|d| d.path == "git.commit.autoWrapWidth" && d.reason == "Supported M1 setting.")
    );
}
#[test]
fn typed_bindings_union_legacy_alternates_and_normalize_disabled_null_and_lists() {
    let settings = Settings::load_m1(&[supplied("keybinding:\n  universal:\n    quit: [x, '<disabled>', '', '<ctrl+c>']\n    quit-alt1: ['<c-c>', x]\n    refresh: null\n  files: {commitChanges: '<disabled>'}\n  commitMessage: {commitMenu: '<ctrl+k>'}")]).unwrap();
    assert_eq!(
        settings.m1().binding("universal", "quit"),
        &[
            Key::parse("x").unwrap().unwrap(),
            Key::parse("<c-c>").unwrap().unwrap()
        ]
    );
    assert!(settings.m1().binding("universal", "refresh").is_empty());
    assert!(settings.m1().binding("files", "commitChanges").is_empty());
    assert_eq!(
        settings.m1().binding("commitMessage", "commitMenu")[0],
        Key::parse("<ctrl+k>").unwrap().unwrap()
    );
}
#[test]
fn invalid_supported_fields_keys_and_layout_are_transactional_and_source_aware() {
    let mut settings = Settings::load_m1(&[]).unwrap();
    for yaml in [
        "gui: {skipNoStagedFilesWarning: nope}",
        "gui: {sidePanelWidth: 1.5}",
        "gui: {mainPanelSplitMode: diagonal}",
        "gui: {sidePanels: [[files], [files]]}",
        "gui: {tabWidth: 0}",
        "git: {commit: {autoWrapWidth: 0}}",
        "git: {diffContextSize: -1}",
        "git: {renameSimilarityThreshold: 101}",
        "keybinding: nope",
        "keybinding: {universal: {quit: ['<bogus>']}}",
        "refresher: {refreshInterval: -1}",
    ] {
        let error = settings.reload(DEFAULTS, &[supplied(yaml)]).err().unwrap();
        assert!(
            error.to_string().contains("/fixture/config.yml"),
            "{yaml}: {error}"
        );
        assert_eq!(settings.m1().message.auto_wrap_width, 72);
        assert_eq!(settings.m1().panels.tab_width, 4);
    }
}
#[test]
fn custom_commands_templates_and_supported_message_prefixes_are_explicit() {
    let settings = Settings::load_m1(&[supplied("customCommands: [{command: '{{runCommand .Form.command}}'}]\ngit: {commitPrefix: [{pattern: '^feature/', replace: '$1'}]}\nos: {edit: fixture-editor}\nunknownFutureSetting: true")]).unwrap();
    assert!(
        settings
            .diagnostics
            .iter()
            .any(|d| d.path == "customCommands.command" && d.reason == CUSTOM_COMMANDS_UNAVAILABLE)
    );
    assert!(
        settings
            .diagnostics
            .iter()
            .any(|d| d.path == "git.commitPrefix" && d.reason == "Supported M1 setting.")
    );
    assert!(
        settings
            .diagnostics
            .iter()
            .any(|d| d.path == "os.edit" && d.reason.contains("not applied"))
    );
    assert!(
        settings
            .diagnostics
            .iter()
            .any(|d| d.path == "unknownFutureSetting")
    );
}
