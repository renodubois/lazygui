use super::*;
use crate::{
    git::process::{Executor, Output, ProcessHost},
    git_fixture::Fixture,
};
use std::{
    process::Command,
    sync::{Mutex, mpsc},
    time::Duration,
};

struct Request {
    args: Vec<OsString>,
    cancel: Arc<AtomicBool>,
    reply: mpsc::Sender<io::Result<Output>>,
}
struct Controlled(Mutex<mpsc::Sender<Request>>);
impl Executor for Controlled {
    fn execute(&self, command: Command, _: Vec<u8>, cancel: Arc<AtomicBool>) -> io::Result<Output> {
        let (reply, receive) = mpsc::channel();
        self.0
            .lock()
            .unwrap()
            .send(Request {
                args: command.get_args().map(OsStr::to_owned).collect(),
                cancel,
                reply,
            })
            .unwrap();
        let result = receive.recv().unwrap();
        if result
            .as_ref()
            .is_err_and(|error| error.to_string() == "injected transport panic")
        {
            panic!("injected transport panic");
        }
        result
    }
}
fn request(receive: &mpsc::Receiver<Request>) -> Request {
    receive.recv_timeout(Duration::from_secs(2)).unwrap()
}
fn reply(request: Request, bytes: &[u8]) {
    request
        .reply
        .send(Ok(Output {
            code: Some(0),
            stdout: bytes.to_vec(),
            stderr: vec![],
            cancelled: false,
            truncated: false,
        }))
        .unwrap();
}
fn deliver(owner: &mut WorkingTree) {
    assert!(owner.apply(owner.updates().recv_blocking().unwrap()));
}
fn controlled(
    f: &Fixture,
    gates: Arc<git::MutationGates>,
) -> (WorkingTree, ProcessHost, mpsc::Receiver<Request>) {
    let host = ProcessHost::new();
    let (send, receive) = mpsc::channel();
    let client = git::Client::with_executor(
        f.root.path().to_owned(),
        host.retain(Arc::new(Controlled(Mutex::new(send)))),
    );
    let owner = WorkingTree::new(
        client,
        f.client().discover().unwrap(),
        gates,
        git::DiffOptions::default(),
    );
    (owner, host, receive)
}
fn initial(receive: &mpsc::Receiver<Request>, patch: &[u8]) {
    reply(request(receive), b" M file\0");
    reply(request(receive), patch);
    reply(request(receive), b"");
    reply(request(receive), b" M file\0");
}

#[test]
fn gate_wait_shutdown_keeps_admission_through_cleanup_and_delivery() {
    let f = Fixture::new();
    f.write("file", b"old\ncontext\n");
    f.commit();
    f.write("file", b"new\ncontext\n");
    let identity = f.client().discover().unwrap();
    let patch = f.client().diff(Path::new("file"), Side::Worktree).unwrap();
    let gates = Arc::new(git::MutationGates::new());
    let (mut owner, host, receive) = controlled(&f, gates.clone());
    initial(&receive, &patch);
    deliver(&mut owner);
    let blocking = gates
        .try_acquire(&identity, git::MutationScope::Worktree)
        .unwrap()
        .unwrap();
    owner.act_current(Side::Worktree); // Admission is synchronous even before worker gate wait.
    let shutdown = host.shutdown();
    assert!(!shutdown.is_complete());
    let cleanup = request(&receive);
    assert!(cleanup.args.iter().any(|arg| arg == "status"));
    assert!(!cleanup.cancel.load(Ordering::Acquire));
    reply(cleanup, b" M file\0");
    reply(request(&receive), &patch);
    reply(request(&receive), b"");
    reply(request(&receive), b" M file\0");
    let update = owner.updates().recv_blocking().unwrap();
    drop(blocking);
    assert!(!shutdown.is_complete());
    assert!(owner.apply(update));
    shutdown.wait_blocking();
    assert!(owner.error().unwrap().contains("cancelled"));
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_some()
    );
    owner.refresh();
    assert!(!owner.loading());
    assert!(owner.error().unwrap().contains("shutting down"));
}

