//! Connected M1 acceptance: production Root/shell/owners, installed Git, fake GUI time.
use super::{
    AppShell,
    test_support::{self, Fixture},
};
use gpui_kit::{App, Entity, TestAppContext, VisualTestContext, test::TestWindowExt};
use lazygui::{
    commit::Outcome,
    git::{Head, Side},
};
use std::{
    ffi::{OsStr, OsString},
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Duration,
};

fn ready(visual: &mut VisualTestContext, shell: &Entity<AppShell>) {
    test_support::wait(visual, |cx| idle(shell, cx));
}
fn idle(shell: &Entity<AppShell>, cx: &App) -> bool {
    !shell.read(cx).repository.read(cx).loading()
        && shell.read(cx).features.as_ref().is_some_and(|f| {
            !f.tree.read(cx).loading()
                && !f.tree.read(cx).busy()
                && !f.history.read(cx).busy()
                && !f.commit.read(cx).busy()
        })
}
fn mutations(fixture: &Fixture) -> Vec<String> {
    fixture
        .installed_git
        .as_ref()
        .unwrap()
        .commands
        .lock()
        .unwrap()
        .iter()
        .filter_map(|args| {
            args.iter()
                .find(|arg| {
                    ["add", "reset", "rm", "apply", "commit"]
                        .iter()
                        .any(|verb| *arg == verb)
                })
                .map(|arg| arg.to_string_lossy().into_owned())
        })
        .collect()
}
fn clear_commands(fixture: &Fixture) {
    fixture
        .installed_git
        .as_ref()
        .unwrap()
        .commands
        .lock()
        .unwrap()
        .clear();
}

#[gpui_kit::test]
fn clean_unborn_detached_bare_linked_and_nonrepo_open_through_retained_shell(
    cx: &mut TestAppContext,
) {
    for kind in ["clean", "unborn", "detached", "linked", "nonrepo", "bare"] {
        let mut fixture = if matches!(kind, "unborn" | "bare" | "nonrepo") {
            Fixture::installed_empty()
        } else {
            Fixture::installed()
        };
        if matches!(kind, "clean" | "detached" | "linked") {
            fixture.run_git(&["reset", "--hard", "-q"]);
        }
        if kind == "detached" {
            fixture.run_git(&["checkout", "--detach", "-q"]);
        }
        if matches!(kind, "bare" | "nonrepo") {
            std::fs::remove_dir_all(fixture.options.cwd.join(".git")).unwrap();
            if kind == "bare" {
                fixture.run_git(&["init", "--bare", "-q", "-b", "main"]);
            }
        }
        let original = fixture.options.cwd.clone();
        if kind == "linked" {
            let linked = fixture.temp.path().join("linked");
            fixture.run_git(&[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked",
                linked.to_str().unwrap(),
            ]);
            fixture.options.cwd = linked;
        }
        clear_commands(&fixture);
        let (visual, shell) = test_support::open(cx, &fixture);
        if kind == "nonrepo" {
            test_support::wait(visual, |cx| !shell.read(cx).repository.read(cx).loading());
        } else {
            ready(visual, &shell);
        }
        visual.update(|window, cx| {
            window.render_frame(cx);
            let owner = shell.read(cx).repository.read(cx);
            if kind == "nonrepo" {
                assert!(owner.session().is_none());
                assert!(owner.error().unwrap().contains("Cannot open repository"));
                assert!(shell.read(cx).features.is_none());
                assert!(
                    window
                        .find("repository-status")
                        .label()
                        .unwrap()
                        .contains("Cannot open repository")
                );
            } else {
                let session = owner.session().unwrap();
                let identity = &session.readiness.identity;
                assert_eq!(
                    identity.worktree.as_ref(),
                    (kind != "bare").then_some(&fixture.options.cwd)
                );
                if kind == "linked" {
                    assert_eq!(identity.common_dir, original.join(".git"));
                    assert_ne!(identity.git_dir, identity.common_dir);
                    assert!(
                        identity
                            .git_dir
                            .starts_with(original.join(".git/worktrees"))
                    );
                    assert_eq!(window.find("current-branch").label(), Some("linked"));
                } else {
                    assert_eq!(
                        identity.git_dir,
                        if kind == "bare" {
                            original.clone()
                        } else {
                            original.join(".git")
                        }
                    );
                    assert_eq!(identity.common_dir, identity.git_dir);
                    match (&session.readiness.head, kind) {
                        (Head::Unborn { branch }, "unborn" | "bare") => assert_eq!(branch, "main"),
                        (Head::Detached { oid }, "detached") => assert!(
                            window
                                .find("current-branch")
                                .label()
                                .unwrap()
                                .starts_with(&format!("Detached {}", &oid[..8]))
                        ),
                        (Head::Branch { branch, .. }, "clean") => assert_eq!(branch, "main"),
                        other => panic!("unexpected readiness {other:?}"),
                    }
                }
                let f = shell.read(cx).features.as_ref().unwrap();
                assert!(f.tree.read(cx).entries().is_empty());

                assert_eq!(
                    f.history.read(cx).commits().len(),
                    usize::from(!matches!(kind, "unborn" | "bare"))
                );
                assert_eq!(window.find("diff-empty").label(), Some("No diff data"));
                assert!(window.try_find(("file", 0usize)).is_none());
                if kind == "bare" {
                    assert!(
                        window
                            .find("repository-status")
                            .label()
                            .unwrap()
                            .ends_with("(bare: read-only)")
                    );
                }
            }
            window.press("2", cx);
            window.press("space", cx);
            if matches!(kind, "bare" | "nonrepo") {
                window.click("action-commit", cx);
                window.render_frame(cx);
                assert!(window.try_find("commit-subject").is_none());
            }
            assert!(mutations(&fixture).is_empty(), "{kind} dispatched a write");
            if kind == "bare" {
                assert_eq!(
                    window.find("operation-feedback").label(),
                    Some("Bare repository: files, staging and commits are unavailable.")
                );
            }
            if let Some(f) = &shell.read(cx).features {
                assert!(
                    f.tree.read(cx).error().is_none(),
                    "{kind}: {:?}",
                    f.tree.read(cx).error()
                );
            }
        });
        assert!(mutations(&fixture).is_empty(), "{kind} dispatched a write");
        test_support::shutdown(visual, &fixture.host);
    }
}

