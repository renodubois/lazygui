use super::*;
use crate::{
    git::process::{Executor, Output, ProcessHost},
    git_fixture::Fixture,
};
use std::{
    process::Command,
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

fn owner(f: &Fixture) -> WorkingTree {
    let client = f.client();
    WorkingTree::new(
        client.clone(),
        client.discover().unwrap(),
        Arc::new(git::MutationGates::new()),
        git::DiffOptions::default(),
    )
}
fn deliver(owner: &mut WorkingTree) {
    let rx = owner.updates();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(update) = rx.try_recv() {
            assert!(owner.apply(update));
            return;
        }
        assert!(Instant::now() < deadline, "worker result deadline");
        thread::sleep(Duration::from_millis(1));
    }
}
fn settle(owner: &mut WorkingTree) {
    while owner.loading() || owner.busy() {
        deliver(owner);
    }
}
fn two_hunks(f: &Fixture) {
    let old: String = (0..30).map(|i| format!("line {i}\n")).collect();
    f.write("file", old.as_bytes());
    f.commit();
    f.write(
        "file",
        old.replace("line 2\n", "changed 2\n")
            .replace("line 25\n", "changed 25\n")
            .as_bytes(),
    );
}

#[test]
fn rapid_hunks_are_refreshed_before_next_intent_and_focus_follows_last_change() {
    let f = Fixture::new();
    two_hunks(&f);
    let mut owner = owner(&f);
    settle(&mut owner);
    assert_eq!(
        owner
            .pane(Side::Worktree)
            .patch
            .as_ref()
            .unwrap()
            .hunks()
            .count(),
        2
    );
    owner.act_current(Side::Worktree);
    owner.act_current(Side::Worktree);
    assert!(owner.busy());
    // A view can be discarded/recreated without recreating this owner/consumer.
    let _new_view_data = owner.entries();
    settle(&mut owner);
    assert!(owner.error().is_none(), "{:?}", owner.error());
    assert!(owner.pane(Side::Worktree).is_empty());
    assert_eq!(owner.focused_side(), Side::Index);
    assert_eq!(
        f.run(&["show", ":file"]),
        std::fs::read(f.root.path().join("file")).unwrap()
    );
    owner.act_current(Side::Index);
    owner.act_current(Side::Index);
    settle(&mut owner);
    assert!(owner.error().is_none());
    assert_eq!(owner.focused_side(), Side::Worktree);
    assert!(owner.pane(Side::Index).is_empty());
    assert!(
        f.run(&["show", ":file"])
            .starts_with(b"line 0\nline 1\nline 2\n")
    );
}

#[test]
fn partial_untracked_delete_and_reverse_keep_worktree_bytes() {
    let f = Fixture::new();
    f.write("new", b"one\ntwo\nthree\n");
    let mut owner = owner(&f);
    settle(&mut owner);
    assert!(
        owner
            .pane(Side::Worktree)
            .canonical
            .starts_with(b"diff --git")
    );
    owner.set_selection_mode(Side::Worktree, SelectionMode::Line);
    owner.act_current(Side::Worktree);
    owner.act_current(Side::Worktree);
    settle(&mut owner);
    assert!(owner.error().is_none(), "{:?}", owner.error());
    assert_eq!(f.run(&["show", ":new"]), b"one\ntwo\n");
    owner.act_current(Side::Worktree);
    settle(&mut owner);
    assert_eq!(f.run(&["show", ":new"]), b"one\ntwo\nthree\n");
    owner.act_file(vec!["new".into()], Side::Index);
    settle(&mut owner);
    assert_eq!(owner.entries()[0].index, b'?');
    assert_eq!(
        std::fs::read(f.root.path().join("new")).unwrap(),
        b"one\ntwo\nthree\n"
    );
    f.commit();
    std::fs::remove_file(f.root.path().join("new")).unwrap();
    owner.refresh();
    settle(&mut owner);
    owner.set_selection_mode(Side::Worktree, SelectionMode::Line);
    owner.set_cursor(Side::Worktree, 0);
    owner.act_current(Side::Worktree);
    settle(&mut owner);
    assert_eq!(f.run(&["show", ":new"]), b"two\nthree\n");
    owner.act_current(Side::Worktree);
    owner.act_current(Side::Worktree);
    settle(&mut owner);
    assert!(owner.error().is_none());
    assert_eq!(owner.entries()[0].index, b'D');
    assert!(!f.root.path().join("new").exists());
    owner.act_file(vec!["new".into()], Side::Index);
    settle(&mut owner);
    assert_eq!(f.run(&["show", ":new"]), b"one\ntwo\nthree\n");
    assert!(!f.root.path().join("new").exists());
}

#[test]
fn external_change_rejects_snapshot_and_reconciles_without_replaying_queued_write() {
    let f = Fixture::new();
    two_hunks(&f);
    let mut owner = owner(&f);
    settle(&mut owner);
    f.write("file", b"unrelated replacement\n");
    owner.act_current(Side::Worktree);
    owner.act_current(Side::Worktree);
    settle(&mut owner);
    assert!(owner.error().unwrap().contains("stale target"));
    assert!(f.run(&["show", ":file"]).starts_with(b"line 0\n"));
    assert!(
        owner
            .pane(Side::Worktree)
            .canonical
            .windows(b"unrelated replacement".len())
            .any(|w| w == b"unrelated replacement")
    );
    // Deliberate retry is a new intention, not an automatic replay.
    owner.act_file(vec!["file".into()], Side::Worktree);
    settle(&mut owner);
    assert!(owner.error().is_none());
    assert_eq!(f.run(&["show", ":file"]), b"unrelated replacement\n");
}

