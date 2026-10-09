use super::*;
use crate::git::process::{Executor, Output};
use std::{
    ffi::OsStr,
    process::Command,
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Default)]
struct State {
    staged: bool,
    detached: bool,
    oid: Option<String>,
    commits: usize,
    adds: usize,
    fail_once: bool,
    changed_on_failure: bool,
    unchanged_on_success: bool,
    fail_reconcile: bool,
    message: Vec<u8>,
    options: git::CommitOptions,
    panic_commit: bool,
    panic_read: bool,
}
struct Fake {
    identity: git::Identity,
    gates: Arc<git::MutationGates>,
    state: Arc<Mutex<State>>,
}
impl Executor for Fake {
    fn execute(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output> {
        if cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let args: Vec<_> = command.get_args().collect();
        assert!(
            self.gates
                .try_acquire(&self.identity, git::MutationScope::All)?
                .is_none(),
            "All lease must span verification and reconciliation"
        );
        let mut state = self.state.lock().unwrap();
        if state.panic_read {
            state.panic_read = false;
            drop(state);
            panic!("injected read panic");
        }
        let mut output = Output {
            code: Some(0),
            stdout: vec![],
            stderr: vec![],
            cancelled: false,
            truncated: false,
        };
        if args.contains(&OsStr::new("commit")) {
            state.commits += 1;
            state.message = input;
            state.options = git::CommitOptions {
                sign_off: args.contains(&OsStr::new("--signoff")),
                no_verify: args.contains(&OsStr::new("--no-verify")),
            };
            if state.panic_commit {
                state.panic_commit = false;
                state.oid = Some("b".repeat(40));
                drop(state);
                panic!("injected panic after mutation");
            }
            if state.fail_once {
                state.fail_once = false;
                if state.changed_on_failure {
                    state.oid = Some("b".repeat(40));
                }
                output.code = Some(1);
                output.stderr = b"\x1b[31mfake hook refused\x1b[0m https://user:secret@example.invalid/hook\x07".to_vec();
            } else if !state.unchanged_on_success {
                state.oid = Some("c".repeat(40));
                state.staged = false;
            }
        } else if args.contains(&OsStr::new("add")) {
            state.adds += 1;
            state.staged = true;
        } else if args.contains(&OsStr::new("status")) {
            if state.fail_reconcile && state.commits > 0 {
                return Err(io::Error::other("read unavailable"));
            }
            output.stdout = if state.staged {
                b"A  file\0".to_vec()
            } else if state.commits == 0 {
                b"?? file\0".to_vec()
            } else {
                vec![]
            };
        } else if args.contains(&OsStr::new("symbolic-ref")) {
            if state.detached {
                output.code = Some(1);
            } else {
                output.stdout = b"refs/heads/main\n".to_vec();
            }
        } else if args.contains(&OsStr::new("show-ref")) {
            output.code = Some(2);
        } else if args.contains(&OsStr::new("--is-bare-repository")) {
            output.stdout = b"false\n".to_vec();
        } else if args.contains(&OsStr::new("--verify")) {
            match &state.oid {
                Some(oid) => output.stdout = format!("{oid}\n").into_bytes(),
                None => output.code = Some(1),
            }
        } else if args.contains(&OsStr::new("--show-toplevel")) {
            output.stdout =
                format!("{}\n", self.identity.worktree.as_ref().unwrap().display()).into_bytes();
        } else if args.contains(&OsStr::new("--absolute-git-dir")) {
            output.stdout = format!("{}\n", self.identity.git_dir.display()).into_bytes();
        } else if args.contains(&OsStr::new("--git-common-dir")) {
            output.stdout = format!("{}\n", self.identity.common_dir.display()).into_bytes();
        } else {
            panic!("unexpected command: {args:?}");
        }
        Ok(output)
    }
}
fn owner(state: State, skip: bool) -> (tempfile::TempDir, Commit, Arc<Mutex<State>>) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().canonicalize().unwrap();
    let git_dir = path.join("git");
    std::fs::create_dir(&git_dir).unwrap();
    let identity = git::Identity {
        worktree: Some(path.clone()),
        git_dir: git_dir.clone(),
        common_dir: git_dir,
    };
    let gates = Arc::new(git::MutationGates::new());
    let state = Arc::new(Mutex::new(state));
    let client = git::Client::with_executor(
        path,
        Arc::new(Fake {
            identity: identity.clone(),
            gates: gates.clone(),
            state: state.clone(),
        }),
    );
    let settings = Settings {
        message: lazygit_config::M1Settings::default().message,
        skip_no_staged_files_warning: skip,
        skip_hook_prefix: String::new(),
        commit_prefixes: lazygit_config::CommitPrefixes::default(),
    };
    (root, Commit::new(client, identity, gates, settings), state)
}
fn deliver(owner: &mut Commit) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(update) = owner.updates().try_recv() {
            assert!(owner.apply(update));
            return;
        }
        assert!(Instant::now() < deadline, "worker did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn fake_hook_failure_retains_draft_corrected_deliberate_retry_reports_actual_head() {
    let (_root, mut owner, state) = owner(
        State {
            staged: true,
            fail_once: true,
            ..State::default()
        },
        false,
    );
    owner.set_draft("subject", "body");
    assert!(owner.submit());
    assert!(!owner.submit());
    deliver(&mut owner);
    assert_eq!(owner.draft().subject, "subject");
    assert!(matches!(
        owner.outcome(),
        Some(Outcome::NotCommitted {
            head: git::Head::Unborn { .. },
            ..
        })
    ));
    assert!(owner.error().unwrap().contains("fake hook refused"));
    assert!(!owner.error().unwrap().contains("secret"));
    assert!(!owner.error().unwrap().contains('\u{1b}'));
    assert_eq!(state.lock().unwrap().commits, 1);
    owner.set_draft("corrected", "retained body");
    assert!(owner.submit());
    deliver(&mut owner);
    assert!(
        matches!(owner.outcome(), Some(Outcome::Committed { head: git::Head::Branch { oid, .. }, .. }) if oid == &"c".repeat(40))
    );
    assert_eq!(owner.draft(), &Draft::default());
    assert!(owner.error().is_none());
    assert_eq!(state.lock().unwrap().message, b"corrected\n\nretained body");
}
#[test]
fn warning_requires_stage_choice_skip_stages_once_and_cancel_keeps_draft() {
    let (_root, mut owner, state) = owner(State::default(), false);
    owner.set_draft("subject", "body");
    assert!(owner.submit());
    deliver(&mut owner);
    assert_eq!(owner.warning(), Some(Warning::NoStagedFiles));
    assert_eq!(state.lock().unwrap().adds, 0);
    assert!(!owner.submit());
    owner.cancel();
    assert_eq!(owner.draft().body, "body");
    assert!(!owner.confirm_stage_all());
    assert!(owner.submit());
    deliver(&mut owner);
    // Another actor stages something while the prompt is open: confirmation
    // still means stage all, not merely commit whatever happens to be staged.
    state.lock().unwrap().staged = true;
    assert!(owner.confirm_stage_all());
    assert!(!owner.confirm_stage_all());
    deliver(&mut owner);
    assert_eq!(state.lock().unwrap().adds, 1);
    assert_eq!(state.lock().unwrap().commits, 1);
    let (_root, mut skip, state) = self::owner(State::default(), true);
    skip.set_draft("subject", "");
    assert!(skip.submit());
    deliver(&mut skip);
    assert!(skip.warning().is_none());
    assert_eq!(state.lock().unwrap().adds, 1);
    assert_eq!(state.lock().unwrap().commits, 1);
}
#[test]
fn uncertain_changed_head_after_error_unchanged_after_success_or_failed_reconciliation() {
    for state in [
        State {
            fail_once: true,
            changed_on_failure: true,
            ..State::default()
        },
        State {
            unchanged_on_success: true,
            ..State::default()
        },
        State {
            fail_reconcile: true,
            ..State::default()
        },
    ] {
        let (_root, mut owner, state) = owner(
            State {
                staged: true,
                oid: Some("a".repeat(40)),
                ..state
            },
            false,
        );
        owner.set_draft("subject", "body");
        owner.submit();
        deliver(&mut owner);
        assert!(matches!(owner.outcome(), Some(Outcome::Uncertain { .. })));
        assert!(owner.error().unwrap().contains("uncertain"));
        assert_eq!(owner.draft().subject, "subject");
        assert_eq!(state.lock().unwrap().commits, 1);
    }
}
#[test]
fn cancellation_waits_for_settlement_no_double_submit_and_later_edits_survive_success() {
    let (_root, mut owner, state) = owner(
        State {
            staged: true,
            ..State::default()
        },
        false,
    );
    let lease = owner
        .gates
        .try_acquire(&owner.identity, git::MutationScope::All)
        .unwrap()
        .unwrap();
    owner.set_draft("subject", "body");
    assert!(owner.submit());
    owner.cancel();
    assert!(owner.busy());
    assert!(!owner.submit());
    deliver(&mut owner);
    assert_eq!(owner.draft().body, "body");
    assert_eq!(state.lock().unwrap().commits, 0);
    drop(lease);
    assert!(owner.submit());
    owner.set_draft("next commit", "new body");
    deliver(&mut owner);
    assert_eq!(owner.draft().subject, "next commit");
}
#[test]
fn feedback_bounded_and_options_are_explicit_argv() {
    assert!(sanitize(&"x".repeat(10000)).len() < 4200);
    let (_root, mut owner, state) = owner(State::default(), true);
    owner.settings.message.sign_off = true;
    owner.settings.skip_hook_prefix = "WIP".into();
    owner.set_draft("WIP test", "");
    assert!(owner.submit());
    deliver(&mut owner);
    assert!(owner.error().is_none());
    let state = state.lock().unwrap();
    assert_eq!(state.adds, 1);
    assert_eq!(state.commits, 1);
    assert_eq!(
        state.options,
        git::CommitOptions {
            sign_off: true,
            no_verify: true
        }
    );
    assert_eq!(state.message, b"WIP test");
}

