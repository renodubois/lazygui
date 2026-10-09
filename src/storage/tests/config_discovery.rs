use super::*;
use std::os::unix::fs::symlink;
fn options(root: &Path) -> DiscoveryOptions {
    DiscoveryOptions {
        cwd: root.into(),
        home: Some(root.join("home")),
        xdg_config_home: Some(root.join("xdg")),
        xdg_config_dirs: vec![root.join("system")],
        ..DiscoveryOptions::default()
    }
}
fn fixture(path: &Path, yaml: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, yaml).unwrap();
}
#[test]
fn anchoring_preserves_linux_path_bytes_comma_order_and_invalid_empty_segments() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let startup = temp.path().join("startup,comma");
    let next = temp.path().join("next");
    fs::create_dir_all(&startup).unwrap();
    fs::create_dir_all(&next).unwrap();
    let byte_path = PathBuf::from(OsString::from_vec(b"byte-\xff.yml".to_vec()));
    fixture(&startup.join(&byte_path), "gui: {tabWidth: 2}");
    fixture(&startup.join("second.yml"), "gui: {tabWidth: 6}");
    let mut options = options(&startup);
    options.cli_config_file = Some(OsString::from_vec(
        b"byte-\xff.yml,second.yml,second.yml".to_vec(),
    ));
    options.lg_config_file = Some("missing.yml,,other.yml".into());
    options.config_dir = Some("profile".into());
    options.home = Some("home".into());
    options.xdg_config_home = Some("relative-xdg-ignored".into());
    options.anchor_global_sources();
    let anchored = options.cli_config_file.clone().unwrap();
    assert!(anchored.as_os_str().as_bytes().contains(&0xff));
    assert_eq!(options.config_dir.as_ref(), Some(&startup.join("profile")));
    assert_eq!(options.home.as_ref(), Some(&startup.join("home")));
    assert_eq!(
        options.xdg_config_home.as_ref(),
        Some(&PathBuf::from("relative-xdg-ignored"))
    );
    options.cwd = next;
    options.anchor_global_sources(); // Idempotent even after a repository switch.
    assert_eq!(options.cli_config_file.as_ref(), Some(&anchored));
    let (settings, discovery) = Settings::load_discovered(&options).unwrap();
    assert_eq!(settings.m1().panels.tab_width, 6);
    assert_eq!(discovery.sources[0].path, startup.join(byte_path));
    assert_eq!(discovery.reports[2].status, SourceStatus::Duplicate);
    assert!(
        discovery.reports[0]
            .message
            .contains("migration unavailable in M1")
    );
    assert!(discovery.reports[0].message.contains("not applied"));
    options.cli_config_file = None;
    assert!(
        discover(&options)
            .err()
            .unwrap()
            .message
            .contains("empty custom config path")
    );
}

