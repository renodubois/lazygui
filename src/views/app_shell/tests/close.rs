use super::*;
use crate::views::app_shell::test_support::{self, Fixture};
use gpui_kit::{TestAppContext, VisualTestContext, test::TestWindowExt};
use lazygui::git::process::{Executor, Output};
use std::{
    io,
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
struct Held {
    release: Mutex<mpsc::Receiver<()>>,
    started: mpsc::Sender<()>,
}
impl Executor for Held {
    fn execute(&self, _: Command, _: Vec<u8>, cancel: Arc<AtomicBool>) -> io::Result<Output> {
        self.started.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        Ok(Output {
            code: Some(0),
            stdout: vec![],
            stderr: vec![],
            cancelled: cancel.load(Ordering::Acquire),
            truncated: false,
        })
    }
}
#[gpui_kit::test]
fn last_close_is_retained_until_owned_process_shutdown_acknowledgment(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (visual, shell) = test_support::open(cx, &fixture);
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading())
    });
    let (release, receive) = mpsc::channel();
    let (started, start) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let operation = fixture
        .host
        .spawn_with_executor(
            Command::new("fixture-no-execution"),
            vec![],
            Arc::new(Held {
                release: Mutex::new(receive),
                started,
            }),
            cancel.clone(),
        )
        .unwrap();
    start.recv().unwrap();
    visual.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.request_close(false, window, cx));
        assert_eq!(cx.windows().len(), 1);
        assert!(cancel.load(Ordering::Acquire));
        assert!(!fixture.host.shutdown().is_complete());
    });
    visual.run_until_parked();
    assert_eq!(visual.cx.update(|cx| cx.windows().len()), 1);
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        visual
            .background_executor
            .advance_clock(Duration::from_millis(16));
        visual.run_until_parked();
        if visual.cx.update(|cx| cx.windows().is_empty()) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(operation.finish().unwrap().cancelled);
    assert!(fixture.host.shutdown().is_complete());
}
fn wait_closed(visual: &mut VisualTestContext) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while visual.cx.update(|cx| !cx.windows().is_empty()) {
        visual
            .background_executor
            .advance_clock(Duration::from_millis(16));
        visual.run_until_parked();
        assert!(
            Instant::now() < deadline,
            "close did not settle the feature workflow"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[gpui_kit::test]
fn close_during_connected_staging_retains_cleanup_and_lease_until_result_is_dropped(
    cx: &mut TestAppContext,
) {
    let mut fixture = Fixture::installed();
    let write = fixture.hold("add");
    let cleanup = fixture.hold("status");
    let (visual, shell) = test_support::open(cx, &fixture);
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading() && !f.history.read(cx).busy())
    });
    let (identity, gates, weak) = visual.update(|_, cx| {
        let f = shell.read(cx).features.as_ref().unwrap();
        (
            f.tree.read(cx).identity().clone(),
            shell.read(cx).gates.clone(),
            f.tree.downgrade(),
        )
    });
    write.armed.store(true, Ordering::Release);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click(("file", 0usize), cx);
        window.press("space", cx);
    });
    write.started.recv_timeout(Duration::from_secs(5)).unwrap();
    cleanup.armed.store(true, Ordering::Release);
    visual.update(|window, cx| window.press("q", cx));
    visual.run_until_parked();
    assert!(weak.upgrade().is_none());
    assert!(
        write
            .cancellation
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .load(Ordering::Acquire)
    );
    let ack = fixture.host.shutdown();
    assert!(!ack.is_complete());
    assert_eq!(visual.cx.update(|cx| cx.windows().len()), 1);
    assert!(
        gates
            .try_acquire(&identity, lazygui::git::MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    write.release.send(()).unwrap();
    cleanup
        .started
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(
        !cleanup
            .cancellation
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .load(Ordering::Acquire),
        "cleanup reads use their admitted shutdown-safe lane"
    );
    assert!(!ack.is_complete());
    assert!(
        gates
            .try_acquire(&identity, lazygui::git::MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    cleanup.release.send(()).unwrap();
    wait_closed(visual);
    assert!(ack.is_complete());
    assert!(
        gates
            .try_acquire(&identity, lazygui::git::MutationScope::All)
            .unwrap()
            .is_some()
    );
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nold\nthree\n");
    assert_eq!(
        std::fs::read(fixture.options.cwd.join("a.txt")).unwrap(),
        b"one\nnew\nthree\n"
    );
    assert!(!fixture.options.cwd.join(".git/index.lock").exists());
}

#[gpui_kit::test]
fn close_during_real_hook_cancels_child_then_waits_for_commit_reconciliation_and_lease(
    cx: &mut TestAppContext,
) {
    let mut fixture = Fixture::installed();
    fixture.config(
        "confirmOnQuit: true\ngit:\n  autoRefresh: false\n  autoDetectExternalChanges: false\n",
    );
    fixture.run_git(&["add", "a.txt"]);
    let head = fixture.run_git(&["rev-parse", "HEAD"]);
    fixture.hook("pre-commit", "printf started > \"$HOME/hook-started\"\nwhile :; do sleep 1; done\nprintf escaped > \"$HOME/hook-escaped\"");
    let cleanup = fixture.hold("status");
    let (visual, shell) = test_support::open(cx, &fixture);
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading() && !f.history.read(cx).busy())
    });
    let (identity, gates, weak) = visual.update(|window, cx| {
        window.render_frame(cx);
        let f = shell.read(cx).features.as_ref().unwrap();
        let values = (
            f.tree.read(cx).identity().clone(),
            shell.read(cx).gates.clone(),
            f.commit.downgrade(),
        );
        window.click("action-commit", cx);
        window.render_frame(cx);
        window.click("commit-subject", cx);
        window.input("cancel running hook", cx);
        window.press("ctrl-s", cx);
        values
    });
    test_support::wait(visual, |_| {
        fixture.temp.path().join("home/hook-started").exists()
    });
    cleanup.armed.store(true, Ordering::Release);
    visual.update(|window, cx| {
        assert!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .commit
                .read(cx)
                .busy()
        );
        window.press("escape", cx); // dismiss editor, not proof of operation settlement
        window.press("q", cx);
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(shell.read(cx).closing.is_none());
        window.click("quit-confirm", cx);
    });
    visual.run_until_parked();
    cleanup
        .started
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(weak.upgrade().is_none());
    let ack = fixture.host.shutdown();
    assert!(!ack.is_complete());
    assert_eq!(visual.cx.update(|cx| cx.windows().len()), 1);
    assert!(
        gates
            .try_acquire(&identity, lazygui::git::MutationScope::All)
            .unwrap()
            .is_none()
    );
    assert!(!fixture.temp.path().join("home/hook-escaped").exists());
    assert!(!fixture.options.cwd.join(".git/index.lock").exists());
    cleanup.release.send(()).unwrap();
    wait_closed(visual);
    assert!(ack.is_complete());
    assert!(
        gates
            .try_acquire(&identity, lazygui::git::MutationScope::All)
            .unwrap()
            .is_some()
    );
    assert_eq!(fixture.run_git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nnew\nthree\n");
}

