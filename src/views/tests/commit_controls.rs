use super::*;
use gpui_kit::{TestAppContext, component::Root, test::TestWindowExt};
use std::sync::{Arc, Mutex};
#[gpui_kit::test]
fn real_subject_body_shortcuts_menu_arrows_escape_and_global_suppression(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let intentions = Arc::new(Mutex::new(Vec::new()));
    let sink = intentions.clone();
    let (_, visual) = cx.add_window_view(|window, cx| {
        let controls = cx.new(|cx| {
            CommitControls::new(window, cx, move |intent, _| {
                sink.lock().unwrap().push(match intent {
                    Intent::Submit { subject, body } => format!("submit:{subject}:{body}"),
                    Intent::Changed { .. } => "changed".into(),
                    Intent::ConfirmStageAll => "stage-all".into(),
                    Intent::Copy => "copy".into(),
                    Intent::Cancel => "cancel".into(),
                    Intent::CloseWindow => "close".into(),
                    Intent::Diagnostic(reason) => format!("diagnostic:{reason}"),
                });
            })
        });
        Root::new(controls, window, cx)
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("commit-subject", cx);
        for key in ["j", "k", "space", "c", "q", "s"] {
            window.press(key, cx);
            window.render_frame(cx);
        }
        window.press("left", cx);
        window.press("right", cx);
        window.press("enter", cx);
        window.press("tab", cx);
        for key in ["j", "k", "space", "c", "q", "s"] {
            window.press(key, cx);
            window.render_frame(cx);
        }
        window.press("enter", cx);
        for key in ["q", "space", "c"] {
            window.press(key, cx);
            window.render_frame(cx);
        }
        window.press("ctrl-s", cx);
        window.press("ctrl-enter", cx);
        window.press("ctrl-o", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-menu").is_some());
        window.press("q", cx); // menu suppresses global close
        window.press("down", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-menu").is_none());
        window.press("escape", cx);
    });
    assert_eq!(
        intentions
            .lock()
            .unwrap()
            .iter()
            .filter(|x| x.as_str() != "changed")
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            "submit:jk cqs:",
            "submit:jk cqs:jk cqs\nq c",
            "submit:jk cqs:jk cqs\nq c",
            "cancel"
        ]
    );
}
#[gpui_kit::test]
fn marked_composition_and_pasted_text_never_submit_or_invoke_global_keys(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let mut controls = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            CommitControls::new(window, cx, move |intent, _| {
                sink.lock().unwrap().push(intent);
            })
        });
        controls = Some(view.clone());
        Root::new(view, window, cx)
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        window.render_frame(cx);
        let controls = controls.as_ref().unwrap();
        controls.update(cx, |view, _| {
            view.set_history(vec![Draft::from_message("history\n\nrecalled body")])
        });
        let subject = controls.read(cx).subject.clone();
        let body = controls.read(cx).body.clone();
        for in_body in [false, true] {
            for key in [
                "up",
                "down",
                "left",
                "right",
                "escape",
                "ctrl-o",
                "tab",
                "ctrl-s",
                "ctrl-enter",
                "enter",
            ] {
                controls.update(cx, |view, cx| view.set_draft(&Draft::default(), window, cx));
                if in_body {
                    body.update(cx, |state, cx| {
                        state.focus_handle(cx).focus(window, cx);
                        state.replace_and_mark_text_in_range(
                            None,
                            "composition",
                            Some(0..11),
                            window,
                            cx,
                        );
                    });
                } else {
                    subject.update(cx, |state, cx| {
                        state.focus_handle(cx).focus(window, cx);
                        state.replace_and_mark_text_in_range(
                            None,
                            "composition",
                            Some(0..11),
                            window,
                            cx,
                        );
                    });
                }
                window.render_frame(cx);
                window.press(key, cx);
                window.render_frame(cx);
                let view = controls.read(cx);
                assert!(!view.menu, "marked {key} opened menu");
                assert_eq!(view.history_index, None, "marked {key} recalled history");
                if key != "tab" {
                    assert!(
                        if in_body {
                            body.read(cx).focus_handle(cx).is_focused(window)
                        } else {
                            subject.read(cx).focus_handle(cx).is_focused(window)
                        },
                        "marked {key} switched field"
                    );
                } else if in_body {
                    // Kit's normal propagated Tab may traverse focus. It must not
                    // become our togglePanel workflow (which wraps back to subject).
                    assert!(!subject.read(cx).focus_handle(cx).is_focused(window));
                }
                assert!(
                    events
                        .lock()
                        .unwrap()
                        .iter()
                        .all(|intent| matches!(intent, Intent::Changed { .. })),
                    "marked {key} invoked workflow"
                );
                if key == "escape" {
                    // Kit must still receive Escape and finish its marked edit.
                    assert!(if in_body {
                        body.update(cx, |state, cx| {
                            state.marked_text_range(window, cx).is_none()
                        })
                    } else {
                        subject.update(cx, |state, cx| {
                            state.marked_text_range(window, cx).is_none()
                        })
                    });
                }
                subject.update(cx, |state, cx| state.unmark_text(window, cx));
                body.update(cx, |state, cx| state.unmark_text(window, cx));
            }
        }
        // Exercise Kit's actual clipboard Paste action, not replace_text/window.input.
        controls.update(cx, |view, cx| {
            view.set_draft(&Draft::from_message("subject"), window, cx)
        });
        body.read(cx).focus_handle(cx).focus(window, cx);
        cx.write_to_clipboard(ClipboardItem::new_string("j k space c q s\nbody".into()));
        window.render_frame(cx);
        window.dispatch_action(Box::new(gpui_kit::base::input::Paste), cx);
    });
    visual.run_until_parked(); // Window::dispatch_action is deferred.
    visual.update(|window, cx| {
        let controls = controls.as_ref().unwrap();
        window.render_frame(cx);
        assert_eq!(controls.read(cx).draft(cx).body, "j k space c q s\nbody");
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .all(|intent| matches!(intent, Intent::Changed { .. }))
        );
        window.press("ctrl-enter", cx);
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| matches!(e, Intent::Submit { .. }))
                .count(),
            1
        );
    });
}