#[test]
fn unusual_byte_paths_rename_and_partial_fallbacks_use_entry_identity() {
    use std::os::unix::{
        ffi::OsStringExt,
        fs::{PermissionsExt, symlink},
    };
    let f = Fixture::new();
    let names = [
        b"-leading".to_vec(),
        b"space tab\tnewline\n".to_vec(),
        vec![b'n', 0xff],
    ];
    for name in &names {
        std::fs::write(
            f.root.path().join(OsString::from_vec(name.clone())),
            b"text\n",
        )
        .unwrap();
    }
    f.write("binary", b"a\0b");
    f.write("no-newline", b"text");
    symlink("binary", f.root.path().join("link")).unwrap();
    let mut owner = owner(&f);
    settle(&mut owner);
    for name in ["binary", "no-newline", "link"] {
        owner.select(name.into());
        assert!(!owner.pane(Side::Worktree).partial_enabled());
        owner.act_current(Side::Worktree);
        assert!(owner.error().is_some());
    }
    let paths = owner.entries().iter().map(|e| e.path.clone()).collect();
    owner.act_file(paths, Side::Worktree);
    settle(&mut owner);
    assert!(owner.error().is_none(), "{:?}", owner.error());
    f.run(&["commit", "-qm", "fixture"]);
    f.run(&["mv", "--", "no-newline", "renamed"]);
    owner.refresh();
    settle(&mut owner);
    owner.select("renamed".into());
    assert_eq!(
        owner
            .entries()
            .iter()
            .find(|e| e.path == "renamed")
            .unwrap()
            .original
            .as_deref(),
        Some(OsStr::new("no-newline"))
    );
    assert!(!owner.pane(Side::Index).partial_enabled());
    owner.act_file(vec!["renamed".into()], Side::Index);
    settle(&mut owner);
    assert!(owner.error().is_none(), "{:?}", owner.error());
    assert!(
        f.run(&["ls-files", "-z"])
            .windows(11)
            .any(|w| w == b"no-newline\0")
    );
    owner.act_file(
        owner.entries().iter().map(|e| e.path.clone()).collect(),
        Side::Worktree,
    );
    settle(&mut owner);
    assert!(owner.error().is_none());
    assert!(
        !f.run(&["ls-files", "-z"])
            .windows(11)
            .any(|w| w == b"no-newline\0")
    );
    f.run(&["commit", "-qm", "rename"]);
    let binary = f.root.path().join("binary");
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    owner.refresh();
    settle(&mut owner);
    owner.select("binary".into());
    assert!(!owner.pane(Side::Worktree).partial_enabled());
    owner.act_file(vec!["binary".into()], Side::Worktree);
    settle(&mut owner);
    assert!(owner.error().is_none());
}

#[test]
fn canonical_options_are_separate_from_display_and_range_uses_change_ids() {
    let f = Fixture::new();
    f.write("file", b"a\nb\nc\n");
    f.commit();
    f.write("file", b" A\nb\n C\n");
    let mut owner = owner(&f);
    settle(&mut owner);
    owner.set_options(git::DiffOptions {
        context: 5,
        whitespace: git::Whitespace::IgnoreAllSpace,
    });
    settle(&mut owner);
    assert!(
        owner
            .pane(Side::Worktree)
            .disabled_reason
            .as_deref()
            .unwrap()
            .contains("ignoring whitespace")
    );
    assert!(owner.pane(Side::Worktree).patch.is_some());
    owner.act_selection(Side::Worktree, [0].into_iter().collect());
    assert!(!owner.busy());
    owner.set_options(git::DiffOptions {
        context: 0,
        whitespace: git::Whitespace::Exact,
    });
    settle(&mut owner);
    assert!(
        owner
            .pane(Side::Worktree)
            .disabled_reason
            .as_deref()
            .unwrap()
            .contains("zero-context")
    );
    owner.set_options(git::DiffOptions::default());
    settle(&mut owner);
    owner.set_selection_mode(Side::Worktree, SelectionMode::Range);
    owner.set_selection(Side::Worktree, [0, 1].into_iter().collect());
    owner.act_current(Side::Worktree);
    settle(&mut owner);
    assert_eq!(f.run(&["show", ":file"]), b" A\nb\nc\n");
    assert_eq!(
        std::fs::read(f.root.path().join("file")).unwrap(),
        b" A\nb\n C\n"
    );
    assert!(!owner.pane(Side::Worktree).is_empty());
    assert!(!owner.pane(Side::Index).is_empty());
    owner.set_focus(Side::Index);
    owner.act_selection(Side::Index, [0, 1].into_iter().collect());
    settle(&mut owner);
    assert_eq!(f.run(&["show", ":file"]), b"a\nb\nc\n");
    assert_eq!(owner.focused_side(), Side::Worktree);
    assert_eq!(
        std::fs::read(f.root.path().join("file")).unwrap(),
        b" A\nb\n C\n"
    );
}

