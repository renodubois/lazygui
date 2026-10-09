use super::*;
#[test]
fn cancellation_settles_direct_child_and_pipe_holding_helper() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let mut command = Command::new("/bin/sh");
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .arg("-c")
        .arg("sleep 60 & echo $$ $! > \"$1\"; wait")
        .arg("fixture")
        .arg(&pidfile);
    let operation = Operation::spawn(command, vec![]);
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !pidfile.exists() {
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let pids: Vec<i32> = std::fs::read_to_string(pidfile)
        .unwrap()
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();
    operation.cancel();
    let output = operation.finish().unwrap();
    assert!(output.cancelled);
    assert_eq!(unsafe { libc::kill(pids[0], 0) }, -1); // direct child reaped
    // Descendant can briefly be a zombie awaiting its system reaper, but cannot run/wait.
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", pids[1])) {
        assert_eq!(stat.split(") ").nth(1).unwrap().chars().next(), Some('Z'));
    }
}
#[test]
fn dropping_owned_operation_cancels_and_bounded_output_is_not_unbounded() {
    let mut command = Command::new("/bin/sleep");
    command.arg("60");
    let start = std::time::Instant::now();
    let operation = Operation::spawn(command, vec![]);
    let settlement = operation.settlement();
    drop(operation);
    assert!(start.elapsed() < Duration::from_secs(3));
    settlement.wait_blocking();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "head -c 1100000 /dev/zero"]);
    let output = Operation::spawn(command, vec![]).finish().unwrap();
    assert!(output.truncated);
    assert_eq!(output.stdout.len(), CAP);
}

#[test]
fn drop_is_nonblocking_and_shutdown_ack_waits_for_retained_worker() {
    struct Controlled {
        started: std::sync::mpsc::Sender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }
    impl Executor for Controlled {
        fn execute(&self, _: Command, _: Vec<u8>, cancel: Arc<AtomicBool>) -> io::Result<Output> {
            self.started.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
            assert!(cancel.load(Ordering::Acquire));
            Ok(Output {
                code: None,
                stdout: vec![],
                stderr: vec![],
                cancelled: true,
                truncated: false,
            })
        }
    }
    let host = ProcessHost::new();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let operation = host
        .spawn_with_executor(
            Command::new("unused"),
            vec![],
            Arc::new(Controlled {
                started: started_tx,
                release: Mutex::new(release_rx),
            }),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let settlement = operation.settlement();
    drop(operation); // Would deadlock here if Drop joined the controlled worker.
    let shutdown = host.shutdown();
    assert!(!shutdown.is_complete());
    assert!(!settlement.is_complete());
    assert!(host.spawn(Command::new("unused"), vec![]).is_err());
    release_tx.send(()).unwrap();
    shutdown.wait_blocking();
    assert!(settlement.is_complete());
    assert!(host.shutdown().is_complete());
}

#[test]
fn retained_executor_success_does_not_cancel_following_commands() {
    let host = ProcessHost::new();
    let executor = host.retain(Arc::new(Native));
    let flag = Arc::new(AtomicBool::new(false));
    for _ in 0..2 {
        let output = executor
            .execute(Command::new("/bin/true"), vec![], flag.clone())
            .unwrap();
        assert_eq!(output.code, Some(0));
        assert!(!flag.load(Ordering::Acquire));
    }
    host.shutdown().wait_blocking();
}

#[test]
fn stderr_is_bounded_and_early_exit_preserves_exit_diagnostic() {
    let mut command = Command::new("/bin/sh");
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args(["-c", "head -c 1100000 /dev/zero >&2; exit 7"]);
    let output = Operation::spawn(command, vec![]).finish().unwrap();
    assert_eq!(output.code, Some(7));
    assert_eq!(output.stderr.len(), CAP);
    assert!(output.truncated);
    let mut command = Command::new("/bin/sh");
    command
        .env_clear()
        .args(["-c", "echo rejected >&2; exit 23"]);
    let output = Operation::spawn(command, vec![b'x'; CAP * 2])
        .finish()
        .unwrap();
    assert_eq!(output.code, Some(23));
    assert_eq!(output.stderr, b"rejected\n");
}

#[test]
fn cancellation_before_spawn_does_not_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("marker");
    let mut command = Command::new("/usr/bin/touch");
    command.arg(&marker);
    let output = Native
        .execute(command, vec![], Arc::new(AtomicBool::new(true)))
        .unwrap();
    assert!(output.cancelled);
    assert!(!marker.exists());
}