#[gpui_kit::test]
fn files_two_jk_directory_enter_zero_escape_and_contextual_controls(cx: &mut TestAppContext) {
    let fixture = Fixture::installed();
    fixture.run_git(&["reset", "--hard", "-q"]);
    fixture.write("dir/a.txt", b"a\n");
    fixture.write("dir/b.txt", b"b\n");
    clear_commands(&fixture);
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.press("2", cx);
        assert!(
            window
                .find("action-commit")
                .label()
                .unwrap()
                .contains("(c)")
        );
        window.press("k", cx); // directory row
        window.press("enter", cx); // collapse, not diff
        window.render_frame(cx);
        assert!(window.try_find(("file", 0usize)).is_none());
        window.press("enter", cx); // expand
        window.press("j", cx); // first file
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .selected(),
            Some(OsStr::new("dir/a.txt"))
        );
        window.press("j", cx);
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .selected(),
            Some(OsStr::new("dir/b.txt"))
        );
        window.press("k", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.find("action-commit").label().unwrap().eq("Commit"));
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            window
                .find("action-commit")
                .label()
                .unwrap()
                .contains("(c)")
        );
        window.press("0", cx);
        window.render_frame(cx);
        assert!(window.find("action-commit").label().unwrap().eq("Commit"));
        window.press("escape", cx);
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .selected(),
            Some(OsStr::new("dir/a.txt"))
        );
    });
    assert!(mutations(&fixture).is_empty());
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn byte_paths_stage_and_unstage_by_connected_file_click_without_loss(cx: &mut TestAppContext) {
    for path in [
        OsString::from("space name"),
        OsString::from("tab\tname"),
        OsString::from("newline\nname"),
        OsString::from("-leading"),
        OsString::from_vec(b"raw-\xff".to_vec()),
    ] {
        let fixture = Fixture::installed_empty();
        fixture.write(&path, b"old\n");
        fixture.run_git(&["add", "."]);
        fixture.run_git(&["commit", "-qm", "base"]);
        fixture.write(&path, b"new\n");
        clear_commands(&fixture);
        let (visual, shell) = test_support::open(cx, &fixture);
        ready(visual, &shell);
        visual.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                shell
                    .read(cx)
                    .features
                    .as_ref()
                    .unwrap()
                    .tree
                    .read(cx)
                    .entries()[0]
                    .path,
                path
            );
            window.click(("file", 0usize), cx);
            window.click("action-stage", cx);
        });
        ready(visual, &shell);
        let mut index_path = OsString::from(":");
        index_path.push(&path);
        assert_eq!(
            fixture.run_git_os(&[OsStr::new("show"), &index_path]),
            b"new\n"
        );
        visual.update(|window, cx| {
            window.render_frame(cx);
            let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
            assert_eq!(tree.selected(), Some(path.as_os_str()));
            assert_eq!(tree.focused_side(), Side::Index);
            assert!(window.try_find("diff-unstaged").is_none());
            window.press("space", cx);
        });
        ready(visual, &shell);
        assert_eq!(
            fixture.run_git_os(&[OsStr::new("show"), &index_path]),
            b"old\n"
        );
        assert_eq!(
            std::fs::read(fixture.options.cwd.join(&path)).unwrap(),
            b"new\n"
        );
        visual.update(|window, cx| {
            window.render_frame(cx);
            let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
            assert_eq!(tree.focused_side(), Side::Worktree);
            assert_eq!(tree.entries()[0].path, path);
            assert!(window.try_find("diff-index").is_none());
            assert!(tree.error().is_none());
        });
        assert_eq!(mutations(&fixture), ["add", "reset"]);
        test_support::shutdown(visual, &fixture.host);
    }
}

