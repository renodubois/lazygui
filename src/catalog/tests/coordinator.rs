use super::*;
use crate::test_support::catalog::{Controlled, record};
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn superseding_a_request_keeps_only_the_latest_result(cx: &mut TestAppContext) {
    let adapter = Controlled::default();
    let execution = Execution::controlled(cx.background_executor.clone());
    let mut catalog = Catalog::new(adapter.client(), execution, None, None);
    let updates = catalog.updates();
    catalog.load("old".into());
    cx.run_until_parked();
    adapter.reply(0, Ok(vec![record("old")]));
    cx.run_until_parked(); // Old completion is already queued, even if cancellation follows.
    catalog.search("  new  ".into());
    catalog.apply(updates.try_recv().unwrap());
    assert!(catalog.read().loading());
    cx.run_until_parked();
    adapter.reply(1, Ok(vec![record("new")]));
    cx.run_until_parked();
    catalog.apply(updates.try_recv().unwrap());
    assert_eq!(catalog.read().query(), "new");
    assert_eq!(catalog.read().selected().unwrap().id, "new");
    assert_eq!(adapter.queries(), vec!["old", "new"]);
}
#[gpui_kit::test]
fn deadline_and_owner_drop_do_not_require_a_real_timer_or_network(cx: &mut TestAppContext) {
    let adapter = Controlled::default();
    let execution = Execution::controlled(cx.background_executor.clone());
    let mut catalog = Catalog::new(adapter.client(), execution, None, None);
    let updates = catalog.updates();
    catalog.load(String::new());
    cx.run_until_parked();
    cx.background_executor.advance_clock(Duration::from_secs(6));
    cx.run_until_parked();
    catalog.apply(updates.try_recv().unwrap());
    assert_eq!(catalog.read().error(), Some("Catalog request timed out."));
    catalog.load("pending".into());
    cx.run_until_parked();
    drop(catalog);
    cx.run_until_parked();
    adapter.reply(1, Ok(vec![record("late")]));
    cx.run_until_parked();
    assert!(updates.try_recv().is_err());
}
