use crate::views::app_shell::test_support::{self, Fixture};
use gpui_kit::{TestAppContext, test::TestWindowExt};
use lazygui::git::Side;

fn range_anchor_after_mutation(cx: &mut TestAppContext, side: Side) {
    for sticky in [false, true] {
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
        if side == Side::Index {
            fixture.run_git(&["add", "file"]);
        }
        let (visual, shell) = test_support::open(cx, &fixture);
        test_support::wait(visual, |cx| {
            shell
                .read(cx)
                .features
                .as_ref()
                .is_some_and(|f| !f.tree.read(cx).loading() && !f.history.read(cx).busy())
        });
        let tree = visual.update(|window, cx| {
            window.render_frame(cx);
            window.click(("file", 0usize), cx);
            window.press("enter", cx);
            let tree = shell.read(cx).features.as_ref().unwrap().tree.clone();
            assert_eq!(tree.read(cx).focused_side(), side);
            assert_eq!(
                tree.read(cx)
                    .pane(side)
                    .patch
                    .as_ref()
                    .unwrap()
                    .hunks()
                    .count(),
                3
            );
            window.press("down", cx);
            window.press("down", cx); // original third hunk, canonical ID 4
            if sticky {
                window.press("v", cx);
            }
            window.press("shift-up", cx); // anchor 4, cursor 3
            assert_eq!(tree.read(cx).pane(side).selection, [3, 4].into());
            // The owner is also an intention source (queued actions can target an
            // earlier hunk). Keep the view's anchor while consuming IDs before it.
            tree.update(cx, |owner, cx| {
                owner.act_selection(side, [0, 1].into());
                cx.notify();
            });
            tree
        });
        test_support::wait(visual, |cx| !tree.read(cx).busy());
        let expected = if side == Side::Worktree {
            original.replace("line 2\n", "changed 2\n")
        } else {
            changed.replace("changed 2\n", "line 2\n")
        };
        assert_eq!(fixture.run_git(&["show", ":file"]), expected.as_bytes());
        visual.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(tree.read(cx).pane(side).cursor, 0);
            window.press("shift-down", cx);
            // Anchor 4 must now mean ID 2, not stale ID 4 (or clamped ID 3).
            assert_eq!(tree.read(cx).pane(side).selection, [1, 2].into());
            // Reconcile a second write, proving the stored canonical advances.
            tree.update(cx, |owner, cx| {
                owner.act_selection(side, [0, 1].into());
                cx.notify();
            });
        });
        test_support::wait(visual, |cx| !tree.read(cx).busy());
        visual.update(|window, cx| {
            window.press("shift-down", cx);
            assert_eq!(tree.read(cx).pane(side).selection, [0, 1].into());
            window.press("space", cx); // now consume the anchor itself
        });
        test_support::wait(visual, |cx| !tree.read(cx).busy());
        assert_eq!(
            fixture.run_git(&["show", ":file"]),
            if side == Side::Worktree {
                changed.as_bytes()
            } else {
                original.as_bytes()
            }
        );
        assert_eq!(
            std::fs::read(fixture.options.cwd.join("file")).unwrap(),
            changed.as_bytes()
        );
        visual.update(|_, cx| assert!(tree.read(cx).error().is_none()));
        test_support::shutdown(visual, &fixture.host);
    }
}

#[gpui_kit::test]
fn worktree_range_anchor_remaps_after_each_completed_write(cx: &mut TestAppContext) {
    range_anchor_after_mutation(cx, Side::Worktree);
}

#[gpui_kit::test]
fn index_range_anchor_remaps_after_each_completed_write(cx: &mut TestAppContext) {
    range_anchor_after_mutation(cx, Side::Index);
}

#[gpui_kit::test]
fn exact_a_tab_v_and_shift_arrows_dispatch_real_partial_index_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::installed();
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
        window.click(("file", 0usize), cx);
        window.press("enter", cx);
        window.render_frame(cx);
        // Hunk mode highlights every effective change, not only the cursor.
        assert_eq!(
            window.find(("diff-unstaged", 6usize)).label(),
            Some("Selected: -old")
        );
        assert_eq!(
            window.find(("diff-unstaged", 7usize)).label(),
            Some("Selected: +new")
        );
        window.press("a", cx); // main.toggleSelectHunk, not an Enter substitute
        window.press("shift-down", cx);
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .pane(Side::Worktree)
                .selection
                .len(),
            2
        );
        window.press("shift-up", cx);
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .pane(Side::Worktree)
                .selection
                .len(),
            1
        );
        window.press("space", cx);
    });
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).busy())
    });
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nthree\n");
    assert_eq!(fixture.run_git(&["show", ":b.txt"]), b"one\nold\nthree\n");
    assert_eq!(
        std::fs::read(fixture.options.cwd.join("a.txt")).unwrap(),
        b"one\nnew\nthree\n"
    );
    visual.update(|window, cx| {
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert_eq!(tree.focused_side(), Side::Worktree);
        assert!(!tree.pane(Side::Worktree).is_empty());
        assert!(!tree.pane(Side::Index).is_empty());
        window.render_frame(cx);
        window.press("tab", cx); // stays in main and chooses the staged side
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
        window.press("space", cx);
    });
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).busy())
    });
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nold\nthree\n");
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.press("v", cx);
        window.press("down", cx);
        window.press("shift-up", cx);
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .pane(Side::Worktree)
                .selection
                .len(),
            1
        );
        window.press("shift-down", cx);
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .pane(Side::Worktree)
                .selection
                .len(),
            2
        );
        window.press("space", cx);
    });
    test_support::wait(visual, |cx| {
        shell
            .read(cx)
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).busy())
    });
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nnew\nthree\n");
    assert_eq!(fixture.run_git(&["show", ":b.txt"]), b"one\nold\nthree\n");
    test_support::shutdown(visual, &fixture.host);
}