#[gpui_kit::test]
fn new_deleted_binary_symlink_and_mode_only_fallbacks_keep_real_index_and_worktree(
    cx: &mut TestAppContext,
) {
    for kind in ["new", "deleted", "binary", "symlink", "mode"] {
        let fixture = Fixture::installed_empty();
        fixture.write("keep", b"unchanged\n");
        if kind != "new" {
            if kind == "symlink" {
                std::os::unix::fs::symlink("keep", fixture.options.cwd.join("target")).unwrap();
            } else {
                fixture.write(
                    "target",
                    if kind == "binary" {
                        b"old\0binary"
                    } else {
                        b"old\n"
                    },
                );
            }
        }
        fixture.run_git(&["add", "."]);
        fixture.run_git(&["commit", "-qm", "base"]);
        match kind {
            "new" => fixture.write("target", b"new\n"),
            "deleted" => std::fs::remove_file(fixture.options.cwd.join("target")).unwrap(),
            "binary" => fixture.write("target", b"new\0binary"),
            "symlink" => {
                std::fs::remove_file(fixture.options.cwd.join("target")).unwrap();
                std::os::unix::fs::symlink("missing", fixture.options.cwd.join("target")).unwrap();
            }
            "mode" => std::fs::set_permissions(
                fixture.options.cwd.join("target"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        let before = fixture.run_git(&["ls-files", "--stage", "-z"]);
        let (visual, shell) = test_support::open(cx, &fixture);
        ready(visual, &shell);
        clear_commands(&fixture);
        visual.update(|window, cx| {
            window.render_frame(cx);
            window.click(("file", 0usize), cx);
            if matches!(kind, "binary" | "symlink" | "mode") {
                let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
                assert!(!tree.pane(Side::Worktree).partial_enabled());
                assert!(tree.pane(Side::Worktree).disabled_reason.is_some());
                window.press("enter", cx);
                window.press("space", cx); // disabled partial action must not fall through
                assert!(mutations(&fixture).is_empty());
                assert!(
                    shell
                        .read(cx)
                        .features
                        .as_ref()
                        .unwrap()
                        .tree
                        .read(cx)
                        .error()
                        .is_some()
                );
                window.press("escape", cx);
            }
            window.click("action-stage", cx);
        });
        ready(visual, &shell);
        let staged = fixture.run_git(&["ls-files", "--stage", "-z"]);
        assert_ne!(staged, before);
        if kind == "deleted" {
            assert!(!staged.windows(6).any(|s| s == b"target"));
        } else {
            assert_eq!(
                fixture.run_git(&["show", ":target"]),
                match kind {
                    "binary" => b"new\0binary".as_slice(),
                    "symlink" => b"missing",
                    "mode" => b"old\n",
                    _ => b"new\n",
                }
            );
            let text = String::from_utf8_lossy(&staged);
            if kind == "symlink" {
                assert!(text.contains("120000"));
            }
            if kind == "mode" {
                assert!(text.contains("100755"));
            }
        }
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
                    .focused_side(),
                Side::Index
            );
            window.press("space", cx);
        });
        ready(visual, &shell);
        assert_eq!(fixture.run_git(&["ls-files", "--stage", "-z"]), before);
        visual.update(|window, cx| {
            window.render_frame(cx);
            let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
            assert_eq!(tree.focused_side(), Side::Worktree);
            assert!(window.try_find("diff-index").is_none());
            assert!(tree.error().is_none());
            assert_eq!(
                (tree.entries()[0].index, tree.entries()[0].worktree),
                if kind == "new" {
                    (b'?', b'?')
                } else if kind == "deleted" {
                    (b' ', b'D')
                } else {
                    (b' ', b'M')
                }
            );
        });
        match kind {
            "deleted" => assert!(!fixture.options.cwd.join("target").exists()),
            "symlink" => assert_eq!(
                std::fs::read_link(fixture.options.cwd.join("target")).unwrap(),
                std::path::Path::new("missing")
            ),
            "mode" => assert_ne!(
                std::fs::metadata(fixture.options.cwd.join("target"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0
            ),
            _ => assert_eq!(
                std::fs::read(fixture.options.cwd.join("target")).unwrap(),
                if kind == "binary" {
                    b"new\0binary".as_slice()
                } else {
                    b"new\n"
                }
            ),
        }
        assert_eq!(mutations(&fixture), ["add", "reset"]);
        test_support::shutdown(visual, &fixture.host);
    }
}

#[gpui_kit::test]
fn staged_rename_whole_file_unstage_and_restage_never_dispatches_text_patch(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::installed_empty();
    fixture.write("old name", b"one\ntwo\n");
    fixture.run_git(&["add", "."]);
    fixture.run_git(&["commit", "-qm", "base"]);
    fixture.run_git(&["mv", "old name", "new name"]);
    let staged = fixture.run_git(&["ls-files", "--stage", "-z"]);
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    clear_commands(&fixture);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click(("file", 0usize), cx);
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert_eq!(tree.entries()[0].path, "new name");
        assert_eq!(
            tree.entries()[0].original.as_deref(),
            Some(OsStr::new("old name"))
        );
        assert_eq!(tree.focused_side(), Side::Index);
        assert!(!tree.pane(Side::Index).partial_enabled());
        window.press("0", cx);
        window.press("space", cx);
        assert!(mutations(&fixture).is_empty());
        window.press("escape", cx);
        window.press("space", cx);
    });
    ready(visual, &shell);
    assert_eq!(fixture.run_git(&["show", ":old name"]), b"one\ntwo\n");
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.press("v", cx);
        window.press("shift-down", cx); // old deletion + untracked destination
        window.press("space", cx);
    });
    ready(visual, &shell);
    assert_eq!(fixture.run_git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(
        std::fs::read(fixture.options.cwd.join("new name")).unwrap(),
        b"one\ntwo\n"
    );
    assert!(!fixture.options.cwd.join("old name").exists());
    assert!(!mutations(&fixture).iter().any(|m| m == "apply"));
    visual.update(|_, cx| {
        assert!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .error()
                .is_none()
        )
    });
    test_support::shutdown(visual, &fixture.host);
}

