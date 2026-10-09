use super::*;
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