#[test]
fn cross_owner_same_generation_updates_are_rejected_without_settling_busy() {
    let (_root_a, mut a, _) = owner(
        State {
            staged: true,
            ..State::default()
        },
        false,
    );
    let (_root_b, mut b, _) = owner(
        State {
            staged: true,
            ..State::default()
        },
        false,
    );
    a.set_draft("owner a", "a body");
    b.set_draft("owner b", "b body");
    assert!(a.submit());
    assert!(b.submit());
    assert_eq!(a.generation, b.generation);
    let update = a.updates().recv_blocking().unwrap();
    assert!(!b.apply(update));
    assert!(b.busy());
    assert_eq!(b.draft().subject, "owner b");
    deliver(&mut b);
    assert!(!b.busy());
    assert_eq!(b.draft(), &Draft::default());
}
#[test]
fn worker_panics_settle_busy_and_write_panics_reconcile_while_gated() {
    for mutation in [false, true] {
        let (_root, mut owner, state) = owner(
            State {
                staged: true,
                panic_commit: mutation,
                panic_read: !mutation,
                ..State::default()
            },
            false,
        );
        owner.set_draft("retained subject", "retained body");
        assert!(owner.submit());
        deliver(&mut owner);
        assert!(!owner.busy());
        assert!(owner.error().unwrap().contains("panicked"));
        assert_eq!(owner.draft().subject, "retained subject");
        if mutation {
            assert!(
                matches!(owner.outcome(), Some(Outcome::Uncertain { head: Some(git::Head::Branch { oid, .. }), status: Some(_) }) if oid == &"b".repeat(40))
            );
            assert_eq!(state.lock().unwrap().commits, 1);
        } else {
            assert_eq!(
                owner.outcome(),
                Some(&Outcome::Uncertain {
                    head: None,
                    status: None
                })
            );
            assert_eq!(state.lock().unwrap().commits, 0);
        }
        // Panic recovery released the lease only after reconciliation settlement.
        assert!(
            owner
                .gates
                .try_acquire(&owner.identity, git::MutationScope::All)
                .unwrap()
                .is_some()
        );
        assert!(owner.submit());
        deliver(&mut owner);
        assert!(matches!(owner.outcome(), Some(Outcome::Committed { .. })));
    }
}
#[test]
fn detached_head_reported_and_cancelled_queued_warning_cannot_reopen_prompt() {
    let (_root, mut owner, _) = owner(
        State {
            staged: true,
            detached: true,
            oid: Some("a".repeat(40)),
            ..State::default()
        },
        false,
    );
    owner.set_draft("detached commit", "body");
    assert!(owner.submit());
    deliver(&mut owner);
    assert!(
        matches!(owner.outcome(), Some(Outcome::Committed { head: git::Head::Detached { oid }, .. }) if oid == &"c".repeat(40))
    );

    let (_root, mut owner, _) = self::owner(State::default(), false);
    owner.set_draft("retained", "body");
    owner.submit();
    // Manufacture no alternate workflow: receive the actual worker's queued result,
    // then dismiss before the shell delivers it.
    let update = owner.updates().recv_blocking().unwrap();
    owner.cancel();
    assert!(owner.apply(update));
    assert!(owner.warning().is_none());
    assert_eq!(owner.draft().subject, "retained");
    assert!(
        !owner.apply(Update {
            owner: owner.token.clone(),
            generation: owner.generation,
            revision: 0,
            result: Completion::Warning,
            _scope: owner
                .client
                .begin_workflow(Arc::new(AtomicBool::new(false)))
                .unwrap()
                .1,
        })
    );
}