struct ReadRequest {
    args: Vec<OsString>,
    input: Vec<u8>,
    cancel: Arc<AtomicBool>,
    reply: mpsc::Sender<io::Result<Output>>,
}
struct Controlled {
    requests: Mutex<mpsc::Sender<ReadRequest>>,
}
impl Executor for Controlled {
    fn execute(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output> {
        let (reply, rx) = mpsc::channel();
        self.requests
            .lock()
            .unwrap()
            .send(ReadRequest {
                args: command.get_args().map(OsStr::to_owned).collect(),
                input,
                cancel,
                reply,
            })
            .unwrap();
        rx.recv().unwrap()
    }
}
fn reply(request: ReadRequest, stdout: Vec<u8>) {
    request
        .reply
        .send(Ok(Output {
            code: Some(0),
            stdout,
            stderr: vec![],
            cancelled: false,
            truncated: false,
        }))
        .unwrap();
}

fn empty(request: ReadRequest) {
    reply(request, vec![]);
}

#[test]
fn out_of_order_read_failure_preserves_data_and_owner_drop_cancels_nonblocking() {
    let f = Fixture::new();
    let identity = f.client().discover().unwrap();
    let (tx, rx) = mpsc::channel();
    let host = ProcessHost::new();
    let client = git::Client::with_executor(
        f.root.path().to_owned(),
        host.retain(Arc::new(Controlled {
            requests: Mutex::new(tx),
        })),
    );
    let mut owner = WorkingTree::new(
        client,
        identity.clone(),
        Arc::new(git::MutationGates::new()),
        git::DiffOptions::default(),
    );
    let first = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    owner.refresh();
    assert!(first.cancel.load(Ordering::Acquire));
    let second = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    empty(second);
    empty(rx.recv_timeout(Duration::from_secs(2)).unwrap());
    deliver(&mut owner);
    assert!(!owner.loading());
    empty(first); // Ignored cancellation cannot overwrite a newer read generation.
    let updates = owner.updates();
    let old = updates.recv_blocking().unwrap();
    assert!(!owner.apply(old));
    owner.refresh();
    let pending = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    pending
        .reply
        .send(Err(io::Error::other("injected read failure")))
        .unwrap();
    deliver(&mut owner);
    assert!(owner.error().unwrap().contains("injected read failure"));
    owner.refresh();
    let pending = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let started = Instant::now();
    drop(owner);
    assert!(started.elapsed() < Duration::from_millis(100));
    assert!(pending.cancel.load(Ordering::Acquire));
    empty(pending);
    host.shutdown().wait_blocking();
    assert!(updates.is_closed());
}

fn respond_initial(rx: &mpsc::Receiver<ReadRequest>, canonical: &[u8]) {
    let status = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(status.args.iter().any(|a| a == "status"));
    reply(status, b" M file\0".to_vec());
    let worktree = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(worktree.args.iter().any(|a| a == "diff"));
    reply(worktree, canonical.to_vec());
    empty(rx.recv_timeout(Duration::from_secs(2)).unwrap());
    reply(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        b" M file\0".to_vec(),
    );
}

#[test]
fn drop_during_write_cancels_scope_without_blocking_and_keeps_gate_until_worker_settles() {
    let f = Fixture::new();
    two_hunks(&f);
    let identity = f.client().discover().unwrap();
    let canonical = f.client().diff(Path::new("file"), Side::Worktree).unwrap();
    let gates = Arc::new(git::MutationGates::new());
    let host = ProcessHost::new();
    let (tx, rx) = mpsc::channel();
    let client = git::Client::with_executor(
        f.root.path().to_owned(),
        host.retain(Arc::new(Controlled {
            requests: Mutex::new(tx),
        })),
    );
    let mut owner = WorkingTree::new(
        client,
        identity.clone(),
        gates.clone(),
        git::DiffOptions::default(),
    );
    respond_initial(&rx, &canonical);
    deliver(&mut owner);
    owner.act_current(Side::Worktree);
    reply(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        b" M file\0".to_vec(),
    );
    reply(rx.recv_timeout(Duration::from_secs(2)).unwrap(), canonical);
    let write = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(write.args.iter().any(|a| a == "apply"));
    assert!(!write.input.is_empty());
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    let updates = owner.updates();
    let started = Instant::now();
    drop(owner);
    assert!(started.elapsed() < Duration::from_millis(100));
    assert!(write.cancel.load(Ordering::Acquire));
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    empty(write); // Even an executor ignoring cancellation cannot spawn a retry.
    let shutdown = host.shutdown();
    assert!(!shutdown.is_complete());
    // Cleanup reads are admitted by the retained workflow despite host shutdown.
    empty(rx.recv_timeout(Duration::from_secs(2)).unwrap());
    empty(rx.recv_timeout(Duration::from_secs(2)).unwrap());
    shutdown.wait_blocking();
    let deadline = Instant::now() + Duration::from_secs(2);
    while gates
        .try_acquire(&identity, git::MutationScope::Worktree)
        .unwrap()
        .is_none()
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert!(rx.try_recv().is_err());
    assert!(updates.is_closed());
}

#[test]
fn refresh_failure_keeps_prior_usable_data_and_cross_owner_update_is_rejected() {
    let f = Fixture::new();
    two_hunks(&f);
    let identity = f.client().discover().unwrap();
    let canonical = f.client().diff(Path::new("file"), Side::Worktree).unwrap();
    let (tx, rx) = mpsc::channel();
    let client = git::Client::with_executor(
        f.root.path().to_owned(),
        Arc::new(Controlled {
            requests: Mutex::new(tx),
        }),
    );
    let mut retained = WorkingTree::new(
        client,
        identity,
        Arc::new(git::MutationGates::new()),
        git::DiffOptions::default(),
    );
    respond_initial(&rx, &canonical);
    deliver(&mut retained);
    assert_eq!(retained.selected(), Some(OsStr::new("file")));
    retained.refresh();
    let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    request
        .reply
        .send(Err(io::Error::other("offline read")))
        .unwrap();
    deliver(&mut retained);
    assert_eq!(retained.pane(Side::Worktree).canonical, canonical);
    assert_eq!(retained.entries().len(), 1);
    assert!(retained.error().unwrap().contains("offline read"));
    let mut replacement = owner(&f);
    let update = replacement.updates().recv_blocking().unwrap();
    assert!(!retained.apply(update));
    replacement.refresh();
    settle(&mut replacement);
    assert_eq!(replacement.selected(), Some(OsStr::new("file")));
}

#[test]
fn uncertain_reconciliation_failure_keeps_gate_until_delivery_and_never_replays() {
    let f = Fixture::new();
    two_hunks(&f);
    let identity = f.client().discover().unwrap();
    let canonical = f.client().diff(Path::new("file"), Side::Worktree).unwrap();
    let (tx, rx) = mpsc::channel();
    let client = git::Client::with_executor(
        f.root.path().to_owned(),
        Arc::new(Controlled {
            requests: Mutex::new(tx),
        }),
    );
    let gates = Arc::new(git::MutationGates::new());
    let mut owner = WorkingTree::new(
        client,
        identity.clone(),
        gates.clone(),
        git::DiffOptions::default(),
    );
    respond_initial(&rx, &canonical);
    deliver(&mut owner);
    owner.act_current(Side::Worktree);
    owner.act_current(Side::Worktree);
    reply(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        b" M file\0".to_vec(),
    );
    reply(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        canonical.clone(),
    );
    let apply = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(apply.args.iter().any(|a| a == "apply"));
    empty(apply);
    let read = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    read.reply
        .send(Err(io::Error::other("reconciliation unavailable")))
        .unwrap();
    let update = owner.updates().recv_blocking().unwrap();
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    owner.apply(update);
    assert!(owner.error().unwrap().contains("uncertain"));
    assert_eq!(owner.pane(Side::Worktree).canonical, canonical);
    assert!(!owner.busy());
    assert!(rx.try_recv().is_err());
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_some()
    );
}

