use super::*;
use crate::git_fixture::Fixture;

#[test]
fn readiness_head_history_unborn_branch_detached_and_bare() {
    let f = Fixture::new();
    let ready = f.client().readiness().unwrap();
    assert!(ready.version >= GitVersion::MINIMUM);
    assert!(matches!(ready.head, Head::Unborn { .. }));
    assert!(f.client().history(20).unwrap().is_empty());
    f.write("file", b"one\n");
    f.commit();
    let history = f.client().history(20).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].subject, b"fixture");
    assert_eq!(history[0].message, b"fixture\n");
    assert_eq!(history[0].author, b"Fixture");
    assert!(history[0].parents.is_empty());
    let oid = history[0].oid.clone();
    assert!(
        matches!(f.client().head().unwrap(), Head::Branch { oid: actual, .. } if actual == oid)
    );
    f.run(&["checkout", "--detach", "-q"]);
    assert_eq!(f.client().head().unwrap(), Head::Detached { oid });
    f.write("file", b"two\n");
    f.commit();
    let history = f.client().history(2).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].parents, vec![history[1].oid.clone()]);
    assert_eq!(f.client().history(1).unwrap().len(), 1);
    assert!(f.client().history(0).unwrap().is_empty());
    assert!(f.client().history(1001).is_err());
    f.run(&["init", "--bare", "-q", "bare"]);
    let bare = f.client_at(f.root.path().join("bare"));
    assert!(bare.readiness().unwrap().identity.worktree.is_none());
    assert!(bare.history(10).unwrap().is_empty());
}

#[test]
fn version_floor_and_malformed_codecs() {
    assert_eq!(
        decode_version(b"git version 2.56.0\n").unwrap(),
        GitVersion::MINIMUM
    );
    assert_eq!(
        decode_version(b"git version 2.56.1.windows.1\n")
            .unwrap()
            .patch,
        1
    );
    assert!(decode_version(b"not git\n").is_err());
    assert!(decode_version(b"git version 2.56\n").is_err());
    assert!(decode_history(b"invalid\0").is_err());
    assert!(decode_history(b"unterminated").is_err());
    struct Old;
    impl Executor for Old {
        fn execute(
            &self,
            command: Command,
            _: Vec<u8>,
            _: Arc<AtomicBool>,
        ) -> io::Result<process::Output> {
            assert!(command.get_args().any(|arg| arg == "--version"));
            Ok(process::Output {
                code: Some(0),
                stdout: b"git version 2.55.9\n".to_vec(),
                stderr: vec![],
                cancelled: false,
                truncated: false,
            })
        }
    }
    let error = Client::with_executor("/fixture".into(), Arc::new(Old))
        .readiness()
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    assert!(error.to_string().contains("2.56.0"));
}

#[test]
fn display_whitespace_and_context_cannot_change_default_canonical_bytes() {
    let f = Fixture::new();
    f.write("file", b"one\ntwo\nthree\n");
    f.commit();
    f.write("file", b"one \ntwo\nthree\n");
    let client = f.client();
    let path = Path::new("file");
    let canonical = client.diff(path, Side::Worktree).unwrap();
    assert!(!canonical.is_empty());
    assert!(
        client
            .display_diff(
                path,
                Side::Worktree,
                DiffOptions {
                    context: 0,
                    whitespace: Whitespace::IgnoreAtEol
                }
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(client.diff(path, Side::Worktree).unwrap(), canonical);
    let zero = client.canonical_diff(path, Side::Worktree, 0).unwrap();
    assert!(
        zero.windows(b"@@ -1 +1 @@".len())
            .any(|bytes| bytes == b"@@ -1 +1 @@")
    );
}

#[test]
fn untracked_is_verified_before_no_index_read_including_literal_magic_names() {
    let f = Fixture::new();
    f.write(":(glob)*", b"literal\n");
    f.write("other", b"other\n");
    let path = Path::new(":(glob)*");
    assert!(!f.client().untracked_diff(path).unwrap().is_empty());
    f.client().stage_file(path).unwrap();
    assert!(f.client().untracked_diff(path).is_err());
    let entries = f.client().status().unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.path == ":(glob)*" && e.index == b'A')
    );
    assert!(entries.iter().any(|e| e.path == "other" && e.index == b'?'));
    assert!(f.client().untracked_diff(Path::new("missing")).is_err());
    assert!(f.client().untracked_diff(Path::new("../outside")).is_err());
    assert!(f.client().stage_paths(&[]).is_err());
}

