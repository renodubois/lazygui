use super::*;
use crate::views::app_shell::{
    AppShell,
    test_support::{self, Fixture},
};
use gpui_kit::{TestAppContext, VisualTestContext, test::TestWindowExt};

#[gpui_kit::test]
fn consumed_or_externally_changed_diff_anchor_is_cleared_not_clamped(cx: &mut TestAppContext) {
    let fixture = Fixture::installed_empty();
    let original: String = (0..45).map(|i| format!("line {i}\n")).collect();
    let mut changed = original.clone();
    for i in [2, 17, 32] {
        changed = changed.replace(&format!("line {i}\n"), &format!("changed {i}\n"));
    }
    fixture.write("file", original.as_bytes());
    fixture.run_git(&["add", "."]);
    fixture.run_git(&["commit", "-qm", "three hunks"]);
    fixture.write("file", changed.as_bytes());
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    let screen = visual.update(|window, cx| {
        let screen = test_support::screen(shell.read(cx));
        window.render_frame(cx);
        window.click(("file", 0usize), cx);
        window.press("enter", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("v", cx);
        window.press("shift-down", cx);
        assert_eq!(screen.read(cx).diff_anchor.as_ref().unwrap().id, 4);
        window.press("space", cx); // consumes the original anchor, leaves two hunks
        screen
    });
    test_support::wait(visual, |cx| {
        !screen
            .read(cx)
            .features
            .as_ref()
            .unwrap()
            .tree
            .read(cx)
            .busy()
    });
    visual.update(|window, cx| {
        assert!(screen.read(cx).diff_anchor.is_none());
        window.press("shift-down", cx);
        let anchor = screen.read(cx).diff_anchor.as_ref().unwrap();
        assert_eq!(anchor.id, 0);
        assert_eq!(anchor.path, OsString::from("file"));
        assert_eq!(anchor.side, Side::Worktree);
        assert_eq!(
            screen
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .pane(Side::Worktree)
                .selection,
            [0, 1].into()
        );
    });
    // Same path and same number of changes, but the original canonical identity
    // no longer exists. Numeric in-bounds IDs are not evidence of survival.
    fixture.write(
        "file",
        changed.replace("changed 2\n", "external 2\n").as_bytes(),
    );
    visual.update(|window, cx| window.click("action-refresh", cx));
    test_support::wait(visual, |cx| {
        !screen
            .read(cx)
            .features
            .as_ref()
            .unwrap()
            .tree
            .read(cx)
            .loading()
    });
    visual.update(|window, cx| {
        assert!(screen.read(cx).diff_anchor.is_none());
        window.press("shift-down", cx);
        assert!(screen.read(cx).diff_anchor.is_some());
    });
    // Removing the file's changes clears both its canonical identity and anchor.
    fixture.write("file", fixture.run_git(&["show", ":file"]).as_slice());
    visual.update(|window, cx| window.click("action-refresh", cx));
    test_support::wait(visual, |cx| {
        !screen
            .read(cx)
            .features
            .as_ref()
            .unwrap()
            .tree
            .read(cx)
            .loading()
    });
    visual.update(|_, cx| assert!(screen.read(cx).diff_anchor.is_none()));
    test_support::shutdown(visual, &fixture.host);
}

fn ready(visual: &mut VisualTestContext, shell: &Entity<AppShell>) {
    test_support::wait(visual, |cx| {
        let screen = test_support::screen(shell.read(cx));
        screen
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading() && !f.history.read(cx).busy())
    });
}

