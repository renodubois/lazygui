use super::CatalogView;
use crate::catalog::Catalog;
use crate::{
    connectors::catalog::Client,
    runtime::Execution,
    test_support::catalog::{Controlled, record},
};
use gpui_kit::{AppContext as _, TestAppContext};
use gpui_kit::{component::Root, test::TestWindowExt};

#[gpui_kit::test]
fn recreating_screen_uses_existing_feature_and_does_not_dispatch_again(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let adapter = Controlled::default();
    let execution = Execution::controlled(cx.background_executor.clone());
    let catalog = cx.update(|cx| cx.new(|_| Catalog::new(adapter.client(), execution, None, None)));
    let updates = cx.update(|cx| {
        catalog.update(cx, |owner, _| {
            owner.load("pending".into());
            owner.updates()
        })
    });
    let mut screen = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| CatalogView::new(catalog.clone(), window, cx));
        screen = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        screen.as_ref().unwrap().update(cx, |view, cx| {
            *view = CatalogView::new(catalog.clone(), window, cx);
            cx.notify();
        });
    });
    adapter.reply(0, Ok(vec![record("kept")]));
    visual.run_until_parked();
    visual.update(|_, cx| {
        catalog.update(cx, |owner, cx| {
            owner.apply(updates.try_recv().unwrap());
            cx.notify();
        });
    });
    visual.run_until_parked();
    assert_eq!(adapter.queries(), vec!["pending"]);
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("record-kept").is_some());
    });
}
#[gpui_kit::test]
fn standalone_screen_does_not_construct_or_start_a_second_owner(cx: &mut TestAppContext) {
    let execution = Execution::controlled(cx.background_executor.clone());
    let catalog = cx.update(|cx| cx.new(|_| Catalog::new(Client::memory(), execution, None, None)));
    assert!(!cx.update(|cx| catalog.read(cx).read().loading()));
}