#[gpui_kit::test]
fn draft_changes_cancel_supplied_full_message_history_and_custom_bindings(cx: &mut TestAppContext) {
    // The host supplies actual full-message drafts; no Git/owner workflow in a view test.
    let history = vec![
        Draft::from_message("last message\n\nlast body\nsecond line\n"),
        Draft::from_message("older message\n\nolder body"),
    ];
    cx.update(gpui_kit::init);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let mut controls = None;
    let mut settings = M1Settings::default();
    let key = |s| Key::parse(s).unwrap().unwrap();
    settings
        .keybindings
        .get_mut("universal")
        .unwrap()
        .insert("submitEditorText".into(), vec![key("<ctrl+d>")]);
    settings
        .keybindings
        .get_mut("universal")
        .unwrap()
        .insert("confirmInEditor".into(), vec![key("<ctrl+b>")]);
    settings
        .keybindings
        .get_mut("commitMessage")
        .unwrap()
        .insert("commitMenu".into(), vec![key("<ctrl+m>")]);
    let (_, visual) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            CommitControls::new_with_draft(
                window,
                cx,
                Draft {
                    subject: "retained".into(),
                    body: "original body".into(),
                },
                settings,
                move |intent, _| sink.lock().unwrap().push(intent),
            )
        });
        controls = Some(view.clone());
        Root::new(view, window, cx)
    });
    visual.run_until_parked();
    assert!(events.lock().unwrap().is_empty());
    visual.update(|window, cx| {
        let controls = controls.as_ref().unwrap();
        window.render_frame(cx);
        window.press("up", cx); // Empty history never fabricates previous messages.
        assert_eq!(controls.read(cx).draft(cx).subject, "retained");
        controls.update(cx, |view, _| view.set_history(history.clone()));
        window.press("up", cx);
        assert_eq!(controls.read(cx).draft(cx).body, "last body\nsecond line\n");
        // Re-observing identical owner history must not restart recall navigation.
        controls.update(cx, |view, _| view.set_history(history.clone()));
        window.press("up", cx);
        assert_eq!(controls.read(cx).draft(cx).subject, "older message");
        window.press("down", cx);
        window.press("down", cx);
        assert_eq!(controls.read(cx).draft(cx).subject, "retained");
        window.input(" edited", cx);
        window.press("enter", cx);
        window.press("ctrl-s", cx); // replaced binding: no default fallback
        window.press("ctrl-d", cx);
        window.press("tab", cx);
        window.input("more body", cx);
        window.press("enter", cx);
        window.press("ctrl-b", cx);
        window.press("ctrl-o", cx); // replaced menu binding
        window.render_frame(cx);
        assert!(window.try_find("commit-menu").is_none());
        window.press("ctrl-m", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-menu").is_some());
        window.press("q", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            controls
                .read(cx)
                .body
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        window.press("escape", cx);
    });
    visual.run_until_parked();
    let events = events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Intent::Submit { .. }))
            .count(),
        2
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Intent::Changed { subject, .. } if subject == "retained edited"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Intent::Changed { body, .. } if body.contains("more body")))
    );
    assert!(events.iter().any(|e| matches!(e, Intent::Cancel)));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Intent::Copy | Intent::CloseWindow))
    );
}
#[gpui_kit::test]
fn real_clipboard_menu_replace_warning_busy_and_popup_suppression(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let mut controls = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            CommitControls::new_with_draft(
                window,
                cx,
                Draft {
                    subject: "subject".into(),
                    body: "body".into(),
                },
                M1Settings::default(),
                move |intent, _| sink.lock().unwrap().push(intent),
            )
        });
        controls = Some(view.clone());
        Root::new(view, window, cx)
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        let controls = controls.as_ref().unwrap();
        window.render_frame(cx);
        window.press("ctrl-o", cx);
        window.render_frame(cx);
        window.press("q", cx);
        window.press("enter", cx);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("subject\n\nbody")
        );
        window.render_frame(cx);
        cx.write_to_clipboard(ClipboardItem::new_string(
            "pasted subject\n\npasted body".into(),
        ));
        window.press("ctrl-o", cx);
        assert!(controls.read(cx).menu, "second menu must open");
        window.render_frame(cx);
        window.press("down", cx);
        assert_eq!(controls.read(cx).menu_row, 1);
        window.press("enter", cx);
        assert!(
            controls.read(cx).pending_paste.is_some(),
            "paste must require replacement confirmation"
        );
        window.render_frame(cx);
        assert!(window.try_find("commit-paste-warning").is_some());
        assert_eq!(controls.read(cx).draft(cx).subject, "subject");
        window.press("ctrl-enter", cx);
        window.press("q", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(controls.read(cx).draft(cx).subject, "pasted subject");
        assert_eq!(controls.read(cx).draft(cx).body, "pasted body");
        controls.update(cx, |view, cx| {
            view.set_feedback(true, None, None, window, cx)
        });
        window.press("ctrl-enter", cx);
        window.press("enter", cx);
        controls.update(cx, |view, cx| {
            view.set_feedback(false, Some(Warning::NoStagedFiles), None, window, cx)
        });
        window.render_frame(cx);
        window.press("q", cx);
        window.press("ctrl-enter", cx);
        window.press("enter", cx);
        controls.update(cx, |view, cx| {
            view.set_feedback(false, None, None, window, cx)
        });
        window.render_frame(cx);
        controls.update(cx, |view, cx| view.focus.focus(window, cx));
        window.press("ctrl-o", cx);
    });
    visual.run_until_parked();
    let events = events.lock().unwrap();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Intent::Submit { .. } | Intent::CloseWindow))
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Intent::ConfirmStageAll))
            .count(),
        1
    );
    assert_eq!(
        events.iter().filter(|e| matches!(e, Intent::Copy)).count(),
        1
    );
}

