use crate::views::app_shell::test_support::{self, Fixture};
use gpui_kit::{TestAppContext, test::TestWindowExt};
use lazygui::git::{Side, Whitespace};

#[gpui_kit::test]
fn canonical_stacked_diff_tree_filter_ranges_clipboard_and_options_use_real_root(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    {
        let mut state = fixture.git.state.lock().unwrap();
        state.nested = true;
        state.mixed = true;
    }
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
        assert!(window.try_find("diff-unstaged").is_some());
        assert!(window.try_find("diff-index").is_some());
        assert!(window.try_find("file-tree-root").is_some());
        window.click(("directory", 1usize), cx);
        window.render_frame(cx);
        assert!(window.try_find(("file", 0usize)).is_none());
        window.click("action-tree", cx);
        window.render_frame(cx);
        assert_eq!(window.find(("file", 0usize)).label(), Some("MM dir/a.txt"));
        window.click(("file", 0usize), cx);
        window.press("shift-down", cx);
        window.click("action-copy", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "dir/b.txt"
        );
        window.click("action-layout", cx);
        window.click("action-filter", cx);
        window.render_frame(cx);
        window.click("repository-prompt", cx);
        window.input("dir/a", cx);
        window.press("enter", cx);
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("file", 1usize)).is_none());
        window.click(("file", 0usize), cx);
        window.press("0", cx);
        window.press("]", cx);
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .focused_side(),
            Side::Index
        );
        window.click("action-whitespace", cx);
        window.click("action-context-up", cx);
    });
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading() && f.tree.read(cx).options().context == 4)
    });
    visual.update(|window, cx| {
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert_eq!(tree.options().whitespace, Whitespace::IgnoreAllSpace);
        assert!(!tree.pane(Side::Index).partial_enabled());
        window.render_frame(cx);
        assert!(window.try_find("diff-unstaged").is_some());
        assert!(window.try_find("diff-index").is_some());
    });
    assert_eq!(fixture.git.state.lock().unwrap().writes, 0);
    test_support::shutdown(visual, &fixture.host);
}
#[gpui_kit::test]
fn configured_quit_confirmation_is_exclusive_and_dismissible(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.temp.path().join("config.yml"),
        "confirmOnQuit: true\ngit:\n  autoRefresh: false\n  autoDetectExternalChanges: false\n",
    )
    .unwrap();
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
        window.press("shift-q", cx); // quitWithoutChangingDirectory uses the same safe close path
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("quit-confirm").is_some());
        assert!(shell.read(cx).closing.is_none());
        window.press("space", cx);
        window.press("c", cx);
        assert_eq!(fixture.git.state.lock().unwrap().writes, 0);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("quit-confirm").is_none());
    });
    test_support::shutdown(visual, &fixture.host);
}