/// Holds only a transport call, not any production workflow. Used to force a queued key.
struct Hold {
    inner: Arc<dyn lazygui::git::process::Executor>,
    verb: &'static str,
    armed: AtomicBool,
    started: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl lazygui::git::process::Executor for Hold {
    fn execute(
        &self,
        command: std::process::Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> std::io::Result<lazygui::git::process::Output> {
        if command.get_args().any(|a| a == self.verb)
            && self.armed.swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            self.started.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
        }
        self.inner.execute(command, input, cancel)
    }
}

#[gpui_kit::test]
fn queued_rapid_space_space_stages_distinct_fresh_hunks_then_last_focus_follows_index(
    cx: &mut TestAppContext,
) {
    let mut fixture = Fixture::installed_empty();
    let old = (1..=24).map(|n| format!("line {n}\n")).collect::<String>();
    let new = old
        .replace("line 2\n", "changed 2\n")
        .replace("line 22\n", "changed 22\n");
    fixture.write("target", old.as_bytes());
    fixture.run_git(&["add", "."]);
    fixture.run_git(&["commit", "-qm", "base"]);
    fixture.write("target", new.as_bytes());
    let (started, receive) = std::sync::mpsc::channel();
    let (release, held) = std::sync::mpsc::channel();
    fixture.executor = Some(Arc::new(Hold {
        inner: fixture.executor.clone().unwrap(),
        verb: "apply",
        armed: AtomicBool::new(true),
        started,
        release: Mutex::new(held),
    }));
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    clear_commands(&fixture);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.press("2", cx);
        window.press("enter", cx);
        assert_eq!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .tree
                .read(cx)
                .pane(Side::Worktree)
                .patch
                .as_ref()
                .unwrap()
                .hunks()
                .count(),
            2
        );
        window.press("space", cx);
        window.press("space", cx);
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
    });
    receive.recv_timeout(Duration::from_secs(2)).unwrap();
    release.send(()).unwrap();
    ready(visual, &shell);
    assert_eq!(fixture.run_git(&["show", ":target"]), new.as_bytes());
    assert_eq!(mutations(&fixture), ["apply", "apply"]);
    visual.update(|window, cx| {
        window.render_frame(cx);
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert!(tree.error().is_none());
        assert!(tree.pane(Side::Worktree).is_empty());
        assert_eq!(tree.focused_side(), Side::Index);
        assert!(window.try_find("diff-unstaged").is_none());
        assert!(window.try_find("diff-index").is_some());
        window.press("space", cx);
        window.press("space", cx);
    });
    ready(visual, &shell);
    assert_eq!(fixture.run_git(&["show", ":target"]), old.as_bytes());
    assert_eq!(
        std::fs::read(fixture.options.cwd.join("target")).unwrap(),
        new.as_bytes()
    );
    visual.update(|window, cx| {
        window.render_frame(cx);
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert_eq!(tree.focused_side(), Side::Worktree);
        assert!(tree.error().is_none());
        assert!(window.try_find("diff-index").is_none());
    });
    assert_eq!(mutations(&fixture), ["apply", "apply", "apply", "apply"]);
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn held_space_down_space_and_range_preserve_three_hunk_targets_on_both_sides(
    cx: &mut TestAppContext,
) {
    use std::sync::atomic::Ordering;
    for side in [Side::Worktree, Side::Index] {
        for range in [false, true] {
            let mut fixture = Fixture::installed_empty();
            let old = (1..=36).map(|n| format!("line {n}\n")).collect::<String>();
            let new = old
                .replace("line 2\n", "changed 2\n")
                .replace("line 18\n", "changed 18\n")
                .replace("line 34\n", "changed 34\n");
            fixture.write("target", old.as_bytes());
            fixture.run_git(&["add", "."]);
            fixture.run_git(&["commit", "-qm", "base"]);
            fixture.write("target", new.as_bytes());
            if side == Side::Index {
                fixture.run_git(&["add", "target"]);
            }
            let held = fixture.hold("apply");
            let (visual, shell) = test_support::open(cx, &fixture);
            ready(visual, &shell);
            clear_commands(&fixture);
            held.armed.store(true, Ordering::Release);
            visual.update(|window, cx| {
                window.render_frame(cx);
                window.click(("file", 0usize), cx);
                window.press("enter", cx);
                let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
                assert_eq!(tree.focused_side(), side);
                assert_eq!(tree.pane(side).patch.as_ref().unwrap().hunks().count(), 3);
                window.press("space", cx);
            });
            held.started.recv_timeout(Duration::from_secs(5)).unwrap();
            visual.update(|window, cx| {
                window.press("down", cx); // middle hunk in the still-visible original snapshot
                if range {
                    window.press("v", cx);
                    window.press("down", cx);
                    window.press("down", cx);
                    window.press("down", cx); // middle + last hunk, not old ordinal after refresh
                }
                let pane = shell
                    .read(cx)
                    .features
                    .as_ref()
                    .unwrap()
                    .tree
                    .read(cx)
                    .pane(side);
                assert_eq!(
                    pane.current_selection(),
                    if range {
                        (2..6).collect()
                    } else {
                        (2..4).collect()
                    }
                );
                window.press("space", cx);
            });
            held.release.send(()).unwrap();
            ready(visual, &shell);
            let expected = match (side, range) {
                (Side::Worktree, true) => new.clone(),
                (Side::Index, true) => old.clone(),
                (Side::Worktree, false) => old
                    .replace("line 2\n", "changed 2\n")
                    .replace("line 18\n", "changed 18\n"),
                (Side::Index, false) => old.replace("line 34\n", "changed 34\n"),
            };
            assert_eq!(
                fixture.run_git(&["show", ":target"]),
                expected.as_bytes(),
                "side={side:?}, range={range}: queued target must not skip the middle hunk"
            );
            assert_eq!(
                std::fs::read(fixture.options.cwd.join("target")).unwrap(),
                new.as_bytes()
            );
            assert_eq!(mutations(&fixture), ["apply", "apply"]);
            visual.update(|window, cx| {
                window.render_frame(cx);
                let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
                assert!(
                    tree.error().is_none(),
                    "side={side:?}, range={range}: {:?}",
                    tree.error()
                );
                if range {
                    assert!(tree.pane(side).is_empty());
                    assert_ne!(tree.focused_side(), side);
                } else {
                    assert_eq!(tree.pane(side).patch.as_ref().unwrap().hunks().count(), 1);
                    assert_eq!(tree.focused_side(), side);
                }
            });
            test_support::shutdown(visual, &fixture.host);
        }
    }
}

