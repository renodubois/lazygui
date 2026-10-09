mod composition;
mod journeys;

use crate::{
    connectors::catalog::Client,
    runtime::Execution,
    storage::Config,
    views::app_shell::{self, AppShell},
};
use gpui_kit::{Entity, TestAppContext, VisualTestContext, component::Root};

fn open(cx: &mut TestAppContext, client: Client) -> (&mut VisualTestContext, Entity<AppShell>) {
    cx.update(gpui_kit::init);
    let mut shell = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let execution = Execution::controlled(cx.background_executor().clone());
        let root = app_shell::open(window, cx, client, execution, Config::default(), None, None);
        shell = Some(root.clone());
        Root::new(root, window, cx)
    });
    (cx, shell.unwrap())
}
