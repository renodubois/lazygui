use super::*;
use crate::git::process::Output;
use std::{io, process::Command};

struct Reads {
    version: &'static str,
}
impl Executor for Reads {
    fn execute(&self, command: Command, _: Vec<u8>, _: Arc<AtomicBool>) -> io::Result<Output> {
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let root = command.get_current_dir().unwrap();
        let stdout = if args.iter().any(|a| a == "--version") {
            self.version.as_bytes().to_vec()
        } else if args.iter().any(|a| a == "--is-bare-repository") {
            b"false\n".to_vec()
        } else if args.iter().any(|a| a == "--show-toplevel") {
            format!("{}\n", root.display()).into_bytes()
        } else if args
            .iter()
            .any(|a| matches!(a.as_str(), "--absolute-git-dir" | "--git-common-dir"))
        {
            format!("{}/.git\n", root.display()).into_bytes()
        } else if args.iter().any(|a| a == "symbolic-ref") {
            b"refs/heads/main\n".to_vec()
        } else if args.iter().any(|a| a == "HEAD^{commit}") {
            format!("{}\n", "a".repeat(40)).into_bytes()
        } else {
            return Err(io::Error::other("not a repository"));
        };
        Ok(Output {
            code: Some(0),
            stdout,
            stderr: vec![],
            cancelled: false,
            truncated: false,
        })
    }
}
fn options(root: &std::path::Path, config: &std::path::Path) -> DiscoveryOptions {
    DiscoveryOptions {
        cwd: root.into(),
        cli_config_file: Some(config.as_os_str().into()),
        home: Some(root.into()),
        xdg_config_dirs: vec![],
        ..Default::default()
    }
}
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let one = temp.path().join("one");
    let two = temp.path().join("two");
    std::fs::create_dir_all(one.join(".git")).unwrap();
    std::fs::create_dir_all(two.join(".git")).unwrap();
    let config = temp.path().join("config.yml");
    std::fs::write(
        &config,
        "git:\n  diffContextSize: 5\nkeybinding:\n  universal:\n    quit: x\n",
    )
    .unwrap();
    (temp, one, two, config)
}
#[test]
fn readiness_binds_identity_and_switch_rejects_already_queued_old_completion() {
    let (_temp, one, two, config) = fixture();
    let host = ProcessHost::new();
    let mut owner = Repository::with_executor(
        host.clone(),
        options(&one, &config),
        Arc::new(Reads {
            version: "git version 2.56.0\n",
        }),
    );
    let old = owner.updates().recv_blocking().unwrap();
    owner.switch_in_place(two.clone());
    assert!(owner.session().is_none());
    assert!(!owner.apply(old));
    let update = owner.updates().recv_blocking().unwrap();
    assert!(owner.apply(update));
    assert_eq!(
        owner
            .session()
            .unwrap()
            .readiness
            .identity
            .worktree
            .as_ref(),
        Some(&two)
    );
    assert_eq!(owner.session().unwrap().settings.m1().diff.context_size, 5);
    host.shutdown().wait_blocking();
}
#[test]
fn relative_global_sources_stay_at_startup_across_switch_and_owner_reload() {
    use crate::lazygit_config::{SourceKind, SourceStatus};
    for selector in ["cli", "env", "dir"] {
        let (_temp, one, two, _) = fixture();
        let relative = if selector == "dir" {
            "settings/config.yml"
        } else {
            "shared.yml"
        };
        let startup_config = one.join(relative);
        std::fs::create_dir_all(startup_config.parent().unwrap()).unwrap();
        std::fs::write(
            &startup_config,
            "git: {diffContextSize: 7}\nkeybinding: {universal: {quit: x}}",
        )
        .unwrap();
        let trap = two.join(relative);
        std::fs::create_dir_all(trap.parent().unwrap()).unwrap();
        std::fs::write(
            &trap,
            "git: {diffContextSize: 99}\nkeybinding: {universal: {quit: z}}",
        )
        .unwrap();
        std::fs::write(one.join(".git/lazygit.yml"), "gui: {tabWidth: 2}").unwrap();
        std::fs::write(two.join(".git/lazygit.yml"), "gui: {tabWidth: 8}").unwrap();
        let mut options = options(&one, std::path::Path::new(relative));
        if selector != "cli" {
            options.cli_config_file = None;
        }
        if selector == "env" {
            options.lg_config_file = Some(relative.into());
        } else if selector == "dir" {
            options.config_dir = Some("settings".into());
        }
        let host = ProcessHost::new();
        let mut owner = Repository::with_executor(
            host.clone(),
            options,
            Arc::new(Reads {
                version: "git version 2.56.0\n",
            }),
        );
        let update = owner.updates().recv_blocking().unwrap();
        assert!(owner.apply(update), "{selector}: {:?}", owner.error());
        assert_eq!(owner.session().unwrap().settings.m1().panels.tab_width, 2);
        owner.switch_in_place(two.clone());
        let update = owner.updates().recv_blocking().unwrap();
        assert!(owner.apply(update), "{selector}: {:?}", owner.error());
        let session = owner.session().unwrap();
        assert_eq!(session.settings.m1().diff.context_size, 7);
        assert_eq!(session.settings.m1().panels.tab_width, 8);
        let prior_keys = session.settings.m1().binding("universal", "quit").to_vec();
        assert_eq!(
            prior_keys[0],
            crate::input::Key::parse("x").unwrap().unwrap()
        );
        let global = session
            .source_reports
            .iter()
            .find(|r| r.kind == SourceKind::Global)
            .unwrap();
        assert_eq!(global.requested_path, startup_config);
        assert_eq!(global.identity.as_ref(), Some(&startup_config));
        assert_eq!(global.status, SourceStatus::Loaded);
        assert!(
            session
                .source_reports
                .iter()
                .any(|r| r.kind == SourceKind::GitDirectory
                    && r.requested_path == two.join(".git/lazygit.yml")
                    && r.status == SourceStatus::Loaded)
        );

        std::fs::write(
            &startup_config,
            "keybinding: {universal: {quit: '<bogus>'}}",
        )
        .unwrap();
        owner.reload_config();
        let update = owner.updates().recv_blocking().unwrap();
        assert!(!owner.apply(update));
        assert_eq!(
            owner
                .session()
                .unwrap()
                .settings
                .m1()
                .binding("universal", "quit"),
            prior_keys
        );
        assert!(
            owner
                .error()
                .unwrap()
                .contains(&startup_config.display().to_string())
        );
        std::fs::write(
            &startup_config,
            "git: {diffContextSize: 11}\nkeybinding: {universal: {quit: y}}",
        )
        .unwrap();
        std::fs::remove_file(two.join(".git/lazygit.yml")).unwrap();
        owner.reload_config();
        let update = owner.updates().recv_blocking().unwrap();
        assert!(owner.apply(update));
        let session = owner.session().unwrap();
        assert_eq!(session.settings.m1().diff.context_size, 11);
        assert_eq!(session.settings.m1().panels.tab_width, 4);
        let mut next_keys = prior_keys;
        next_keys[0] = crate::input::Key::parse("y").unwrap().unwrap();
        assert_eq!(
            session.settings.m1().binding("universal", "quit"),
            next_keys
        );
        assert!(session.source_reports.iter().any(
            |r| r.kind == SourceKind::GitDirectory && r.status == SourceStatus::MissingOptional
        ));
        host.shutdown().wait_blocking();
    }
}

