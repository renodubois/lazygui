//! M0 headless window-lifetime probe, not the product shell.
//! Startup injects a non-rendering operation entity; the view never spawns a child.
use crate::git::process::Operation;
use gpui_kit::{Context, Entity, IntoElement, Render, Window, div};
pub struct ProcessProbe {
    _operation: Entity<Operation>,
}
impl ProcessProbe {
    pub fn new(operation: Entity<Operation>) -> Self {
        Self {
            _operation: operation,
        }
    }
}
impl Render for ProcessProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
#[cfg(test)]
#[path = "tests/process_probe.rs"]
mod tests;