#[test]
fn queued_current_resolves_latest_file_and_options_after_reconciliation() {
    let f = Fixture::new();
    f.write("a", b"old a\ncontext\n");
    f.write("b", b"old b\ncontext\n");
    f.commit();
    f.write("a", b"new a\ncontext\n");
    f.write("b", b"new b\ncontext\n");
    let mut owner = owner(&f);
    settle(&mut owner);
    assert_eq!(owner.selected(), Some(OsStr::new("a")));
    owner.act_current(Side::Worktree);
    owner.select("b".into());
    owner.act_current(Side::Worktree);
    // A context change during a write schedules a read barrier before the next intent.
    owner.set_options(git::DiffOptions {
        context: 5,
        ..git::DiffOptions::default()
    });
    settle(&mut owner);
    assert!(owner.error().is_none(), "{:?}", owner.error());
    assert_eq!(owner.selected(), Some(OsStr::new("b")));
    assert_eq!(owner.options().context, 5);
    assert_eq!(f.run(&["show", ":a"]), b"new a\ncontext\n");
    assert_eq!(f.run(&["show", ":b"]), b"new b\ncontext\n");
    owner.set_focus(Side::Worktree);
    assert_eq!(owner.focused_side(), Side::Index);
    owner.act_file(vec!["a".into(), "b".into()], Side::Index);
    settle(&mut owner);
    assert!(owner.error().is_none());
    // An external reset makes the selected row disappear: pick the adjacent identity.
    f.write("b", b"old b\ncontext\n");
    owner.refresh();
    settle(&mut owner);
    assert_eq!(owner.selected(), Some(OsStr::new("a")));
}

// Use real Git to produce every snapshot and apply each checked worker payload,
// but hold the actual owner mutation request until all rapid input is queued.
fn git_snapshot_reply(f: &Fixture, rx: &mpsc::Receiver<ReadRequest>) {
    let status = f.run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]);
    reply(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        status.clone(),
    );
    for side in [Side::Worktree, Side::Index] {
        reply(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            f.client().diff(Path::new("file"), side).unwrap(),
        );
    }
    reply(rx.recv_timeout(Duration::from_secs(2)).unwrap(), status);
}
fn verified_write(f: &Fixture, rx: &mpsc::Receiver<ReadRequest>, side: Side) -> ReadRequest {
    reply(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        f.run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]),
    );
    reply(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        f.client().diff(Path::new("file"), side).unwrap(),
    );
    let write = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(write.args.iter().any(|arg| arg == "apply"));
    write
}

