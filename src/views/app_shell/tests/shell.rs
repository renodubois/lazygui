use crate::views::app_shell::test_support::{self, Fixture};
use gpui_kit::{TestAppContext, test::TestWindowExt};
use lazygui::git::Side;

#[gpui_kit::test]
fn retained_shell_native_controls_stage_commit_and_draft_journey(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (visual, shell) = test_support::open(cx, &fixture);
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading() && !f.history.read(cx).busy())
    });
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("current-branch").label(), Some("main"));
        window.click(("file", 0usize), cx);
        window.press("enter", cx);
        window.press("a", cx); // configured main.toggleSelectHunk
        window.press("v", cx);
        window.press("down", cx);
        let features = shell.read(cx).features.as_ref().unwrap();
        assert_eq!(
            features.tree.read(cx).pane(Side::Worktree).selection.len(),
            2
        );
        window.press("escape", cx);
        window.click("action-stage", cx);
    });
    test_support::wait(visual, |cx| {
        shell.read(cx).features.as_ref().is_some_and(|f| {
            !f.tree.read(cx).busy() && !f.tree.read(cx).pane(Side::Index).is_empty()
        })
    });
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("diff-index").is_some());
        assert!(window.try_find("diff-unstaged").is_none());
        window.click("action-commit", cx);
        window.render_frame(cx);
        window.click("commit-subject", cx);
        window.input("qc safe draft", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-controls").is_none());
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .commit
                .read(cx)
                .draft()
                .subject,
            "qc safe draft"
        );
        window.click("action-commit", cx);
        window.render_frame(cx);
        window.click("commit-submit", cx);
        window.click("commit-submit", cx); // owner rejects duplicate even before repaint
    });
    test_support::wait(visual, |cx| {
        shell.read(cx).features.as_ref().is_some_and(|f| {
            matches!(
                f.commit.read(cx).outcome(),
                Some(lazygui::commit::Outcome::Committed { .. })
            ) && !f.history.read(cx).busy()
                && !f.tree.read(cx).loading()
        })
    });
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("commit-controls").is_none());
        assert_eq!(window.find("diff-empty").label(), Some("No diff data"));
        assert!(window.try_find("diff-index").is_none());
        assert!(window.try_find("diff-unstaged").is_none());
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .history
                .read(cx)
                .commits()[0]
                .subject,
            b"connected commit"
        );
    });
    assert_eq!(fixture.git.state.lock().unwrap().writes, 2);
    test_support::shutdown(visual, &fixture.host);
}
#[gpui_kit::test]
fn popup_filter_help_repository_switch_and_config_reload_use_actual_controls(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (visual, shell) = test_support::open(cx, &fixture);
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading())
    });
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("action-filter", cx);
        window.render_frame(cx);
        window.click("repository-prompt", cx);
        window.input("qc", cx);
        window.press("space", cx);
        assert_eq!(fixture.git.state.lock().unwrap().writes, 0);
        window.press("escape", cx);
        window.render_frame(cx);
        window.click("action-help", cx);
        window.render_frame(cx);
        window.press("space", cx);
        window.press("q", cx);
        assert!(shell.read(cx).closing.is_none());
        assert_eq!(fixture.git.state.lock().unwrap().writes, 0);
        window.press("escape", cx);
        window.click("action-config-reload", cx);
    });
    test_support::wait(visual, |cx| !shell.read(cx).repository.read(cx).loading());
    let next = fixture.temp.path().join("next");
    std::fs::create_dir_all(next.join(".git")).unwrap();
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("action-switch", cx);
        window.render_frame(cx);
        window.click("repository-prompt", cx);
        window.input(next.to_str().unwrap(), cx);
        window.press("enter", cx);
    });
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .repository
            .read(cx)
            .session()
            .is_some_and(|s| s.readiness.identity.worktree.as_ref() == Some(&next))
            && shell
                .read(cx)
                .features
                .as_ref()
                .is_some_and(|f| !f.tree.read(cx).loading())
    });
    visual.update(|_, cx| {
        assert_eq!(
            shell
                .read(cx)
                .repository
                .read(cx)
                .session()
                .unwrap()
                .readiness
                .identity
                .worktree
                .as_ref(),
            Some(&next)
        );
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn full_raw_previous_message_recall_retains_body_hard_breaks_and_trailers(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (visual, shell) = test_support::open(cx, &fixture);
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading() && !f.history.read(cx).busy())
    });
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.press("c", cx);
        window.render_frame(cx);
        window.press("up", cx);
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        let draft = shell
            .read(cx)
            .features
            .as_ref()
            .unwrap()
            .commit
            .read(cx)
            .draft();
        assert_eq!(draft.subject, "fixture");
        assert_eq!(
            draft.body,
            "body hard break\nsecond line\n\nSigned-off-by: Fixture <fixture@example.invalid>\n"
        );
        window.press("escape", cx);
    });
    test_support::shutdown(visual, &fixture.host);
}