#[gpui_kit::test]
fn held_refresh_rejects_queued_whole_file_space_after_external_replacement(
    cx: &mut TestAppContext,
) {
    use std::sync::atomic::Ordering;
    for side in [Side::Worktree, Side::Index] {
        let mut fixture = Fixture::installed();
        if side == Side::Index {
            fixture.run_git(&["add", "a.txt"]);
        }
        let refresh = fixture.hold("status");
        let (visual, shell) = test_support::open(cx, &fixture);
        ready(visual, &shell);
        visual.update(|window, cx| {
            window.render_frame(cx);
            window.click(("file", 0usize), cx);
            assert_eq!(
                shell
                    .read(cx)
                    .features
                    .as_ref()
                    .unwrap()
                    .tree
                    .read(cx)
                    .focused_side(),
                side
            );
        });
        refresh.armed.store(true, Ordering::Release);
        visual.update(|window, cx| window.click("action-refresh", cx));
        refresh
            .started
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        visual.update(|window, cx| {
            window.press("2", cx);
            window.press("space", cx); // queue the visible pre-refresh canonical target
            assert!(
                shell
                    .read(cx)
                    .features
                    .as_ref()
                    .unwrap()
                    .tree
                    .read(cx)
                    .loading()
            );
        });
        fixture.write("a.txt", b"one\nexternal replacement\nthree\n");
        if side == Side::Index {
            fixture.run_git(&["add", "a.txt"]);
        }
        clear_commands(&fixture);
        refresh.release.send(()).unwrap();
        ready(visual, &shell);
        assert!(
            mutations(&fixture).is_empty(),
            "stale {side:?} target must not write"
        );
        assert_eq!(
            fixture.run_git(&["show", ":a.txt"]),
            if side == Side::Index {
                b"one\nexternal replacement\nthree\n".as_slice()
            } else {
                b"one\nold\nthree\n".as_slice()
            }
        );
        assert_eq!(fixture.run_git(&["show", ":b.txt"]), b"one\nold\nthree\n");
        assert_eq!(
            std::fs::read(fixture.options.cwd.join("a.txt")).unwrap(),
            b"one\nexternal replacement\nthree\n"
        );
        visual.update(|window, cx| {
            window.render_frame(cx);
            let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
            assert!(
                tree.error().is_some(),
                "stale whole-file target must report rejection"
            );
            assert!(
                tree.pane(side)
                    .canonical
                    .windows(b"external replacement".len())
                    .any(|bytes| bytes == b"external replacement")
            );
        });
        test_support::shutdown(visual, &fixture.host);
    }
}

