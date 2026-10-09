use super::*;
const DEFAULTS: &str = "gui:\n  mouseEvents: true\nkeybinding:\n  universal:\n    quit: q\n    quit-alt1: <c-c>\ncustomCommands: []\n";
fn source(yaml: &str) -> Source {
    Source {
        path: "/fixture/ancestor/.lazygit.yml".into(),
        explicitly_chosen_global: false,
        yaml: yaml.into(),
    }
}
#[test]
fn supplied_fields_command_accumulation_arrays_and_legacy_alternates() {
    let sources = [
        source(
            "gui:\n  mouseEvents: false\ncustomCommands:\n  - command: first\nkeybinding:\n  universal:\n    quit: [x, '<disabled>']\n",
        ),
        source(
            "customCommands:\n  - command: second\nkeybinding:\n  universal:\n    quit-alt1: ['<c-c>', x]\n",
        ),
    ];
    let settings = Settings::load(DEFAULTS, &sources).unwrap();
    assert_eq!(settings.get("gui.mouseEvents").as_bool(), Some(false));
    assert_eq!(
        settings.get("customCommands")[0]["command"].as_str(),
        Some("second")
    );
    assert_eq!(
        settings.get("customCommands")[1]["command"].as_str(),
        Some("first")
    );
    assert_eq!(
        settings.universal_binding("quit").unwrap(),
        vec!["x", "<c-c>"]
    );
    assert_eq!(settings.origins["gui.mouseEvents"], sources[0].path);
}
#[test]
fn transactional_reload_and_source_specific_executable_trust() {
    let mut settings = Settings::load(
        DEFAULTS,
        &[source("customCommands: [{command: 'echo fake'}]")],
    )
    .unwrap();
    let path = source("").path;
    let mut trust = Trust::default();
    assert!(!trust.may_execute(&settings, &path));
    trust.approve(&settings, &path);
    assert!(trust.may_execute(&settings, &path));
    assert!(
        settings
            .reload(
                DEFAULTS,
                &[source("keybinding: {universal: {quit: {invalid: yes}}}")]
            )
            .is_err()
    );
    assert!(trust.may_execute(&settings, &path));
    settings
        .reload(
            DEFAULTS,
            &[source(
                "gui: {mouseEvents: false}\ncustomCommands: [{command: 'echo fake'}]",
            )],
        )
        .unwrap();
    assert!(trust.may_execute(&settings, &path));
    settings
        .reload(
            DEFAULTS,
            &[source("customCommands: [{command: 'echo changed'}]")],
        )
        .unwrap();
    assert!(!trust.may_execute(&settings, &path));
    let mut global = source("customCommands: [{command: 'echo chosen'}]");
    global.explicitly_chosen_global = true;
    assert!(trust.may_execute(&Settings::load(DEFAULTS, &[global]).unwrap(), &path));
}
#[test]
fn captured_defaults_parse_without_writing_shared_files() {
    let settings = Settings::load(
        include_str!("../../../llm-docs/research/lazygit-v0.66.0/defaults-linux.yml"),
        &[],
    )
    .unwrap();
    assert!(
        settings
            .universal_binding("confirmInEditor")
            .unwrap()
            .contains(&"<ctrl+enter>".into())
    );
}