#[test]
fn shutdown_between_write_and_cleanup_cannot_ack_before_gate_delivery_settlement() {
    let f = Fixture::new();
    f.write("file", b"old\ncontext\n");
    f.commit();
    f.write("file", b"new\ncontext\n");
    let identity = f.client().discover().unwrap();
    let patch = f.client().diff(Path::new("file"), Side::Worktree).unwrap();
    let gates = Arc::new(git::MutationGates::new());
    let (mut owner, host, receive) = controlled(&f, gates.clone());
    initial(&receive, &patch);
    deliver(&mut owner);
    owner.act_current(Side::Worktree);
    reply(request(&receive), b" M file\0");
    reply(request(&receive), &patch);
    let write = request(&receive);
    assert!(write.args.iter().any(|arg| arg == "apply"));
    reply(write, b"");
    let cleanup = request(&receive);
    let shutdown = host.shutdown();
    assert!(!shutdown.is_complete());
    assert!(!cleanup.cancel.load(Ordering::Acquire));
    reply(cleanup, b"");
    reply(request(&receive), b"");
    let update = owner.updates().recv_blocking().unwrap();
    assert!(!shutdown.is_complete());
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    assert!(owner.apply(update));
    shutdown.wait_blocking();
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_some()
    );
    assert!(receive.try_recv().is_err());
}

#[test]
fn transport_panic_reconciles_under_lease_and_owner_drop_releases_retained_update() {
    let f = Fixture::new();
    f.write("file", b"old\ncontext\n");
    f.commit();
    f.write("file", b"new\ncontext\n");
    let identity = f.client().discover().unwrap();
    let patch = f.client().diff(Path::new("file"), Side::Worktree).unwrap();
    let gates = Arc::new(git::MutationGates::new());
    let (mut owner, host, receive) = controlled(&f, gates.clone());
    initial(&receive, &patch);
    deliver(&mut owner);
    owner.act_current(Side::Worktree);
    reply(request(&receive), b" M file\0");
    reply(request(&receive), &patch);
    let write = request(&receive);
    assert!(write.args.iter().any(|arg| arg == "apply"));
    write
        .reply
        .send(Err(io::Error::other("injected transport panic")))
        .unwrap();
    let cleanup = request(&receive);
    let shutdown = host.shutdown();
    assert!(!shutdown.is_complete());
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    reply(cleanup, b" M file\0");
    reply(request(&receive), &patch);
    reply(request(&receive), b"");
    reply(request(&receive), b" M file\0");
    let update = owner.updates().recv_blocking().unwrap();
    assert!(update.action_error.as_ref().unwrap().contains("panicked"));
    assert!(!shutdown.is_complete());
    // Dropping the owner closes delivery; an already claimed delivery still owns
    // its lease/admission until its consumer drops it.
    drop(owner);
    assert!(!shutdown.is_complete());
    drop(update);
    shutdown.wait_blocking();
    assert!(
        gates
            .try_acquire(&identity, git::MutationScope::Worktree)
            .unwrap()
            .is_some()
    );
}

#[test]
fn slow_auto_ticks_never_cancel_read_and_queued_action_gets_priority() {
    let f = Fixture::new();
    f.write("file", b"old\ncontext\n");
    f.commit();
    f.write("file", b"new\ncontext\n");
    let patch = f.client().diff(Path::new("file"), Side::Worktree).unwrap();
    let (mut owner, host, receive) = controlled(&f, Arc::new(git::MutationGates::new()));
    let slow = request(&receive);
    for _ in 0..100 {
        owner.auto_refresh();
    }
    assert!(!slow.cancel.load(Ordering::Acquire));
    reply(slow, b" M file\0");
    reply(request(&receive), &patch);
    reply(request(&receive), b"");
    reply(request(&receive), b" M file\0");
    deliver(&mut owner); // Exactly one coalesced refresh, not 100 superseded workers.
    let next = request(&receive);
    owner.act_current(Side::Worktree);
    for _ in 0..100 {
        owner.auto_refresh();
    }
    assert!(!next.cancel.load(Ordering::Acquire));
    reply(next, b" M file\0");
    reply(request(&receive), &patch);
    reply(request(&receive), b"");
    reply(request(&receive), b" M file\0");
    deliver(&mut owner);
    reply(request(&receive), b" M file\0");
    reply(request(&receive), &patch);
    let write = request(&receive);
    assert!(write.args.iter().any(|arg| arg == "apply"));
    reply(write, b"");
    reply(request(&receive), b"");
    reply(request(&receive), b"");
    deliver(&mut owner);
    reply(request(&receive), b"");
    reply(request(&receive), b"");
    deliver(&mut owner);
    assert!(!owner.loading());
    assert!(!owner.busy());
    assert!(owner.error().is_none());
    drop(owner);
    host.shutdown().wait_blocking();
}

