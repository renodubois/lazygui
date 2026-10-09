use super::*;
use gpui_kit::{TestAppContext, component::Theme};
#[gpui_kit::test]
fn installs_dark_theme_without_native_window(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        init(cx);
        assert!(Theme::global(cx).mode.is_dark());
    });
}