#[test]
fn held_mutation_remaps_navigated_hunks_explicit_ranges_and_unmoved_repeats_both_sides() {
    for side in [Side::Worktree, Side::Index] {
        // 0: Space Down Space; 1: explicit range; 2: overlapping range;
        // 3: Space Space, the only case allowed to advance a consumed target.
        for gesture in 0..4 {
            let f = Fixture::new();
            let original: String = (0..60).map(|i| format!("line {i}\n")).collect();
            let mut changed = original.clone();
            for i in [2, 17, 32, 47] {
                changed = changed.replace(&format!("line {i}\n"), &format!("changed {i}\n"));
            }
            f.write("file", original.as_bytes());
            f.commit();
            f.write("file", changed.as_bytes());
            if side == Side::Index {
                f.run(&["add", "file"]);
            }
            let host = ProcessHost::new();
            let (tx, rx) = mpsc::channel();
            let client = git::Client::with_executor(
                f.root.path().to_owned(),
                host.retain(Arc::new(Controlled {
                    requests: Mutex::new(tx),
                })),
            );
            let mut owner = WorkingTree::new(
                client,
                f.client().discover().unwrap(),
                Arc::new(git::MutationGates::new()),
                git::DiffOptions::default(),
            );
            git_snapshot_reply(&f, &rx);
            deliver(&mut owner);
            let canonical = owner.pane(side).canonical.clone();
            assert_eq!(owner.pane(side).patch.as_ref().unwrap().hunks().count(), 4);
            owner.act_current(side);
            let first = verified_write(&f, &rx, side); // Deliberately held.
            match gesture {
                0 => {
                    owner.set_cursor(side, 2);
                    owner.act_current(side);
                }
                1 | 2 => {
                    owner.set_selection_mode(side, SelectionMode::Range);
                    let begin = if gesture == 1 { 2 } else { 0 };
                    owner.set_selection(side, (begin..6).collect());
                    owner.act_current(side);
                }
                _ => owner.act_current(side),
            }
            assert_eq!(owner.queue.len(), 1);
            f.client().apply_index(&first.input).unwrap();
            empty(first);
            let residual =
                diff::Patch::parse(&f.client().diff(Path::new("file"), side).unwrap()).unwrap();
            let second_ids = if gesture == 1 || gesture == 2 {
                (0..4).collect()
            } else {
                (0..2).collect()
            };
            let diff::Selection::Patch(expected) =
                residual.select(&second_ids, side == Side::Index).unwrap()
            else {
                panic!("regular partial target");
            };
            git_snapshot_reply(&f, &rx);
            deliver(&mut owner);
            assert!(
                owner.error().is_none(),
                "{side:?} gesture={gesture}: {:?}",
                owner.error()
            );
            assert_eq!(owner.pane(side).remap_anchor(&canonical, 2), Some(0));
            assert_eq!(owner.pane(side).remap_anchor(&canonical, 0), None);
            let second = verified_write(&f, &rx, side);
            assert_eq!(second.input, expected, "{side:?} gesture={gesture}");
            f.client().apply_index(&second.input).unwrap();
            empty(second);
            git_snapshot_reply(&f, &rx);
            deliver(&mut owner);
            assert!(!owner.busy());
            assert!(owner.error().is_none(), "{:?}", owner.error());
            let mut expected_index = if side == Side::Index {
                changed.clone()
            } else {
                original.clone()
            };
            let chosen = if gesture == 1 || gesture == 2 {
                vec![2, 17, 32]
            } else {
                vec![2, 17]
            };
            for i in chosen {
                let (from, to) = if side == Side::Index {
                    (format!("changed {i}\n"), format!("line {i}\n"))
                } else {
                    (format!("line {i}\n"), format!("changed {i}\n"))
                };
                expected_index = expected_index.replace(&from, &to);
            }
            assert_eq!(f.run(&["show", ":file"]), expected_index.as_bytes());
            assert_eq!(
                std::fs::read(f.root.path().join("file")).unwrap(),
                changed.as_bytes()
            );
            drop(owner);
            host.shutdown().wait_blocking();
        }
    }
}

#[test]
fn held_mutation_rejects_external_undo_changed_bytes_and_consumed_explicit_targets() {
    for side in [Side::Worktree, Side::Index] {
        for interference in 0..3 {
            let f = Fixture::new();
            two_hunks(&f);
            if side == Side::Index {
                f.run(&["add", "file"]);
            }
            let worktree = std::fs::read(f.root.path().join("file")).unwrap();
            let host = ProcessHost::new();
            let (tx, rx) = mpsc::channel();
            let client = git::Client::with_executor(
                f.root.path().to_owned(),
                host.retain(Arc::new(Controlled {
                    requests: Mutex::new(tx),
                })),
            );
            let mut owner = WorkingTree::new(
                client,
                f.client().discover().unwrap(),
                Arc::new(git::MutationGates::new()),
                git::DiffOptions::default(),
            );
            git_snapshot_reply(&f, &rx);
            deliver(&mut owner);
            owner.act_current(side);
            let first = verified_write(&f, &rx, side);
            if interference == 2 {
                owner.act_selection(side, [0, 1].into_iter().collect());
            } else {
                owner.act_current(side);
            }
            f.client().apply_index(&first.input).unwrap();
            match interference {
                0 => {
                    // Exact pre-write bytes are also external interference, not a repeat.
                    if side == Side::Worktree {
                        f.run(&["read-tree", "HEAD"]);
                    } else {
                        f.run(&["add", "file"]);
                    }
                }
                1 => {
                    let external = String::from_utf8(worktree.clone())
                        .unwrap()
                        .replace("changed 25", "external 25");
                    f.write("file", external.as_bytes());
                    if side == Side::Index {
                        f.run(&["add", "file"]);
                        f.write("file", &worktree);
                    }
                }
                _ => {}
            }
            let index_before_delivery = f.run(&["show", ":file"]);
            let worktree_before_delivery = std::fs::read(f.root.path().join("file")).unwrap();
            empty(first);
            git_snapshot_reply(&f, &rx);
            deliver(&mut owner);
            assert!(
                owner.error().is_some(),
                "{side:?} interference={interference}"
            );
            assert!(!owner.busy());
            assert!(
                rx.try_recv().is_err(),
                "no second worker/write after rejected mapping"
            );
            assert_eq!(f.run(&["show", ":file"]), index_before_delivery);
            assert_eq!(
                std::fs::read(f.root.path().join("file")).unwrap(),
                worktree_before_delivery
            );
            drop(owner);
            host.shutdown().wait_blocking();
        }
    }
}