#[gpui_kit::test]
fn real_root_popup_text_and_help_suppress_underlying_navigation(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    visual.update(|window, cx| {
        let screen = test_support::screen(shell.read(cx));
        window.render_frame(cx);
        window.press("shift-/", cx);
        assert_eq!(screen.read(cx).popup, Some(Popup::Help));
        window.press("q", cx);
        window.press("2", cx);
        window.press("space", cx);
        assert_eq!(screen.read(cx).focus, Focus::Files);
        assert_eq!(fixture.git.state.lock().unwrap().writes, 0);
        window.press("escape", cx);
        window.press("ctrl-r", cx);
        window.render_frame(cx);
        window.input("qc repository", cx);
        window.press("j", cx);
        window.press("k", cx);
        assert_eq!(screen.read(cx).prompt.read(cx).text(), "qc repositoryjk");
        assert_eq!(screen.read(cx).focus, Focus::Files);
        window.press("escape", cx);
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn contextual_conflicts_and_help_click_labels_share_the_live_action_source(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    fixture.git.state.lock().unwrap().mixed = true;
    std::fs::write(fixture.temp.path().join("config.yml"), "git:\n  autoRefresh: false\n  autoDetectExternalChanges: false\nkeybinding:\n  universal:\n    nextItem: x\n    confirm: <ctrl+enter>\n    confirmMenu: y\n    return: <alt+x>\n  files:\n    commitChanges: x\n  main:\n    toggleSelectHunk: x\n").unwrap();
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    visual.update(|window, cx| {
        let screen = test_support::screen(shell.read(cx));
        window.render_frame(cx);
        assert_eq!(window.find("action-commit").label(), Some("Commit (x)"));
        assert_eq!(
            screen.read(cx).resolve(&Key::parse("x").unwrap().unwrap()),
            Some(Action::Commit)
        );
        assert!(screen.read(cx).navigation_help().contains("Commit: x"));
        assert!(!screen.read(cx).navigation_help().contains("Next item: x"));
        window.press("x", cx);
        window.render_frame(cx);
        assert_eq!(screen.read(cx).popup, Some(Popup::Commit));
        window.press("alt-x", cx);
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        let screen = test_support::screen(shell.read(cx));
        window.render_frame(cx);
        window.press("0", cx);
        window.press("x", cx);
        let tree = screen.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert_eq!(tree.selection_mode(Side::Worktree), SelectionMode::Line);
        assert!(
            screen
                .read(cx)
                .navigation_help()
                .contains("Toggle hunk / line: x")
        );
        assert!(!screen.read(cx).navigation_help().contains("Next item: x"));
        window.press("tab", cx);
        assert_eq!(screen.read(cx).focus, Focus::Diff);
        assert_eq!(
            screen
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .focused_side(),
            Side::Index
        );
        window.press("shift-/", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("popup-dismiss").label(),
            Some("Dismiss (Alt+x)")
        );
        window.press("escape", cx);
        assert_eq!(screen.read(cx).popup, Some(Popup::Help));
        window.press("alt-x", cx);
        assert!(screen.read(cx).popup.is_none());
        window.press("2", cx);
        window.press("ctrl-s", cx);
        assert_eq!(screen.read(cx).popup, Some(Popup::StatusFilter));
        window.press("y", cx);
        assert!(screen.read(cx).popup.is_none());
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn marked_popup_input_cannot_confirm_dismiss_or_run_repository_keys(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    visual.update(|window, cx| {
        let screen = test_support::screen(shell.read(cx));
        window.render_frame(cx);
        window.press("ctrl-r", cx);
        window.render_frame(cx);
        let prompt = screen.read(cx).prompt.clone();
        prompt.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "composition", Some(0..11), window, cx);
        });
        window.press("enter", cx);
        assert_eq!(screen.read(cx).popup, Some(Popup::Path));
        prompt.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "composition", Some(0..11), window, cx);
        });
        window.press("escape", cx);
        assert_eq!(screen.read(cx).popup, Some(Popup::Path));
        assert_eq!(screen.read(cx).focus, Focus::Files);
        assert_eq!(fixture.git.state.lock().unwrap().writes, 0);
        prompt.update(cx, |input, cx| {
            input.replace_text_in_range(None, "", window, cx)
        });
        window.press("escape", cx);
        assert!(screen.read(cx).popup.is_none());
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn directory_keyboard_rows_enter_expands_file_enter_opens_diff_escape_returns(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    fixture.git.state.lock().unwrap().nested = true;
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    visual.update(|window, cx| {
        let screen = test_support::screen(shell.read(cx));
        window.render_frame(cx);
        window.click(("file", 0usize), cx);
        window.press("up", cx);
        assert_eq!(
            screen.read(cx).directory_cursor.as_deref(),
            Some(std::path::Path::new("dir"))
        );
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.try_find(("file", 0usize)).is_none());
        window.press("space", cx);
        window.press("0", cx);
        window.press("space", cx);
        assert_eq!(fixture.git.state.lock().unwrap().writes, 0);
        window.press("escape", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.try_find(("file", 0usize)).is_some());
        window.press("down", cx);
        window.press("enter", cx);
        assert_eq!(screen.read(cx).focus, Focus::Diff);
        window.press("escape", cx);
        assert_eq!(screen.read(cx).focus, Focus::Files);
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn text_filter_hides_range_targets_without_writes_and_new_range_is_visible_only(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::installed();
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    visual.update(|window, cx| {
        let screen = test_support::screen(shell.read(cx));
        window.render_frame(cx);
        window.click(("file", 0usize), cx);
        window.press("shift-down", cx);
        assert_eq!(screen.read(cx).selected_files.len(), 2);
        window.press("/", cx);
        window.render_frame(cx);
        window.input("missing", cx);
        window.press("enter", cx);
        window.press("space", cx);
        assert!(screen.read(cx).selected_files.is_empty());
        assert!(screen.read(cx).file_anchor.is_none());
    });
    ready(visual, &shell);
    assert_eq!(fixture.run_git(&["diff", "--cached"]), b"");
    visual.update(|window, cx| {
        let screen = test_support::screen(shell.read(cx));
        window.render_frame(cx);
        window.press("/", cx);
        window.render_frame(cx);
        window.press("ctrl-a", cx);
        window.input("a.txt", cx);
        window.press("enter", cx);
        window.press("shift-down", cx);
        assert_eq!(
            screen.read(cx).selected_files,
            [OsString::from("a.txt")].into_iter().collect()
        );
        window.press("space", cx);
    });
    test_support::wait(visual, |cx| {
        let screen = test_support::screen(shell.read(cx));
        !screen
            .read(cx)
            .features
            .as_ref()
            .unwrap()
            .tree
            .read(cx)
            .busy()
    });
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nnew\nthree\n");
    assert_eq!(fixture.run_git(&["show", ":b.txt"]), b"one\nold\nthree\n");
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn exact_status_filter_and_unavailable_tabs_do_not_fabricate_rows(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.git.state.lock().unwrap().mixed = true;
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    visual.update(|window, cx| {
        let screen = test_support::screen(shell.read(cx));
        window.render_frame(cx);
        window.press("ctrl-s", cx);
        window.render_frame(cx);
        assert_eq!(screen.read(cx).popup, Some(Popup::StatusFilter));
        window.press("down", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(screen.read(cx).status_filter, StatusFilter::Staged);
        assert!(window.try_find(("file", 1usize)).is_none());
        window.press("3", cx);
        window.press("]", cx);
        window.render_frame(cx);
        assert!(window.try_find("current-branch").is_none());
        assert_eq!(
            window.find(("tabs-2", 1usize)).label(),
            Some("Remotes (unavailable)")
        );
        window.press("]", cx);
        window.render_frame(cx);
        assert!(window.try_find("current-branch").is_none());
        assert_eq!(
            window.find(("tabs-2", 2usize)).label(),
            Some("Tags (unavailable)")
        );
        window.press("4", cx);
        window.press("]", cx);
        window.render_frame(cx);
        assert!(window.try_find(("commit", 0usize)).is_none());
        window.press("2", cx);
        window.press("]", cx);
        window.render_frame(cx);
        assert!(window.try_find(("file", 0usize)).is_none());
        window.press("space", cx);
        assert_eq!(fixture.git.state.lock().unwrap().writes, 0);
    });
    test_support::shutdown(visual, &fixture.host);
}