#[gpui_kit::test]
fn files_space_mixed_range_stages_pending_members_then_all_staged_range_unstages(
    cx: &mut TestAppContext,
) {
    for reverse in [false, true] {
        let fixture = Fixture::installed();
        fixture.run_git(&["add", "a.txt"]); // staged-only a, unstaged-only b
        fixture.write("untouched.txt", b"untracked remains\n");
        let (visual, shell) = test_support::open(cx, &fixture);
        ready(visual, &shell);
        clear_commands(&fixture);
        visual.update(|window, cx| {
            window.render_frame(cx);
            window.click(("file", usize::from(reverse)), cx);
            window.press(if reverse { "shift-up" } else { "shift-down" }, cx);
            assert_eq!(
                shell
                    .read(cx)
                    .features
                    .as_ref()
                    .unwrap()
                    .tree
                    .read(cx)
                    .selected(),
                Some(OsStr::new(if reverse { "a.txt" } else { "b.txt" }))
            );
            window.press("space", cx);
        });
        ready(visual, &shell);
        assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nnew\nthree\n");
        assert_eq!(fixture.run_git(&["show", ":b.txt"]), b"one\nnew\nthree\n");
        for path in ["a.txt", "b.txt"] {
            assert_eq!(
                std::fs::read(fixture.options.cwd.join(path)).unwrap(),
                b"one\nnew\nthree\n"
            );
        }
        assert!(
            fixture
                .run_git(&["ls-files", "--stage", "--", "untouched.txt"])
                .is_empty()
        );
        assert_eq!(
            std::fs::read(fixture.options.cwd.join("untouched.txt")).unwrap(),
            b"untracked remains\n"
        );
        assert_eq!(mutations(&fixture), ["add"]);
        visual.update(|window, cx| {
            let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
            assert!(tree.error().is_none(), "mixed range: {:?}", tree.error());
            for path in ["a.txt", "b.txt"] {
                assert!(
                    tree.entries()
                        .iter()
                        .any(|e| e.path == path && e.index == b'M' && e.worktree == b' ')
                );
            }
            window.render_frame(cx);
            window.press("space", cx); // same range, now entirely staged: unstage both
        });
        ready(visual, &shell);
        for path in ["a.txt", "b.txt"] {
            assert_eq!(
                fixture.run_git(&["show", &format!(":{path}")]),
                b"one\nold\nthree\n"
            );
            assert_eq!(
                std::fs::read(fixture.options.cwd.join(path)).unwrap(),
                b"one\nnew\nthree\n"
            );
        }
        assert_eq!(mutations(&fixture), ["add", "reset", "reset"]);
        test_support::shutdown(visual, &fixture.host);
    }
}