#[test]
fn repeat_keys_on_a_never_migrate_to_b_and_line_mode_survives_navigation() {
    let f = Fixture::new();
    let old: String = (0..30).map(|i| format!("line {i}\n")).collect();
    f.write("a", old.as_bytes());
    f.write("b", b"old b\ncontext\n");
    f.commit();
    f.write(
        "a",
        old.replace("line 2\n", "changed 2\n")
            .replace("line 25\n", "changed 25\n")
            .as_bytes(),
    );
    f.write("b", b"new b\ncontext\n");
    let client = f.client();
    let mut owner = WorkingTree::new(
        client.clone(),
        client.discover().unwrap(),
        Arc::new(git::MutationGates::new()),
        git::DiffOptions::default(),
    );
    deliver(&mut owner);
    owner.act_current(Side::Worktree);
    owner.act_current(Side::Worktree);
    owner.select("b".into());
    while owner.busy() || owner.loading() {
        deliver(&mut owner);
    }
    assert!(owner.error().is_none(), "{:?}", owner.error());
    assert_eq!(
        f.run(&["show", ":a"]),
        std::fs::read(f.root.path().join("a")).unwrap()
    );
    assert_eq!(f.run(&["show", ":b"]), b"old b\ncontext\n");
    owner.set_selection_mode(Side::Worktree, SelectionMode::Line);
    owner.set_selection_mode(Side::Index, SelectionMode::Range);
    owner.select("a".into());
    owner.select("b".into());
    assert_eq!(owner.selection_mode(Side::Worktree), SelectionMode::Line);
    assert_eq!(owner.selection_mode(Side::Index), SelectionMode::Range);
    assert_eq!(
        owner.pane(Side::Worktree).current_selection(),
        [0].into_iter().collect()
    );
}

#[test]
fn empty_canonical_untracked_target_is_still_dispatched_whole_file() {
    let f = Fixture::new();
    let (mut owner, host, receive) = controlled(&f, Arc::new(git::MutationGates::new()));
    reply(request(&receive), b"?? file\0");
    let diff = request(&receive);
    assert!(diff.args.iter().any(|arg| arg == "--no-index"));
    reply(diff, b"");
    reply(request(&receive), b"?? file\0");
    deliver(&mut owner);
    assert!(owner.pane(Side::Worktree).canonical.is_empty());
    owner.act_file(vec!["file".into()], Side::Worktree);
    reply(request(&receive), b"?? file\0");
    reply(request(&receive), b"");
    let write = request(&receive);
    assert!(write.args.iter().any(|arg| arg == "add"));
    reply(write, b"");
    reply(request(&receive), b"");
    reply(request(&receive), b"");
    deliver(&mut owner);
    assert!(owner.error().is_none());
    drop(owner);
    host.shutdown().wait_blocking();
}

#[test]
fn untracked_snapshot_uses_two_status_traversals_not_one_per_file() {
    let f = Fixture::new();
    let (mut owner, host, receive) = controlled(&f, Arc::new(git::MutationGates::new()));
    let entries: Vec<u8> = (0..20)
        .flat_map(|i| format!("?? file-{i}\0").into_bytes())
        .collect();
    reply(request(&receive), &entries);
    for _ in 0..20 {
        let diff = request(&receive);
        assert!(diff.args.iter().any(|arg| arg == "--no-index"));
        assert!(!diff.args.iter().any(|arg| arg == "status"));
        reply(diff, b"");
    }
    let final_status = request(&receive);
    assert!(final_status.args.iter().any(|arg| arg == "status"));
    reply(final_status, &entries);
    deliver(&mut owner);
    assert_eq!(owner.entries().len(), 20);
    assert!(owner.error().is_none());
    drop(owner);
    host.shutdown().wait_blocking();
}

#[test]
fn empty_untracked_whole_file_stage_and_unstage_work() {
    let f = Fixture::new();
    f.write("empty", b"");
    let client = f.client();
    let mut owner = WorkingTree::new(
        client.clone(),
        client.discover().unwrap(),
        Arc::new(git::MutationGates::new()),
        git::DiffOptions::default(),
    );
    deliver(&mut owner);
    assert_eq!(
        owner
            .pane(Side::Worktree)
            .patch
            .as_ref()
            .map_or(0, |patch| patch.changes()),
        0
    );
    owner.act_file(vec!["empty".into()], Side::Worktree);
    deliver(&mut owner);
    assert!(owner.error().is_none(), "{:?}", owner.error());
    assert_eq!(f.run(&["ls-files", "-z"]), b"empty\0");
    owner.act_file(vec!["empty".into()], Side::Index);
    deliver(&mut owner);
    assert!(owner.error().is_none(), "{:?}", owner.error());
    assert!(f.run(&["ls-files", "-z"]).is_empty());
    assert_eq!(std::fs::read(f.root.path().join("empty")).unwrap(), b"");
}