#[test]
fn queued_added_deleted_ranges_keep_surviving_line_identities_both_sides() {
    for added in [false, true] {
        for side in [Side::Worktree, Side::Index] {
            let f = Fixture::new();
            f.write("anchor", b"anchor\n");
            let contents = b"one\ntwo\nthree\nfour\nfive\n";
            if !added {
                f.write("file", contents);
            }
            f.commit();
            if added {
                f.write("file", contents);
            } else {
                std::fs::remove_file(f.root.path().join("file")).unwrap();
            }
            if side == Side::Index {
                f.run(&["add", "--all"]);
            }
            let mut owner = owner(&f);
            settle(&mut owner);
            owner.select("file".into());
            owner.set_selection_mode(side, SelectionMode::Line);
            owner.act_current(side);
            // Owner delivery is held even if Git already finished: the displayed
            // snapshot remains old until apply, so this is deterministic queued input.
            let first = owner.updates().recv_blocking().unwrap();
            owner.set_selection_mode(side, SelectionMode::Range);
            owner.set_selection(side, [2, 3].into_iter().collect());
            owner.act_current(side);
            assert!(owner.apply(first));
            settle(&mut owner);
            assert!(
                owner.error().is_none(),
                "added={added} side={side:?}: {:?}",
                owner.error()
            );
            let creates = added != (side == Side::Index);
            let lines = [
                b"one\n".as_slice(),
                b"two\n",
                b"three\n",
                b"four\n",
                b"five\n",
            ];
            let expected: Vec<u8> = lines
                .into_iter()
                .enumerate()
                .filter(|(i, _)| [0, 2, 3].contains(i) == creates)
                .flat_map(|(_, line)| line.iter().copied())
                .collect();
            assert_eq!(f.run(&["show", ":file"]), expected);
            if added {
                assert_eq!(std::fs::read(f.root.path().join("file")).unwrap(), contents);
            } else {
                assert!(!f.root.path().join("file").exists());
            }
        }
    }
}

#[test]
fn shared_lease_spans_failed_write_and_reconciliation_delivery() {
    let f = Fixture::new();
    two_hunks(&f);
    let identity = f.client().discover().unwrap();
    let gates = Arc::new(git::MutationGates::new());
    let mut owner = WorkingTree::new(
        f.client(),
        identity.clone(),
        gates.clone(),
        git::DiffOptions::default(),
    );
    settle(&mut owner);
    // The external index lock rejects dispatch. No automatic retry after its removal.
    std::fs::write(identity.git_dir.join("index.lock"), b"fixture lock").unwrap();
    owner.act_current(Side::Worktree);
    owner.act_current(Side::Worktree);
    let update = owner.updates().recv_blocking().unwrap();
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    owner.apply(update);
    assert!(owner.error().unwrap().contains("no automatic retry"));
    assert!(!owner.busy());
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_some()
    );
    std::fs::remove_file(identity.git_dir.join("index.lock")).unwrap();
    assert!(
        f.run(&["show", ":file"])
            .starts_with(b"line 0\nline 1\nline 2\n")
    );
    owner.act_current(Side::Worktree);
    settle(&mut owner);
    assert!(owner.error().is_none());
}

#[test]
fn held_refresh_keeps_queued_whole_file_confirmation_and_rejects_replacement() {
    for side in [Side::Worktree, Side::Index] {
        let f = Fixture::new();
        two_hunks(&f);
        if side == Side::Index {
            f.run(&["add", "file"]);
        }
        let host = ProcessHost::new();
        let (tx, rx) = mpsc::channel();
        let client = git::Client::with_executor(
            f.root.path().to_owned(),
            host.retain(Arc::new(Controlled {
                requests: Mutex::new(tx),
            })),
        );
        let mut owner = WorkingTree::new(
            client,
            f.client().discover().unwrap(),
            Arc::new(git::MutationGates::new()),
            git::DiffOptions::default(),
        );
        git_snapshot_reply(&f, &rx);
        deliver(&mut owner);
        owner.refresh();
        let held = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        owner.act_file(vec!["file".into()], side);
        f.write("file", b"external unrelated replacement\n");
        if side == Side::Index {
            f.run(&["add", "file"]);
        }
        let index = f.run(&["show", ":file"]);
        reply(
            held,
            f.run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]),
        );
        for pane_side in [Side::Worktree, Side::Index] {
            reply(
                rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                f.client().diff(Path::new("file"), pane_side).unwrap(),
            );
        }
        reply(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            f.run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]),
        );
        deliver(&mut owner);
        assert!(owner.error().unwrap().contains("stale queued file"));
        assert!(!owner.busy());
        assert!(
            rx.try_recv().is_err(),
            "no worker or write after stale confirmation"
        );
        assert_eq!(f.run(&["show", ":file"]), index);
        assert_eq!(
            std::fs::read(f.root.path().join("file")).unwrap(),
            b"external unrelated replacement\n"
        );
        drop(owner);
        host.shutdown().wait_blocking();
    }
}