#[gpui_kit::test]
fn default_external_polling_observes_file_and_index_changes_with_fake_time(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::installed();
    fixture.config("{}\n"); // pinned production defaults, not accelerated policy
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    let interval = visual.update(|_, cx| {
        let policy = &shell
            .read(cx)
            .repository
            .read(cx)
            .session()
            .unwrap()
            .settings
            .m1()
            .refresh;
        assert!(policy.auto_refresh && policy.auto_detect_external_changes);
        assert_eq!(policy.refresh_interval_seconds, 10);
        assert_eq!(policy.external_change_check_interval_seconds, 2);
        Duration::from_secs(policy.external_change_check_interval_seconds)
    });
    fixture.write("a.txt", b"external replacement\n");
    fixture.run_git(&["add", "a.txt"]);
    visual.run_until_parked();
    visual.background_executor.advance_clock(interval);
    test_support::wait(visual, |cx| {
        shell.read(cx).features.as_ref().is_some_and(|f| {
            f.tree
                .read(cx)
                .entries()
                .iter()
                .any(|e| e.path == "a.txt" && e.index == b'M' && e.worktree == b' ')
        })
    });
    clear_commands(&fixture);
    visual.update(|window, cx| {
        window.render_frame(cx);
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert_eq!(tree.focused_side(), Side::Index);
        assert!(tree.pane(Side::Worktree).is_empty());
        assert!(
            tree.pane(Side::Index)
                .canonical
                .windows(b"external replacement".len())
                .any(|w| w == b"external replacement")
        );
        assert!(window.try_find("diff-unstaged").is_none());
        window.press("2", cx); // replacement has no safe partial context: whole-file unstage
        assert!(
            window
                .find("action-commit")
                .label()
                .unwrap()
                .contains("(c)"),
            "key 2 must activate Files before Space; contextual label: {:?}",
            window.find("action-commit").label()
        );
        window.press("space", cx);
    });
    ready(visual, &shell);
    visual.update(|_, cx| {
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert!(
            tree.error().is_none(),
            "external poll action: {:?}; commands {:?}",
            tree.error(),
            mutations(&fixture)
        );
    });
    assert_eq!(
        mutations(&fixture),
        ["reset"],
        "Files Space must dispatch whole-file reset, never partial apply"
    );
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nold\nthree\n");
    assert_eq!(
        std::fs::read(fixture.options.cwd.join("a.txt")).unwrap(),
        b"external replacement\n"
    );
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn stale_external_replacement_during_selected_action_is_rejected_without_patch(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::installed();
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    clear_commands(&fixture);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click(("file", 0usize), cx);
        window.press("enter", cx);
        fixture.write("a.txt", b"unrelated replacement\n");
        window.press("space", cx);
    });
    ready(visual, &shell);
    visual.update(|_, cx| {
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert!(tree.error().unwrap().contains("stale"));
        assert!(
            tree.pane(Side::Worktree)
                .canonical
                .windows(b"unrelated replacement".len())
                .any(|w| w == b"unrelated replacement")
        );
    });
    assert!(mutations(&fixture).is_empty());
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nold\nthree\n");
    assert_eq!(
        std::fs::read(fixture.options.cwd.join("a.txt")).unwrap(),
        b"unrelated replacement\n"
    );
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn external_index_change_invalidates_selected_patch_before_dispatch(cx: &mut TestAppContext) {
    let fixture = Fixture::installed();
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click(("file", 0usize), cx);
        window.press("enter", cx);
        fixture.run_git(&["add", "a.txt"]); // another tool modifies only the index
        clear_commands(&fixture);
        window.press("space", cx);
        window.press("space", cx); // rejection clears queued stale repeats too
    });
    ready(visual, &shell);
    assert!(mutations(&fixture).is_empty());
    assert_eq!(fixture.run_git(&["show", ":a.txt"]), b"one\nnew\nthree\n");
    assert_eq!(
        std::fs::read(fixture.options.cwd.join("a.txt")).unwrap(),
        b"one\nnew\nthree\n"
    );
    visual.update(|window, cx| {
        window.render_frame(cx);
        let tree = shell.read(cx).features.as_ref().unwrap().tree.read(cx);
        assert!(tree.error().unwrap().contains("stale"));
        assert_eq!(tree.focused_side(), Side::Index);
        assert!(tree.pane(Side::Worktree).is_empty());
        assert!(window.try_find("diff-unstaged").is_none());
        assert!(window.try_find("diff-index").is_some());
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn real_failing_hook_cancel_reopen_retry_preserves_draft_and_never_double_submits(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::installed();
    fixture.run_git(&["add", "a.txt"]);
    fixture.hook(
        "pre-commit",
        "printf 'attempt\\n' >> \"$HOME/attempts\"\nprintf 'fixture hook rejected\\n' >&2\nexit 1",
    );
    let head = fixture.run_git(&["rev-parse", "HEAD"]);
    let (visual, shell) = test_support::open(cx, &fixture);
    ready(visual, &shell);
    clear_commands(&fixture);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.press("c", cx);
        window.render_frame(cx);
        window.click("commit-subject", cx);
        window.input("retry draft", cx);
        window.press("tab", cx); // real body editor
        window.input("body hard break", cx);
        window.press("enter", cx); // body newline, not submit
        window.input("second line", cx);
        assert!(mutations(&fixture).is_empty(), "body Enter must not submit");
        assert!(
            !shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .commit
                .read(cx)
                .busy()
        );
        window.press("ctrl-s", cx);
        assert!(
            shell
                .read(cx)
                .features
                .as_ref()
                .unwrap()
                .commit
                .read(cx)
                .busy(),
            "Ctrl+S itself must submit, not depend on the following Ctrl+Enter"
        );
        window.press("ctrl-enter", cx); // duplicate before repaint
    });
    ready(visual, &shell);
    visual.update(|window, cx| {
        window.render_frame(cx);
        let owner = shell.read(cx).features.as_ref().unwrap().commit.read(cx);
        assert!(matches!(
            owner.outcome(),
            Some(Outcome::NotCommitted { .. })
        ));
        assert!(owner.error().unwrap().contains("fixture hook rejected"));
        assert_eq!(owner.draft().subject, "retry draft");
        assert_eq!(owner.draft().body, "body hard break\nsecond line");
        assert!(window.try_find("commit-subject").is_some());
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-subject").is_none());
        window.press("c", cx);
        window.render_frame(cx);
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
            "retry draft"
        );
    });
    assert_eq!(fixture.run_git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.temp.path().join("home/attempts")).unwrap(),
        b"attempt\n"
    );
    assert_eq!(mutations(&fixture), ["commit"]);
    fixture.hook(
        "pre-commit",
        "printf 'attempt\\n' >> \"$HOME/attempts\"\nexit 0",
    );
    visual.update(|window, cx| {
        window.click("commit-subject", cx);
        window.press("ctrl-enter", cx);
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
        window.press("ctrl-s", cx); // inverse duplicate: only one retry
    });
    ready(visual, &shell);
    assert_ne!(fixture.run_git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        fixture.run_git(&["log", "-1", "--format=%B"]),
        b"retry draft\n\nbody hard break\nsecond line\n"
    );
    assert_eq!(
        std::fs::read(fixture.temp.path().join("home/attempts")).unwrap(),
        b"attempt\nattempt\n"
    );
    assert_eq!(mutations(&fixture), ["commit", "commit"]);
    visual.update(|window, cx| {
        window.render_frame(cx);
        let f = shell.read(cx).features.as_ref().unwrap();
        assert!(matches!(
            f.commit.read(cx).outcome(),
            Some(Outcome::Committed { .. })
        ));
        assert!(f.commit.read(cx).draft().subject.is_empty());
        assert_eq!(f.history.read(cx).commits()[0].subject, b"retry draft");
        assert!(window.try_find("commit-subject").is_none());
    });
    test_support::shutdown(visual, &fixture.host);
}

