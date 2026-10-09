use super::*;

#[test]
fn applied_fields_and_parsed_unavailable_fields_have_distinct_source_feedback() {
    let active = Source {
        path: "/fixture/active.yml".into(),
        explicitly_chosen_global: true,
        yaml: "gui:\n  sidePanelWidth: 0.4\n  mainPanelSplitMode: vertical\n  showFileTree: false\n  showRootItemInFileTree: false\n  fileTreeSortCaseSensitive: true\n  filterMode: fuzzy\n  tabWidth: 2\n  wrapLinesInDiffView: false\n  useHunkModeInDiffView: false\n  skipNoStagedFilesWarning: true\n  commitLength: {show: false}\ngit:\n  diffContextSize: 8\n  ignoreWhitespaceInDiffView: true\n  commit: {signOff: true, autoWrapCommitMessage: false, autoWrapWidth: 50}\n  skipHookPrefix: TEST\n  autoRefresh: false\n  autoDetectExternalChanges: false\nrefresher: {refreshInterval: 5, externalChangeCheckInterval: 3}\nconfirmOnQuit: true\nquitOnTopLevelReturn: true".into(),
    };
    let parsed = Source {
        path: "/fixture/parsed.yml".into(),
        explicitly_chosen_global: false,
        yaml: "gui:\n  sidePanels: [[files], [commits]]\n  screenMode: full\n  splitDiff: always\n  expandFocusedSidePanel: true\n  expandedSidePanelWeight: 3\n  shrinkSidePanelsToContent: true\n  fileTreeSortOrder: filesFirst\n  switchTabsWithPanelJumpKeys: false\n  showPanelJumps: false\n  scrollHeight: 4\n  scrollPastBottom: false\n  mouseEvents: false\n  skipAmendWarning: true\n  skipDiscardChangeWarning: true\n  skipStashWarning: true\ngit: {renameSimilarityThreshold: 80, commitPrefixes: {}}".into(),
    };
    let settings = Settings::load_m1(&[active.clone(), parsed.clone()]).unwrap();
    let applied: Vec<_> = settings
        .diagnostics
        .iter()
        .filter(|d| d.source == active.path)
        .collect();
    assert_eq!(applied.len(), 23);
    assert!(applied.iter().all(|d| d.reason == "Supported M1 setting."));
    let unavailable: Vec<_> = settings
        .diagnostics
        .iter()
        .filter(|d| d.source == parsed.path && d.path != "git.commitPrefixes")
        .collect();
    assert_eq!(unavailable.len(), 16);
    assert!(
        unavailable
            .iter()
            .all(|d| d.reason.contains("unavailable in M1") && d.reason.contains("not applied"))
    );
    assert!(settings.diagnostics.iter().any(|d| d.source == parsed.path
        && d.path == "git.commitPrefixes"
        && d.reason == "Supported M1 setting."));
    // Parsed values remain inspectable, but this is not a claim that they take effect.
    assert_eq!(settings.m1().diff.rename_similarity_threshold, 80);
    assert_eq!(settings.m1().panels.screen_mode, ScreenMode::Full);
}

#[test]
fn binding_diagnostics_report_exact_active_actions_not_entire_contexts() {
    let source = Source {
        path: "/fixture/bindings.yml".into(),
        explicitly_chosen_global: true,
        yaml: "keybinding:\n  universal: {quit-alt1: x, confirmInEditor: '<ctrl+enter>', suspendApp: z, editConfig: e, cycleDiffRenderers: r}\n  files: {commitChanges: c, nextItem-alt: j, stashAllChanges: s, filteringMenu: f}\n  main: {toggleSelectHunk: v, togglePanel: '<tab>'}\n  commitMessage: {commitMenu: '<ctrl+k>', futureAction: q}\n  branches: {checkoutBranch: '<enter>'}\n  futureContext: {quit: q}".into(),
    };
    let settings = Settings::load_m1(std::slice::from_ref(&source)).unwrap();
    let active = [
        "keybinding.universal.quit-alt1",
        "keybinding.universal.confirmInEditor",
        "keybinding.files.commitChanges",
        "keybinding.files.nextItem-alt",
        "keybinding.main.toggleSelectHunk",
        "keybinding.commitMessage.commitMenu",
    ];
    let diagnostics: Vec<_> = settings
        .diagnostics
        .iter()
        .filter(|d| d.source == source.path)
        .collect();
    assert_eq!(diagnostics.len(), 15);
    for diagnostic in diagnostics {
        if active.contains(&diagnostic.path.as_str()) {
            assert!(
                diagnostic.reason.starts_with("Supported M1 binding"),
                "{}",
                diagnostic.path
            );
        } else {
            assert!(
                diagnostic.reason.contains("unavailable in M1")
                    && diagnostic.reason.contains("not applied"),
                "{}",
                diagnostic.path
            );
        }
    }
}
#[test]
fn every_supplied_setting_gets_source_aware_feedback_without_silent_application() {
    let source = Source {path:"/fixture/config.yml".into(),explicitly_chosen_global:true,
        yaml:"gui: {language: fr}\ncustomCommands: [{output: logWithPty, command: 'echo fake'}]\nunknownFutureSetting: true".into()};
    let settings = Settings::load("{}", std::slice::from_ref(&source)).unwrap();
    assert_eq!(settings.diagnostics.len(), 4);
    assert!(settings.diagnostics.iter().all(|d| d.source == source.path));
    assert!(
        settings
            .diagnostics
            .iter()
            .any(|d| d.path == "gui.language" && d.reason.contains("English"))
    );
    assert!(
        settings
            .diagnostics
            .iter()
            .any(|d| d.path == "customCommands.output" && d.reason.contains("PTY"))
    );
    assert!(
        settings
            .diagnostics
            .iter()
            .any(|d| d.path == "unknownFutureSetting" && d.reason.contains("not applied"))
    );
}