#[gpui_kit::test]
fn simultaneous_shared_host_closes_recheck_last_removal_after_storage_flush(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (first, first_shell) = test_support::open(cx, &fixture);
    test_support::wait(first, |cx| {
        first_shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading())
    });
    let first_window = first.update(|window, _| window.window_handle());
    let (second, second_shell) = test_support::open(&mut first.cx, &fixture);
    test_support::wait(second, |cx| {
        second_shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading())
    });
    let (release, receive) = mpsc::channel();
    let (started, start) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let operation = fixture
        .host
        .spawn_with_executor(
            Command::new("fixture-no-execution"),
            vec![],
            Arc::new(Held {
                release: Mutex::new(receive),
                started,
            }),
            cancel.clone(),
        )
        .unwrap();
    start.recv().unwrap();
    second.update(|window, cx| {
        first_window
            .update(cx, |_, first_window, cx| {
                first_shell.update(cx, |shell, cx| shell.request_close(false, first_window, cx))
            })
            .unwrap();
        second_shell.update(cx, |shell, cx| shell.request_close(false, window, cx));
        assert_eq!(cx.windows().len(), 2);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cancel.load(Ordering::Acquire) {
        second
            .background_executor
            .advance_clock(Duration::from_millis(16));
        second.run_until_parked();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(
        second.cx.update(|cx| cx.windows().len()),
        1,
        "the final window must retain process settlement even when both initially saw two windows"
    );
    release.send(()).unwrap();
    while second.cx.update(|cx| !cx.windows().is_empty()) {
        second
            .background_executor
            .advance_clock(Duration::from_millis(16));
        second.run_until_parked();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(operation.finish().unwrap().cancelled);
    assert!(fixture.host.shutdown().is_complete());
}