#[test]
fn missing_default_never_creates_shared_directories_or_files() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path());
    let (settings, discovery) = Settings::load_discovered(&options).unwrap();
    assert_eq!(settings.m1().diff.context_size, 3);
    assert!(discovery.sources.is_empty());
    assert_eq!(discovery.reports[0].status, SourceStatus::MissingOptional);
    assert!(!options.xdg_config_home.unwrap().exists());
}
#[test]
fn config_dir_legacy_and_xdg_lookup_order() {
    let temp = tempfile::tempdir().unwrap();
    let mut options = options(temp.path());
    let modern = temp.path().join("xdg/lazygit/config.yml");
    let legacy = temp.path().join("system/jesseduffield/lazygit/config.yml");
    fixture(&modern, "confirmOnQuit: false");
    fixture(&legacy, "confirmOnQuit: true");
    assert_eq!(
        discover(&options).unwrap().sources[0].path,
        fs::canonicalize(&legacy).unwrap()
    );
    options.config_dir = Some(temp.path().join("override"));
    let override_path = temp.path().join("override/config.yml");
    fixture(&override_path, "{}");
    assert_eq!(discover(&options).unwrap().sources[0].path, override_path);
    fs::remove_file(&override_path).unwrap();
    assert!(discover(&options).unwrap().sources.is_empty()); // no fallback past CONFIG_DIR
    options.config_dir = None;
    fs::remove_file(&legacy).unwrap();
    assert_eq!(discover(&options).unwrap().sources[0].path, modern);
    options.xdg_config_home = None;
    fixture(&temp.path().join("home/.config/lazygit/config.yml"), "{}");
    assert!(
        discover(&options).unwrap().sources[0]
            .path
            .ends_with("home/.config/lazygit/config.yml")
    );
}
#[test]
fn custom_cli_overrides_env_comma_order_and_missing_is_required() {
    let temp = tempfile::tempdir().unwrap();
    let mut options = options(temp.path());
    fixture(&temp.path().join("first.yml"), "confirmOnQuit: false");
    fixture(&temp.path().join("second.yml"), "confirmOnQuit: true");
    options.lg_config_file = Some("missing-env.yml".into());
    options.cli_config_file = Some("first.yml,second.yml".into());
    let (settings, discovery) = Settings::load_discovered(&options).unwrap();
    assert!(settings.m1().confirm_on_quit);
    assert!(discovery.sources.iter().all(|s| s.explicitly_chosen_global));
    assert_eq!(discovery.sources[0].path, temp.path().join("first.yml"));
    options.cli_config_file = None;
    let error = discover(&options).err().unwrap();
    assert_eq!(error.reports[0].status, SourceStatus::MissingRequired);
    assert!(!temp.path().join("missing-env.yml").exists());
    options.lg_config_file = Some("first.yml,,second.yml".into());
    assert!(discover(&options).is_err());
}
#[test]
fn ancestor_plan_is_above_root_only_outer_first_and_git_dir_last() {
    // Pure path planning tests never inspect real / or /tmp config files.
    let plan = ancestor_paths(Path::new("/fixture/outer/repository"));
    assert_eq!(
        plan.iter().map(|p| p.0.as_path()).collect::<Vec<_>>(),
        vec![
            Path::new("/.lazygit.yml"),
            Path::new("/fixture/.lazygit.yml"),
            Path::new("/fixture/outer/.lazygit.yml")
        ]
    );
    assert!(ancestor_paths(Path::new("/")).is_empty());
    // The same production reader/merger operates on fixture-only planned sources.
    let temp = tempfile::tempdir().unwrap();
    let outer = temp.path().join("outer/.lazygit.yml");
    let inner = temp.path().join("outer/inner/.lazygit.yml");
    let gitdir = temp.path().join("resolved-worktree-git-dir/lazygit.yml");
    fixture(&outer, "gui: {tabWidth: 2}");
    fixture(&inner, "gui: {tabWidth: 6}");
    fixture(&gitdir, "gui: {tabWidth: 8}");
    let result = read_sources(vec![
        (outer, SourceKind::Ancestor, false),
        (inner, SourceKind::Ancestor, false),
        (gitdir.clone(), SourceKind::GitDirectory, false),
    ])
    .unwrap();
    let settings = Settings::load_m1(&result.sources).unwrap();
    assert_eq!(settings.m1().panels.tab_width, 8);
    assert_eq!(settings.origins["gui.tabWidth"], gitdir);
    assert!(result.sources.iter().all(|s| !s.explicitly_chosen_global));
    let mut options = options(temp.path());
    options.git_dir = Some(gitdir.parent().unwrap().into());
    assert_eq!(
        discover(&options).unwrap().sources.last().unwrap().path,
        gitdir
    );
}
#[test]
fn canonical_aliases_are_deduplicated_and_do_not_prepend_commands_twice() {
    let temp = tempfile::tempdir().unwrap();
    fixture(
        &temp.path().join("source.yml"),
        "customCommands: [{command: fixture-only}]",
    );
    symlink(
        temp.path().join("source.yml"),
        temp.path().join("alias.yml"),
    )
    .unwrap();
    let mut options = options(temp.path());
    options.cli_config_file = Some("source.yml,alias.yml".into());
    let result = discover(&options).unwrap();
    assert_eq!(result.sources.len(), 1);
    assert_eq!(result.reports[1].status, SourceStatus::Duplicate);
    assert_eq!(result.reports[0].identity, result.reports[1].identity);
    let settings = Settings::load_m1(&result.sources).unwrap();
    assert_eq!(
        settings.get("customCommands").as_sequence().unwrap().len(),
        1
    );
}
#[test]
fn empty_files_are_valid_and_reload_errors_preserve_previous_candidate() {
    let temp = tempfile::tempdir().unwrap();
    let mut options = options(temp.path());
    options.cli_config_file = Some("shared.yml".into());
    let shared = temp.path().join("shared.yml");
    fixture(&shared, "");
    let (mut settings, _) = Settings::load_discovered(&options).unwrap();
    assert_eq!(fs::read_to_string(&shared).unwrap(), "");
    fixture(&shared, "confirmOnQuit: true");
    settings.reload_discovered(&options).unwrap();
    fixture(&shared, "gui: [broken");
    let error = settings.reload_discovered(&options).err().unwrap();
    assert!(error.message.contains("shared.yml"));
    assert!(error.message.contains("line"));
    assert_eq!(error.reports[0].status, SourceStatus::Failed);
    assert!(settings.m1().confirm_on_quit);
    fs::remove_file(&shared).unwrap();
    assert!(settings.reload_discovered(&options).is_err());
    assert!(settings.m1().confirm_on_quit);
}
#[test]
fn source_failures_are_reported_without_reading_unbounded_or_non_yaml_files() {
    let temp = tempfile::tempdir().unwrap();
    let mut options = options(temp.path());
    options.cli_config_file = Some("bad.yml".into());
    fs::create_dir(temp.path().join("bad.yml")).unwrap();
    assert!(
        discover(&options)
            .err()
            .unwrap()
            .message
            .contains("regular file")
    );
    fs::remove_dir(temp.path().join("bad.yml")).unwrap();
    fs::write(temp.path().join("bad.yml"), [0xff]).unwrap();
    assert_eq!(
        discover(&options).err().unwrap().reports[0].status,
        SourceStatus::Failed
    );
    fs::write(temp.path().join("bad.yml"), vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(discover(&options).err().unwrap().message.contains("1 MiB"));
}