#[test]
fn whole_file_queued_after_held_partial_rejects_external_changes_and_undo() {
    for side in [Side::Worktree, Side::Index] {
        for undo in [false, true] {
            let f = Fixture::new();
            two_hunks(&f);
            if side == Side::Index {
                f.run(&["add", "file"]);
            }
            let host = ProcessHost::new();
            let (tx, rx) = mpsc::channel();
            let client = git::Client::with_executor(
                f.root.path().to_owned(),
                host.retain(Arc::new(Controlled {
                    requests: Mutex::new(tx),
                })),
            );
            let mut owner = WorkingTree::new(
                client,
                f.client().discover().unwrap(),
                Arc::new(git::MutationGates::new()),
                git::DiffOptions::default(),
            );
            git_snapshot_reply(&f, &rx);
            deliver(&mut owner);
            owner.act_current(side);
            let held = verified_write(&f, &rx, side);
            owner.act_file(vec!["file".into()], side);
            f.client().apply_index(&held.input).unwrap();
            if undo {
                if side == Side::Worktree {
                    f.run(&["read-tree", "HEAD"]);
                } else {
                    f.run(&["add", "file"]);
                }
            } else {
                f.write("file", b"external contents\n");
                if side == Side::Index {
                    f.run(&["add", "file"]);
                }
            }
            let index = f.run(&["show", ":file"]);
            let worktree = std::fs::read(f.root.path().join("file")).unwrap();
            empty(held);
            git_snapshot_reply(&f, &rx);
            deliver(&mut owner);
            assert!(owner.error().is_some(), "{side:?} undo={undo}");
            assert!(!owner.busy());
            assert!(rx.try_recv().is_err());
            assert_eq!(f.run(&["show", ":file"]), index);
            assert_eq!(std::fs::read(f.root.path().join("file")).unwrap(), worktree);
            drop(owner);
            host.shutdown().wait_blocking();
        }
    }
}

#[test]
fn known_partial_then_whole_and_rapid_same_side_whole_are_safely_satisfied() {
    for side in [Side::Worktree, Side::Index] {
        for partial in [false, true] {
            let f = Fixture::new();
            two_hunks(&f);
            if side == Side::Index {
                f.run(&["add", "file"]);
            }
            let changed = std::fs::read(f.root.path().join("file")).unwrap();
            let original = f.run(&["show", "HEAD:file"]);
            let mut owner = owner(&f);
            settle(&mut owner);
            if partial {
                owner.act_current(side);
            } else {
                owner.act_file(vec!["file".into()], side);
            }
            // Hold delivery: every queued confirmation still sees the original file.
            let first = owner.updates().recv_blocking().unwrap();
            owner.act_file(vec!["file".into()], side);
            owner.act_file(vec!["file".into()], side);
            assert!(owner.apply(first));
            settle(&mut owner);
            assert!(
                owner.error().is_none(),
                "{side:?} partial={partial}: {:?}",
                owner.error()
            );
            assert_eq!(
                f.run(&["show", ":file"]),
                if side == Side::Index {
                    original
                } else {
                    changed.clone()
                }
            );
            assert_eq!(std::fs::read(f.root.path().join("file")).unwrap(), changed);
        }
    }
}

#[test]
fn last_partial_consumes_queued_whole_confirmation_when_file_becomes_clean() {
    for side in [Side::Worktree, Side::Index] {
        let f = Fixture::new();
        f.write("file", b"one\noriginal\nthree\n");
        f.commit();
        f.write("file", b"one\nstaged\nthree\n");
        f.run(&["add", "file"]);
        f.write("file", b"one\noriginal\nthree\n");
        let mut owner = owner(&f);
        settle(&mut owner);
        owner.act_current(side);
        assert!(owner.busy(), "{side:?}: {:?}", owner.error());
        let first = owner.updates().recv_blocking().unwrap();
        owner.act_file(vec!["file".into()], side);
        owner.apply(first);
        settle(&mut owner);
        assert!(owner.error().is_none(), "{:?}", owner.error());
        assert!(owner.entries().is_empty());
        assert_eq!(f.run(&["show", ":file"]), b"one\noriginal\nthree\n");
        assert_eq!(
            std::fs::read(f.root.path().join("file")).unwrap(),
            b"one\noriginal\nthree\n"
        );
    }
}