struct HeldWrites {
    inner: std::sync::Arc<test_support::Git>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    started: std::sync::mpsc::Sender<String>,
}
impl lazygui::git::process::Executor for HeldWrites {
    fn execute(
        &self,
        command: std::process::Command,
        input: Vec<u8>,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> std::io::Result<lazygui::git::process::Output> {
        let mutation = command
            .get_args()
            .find_map(|a| (a == "add" || a == "commit").then(|| a.to_string_lossy().into_owned()));
        if let Some(mutation) = mutation {
            self.started.send(mutation).unwrap();
            self.release.lock().unwrap().recv().unwrap();
        }
        self.inner.execute(command, input, cancel)
    }
}

#[gpui_kit::test]
fn switching_rejects_busy_and_queued_mutations_without_dropping_their_outcomes(
    cx: &mut TestAppContext,
) {
    let mut fixture = Fixture::new();
    let (release, receive) = std::sync::mpsc::channel();
    let (started, start) = std::sync::mpsc::channel();
    fixture.executor = Some(std::sync::Arc::new(HeldWrites {
        inner: fixture.git.clone(),
        release: std::sync::Mutex::new(receive),
        started,
    }));
    let next = fixture.temp.path().join("next");
    std::fs::create_dir_all(next.join(".git")).unwrap();
    let (visual, shell) = test_support::open(cx, &fixture);
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading())
    });
    let generation = visual.update(|_, cx| shell.read(cx).repository.read(cx).generation());
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.press("space", cx);
        window.press("down", cx);
        window.press("space", cx); // a distinct queued file target
        window.press("ctrl-r", cx);
        window.render_frame(cx);
        window.input(next.to_str().unwrap(), cx);
        window.press("enter", cx);
    });
    assert_eq!(
        start
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap(),
        "add"
    );
    visual.run_until_parked();
    visual.update(|window, cx| {
        assert_eq!(shell.read(cx).repository.read(cx).generation(), generation);
        assert!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .busy()
        );
        window.render_frame(cx);
        assert_eq!(
            window.find("operation-feedback").label(),
            Some("Finish the current mutation before switching repository.")
        );
    });
    release.send(()).unwrap();
    test_support::wait(visual, |_| start.try_recv().is_ok_and(|kind| kind == "add"));
    release.send(()).unwrap();
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).busy())
    });
    assert_eq!(fixture.git.state.lock().unwrap().writes, 2);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.press("c", cx);
        window.render_frame(cx);
        window.input("retained outcome", cx);
        window.press("enter", cx);
    });
    assert_eq!(
        start
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap(),
        "commit"
    );
    visual.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.switch(next.clone(), window, cx));
        assert_eq!(shell.read(cx).repository.read(cx).generation(), generation);
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
    });
    release.send(()).unwrap();
    test_support::wait(visual, |cx| {
        shell.read(cx).features.as_ref().is_some_and(|f| {
            matches!(
                f.commit.read(cx).outcome(),
                Some(lazygui::commit::Outcome::Committed { .. })
            ) && !f.tree.read(cx).loading()
                && !f.history.read(cx).busy()
        })
    });
    test_support::shutdown(visual, &fixture.host);
}