#[gpui_kit::test]
fn no_stage_warning_cancel_reopen_confirm_and_configured_skip_stage_choice_sequence(
    cx: &mut TestAppContext,
) {
    for skip in [false, true] {
        let fixture = Fixture::installed();
        fixture.config(&format!("gui:\n  skipNoStagedFilesWarning: {skip}\ngit:\n  autoRefresh: false\n  autoDetectExternalChanges: false\n"));
        let (visual, shell) = test_support::open(cx, &fixture);
        ready(visual, &shell);
        clear_commands(&fixture);
        let head = fixture.run_git(&["rev-parse", "HEAD"]);
        visual.update(|window, cx| {
            window.render_frame(cx);
            window.click("action-commit", cx);
            window.render_frame(cx);
            window.click("commit-subject", cx);
            window.input("stage choice", cx);
            window.press("enter", cx);
        });
        ready(visual, &shell);
        if !skip {
            visual.update(|window, cx| {
                window.render_frame(cx);
                assert_eq!(
                    shell
                        .read(cx)
                        .features
                        .as_ref()
                        .unwrap()
                        .commit
                        .read(cx)
                        .warning(),
                    Some(lazygui::commit::Warning::NoStagedFiles)
                );
                assert!(window.try_find("commit-stage-all-warning").is_some());
                window.press("space", cx); // warning is not underlying stage action
                window.press("c", cx);
                assert!(mutations(&fixture).is_empty());
                window.press("escape", cx);
                window.render_frame(cx);
                assert!(window.try_find("commit-subject").is_none());
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
                    "stage choice"
                );
                window.click("action-commit", cx);
                window.render_frame(cx);
                window.click("commit-submit", cx);
            });
            ready(visual, &shell);
            assert_eq!(fixture.run_git(&["rev-parse", "HEAD"]), head);
            assert!(fixture.run_git(&["diff", "--cached"]).is_empty());
            visual.update(|window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("commit-stage-all-warning").is_some());
                window.click("commit-confirm-stage-all", cx);
                window.press("enter", cx); // duplicate confirmation after warning leaves
            });
            ready(visual, &shell);
        }
        assert_eq!(mutations(&fixture), ["add", "commit"]);
        assert_ne!(fixture.run_git(&["rev-parse", "HEAD"]), head);
        assert!(fixture.run_git(&["status", "--porcelain"]).is_empty());
        assert_eq!(
            fixture.run_git(&["log", "-1", "--format=%s"]),
            b"stage choice\n"
        );
        assert_eq!(
            fixture.run_git(&["show", "HEAD:a.txt"]),
            b"one\nnew\nthree\n"
        );
        assert_eq!(
            fixture.run_git(&["show", "HEAD:b.txt"]),
            b"one\nnew\nthree\n"
        );
        visual.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("commit-stage-all-warning").is_none());
            assert!(window.try_find("commit-subject").is_none());
            assert_eq!(window.find("diff-empty").label(), Some("No diff data"));
            assert!(window.try_find("diff-unstaged").is_none());
            assert!(window.try_find("diff-index").is_none());
            assert!(window.try_find(("file", 0usize)).is_none());
            assert!(
                shell
                    .read(cx)
                    .features
                    .as_ref()
                    .unwrap()
                    .commit
                    .read(cx)
                    .draft()
                    .subject
                    .is_empty()
            );
        });
        test_support::shutdown(visual, &fixture.host);
    }
}

#[gpui_kit::test]
fn hook_skip_prefix_is_literal_case_sensitive_nonempty_and_retained_in_message(
    cx: &mut TestAppContext,
) {
    for (prefix, subject, skipped) in [
        ("WIP", "WIPexact", true),
        ("WIP", "wip lower", false),
        ("WIP", " WIP spaced", false),
        ("", "ordinary", false),
    ] {
        let fixture = Fixture::installed();
        fixture.run_git(&["add", "a.txt"]);
        fixture.config(&format!("git:\n  skipHookPrefix: '{prefix}'\n  autoRefresh: false\n  autoDetectExternalChanges: false\n"));
        fixture.hook("pre-commit", "printf pre >> \"$HOME/hooks-ran\"\nexit 0");
        fixture.hook("commit-msg", "printf msg >> \"$HOME/hooks-ran\"\nexit 1");
        let head = fixture.run_git(&["rev-parse", "HEAD"]);
        let (visual, shell) = test_support::open(cx, &fixture);
        ready(visual, &shell);
        clear_commands(&fixture);
        visual.update(|window, cx| {
            window.render_frame(cx);
            window.click("action-commit", cx);
            window.render_frame(cx);
            window.click("commit-subject", cx);
            window.input(subject, cx);
            window.press("enter", cx);
            window.press("enter", cx);
        });
        ready(visual, &shell);
        assert_eq!(mutations(&fixture), ["commit"]);
        if skipped {
            assert!(!fixture.temp.path().join("home/hooks-ran").exists());
            assert_ne!(fixture.run_git(&["rev-parse", "HEAD"]), head);
            assert_eq!(
                fixture.run_git(&["log", "-1", "--format=%s"]),
                format!("{subject}\n").as_bytes()
            );
        } else {
            assert_eq!(
                std::fs::read(fixture.temp.path().join("home/hooks-ran")).unwrap(),
                b"premsg"
            );
            assert_eq!(fixture.run_git(&["rev-parse", "HEAD"]), head);
            visual.update(|_, cx| {
                let commit = shell.read(cx).features.as_ref().unwrap().commit.read(cx);
                assert!(matches!(
                    commit.outcome(),
                    Some(Outcome::NotCommitted { .. })
                ));
                assert_eq!(commit.draft().subject, subject);
            });
        }
        test_support::shutdown(visual, &fixture.host);
    }
}
