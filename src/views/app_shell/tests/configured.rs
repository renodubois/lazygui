//! Effective M1 settings cross the production readiness/shell/owner boundary.
use super::{
    AppShell,
    test_support::{self, Fixture},
};
use gpui_kit::{Entity, TestAppContext, VisualTestContext, test::TestWindowExt};
use lazygui::{
    commit::Outcome,
    git::{Side, Whitespace},
    working_tree::SelectionMode,
};

fn settled(visual: &mut VisualTestContext, shell: &Entity<AppShell>) {
    test_support::wait(visual, |cx| {
        shell.read(cx).features.as_ref().is_some_and(|f| {
            !f.tree.read(cx).loading()
                && !f.tree.read(cx).busy()
                && !f.history.read(cx).busy()
                && !f.commit.read(cx).busy()
        })
    });
}

const COMMIT_PREFIX_CONFIG: &str = r#"git:
  autoRefresh: false
  autoDetectExternalChanges: false
  commitPrefix:
    - {pattern: '^feature/([A-Z]+-[0-9]+).*', replace: '[$1] '}
"#;

#[gpui_kit::test]
fn configured_commit_prefix_prepares_native_controls_retains_edits_and_resets_after_commit(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::installed();
    fixture.run_git(&["checkout", "-qb", "feature/ABC-123"]);
    fixture.run_git(&["add", "a.txt"]);
    fixture.config(COMMIT_PREFIX_CONFIG);
    let (visual, shell) = test_support::open(cx, &fixture);
    settled(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("current-branch").label(),
            Some("feature/ABC-123")
        );
        window.click("action-commit", cx);
        window.render_frame(cx);
    });
    // Preparation is delivered by the retained production shell, not a test consumer.
    settled(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("commit-subject").value(), Some("[ABC-123] "));
        let commit = shell.read(cx).features.as_ref().unwrap().commit.read(cx);
        assert!(commit.error().is_none(), "{:?}", commit.error());
        assert_eq!(commit.draft().subject, "[ABC-123] ");
        assert!(commit.draft().body.is_empty());
        window.click("commit-subject", cx);
        window.press("end", cx);
        window.input("ordinary jk letters", cx); // j/k are edits, never history/navigation.
        window.press("tab", cx);
        window.input("body jk first", cx);
        window.press("enter", cx); // body Enter is a hard break, not submission.
        window.input("second line", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("commit-subject").value(),
            Some("[ABC-123] ordinary jk letters")
        );
    });
    visual.run_until_parked(); // Deliver Kit's deferred change subscriptions before reopening.
    visual.update(|window, cx| {
        let commit = shell.read(cx).features.as_ref().unwrap().commit.read(cx);
        assert!(!commit.busy());
        assert_eq!(commit.draft().body, "body jk first\nsecond line");
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-subject").is_none());
        window.click("action-commit", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("commit-subject").value(),
            Some("[ABC-123] ordinary jk letters")
        );
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .commit
                .read(cx)
                .draft()
                .body,
            "body jk first\nsecond line"
        );
        window.click("commit-subject", cx);
        window.press("enter", cx);
    });
    settled(visual, &shell);
    assert_eq!(
        fixture.run_git(&["log", "-1", "--format=%B"]),
        b"[ABC-123] ordinary jk letters\n\nbody jk first\nsecond line\n"
    );
    visual.update(|window, cx| {
        window.render_frame(cx);
        let f = shell.read(cx).features.as_ref().unwrap();
        assert!(matches!(
            f.commit.read(cx).outcome(),
            Some(Outcome::Committed { .. })
        ));
        assert!(f.commit.read(cx).draft().subject.is_empty());
        assert!(f.commit.read(cx).draft().body.is_empty());
        assert_eq!(
            f.history.read(cx).commits()[0].subject,
            b"[ABC-123] ordinary jk letters"
        );
        assert!(window.try_find("commit-subject").is_none());
    });

    // A successful commit starts a genuinely new draft and reads the current branch again.
    fixture.run_git(&["checkout", "-qb", "feature/DEF-456"]);
    fixture.run_git(&["add", "b.txt"]);
    visual.update(|window, cx| window.click("action-refresh", cx));
    settled(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("action-commit", cx);
        window.render_frame(cx);
    });
    settled(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("commit-subject").value(), Some("[DEF-456] "));
        assert!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .commit
                .read(cx)
                .draft()
                .body
                .is_empty()
        );
        window.click("commit-subject", cx);
        window.press("end", cx);
        window.input("next jk draft", cx);
        window.press("tab", cx);
        window.input("new body", cx);
        window.press("ctrl-s", cx);
    });
    settled(visual, &shell);
    assert_eq!(
        fixture.run_git(&["log", "-1", "--format=%B"]),
        b"[DEF-456] next jk draft\n\nnew body\n"
    );
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("commit-subject").is_none());
        assert!(matches!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .commit
                .read(cx)
                .outcome(),
            Some(Outcome::Committed { .. })
        ));
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn configured_commit_prefix_held_preparation_never_overwrites_native_edits(
    cx: &mut TestAppContext,
) {
    use std::{sync::atomic::Ordering, time::Duration};
    let mut fixture = Fixture::installed();
    fixture.run_git(&["checkout", "-qb", "feature/ABC-123"]);
    fixture.run_git(&["add", "a.txt"]);
    fixture.config(COMMIT_PREFIX_CONFIG);
    let held = fixture.hold("symbolic-ref");
    let (visual, shell) = test_support::open(cx, &fixture);
    settled(visual, &shell);
    let head = fixture.run_git(&["rev-parse", "HEAD"]);
    held.armed.store(true, Ordering::Release);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("action-commit", cx);
        window.render_frame(cx);
        assert_eq!(window.find("commit-subject").value(), Some(""));
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
    held.started.recv_timeout(Duration::from_secs(5)).unwrap();
    visual.update(|window, cx| {
        window.click("commit-subject", cx);
        window.input("typed jk while preparing", cx);
        window.press("tab", cx);
        window.input("retained body", cx);
        window.press("enter", cx);
        window.press("enter", cx);
        window.input("jk second line", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("commit-subject").value(),
            Some("typed jk while preparing")
        );
    });
    visual.run_until_parked(); // Deliver edits while transport preparation is still held.
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("commit-subject").value(),
            Some("typed jk while preparing")
        );
        let commit = shell.read(cx).features.as_ref().unwrap().commit.read(cx);
        assert!(
            commit.busy(),
            "edits must race the held production preparation"
        );
        assert_eq!(commit.draft().subject, "typed jk while preparing");
        assert_eq!(commit.draft().body, "retained body\n\njk second line");
    });
    held.release.send(()).unwrap();
    settled(visual, &shell);
    assert_eq!(fixture.run_git(&["rev-parse", "HEAD"]), head);
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("commit-subject").value(),
            Some("typed jk while preparing")
        );
        let commit = shell.read(cx).features.as_ref().unwrap().commit.read(cx);
        assert!(commit.error().is_none(), "{:?}", commit.error());
        assert_eq!(commit.draft().subject, "typed jk while preparing");
        assert_eq!(commit.draft().body, "retained body\n\njk second line");
        window.press("escape", cx);
        window.render_frame(cx);
        window.click("action-commit", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("commit-subject").value(),
            Some("typed jk while preparing")
        );
        window.click("commit-subject", cx);
        window.press("ctrl-s", cx);
    });
    settled(visual, &shell);
    assert_eq!(
        fixture.run_git(&["log", "-1", "--format=%B"]),
        b"typed jk while preparing\n\nretained body\n\njk second line\n"
    );
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("commit-subject").is_none());
        assert!(matches!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .commit
                .read(cx)
                .outcome(),
            Some(Outcome::Committed { .. })
        ));
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn configured_shift_space_line_mode_context_and_whitespace_are_live_native_actions(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::installed();
    fixture.config("gui:\n  showFileTree: false\n  useHunkModeInDiffView: false\ngit:\n  autoRefresh: false\n  autoDetectExternalChanges: false\n  diffContextSize: 2\n  ignoreWhitespaceInDiffView: true\nkeybinding:\n  universal:\n    select: '<shift+space>'\n");
    let (visual, shell) = test_support::open(cx, &fixture);
    settled(visual, &shell);
    fixture
        .installed_git
        .as_ref()
        .unwrap()
        .commands
        .lock()
        .unwrap()
        .clear();
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("file-tree-root").is_none());
        assert!(
            window
                .find("action-stage")
                .label()
                .unwrap()
                .contains("Shift+space")
        );
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert_eq!(tree.options().context, 2);
        assert_eq!(tree.options().whitespace, Whitespace::IgnoreAllSpace);
        assert_eq!(tree.selection_mode(Side::Worktree), SelectionMode::Line);
        assert!(!tree.pane(Side::Worktree).partial_enabled());
        window.click(("file", 0usize), cx);
        window.press("0", cx);
        window.press("space", cx); // old binding must not leak
        window.press("shift-space", cx); // partial is disabled, not a lossy patch
        assert!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .error()
                .unwrap()
                .contains("ignoring whitespace")
        );
        assert!(
            !fixture
                .installed_git
                .as_ref()
                .unwrap()
                .commands
                .lock()
                .unwrap()
                .iter()
                .any(|args| args.iter().any(|arg| arg == "apply" || arg == "add"))
        );
        window.click("action-whitespace", cx);
    });
    settled(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert_eq!(tree.options().whitespace, Whitespace::Exact);
        assert!(tree.pane(Side::Worktree).partial_enabled());
        window.press("a", cx); // configured initial line mode -> hunk mode
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .selection_mode(Side::Worktree),
            SelectionMode::Hunk
        );
        window.press("shift-space", cx);
    });
    settled(visual, &shell);
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nnew\nthree\n");
    assert_eq!(fixture.run_git(&["show", ":b.txt"]), b"one\nold\nthree\n");
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("diff-unstaged").is_none());
        assert!(window.try_find("diff-index").is_some());
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .selection_mode(Side::Index),
            SelectionMode::Line
        );
        window.press("a", cx); // each pane owns its configured mode independently
        window.press("shift-space", cx);
    });
    settled(visual, &shell);
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nold\nthree\n");
    assert_eq!(
        std::fs::read(fixture.options.cwd.join("a.txt")).unwrap(),
        b"one\nnew\nthree\n"
    );
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn reload_is_transactional_live_and_retains_draft_while_dropping_old_pending_read(
    cx: &mut TestAppContext,
) {
    use std::{sync::atomic::Ordering, time::Duration};
    let mut fixture = Fixture::installed();
    let held = fixture.hold("diff");
    let (visual, shell) = test_support::open(cx, &fixture);
    settled(visual, &shell);
    let old = visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("action-commit", cx);
        window.render_frame(cx);
        window.click("commit-subject", cx);
        window.input("retained reload draft", cx);
        window.press("tab", cx);
        window.input("first", cx);
        window.press("enter", cx);
        window.input("second", cx);
        window.press("escape", cx);
        shell.read(cx).features.as_ref().unwrap().tree.downgrade()
    });
    fixture.config("gui: [invalid\n");
    visual.update(|window, cx| window.click("action-config-reload", cx));
    test_support::wait(visual, |cx| !shell.read(cx).repository.read(cx).loading());
    visual.update(|window, cx| {
        window.render_frame(cx);
        let f = shell.read(cx).features.as_ref().unwrap();
        assert_eq!(f.tree.entity_id(), old.upgrade().unwrap().entity_id());
        assert!(
            shell
                .read(cx)
                .repository
                .read(cx)
                .error()
                .unwrap()
                .contains("Configuration")
        );
        assert_eq!(
            f.tree.read(cx).selection_mode(Side::Worktree),
            SelectionMode::Hunk
        );
        assert_eq!(f.commit.read(cx).draft().body, "first\nsecond");
        // Failed reload keeps the actual old select binding, not just a settings snapshot.
        window.click(("file", 0usize), cx);
        window.press("space", cx);
    });
    settled(visual, &shell);
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nnew\nthree\n");
    visual.update(|window, cx| window.press("space", cx));
    settled(visual, &shell);
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nold\nthree\n");

    held.armed.store(true, Ordering::Release);
    visual.update(|window, cx| window.click("action-refresh", cx));
    held.started.recv_timeout(Duration::from_secs(5)).unwrap();
    fixture
        .installed_git
        .as_ref()
        .unwrap()
        .commands
        .lock()
        .unwrap()
        .clear();
    fixture.config("gui:\n  useHunkModeInDiffView: false\ngit:\n  autoRefresh: false\n  autoDetectExternalChanges: false\n  diffContextSize: 1\nkeybinding:\n  universal:\n    select: '<shift+space>'\n");
    visual.update(|window, cx| window.click("action-config-reload", cx));
    test_support::wait(visual, |cx| !shell.read(cx).repository.read(cx).loading());
    settled(visual, &shell);
    assert!(
        old.upgrade().is_none(),
        "reload must drop the replaced owner's pending read"
    );
    {
        let commands = fixture
            .installed_git
            .as_ref()
            .unwrap()
            .commands
            .lock()
            .unwrap();
        let diffs: Vec<_> = commands
            .iter()
            .filter(|args| args.iter().any(|arg| arg == "diff"))
            .collect();
        assert!(!diffs.is_empty(), "replacement owner must read real diffs");
        assert!(
            diffs
                .iter()
                .all(|args| args.iter().any(|arg| arg == "--unified=1")),
            "reloaded context policy must reach installed Git: {diffs:?}"
        );
    }
    assert!(
        held.cancellation
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .load(Ordering::Acquire)
    );
    visual.update(|window, cx| {
        window.render_frame(cx);
        let f = shell.read(cx).features.as_ref().unwrap();
        assert_eq!(f.tree.read(cx).options().context, 1);
        assert_eq!(f.tree.read(cx).options().whitespace, Whitespace::Exact);
        for side in [Side::Worktree, Side::Index] {
            assert_eq!(f.tree.read(cx).selection_mode(side), SelectionMode::Line);
        }
        assert_eq!(f.commit.read(cx).draft().subject, "retained reload draft");
        assert_eq!(f.commit.read(cx).draft().body, "first\nsecond");
        assert!(
            window
                .find("action-stage")
                .label()
                .unwrap()
                .contains("Shift+space")
        );
        window.click("action-commit", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-subject").is_some());
        window.press("escape", cx);
        window.click(("file", 0usize), cx);
        window.press("enter", cx);
        window.press("space", cx); // removed binding must not stage anything
    });
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nold\nthree\n");
    visual.update(|window, cx| window.press("shift-space", cx));
    settled(visual, &shell);
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nthree\n");
    visual.update(|window, cx| {
        window.press("tab", cx);
        window.press("shift-space", cx);
    });
    settled(visual, &shell);
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nold\nthree\n");
    assert_eq!(
        std::fs::read(fixture.options.cwd.join("a.txt")).unwrap(),
        b"one\nnew\nthree\n"
    );
    let acknowledgment = fixture.host.shutdown();
    assert!(
        !acknowledgment.is_complete(),
        "dropped owner's transport must remain host-retained"
    );
    held.release.send(()).unwrap();
    test_support::wait(visual, |_| acknowledgment.is_complete());
    visual.update(|_, cx| {
        let f = shell.read(cx).features.as_ref().unwrap();
        assert!(f.tree.read(cx).error().is_none());
        assert_eq!(f.tree.read(cx).options().context, 1);
        assert_eq!(f.commit.read(cx).draft().subject, "retained reload draft");
    });
}