#[test]
fn rename_whole_file_operations_keep_both_path_identities_and_worktree_bytes() {
    let f = Fixture::new();
    f.write("old", b"content\n");
    f.commit();
    f.run(&["mv", "old", "new"]);
    let client = f.client();
    let rename = client.status().unwrap().remove(0);
    assert_eq!(rename.original.as_deref(), Some(OsStr::new("old")));
    f.write("new", b"content\nextra\n");
    client.stage_entry(&rename).unwrap();
    assert_eq!(f.run(&["show", ":new"]), b"content\nextra\n");
    client.unstage_entry(&rename).unwrap();
    assert_eq!(f.run(&["show", ":old"]), b"content\n");
    assert_eq!(
        std::fs::read(f.root.path().join("new")).unwrap(),
        b"content\nextra\n"
    );
    assert!(!f.root.path().join("old").exists());
    client.stage_paths(&["old".into(), "new".into()]).unwrap();
    assert_eq!(f.run(&["show", ":new"]), b"content\nextra\n");
}

#[test]
fn unborn_unstage_does_not_fail_on_worktree_edits_and_commit_uses_full_stdin() {
    let f = Fixture::new();
    f.write("file", b"staged\n");
    let client = f.client();
    client.stage_file(Path::new("file")).unwrap();
    f.write("file", b"edited\n");
    client.unstage_file(Path::new("file")).unwrap();
    assert_eq!(
        std::fs::read(f.root.path().join("file")).unwrap(),
        b"edited\n"
    );
    client.stage_file(Path::new("file")).unwrap();
    let message = b"subject with 'quotes' and --options\n\nbody\nsecond line\n";
    client.commit(message).unwrap();
    assert_eq!(
        f.run(&["log", "-1", "--format=%B"]),
        [message.as_slice(), b"\n"].concat()
    );
    assert_eq!(
        client.history(1).unwrap()[0].subject,
        b"subject with 'quotes' and --options"
    );
    assert_eq!(client.history(1).unwrap()[0].message, message);
    assert!(client.commit(b"invalid\0message").is_err());
}

