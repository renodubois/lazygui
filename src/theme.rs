//! Product visual policy lives here, not in workflow owners.
use gpui_kit::{
    App,
    component::{Theme, ThemeMode},
};
pub(crate) fn init(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
}
#[cfg(test)]
#[path = "theme/tests/config.rs"]
mod tests;