#[gpui_kit::test]
fn reload_with_open_commit_overlay_drops_captured_owner_and_reopens_with_new_policy(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::installed();
    fixture.run_git(&["add", "a.txt"]);
    fixture.config("git:\n  autoRefresh: false\n  autoDetectExternalChanges: false\nkeybinding:\n  universal:\n    return: '<alt+x>'\n");
    let (visual, shell) = test_support::open(cx, &fixture);
    settled(visual, &shell);
    let old = visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("action-commit", cx);
        window.render_frame(cx);
        window.click("commit-subject", cx);
        window.input("retained overlay", cx);
        window.press("tab", cx);
        window.input("alpha beta gamma delta", cx);
        shell.read(cx).features.as_ref().unwrap().commit.downgrade()
    });
    fixture.config("gui: [invalid\n");
    visual.update(|window, cx| {
        // Toolbar actions are intentionally exclusive while a popup is open.
        window.click("action-config-reload", cx);
        assert!(!shell.read(cx).repository.read(cx).loading());
        // A real reload result can still arrive with an overlay open (the owner
        // intention is the same one dispatched by the toolbar).
        shell.read(cx).repository.clone().update(cx, |owner, cx| {
            owner.reload_config();
            cx.notify();
        });
    });
    test_support::wait(visual, |cx| !shell.read(cx).repository.read(cx).loading());
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(shell.read(cx).repository.read(cx).error().is_some());
        assert_eq!(
            shell.read(cx).features.as_ref().unwrap().commit.entity_id(),
            old.upgrade().unwrap().entity_id()
        );
        assert!(window.try_find("commit-subject").is_some());
        window.click("commit-subject", cx);
        window.press("escape", cx); // not the configured dismiss binding
        window.render_frame(cx);
        assert!(window.try_find("commit-subject").is_some());
        window.press("alt-x", cx); // old controls still own their old callback
        window.render_frame(cx);
        assert!(window.try_find("commit-subject").is_none());
        assert_eq!(
            old.upgrade().unwrap().read(cx).draft().subject,
            "retained overlay"
        );
        window.click("action-commit", cx);
        window.render_frame(cx);
        window.click("commit-subject", cx);
        window.press("tab", cx);
        window.press("end", cx);
        window.input(" epsilon", cx);
    });
    fixture.config("git:\n  autoRefresh: false\n  autoDetectExternalChanges: false\n  commit:\n    autoWrapCommitMessage: true\n    autoWrapWidth: 12\n    signOff: true\nkeybinding:\n  universal:\n    confirmInEditor: '<ctrl+x>'\n    return: '<alt+y>'\n");
    visual.update(|_, cx| {
        shell.read(cx).repository.clone().update(cx, |owner, cx| {
            owner.reload_config();
            cx.notify();
        });
    });
    test_support::wait(visual, |cx| !shell.read(cx).repository.read(cx).loading());
    settled(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            old.upgrade().is_none(),
            "old overlay callbacks must release the replaced owner"
        );
        assert!(window.try_find("commit-subject").is_none());
        let commit = shell.read(cx).features.as_ref().unwrap().commit.read(cx);
        assert_eq!(commit.draft().subject, "retained overlay");
        assert_eq!(commit.draft().body, "alpha beta gamma delta epsilon");
        window.click("action-commit", cx);
        window.render_frame(cx);
        window.press("alt-x", cx); // removed dismiss binding must not close
        window.render_frame(cx);
        assert!(window.try_find("commit-subject").is_some());
        window.press("alt-y", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-subject").is_none());
        window.click("action-commit", cx);
        window.render_frame(cx);
        window.press("ctrl-x", cx); // new control callback targets the new owner
    });
    settled(visual, &shell);
    assert_eq!(fixture.run_git(&["log", "-1", "--format=%B"]), b"retained overlay\n\nalpha beta \ngamma delta \nepsilon\n\nSigned-off-by: Fixture <fixture@example.invalid>\n\n");
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(matches!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .commit
                .read(cx)
                .outcome(),
            Some(Outcome::Committed { .. })
        ));
        assert!(window.try_find("commit-subject").is_none());
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn configured_wrap_and_signoff_flow_through_real_commit_controls_and_object(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::installed();
    fixture.run_git(&["add", "a.txt"]);
    fixture.config("git:\n  autoRefresh: false\n  autoDetectExternalChanges: false\n  commit:\n    autoWrapCommitMessage: true\n    autoWrapWidth: 12\n    signOff: true\n");
    let (visual, shell) = test_support::open(cx, &fixture);
    settled(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("action-commit", cx);
        window.render_frame(cx);
        window.click("commit-subject", cx);
        window.input("configured message", cx);
        window.press("tab", cx);
        window.input("alpha beta gamma delta", cx);
        window.press("enter", cx); // explicit hard break survives wrapping
        window.input("epsilon", cx);
        window.press("ctrl-enter", cx);
    });
    settled(visual, &shell);
    assert_eq!(fixture.run_git(&["log", "-1", "--format=%B"]), b"configured message\n\nalpha beta \ngamma delta\nepsilon\n\nSigned-off-by: Fixture <fixture@example.invalid>\n\n");
    visual.update(|window, cx| {
        window.render_frame(cx);
        let f = shell.read(cx).features.as_ref().unwrap();
        assert!(matches!(
            f.commit.read(cx).outcome(),
            Some(Outcome::Committed { .. })
        ));
        assert_eq!(
            f.history.read(cx).commits()[0].subject,
            b"configured message"
        );
        assert!(window.try_find("commit-subject").is_none());
    });
    test_support::shutdown(visual, &fixture.host);
}
