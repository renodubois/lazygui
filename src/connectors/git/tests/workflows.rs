use super::*;
use crate::git_fixture::Fixture;

#[test]
fn workflow_between_commands_survives_shutdown_and_allows_only_cleanup_reads() {
    let f = Fixture::new();
    f.write("new", b"content\n");
    let host = ProcessHost::new();
    // Use the fixture's isolated executor beneath the production retention wrapper.
    let base = Client::with_executor(
        f.root.path().to_owned(),
        host.retain(f.client().executor.clone()),
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let (mut scoped, scope) = base.begin_workflow(cancel.clone()).unwrap();
    let identity = scoped.discover().unwrap();
    scoped = scoped.bind(&identity).unwrap();
    let gates = MutationGates::new();
    let lease = gates
        .try_acquire(&identity, MutationScope::Worktree)
        .unwrap()
        .unwrap();
    scoped.stage_file(Path::new("new")).unwrap();
    // No command is active here. The workflow, not a child, prevents early ack.
    let shutdown = host.shutdown();
    assert!(cancel.load(Ordering::Acquire));
    assert!(!shutdown.is_complete());
    assert!(
        base.begin_workflow(Arc::new(AtomicBool::new(false)))
            .is_err()
    );
    let cleanup = scoped.reconciliation();
    assert_eq!(cleanup.status().unwrap()[0].index, b'A');
    assert_eq!(
        cleanup.stage_file(Path::new("new")).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    drop(scoped);
    drop(cleanup);
    assert!(!shutdown.is_complete());
    drop(lease);
    assert!(
        gates
            .try_acquire(&identity, MutationScope::Worktree)
            .unwrap()
            .is_some()
    );
    assert!(!shutdown.is_complete());
    drop(scope);
    shutdown.wait_blocking();
}

#[test]
fn inherited_index_and_repository_selection_do_not_escape_bound_identity() {
    const MARKER: &str = "LAZYGUI_INDEX_ISOLATION_CHILD";
    if std::env::var_os(MARKER).is_some() {
        let path = PathBuf::from(std::env::var_os("LAZYGUI_INDEX_TEST_ROOT").unwrap());
        let alternate = PathBuf::from(std::env::var_os("GIT_INDEX_FILE").unwrap());
        // Native Git receives actual inherited overrides; no fixture executor hides them.
        let client = Client::new(path.clone());
        let identity = client.discover().unwrap();
        assert_eq!(identity.worktree.as_deref(), Some(path.as_path()));
        let client = client.bind(&identity).unwrap();
        client.stage_file(Path::new("new")).unwrap();
        assert_eq!(client.status().unwrap()[0].index, b'A');
        assert!(identity.git_dir.join("index").exists());
        assert!(!alternate.exists());
        return;
    }
    let f = Fixture::new();
    f.write("new", b"content\n");
    let other = tempfile::tempdir().unwrap();
    let alternate = other.path().join("alternate-index");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "git::workflow_tests::inherited_index_and_repository_selection_do_not_escape_bound_identity", "--nocapture"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", other.path())
        .env("XDG_CONFIG_HOME", other.path())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env(MARKER, "1")
        .env("LAZYGUI_INDEX_TEST_ROOT", f.root.path())
        .env("GIT_INDEX_FILE", &alternate)
        .env("GIT_DIR", other.path().join("nonexistent.git"))
        .env("GIT_WORK_TREE", other.path())
        .env("GIT_COMMON_DIR", other.path())
        .env("GIT_OBJECT_DIRECTORY", other.path().join("nonexistent-objects"))
        .output().unwrap();
    assert!(
        output.status.success(),
        "isolated child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!alternate.exists());
    assert_eq!(f.client().status().unwrap()[0].index, b'A');
}

#[test]
fn strip_only_repository_selection_preserves_hook_filter_signing_configuration() {
    struct Inspect;
    impl Executor for Inspect {
        fn execute(
            &self,
            command: Command,
            _: Vec<u8>,
            _: Arc<AtomicBool>,
        ) -> io::Result<process::Output> {
            let envs: Vec<_> = command.get_envs().collect();
            assert!(envs.contains(&(OsStr::new("GIT_INDEX_FILE"), None)));
            assert!(envs.contains(&(OsStr::new("GIT_DIR"), None)));
            for key in [
                "GIT_CONFIG_COUNT",
                "GIT_CONFIG_GLOBAL",
                "GIT_CONFIG_SYSTEM",
                "GIT_CONFIG_PARAMETERS",
                "GIT_EDITOR",
                "GIT_ASKPASS",
                "GIT_AUTHOR_NAME",
                "GIT_COMMITTER_NAME",
            ] {
                assert!(
                    !envs.iter().any(|(name, _)| *name == key),
                    "configured workflow env overridden: {key}"
                );
            }
            Ok(process::Output {
                code: Some(0),
                stdout: vec![],
                stderr: vec![],
                cancelled: false,
                truncated: false,
            })
        }
    }
    Client::with_executor(PathBuf::from("/fixture"), Arc::new(Inspect))
        .status()
        .unwrap();
}