#[test]
fn reload_is_transactional_and_unsupported_version_has_no_session() {
    let (_temp, one, _, config) = fixture();
    let host = ProcessHost::new();
    let mut owner = Repository::with_executor(
        host.clone(),
        options(&one, &config),
        Arc::new(Reads {
            version: "git version 2.56.0\n",
        }),
    );
    let update = owner.updates().recv_blocking().unwrap();
    owner.apply(update);
    let prior_reports: Vec<_> = owner
        .session()
        .unwrap()
        .source_reports
        .iter()
        .map(|r| (r.requested_path.clone(), r.status))
        .collect();
    let prior_keys = owner
        .session()
        .unwrap()
        .settings
        .m1()
        .binding("universal", "quit")
        .to_vec();
    std::fs::write(&config, "keybinding: {universal: {quit: '<bogus>'}}").unwrap();
    owner.reload_config();
    let update = owner.updates().recv_blocking().unwrap();
    assert!(!owner.apply(update));
    assert_eq!(owner.session().unwrap().settings.m1().diff.context_size, 5);
    assert_eq!(
        owner
            .session()
            .unwrap()
            .settings
            .m1()
            .binding("universal", "quit"),
        prior_keys
    );
    assert_eq!(
        owner
            .session()
            .unwrap()
            .source_reports
            .iter()
            .map(|r| (r.requested_path.clone(), r.status))
            .collect::<Vec<_>>(),
        prior_reports
    );
    assert!(
        owner
            .error()
            .unwrap()
            .contains(&config.display().to_string())
    );
    assert!(owner.error().unwrap().contains("Failed"));
    std::fs::write(&config, "git:\n  diffContextSize: 9\n").unwrap();
    owner.reload_config();
    let update = owner.updates().recv_blocking().unwrap();
    assert!(owner.apply(update));
    assert_eq!(owner.session().unwrap().settings.m1().diff.context_size, 9);
    let mut old_git = Repository::with_executor(
        host.clone(),
        options(&one, &config),
        Arc::new(Reads {
            version: "git version 2.40.0\n",
        }),
    );
    let update = old_git.updates().recv_blocking().unwrap();
    assert!(!old_git.apply(update));
    assert!(old_git.session().is_none());
    assert!(old_git.error().unwrap().contains("unsupported"));
    host.shutdown().wait_blocking();
}
