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
    drop(Operation::spawn(command, vec![]));
    assert!(start.elapsed() < Duration::from_secs(3));
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "head -c 1100000 /dev/zero"]);
    let output = Operation::spawn(command, vec![]).finish().unwrap();
    assert!(output.truncated);
    assert_eq!(output.stdout.len(), CAP);
}
