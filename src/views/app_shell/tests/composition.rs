use super::{
    AppShell, RepositoryView,
    test_support::{self, Fixture},
};
use gpui_kit::{AppContext, TestAppContext, test::TestWindowExt};
use std::{sync::atomic::Ordering, time::Duration};

#[gpui_kit::test]
fn recreating_screen_keeps_retained_owners_and_single_pending_result_consumer(
    cx: &mut TestAppContext,
) {
    let mut fixture = Fixture::installed();
    let held = fixture.hold("diff");
    let (visual, shell) = test_support::open(cx, &fixture);
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading() && !f.history.read(cx).busy())
    });
    let owners = visual.update(|_, cx| {
        let f = shell.read(cx).features.as_ref().unwrap();
        (
            f.tree.entity_id(),
            f.history.entity_id(),
            f.commit.entity_id(),
        )
    });
    held.armed.store(true, Ordering::Release);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("action-refresh", cx);
    });
    held.started.recv_timeout(Duration::from_secs(5)).unwrap();
    test_support::wait(visual, |cx| {
        !shell
            .read(cx)
            .features
            .as_ref()
            .unwrap()
            .history
            .read(cx)
            .busy()
    });
    let commands = fixture
        .installed_git
        .as_ref()
        .unwrap()
        .commands
        .lock()
        .unwrap()
        .len();
    visual.update(|window, cx| {
        shell.update(cx, |host: &mut AppShell, cx| {
            let repository = host.repository.clone();
            let features = host.features.as_ref().unwrap();
            let tree = features.tree.clone();
            let history = features.history.clone();
            let commit = features.commit.clone();
            let settings = repository.read(cx).session().unwrap().settings.m1().clone();
            let screen = cx.new(|cx| RepositoryView::new(repository, window, cx, |_, _, _| {}));
            screen.update(cx, |view, cx| {
                view.set_features(tree, history, commit, settings, cx)
            });
            host.screen = screen;
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find(("file", 0usize)).is_some());
        let f = shell.read(cx).features.as_ref().unwrap();
        assert_eq!(
            owners,
            (
                f.tree.entity_id(),
                f.history.entity_id(),
                f.commit.entity_id()
            )
        );
        assert!(f.tree.read(cx).loading());
    });
    assert_eq!(
        commands,
        fixture
            .installed_git
            .as_ref()
            .unwrap()
            .commands
            .lock()
            .unwrap()
            .len()
    );
    held.release.send(()).unwrap();
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).loading() && !f.history.read(cx).busy())
    });
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click(("file", 0usize), cx);
        window.press("space", cx);
    });
    test_support::wait(visual, |cx| {
        !shell
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