#[gpui_kit::test]
fn shifted_enter_stays_editing_and_contextual_menu_wins_rebound_confirmation(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let mut controls = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            CommitControls::new_with_draft(
                window,
                cx,
                Draft::from_message("subject\n\nbody"),
                M1Settings::default(),
                move |intent, _| sink.lock().unwrap().push(intent),
            )
        });
        controls = Some(view.clone());
        Root::new(view, window, cx)
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        let controls = controls.as_ref().unwrap();
        window.render_frame(cx);
        window.press("shift-enter", cx);
        window.press("tab", cx);
        window.render_frame(cx);
        window.press("shift-enter", cx);
        assert!(controls.read(cx).draft(cx).body.contains('\n'));
        // Also exercise the action fallback (there is no physical key event).
        window.dispatch_action(
            Box::new(Enter {
                secondary: true,
                shift: true,
            }),
            cx,
        );
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        let controls = controls.as_ref().unwrap();
        assert_eq!(controls.read(cx).draft(cx).body.matches('\n').count(), 2);
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .all(|e| matches!(e, Intent::Changed { .. }))
        );
        let mut settings = M1Settings::default();
        for name in ["confirm", "confirmInEditor"] {
            settings
                .keybindings
                .get_mut("universal")
                .unwrap()
                .insert(name.into(), vec![Key::parse("<ctrl+o>").unwrap().unwrap()]);
        }
        controls.update(cx, |view, cx| view.set_settings(settings, cx));
        window.render_frame(cx);
        let help = window
            .find("commit-shortcut-help")
            .label()
            .unwrap()
            .to_owned();
        assert!(help.contains("Commit options (Ctrl+o)"));
        assert!(help.contains("Commit (shortcut disabled)"));
        assert_eq!(
            window.find("commit-options").label(),
            Some("Commit options (Ctrl+o)")
        );
        window.press("ctrl-o", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-menu").is_some());
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            controls
                .read(cx)
                .body
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        window.click("commit-options", cx);
        window.render_frame(cx);
        assert!(window.try_find("commit-menu").is_some());
        window.press("escape", cx);
    });
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .all(|e| matches!(e, Intent::Changed { .. }))
    );
}

