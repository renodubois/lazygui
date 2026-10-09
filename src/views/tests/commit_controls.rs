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
        window.input("qc ", cx);
        window.press("left", cx);
        window.press("right", cx);
        window.press("enter", cx);
        window.press("tab", cx);
        window.input("body", cx);
        window.press("enter", cx);
        window.input("q c", cx);
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
        *intentions.lock().unwrap(),
        vec![
            "submit:qc :",
            "submit:qc :body\nq c",
            "submit:qc :body\nq c",
            "cancel"
        ]
    );
}
#[gpui_kit::test]
fn marked_composition_and_pasted_text_never_submit_or_invoke_global_keys(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let count = Arc::new(Mutex::new(0));
    let sink = count.clone();
    let mut controls = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        let view =
            cx.new(|cx| CommitControls::new(window, cx, move |_, _| *sink.lock().unwrap() += 1));
        controls = Some(view.clone());
        Root::new(view, window, cx)
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        window.render_frame(cx);
        let subject = controls.as_ref().unwrap().read(cx).subject.clone();
        subject.update(cx, |state, cx| {
            state.replace_and_mark_text_in_range(None, "composition", Some(0..11), window, cx);
        });
        window.press("ctrl-enter", cx);
        window.press("enter", cx);
        assert_eq!(*count.lock().unwrap(), 0);
        subject.update(cx, |state, cx| {
            state.unmark_text(window, cx);
            state.replace_text_in_range(None, "q c Space\nbody", window, cx);
        });
        assert_eq!(*count.lock().unwrap(), 0);
        window.press("enter", cx);
        assert_eq!(*count.lock().unwrap(), 1);
    });
}
