use super::*;
use gpui_kit::test::TestWindowExt;

#[gpui_kit::test]
fn production_shell_composes_the_example_screen(cx: &mut TestAppContext) {
    let (cx, _shell) = open(cx, Client::memory());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("catalog-search").is_some());
        assert!(window.try_find("record-views").is_some());
        assert!(window.try_find("record-detail").is_some());
        assert!(window.try_find("catalog-reload").is_some());
    });
}