#[test]
fn full_history_message_is_raw_length_framed_not_reencoded_or_nul_delimited() {
    let f = Fixture::new();
    f.write("file", b"one\n");
    f.commit();
    let client = f.client();
    let oid = client.history(1).unwrap()[0].oid.clone();
    let original = client
        .run(
            &[
                OsStr::new("cat-file"),
                OsStr::new("commit"),
                OsStr::new(&oid),
            ],
            &[],
            true,
        )
        .unwrap();
    let separator = original
        .windows(2)
        .position(|bytes| bytes == b"\n\n")
        .unwrap()
        + 2;
    let message = b"raw subject\n\nbody \xff with\0NUL and CR\r\n\ntrailer\n\n";
    let object = [&original[..separator], message].concat();
    let raw_oid = client
        .run(
            &[
                OsStr::new("hash-object"),
                OsStr::new("--literally"),
                OsStr::new("-t"),
                OsStr::new("commit"),
                OsStr::new("-w"),
                OsStr::new("--stdin"),
            ],
            &object,
            false,
        )
        .unwrap();
    let raw_oid = decode_oid(line(&raw_oid).unwrap()).unwrap();
    client
        .run(
            &[
                OsStr::new("update-ref"),
                OsStr::new("HEAD"),
                OsStr::new(&raw_oid),
            ],
            &[],
            false,
        )
        .unwrap();
    f.run(&["config", "i18n.logOutputEncoding", "UTF-8"]);
    f.run(&["config", "log.showSignature", "true"]);
    let records = client.history(10).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].oid, raw_oid);
    assert_eq!(records[0].message, message);
    assert_eq!(records[0].subject, b"raw subject");
}
#[test]
fn malformed_batch_objects_fail_closed_instead_of_shortening_history_messages() {
    let oid = "a".repeat(40);
    let row = format!("{oid}\0\0Author\x001\0Subject\0");
    for bytes in [
        format!("{oid} commit 999\nshort\n"),
        format!("{oid} tree 2\nxx\n"),
        format!("{} commit 2\nxx\n", "b".repeat(40)),
        format!("{oid} commit bad\nxx\n"),
        format!("{oid} commit 2\nxx\n"),
        format!("{oid} commit 2\n\n\n"),
        format!("{oid} commit 2\n\n\n\ntrailing"),
    ] {
        let mut commits = decode_history(row.as_bytes()).unwrap();
        assert!(
            decode_messages(bytes.as_bytes(), &mut commits).is_err(),
            "{bytes:?}"
        );
    }
    let mut commits = decode_history(row.as_bytes()).unwrap();
    let object = b"tree ignored\n\nSubject\n\nbody\0\xff";
    let bytes = [
        format!("{oid} commit {}\n", object.len()).as_bytes(),
        object,
        b"\n",
    ]
    .concat();
    decode_messages(&bytes, &mut commits).unwrap();
    assert_eq!(commits[0].message, b"Subject\n\nbody\0\xff");
}
#[test]
fn cancellation_is_injected_and_failure_keeps_bounded_stderr_without_logging_it() {
    struct Fail;
    impl Executor for Fail {
        fn execute(
            &self,
            command: Command,
            input: Vec<u8>,
            cancel: Arc<AtomicBool>,
        ) -> io::Result<process::Output> {
            assert!(!cancel.load(Ordering::Acquire));
            assert!(command.get_args().any(|arg| arg == "--file=-"));
            assert!(!command.get_args().any(|arg| arg == "private message"));
            assert!(
                !command
                    .get_envs()
                    .any(|(key, _)| key == "GIT_OPTIONAL_LOCKS")
            );
            assert_eq!(input, b"private message");
            Ok(process::Output {
                code: Some(17),
                stdout: vec![],
                stderr: b"hook rejected private payload".to_vec(),
                cancelled: false,
                truncated: false,
            })
        }
    }
    let client = Client::with_executor("/fixture".into(), Arc::new(Fail));
    let error = client.commit(b"private message").unwrap_err();
    let failure = error
        .get_ref()
        .unwrap()
        .downcast_ref::<GitFailure>()
        .unwrap();
    assert_eq!(failure.code, Some(17));
    assert_eq!(failure.stderr, b"hook rejected private payload");
    assert!(!format!("{error:?}").contains("private payload"));
    let cancelled = client.with_cancellation(Arc::new(AtomicBool::new(true)));
    assert_eq!(
        cancelled.status().unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn ordinary_hook_failure_has_real_stderr_and_does_not_claim_a_commit() {
    use std::os::unix::fs::PermissionsExt;
    struct WithHooks {
        home: PathBuf,
        hooks: PathBuf,
    }
    impl Executor for WithHooks {
        fn execute(
            &self,
            mut command: Command,
            input: Vec<u8>,
            cancel: Arc<AtomicBool>,
        ) -> io::Result<process::Output> {
            command
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", &self.home)
                .env("XDG_CONFIG_HOME", &self.home)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_AUTHOR_NAME", "Fixture")
                .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
                .env("GIT_COMMITTER_NAME", "Fixture")
                .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
                .env("GIT_CONFIG_COUNT", "3")
                .env("GIT_CONFIG_KEY_0", "core.hooksPath")
                .env("GIT_CONFIG_VALUE_0", &self.hooks)
                .env("GIT_CONFIG_KEY_1", "commit.gpgSign")
                .env("GIT_CONFIG_VALUE_1", "false")
                .env("GIT_CONFIG_KEY_2", "credential.helper")
                .env("GIT_CONFIG_VALUE_2", "");
            Native.execute(command, input, cancel)
        }
    }
    let f = Fixture::new();
    f.write("file", b"one\n");
    f.client().stage_file(Path::new("file")).unwrap();
    let hooks = f.root.path().join("fixture-hooks");
    std::fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(
        &hook,
        b"#!/bin/sh\nprintf 'fixture hook rejected\\n' >&2\nexit 7\n",
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
    let host = ProcessHost::new();
    let client = Client::with_executor(
        f.root.path().to_owned(),
        host.retain(Arc::new(WithHooks {
            home: f.root.path().join("home"),
            hooks,
        })),
    );
    let error = client.commit(b"subject\n\nbody\n").unwrap_err();
    let failure = error
        .get_ref()
        .unwrap()
        .downcast_ref::<GitFailure>()
        .unwrap();
    assert_eq!(failure.code, Some(1));
    assert_eq!(failure.stderr, b"fixture hook rejected\n");
    assert!(matches!(client.head().unwrap(), Head::Unborn { .. }));
    // Explicitly chosen retry, not automatic replay; changing the fake hook permits it.
    std::fs::write(&hook, b"#!/bin/sh\nexit 0\n").unwrap();
    client.commit(b"subject\n\nbody\n").unwrap();
    assert_eq!(client.history(1).unwrap()[0].subject, b"subject");
    host.shutdown().wait_blocking();
}

#[test]
fn bound_identity_uses_root_paths_and_survives_linked_git_file_retargeting() {
    let f = Fixture::new();
    f.write("file", b"one\n");
    f.commit();
    std::fs::create_dir(f.root.path().join("nested")).unwrap();
    let id = f.client().discover().unwrap();
    let nested = f.client_at(f.root.path().join("nested")).bind(&id).unwrap();
    f.write("file", b"two\n");
    assert!(
        !nested
            .diff(Path::new("file"), Side::Worktree)
            .unwrap()
            .is_empty()
    );
    nested.stage_file(Path::new("file")).unwrap();
    assert_eq!(f.run(&["show", ":file"]), b"two\n");
    f.run(&["worktree", "add", "-qb", "linked", "linked"]);
    let linked_raw = f.client_at(f.root.path().join("linked"));
    let linked_id = linked_raw.discover().unwrap();
    let linked = linked_raw.bind(&linked_id).unwrap();
    std::fs::write(
        f.root.path().join("linked/.git"),
        format!("gitdir: {}\n", id.git_dir.display()),
    )
    .unwrap();
    assert!(matches!(linked.head().unwrap(), Head::Branch { branch, .. } if branch == "linked"));
    assert!(
        !matches!(linked_raw.head().unwrap(), Head::Branch { branch, .. } if branch == "linked")
    );
    f.run(&["init", "--bare", "-q", "bare"]);
    let bare_raw = f.client_at(f.root.path().join("bare"));
    let bare = bare_raw.bind(&bare_raw.discover().unwrap()).unwrap();
    assert_eq!(
        bare.stage_file(Path::new("file")).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
}

#[test]
fn corrupt_existing_head_is_not_misclassified_as_unborn_and_cannot_remove_index() {
    let f = Fixture::new();
    let branch = match f.client().head().unwrap() {
        Head::Unborn { branch } => branch,
        _ => panic!("expected unborn"),
    };
    f.write("file", b"one\n");
    f.client().stage_file(Path::new("file")).unwrap();
    let reference = f.root.path().join(".git/refs/heads").join(branch);
    std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
    std::fs::write(reference, b"1111111111111111111111111111111111111111\n").unwrap();
    assert!(f.client().head().is_err());
    assert!(f.client().unstage_file(Path::new("file")).is_err());
    assert_eq!(f.run(&["ls-files", "-z"]), b"file\0");
}

#[test]
fn canonical_headers_do_not_inherit_presentation_prefixes_or_indicators() {
    let f = Fixture::new();
    f.write("file", b"one\n");
    f.commit();
    f.run(&["config", "diff.noprefix", "true"]);
    f.run(&["config", "diff.outputIndicatorNew", ">"]);
    f.run(&["config", "diff.outputIndicatorOld", "<"]);
    f.write("file", b"two\n");
    let patch = f.client().diff(Path::new("file"), Side::Worktree).unwrap();
    assert!(patch.starts_with(b"diff --git a/file b/file\n"));
    assert!(
        patch
            .windows(b"-one\n+two\n".len())
            .any(|bytes| bytes == b"-one\n+two\n")
    );
}
