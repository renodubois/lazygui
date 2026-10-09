use super::*;
use crate::test_support::catalog::{Controlled, record};
use gpui_kit::test::TestWindowExt;

#[gpui_kit::test]
fn search_selection_empty_and_error_journey_uses_real_controls(cx: &mut TestAppContext) {
    let adapter = Controlled::default();
    let (cx, _shell) = open(cx, adapter.client());
    cx.run_until_parked();
    adapter.reply(0, Ok(vec![record("a"), record("b")]));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("record-b", cx);
        window.render_frame(cx);
        assert_eq!(window.find("record-title").label(), Some("Record b"));
        window.click("catalog-search", cx);
        window.input("nothing", cx);
        window.click("search-submit", cx);
    });
    cx.run_until_parked();
    assert_eq!(adapter.queries(), vec!["", "nothing"]);
    adapter.reply(1, Ok(vec![]));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("record-a").is_none());
        assert_eq!(
            window.find("catalog-status").label(),
            Some("No records match your query.")
        );
        window.click("catalog-reload", cx);
    });
    cx.run_until_parked();
    adapter.reply(2, Err(crate::connectors::catalog::Error::Transport));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("catalog-status").label(),
            Some("Could not reach the catalog.")
        );
    });
}
