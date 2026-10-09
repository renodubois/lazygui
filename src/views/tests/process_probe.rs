use super::*;
use gpui_kit::{AppContext, TestAppContext, component::Root};
use std::{
    process::Command,
    time::{Duration, Instant},
};
#[gpui_kit::test]
fn closing_headless_root_releases_owned_child_and_reaps_it(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let pidfile = temp.path().join("pid");
    let mut command = Command::new("/bin/sh");
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .arg("-c")
        .arg("echo $$ > \"$1\"; exec sleep 60")
        .arg("fixture")
        .arg(&pidfile);
    let operation = Operation::spawn(command, vec![]);
    let settlement = operation.settlement();
    let (_, visual) = cx.add_window_view(|window, cx| {
        let owner = cx.new(|_| operation);
        let probe = cx.new(|_| ProcessProbe::new(owner));
        Root::new(probe, window, cx)
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while !pidfile.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let pid: i32 = std::fs::read_to_string(pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    visual.update(|window, _| window.remove_window());
    visual.run_until_parked();
    // Drop only requests cancellation; the retained worker owns child settlement.
    // Blocking waits are test/worker-only, never part of a GUI-thread Drop.
    settlement.wait_blocking();
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}