#[gpui_kit::test]
fn no_staged_warning_uses_confirm_not_confirm_menu_and_safely_suppresses_keys(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let mut controls = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            CommitControls::new(window, cx, move |intent, _| {
                sink.lock().unwrap().push(intent)
            })
        });
        controls = Some(view.clone());
        Root::new(view, window, cx)
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        let controls = controls.as_ref().unwrap();
        let mut settings = M1Settings::default();
        settings.keybindings.get_mut("universal").unwrap().insert(
            "confirm".into(),
            vec![Key::parse("<ctrl+o>").unwrap().unwrap()],
        );
        settings.keybindings.get_mut("universal").unwrap().insert(
            "return".into(),
            vec![Key::parse("<ctrl+x>").unwrap().unwrap()],
        );
        controls.update(cx, |view, cx| {
            view.set_settings(settings.clone(), cx);
            view.set_feedback(false, Some(Warning::NoStagedFiles), None, window, cx);
        });
        window.render_frame(cx);
        assert_eq!(
            window.find("commit-confirm-stage-all").label(),
            Some("Stage all and commit (Ctrl+o)")
        );
        assert!(
            window
                .find("commit-shortcut-help")
                .label()
                .unwrap()
                .contains("Stage all and commit (Ctrl+o)")
        );
        for key in [
            "enter",
            "ctrl-enter",
            "escape",
            "j",
            "k",
            "space",
            "c",
            "q",
            "s",
            "tab",
        ] {
            window.press(key, cx);
            window.render_frame(cx);
        }
        assert!(events.lock().unwrap().is_empty());
        assert_eq!(controls.read(cx).draft(cx), Draft::default());
        assert!(!controls.read(cx).menu);
        window.press("ctrl-o", cx);
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| matches!(e, Intent::ConfirmStageAll))
                .count(),
            1
        );
        controls.update(cx, |view, cx| {
            view.set_feedback(true, Some(Warning::NoStagedFiles), None, window, cx)
        });
        window.render_frame(cx);
        window.press("ctrl-o", cx);
        window.click("commit-confirm-stage-all", cx);
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| matches!(e, Intent::ConfirmStageAll))
                .count(),
            1
        );
        settings
            .keybindings
            .get_mut("universal")
            .unwrap()
            .insert("confirm".into(), vec![]);
        controls.update(cx, |view, cx| {
            view.set_settings(settings, cx);
            view.set_feedback(false, Some(Warning::NoStagedFiles), None, window, cx);
        });
        window.render_frame(cx);
        assert_eq!(
            window.find("commit-confirm-stage-all").label(),
            Some("Stage all and commit (shortcut disabled)")
        );
        window.press("enter", cx);
        window.press("ctrl-o", cx);
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| matches!(e, Intent::ConfirmStageAll))
                .count(),
            1
        );
        // Disabling the keyboard binding does not remove the deliberate click path.
        window.click("commit-confirm-stage-all", cx);
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| matches!(e, Intent::ConfirmStageAll))
                .count(),
            2
        );
        window.press("ctrl-x", cx);
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, Intent::Cancel))
        );
        assert!(!events.lock().unwrap().iter().any(|e| matches!(
            e,
            Intent::Submit { .. } | Intent::Copy | Intent::CloseWindow
        )));
    });
}