#[test]
fn whole_file_queued_after_held_whole_rejects_external_replacement() {
    for side in [Side::Worktree, Side::Index] {
        let f = Fixture::new();
        two_hunks(&f);
        if side == Side::Index {
            f.run(&["add", "file"]);
        }
        let host = ProcessHost::new();
        let (tx, rx) = mpsc::channel();
        let client = git::Client::with_executor(
            f.root.path().to_owned(),
            host.retain(Arc::new(Controlled {
                requests: Mutex::new(tx),
            })),
        );
        let mut owner = WorkingTree::new(
            client,
            f.client().discover().unwrap(),
            Arc::new(git::MutationGates::new()),
            git::DiffOptions::default(),
        );
        git_snapshot_reply(&f, &rx);
        deliver(&mut owner);
        owner.act_file(vec!["file".into()], side);
        reply(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            f.run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]),
        );
        for pane_side in [Side::Worktree, Side::Index] {
            reply(
                rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                f.client().diff(Path::new("file"), pane_side).unwrap(),
            );
        }
        if side == Side::Index {
            reply(
                rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                f.run(&["symbolic-ref", "--quiet", "HEAD"]),
            );
            reply(
                rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                f.run(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]),
            );
        }
        let held = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            held.args
                .iter()
                .any(|arg| arg == if side == Side::Index { "reset" } else { "add" })
        );
        owner.act_file(vec!["file".into()], side);
        if side == Side::Index {
            f.client().unstage_file(Path::new("file")).unwrap();
        } else {
            f.client().stage_file(Path::new("file")).unwrap();
        }
        f.write("file", b"external replacement after whole write\n");
        if side == Side::Index {
            f.run(&["add", "file"]);
        }
        let index = f.run(&["show", ":file"]);
        empty(held);
        git_snapshot_reply(&f, &rx);
        deliver(&mut owner);
        assert!(owner.error().unwrap().contains("stale queued file"));
        assert!(!owner.busy());
        assert!(rx.try_recv().is_err());
        assert_eq!(f.run(&["show", ":file"]), index);
        assert_eq!(
            std::fs::read(f.root.path().join("file")).unwrap(),
            b"external replacement after whole write\n"
        );
        drop(owner);
        host.shutdown().wait_blocking();
    }
}

#[test]
fn explicit_file_ranges_skip_only_verified_satisfied_sides() {
    let f = Fixture::new();
    for path in ["staged", "unstaged"] {
        f.write(path, b"old\n");
    }
    f.commit();
    f.write("staged", b"staged bytes\n");
    f.run(&["add", "staged"]);
    f.write("unstaged", b"unstaged bytes\n");
    let mut owner = owner(&f);
    settle(&mut owner);
    owner.act_file(vec!["staged".into(), "unstaged".into()], Side::Worktree);
    settle(&mut owner);
    assert!(owner.error().is_none(), "{:?}", owner.error());
    assert_eq!(f.run(&["show", ":staged"]), b"staged bytes\n");
    assert_eq!(f.run(&["show", ":unstaged"]), b"unstaged bytes\n");
    f.run(&["reset", "-q", "HEAD", "--", "unstaged"]);
    owner.refresh();
    settle(&mut owner);
    owner.act_file(vec!["staged".into(), "unstaged".into()], Side::Index);
    settle(&mut owner);
    assert!(owner.error().is_none());
    assert_eq!(f.run(&["show", ":staged"]), b"old\n");
    assert_eq!(f.run(&["show", ":unstaged"]), b"old\n");
    assert_eq!(
        std::fs::read(f.root.path().join("staged")).unwrap(),
        b"staged bytes\n"
    );
    assert_eq!(
        std::fs::read(f.root.path().join("unstaged")).unwrap(),
        b"unstaged bytes\n"
    );
    // Staged-only is not assumed satisfied if its worktree changes after display.
    f.run(&["add", "staged"]);
    owner.refresh();
    settle(&mut owner);
    f.write("staged", b"external replacement\n");
    owner.act_file(vec!["staged".into(), "unstaged".into()], Side::Worktree);
    settle(&mut owner);
    assert!(owner.error().unwrap().contains("stale target"));
    assert_eq!(f.run(&["show", ":staged"]), b"staged bytes\n");
    assert_eq!(f.run(&["show", ":unstaged"]), b"old\n");
}

#[test]
fn rename_destination_and_original_bytes_are_part_of_whole_file_confirmation() {
    use std::os::unix::ffi::OsStringExt;
    for replace_original in [false, true] {
        let f = Fixture::new();
        let original = OsString::from_vec(vec![b'o', 0xff]);
        let destination = OsString::from_vec(vec![b'n', 0xfe]);
        std::fs::write(f.root.path().join(&original), b"rename contents\n").unwrap();
        f.commit();
        std::fs::rename(
            f.root.path().join(&original),
            f.root.path().join(&destination),
        )
        .unwrap();
        f.run(&["add", "--all"]);
        // Keep status letters unchanged across the replacement: canonical bytes,
        // including an already-untracked recreated original, must be checked.
        std::fs::write(
            f.root.path().join(if replace_original {
                &original
            } else {
                &destination
            }),
            b"confirmed rename bytes\n",
        )
        .unwrap();
        let mut owner = owner(&f);
        settle(&mut owner);
        assert_eq!(owner.entries()[0].original.as_ref(), Some(&original));
        let selected = destination.clone();
        // The queued snapshot predates the held refresh and external replacement.
        owner.refresh();
        let held = owner.updates().recv_blocking().unwrap();
        owner.act_file(vec![selected], Side::Index);
        std::fs::write(
            f.root.path().join(if replace_original {
                &original
            } else {
                &destination
            }),
            b"external rename bytes\n",
        )
        .unwrap();
        assert!(owner.apply(held));
        settle(&mut owner);
        assert!(owner.error().is_some(), "original={replace_original}");
        assert_eq!(
            f.client()
                .status()
                .unwrap()
                .iter()
                .find(|e| e.path == destination)
                .unwrap()
                .index,
            b'R'
        );
        // Fresh intentional retry still retains exact non-UTF8 rename identities.
        owner.act_file(vec![destination.clone()], Side::Index);
        settle(&mut owner);
        assert!(owner.error().is_none(), "{:?}", owner.error());
        assert!(!f.client().status().unwrap().iter().any(|e| e.index == b'R'));
    }
}
