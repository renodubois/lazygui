//! Window-scoped stable feature host. One opaque-result consumer, no result policy.
use super::catalog::CatalogView;
use crate::{
    catalog::Catalog,
    connectors::catalog::Client,
    runtime::Execution,
    storage::{Config, Persistence},
};
use gpui_kit::*;

pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    client: Client,
    execution: Execution,
    config: Config,
    persistence: Option<Persistence>,
    warning: Option<String>,
) -> Entity<AppShell> {
    cx.new(|cx| AppShell::new(window, cx, client, execution, config, persistence, warning))
}
pub(crate) struct AppShell {
    catalog: Entity<Catalog>,
    screen: Entity<CatalogView>,
    _delivery: Task<()>,
}
impl AppShell {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        client: Client,
        execution: Execution,
        config: Config,
        persistence: Option<Persistence>,
        warning: Option<String>,
    ) -> Self {
        let catalog = cx.new(|_| Catalog::new(client, execution, persistence, warning));
        catalog.update(cx, |owner, cx| {
            owner.load(config.query);
            cx.notify();
        });
        let updates = catalog.read(cx).updates();
        let screen = cx.new(|cx| CatalogView::new(catalog.clone(), window, cx));
        let delivery = cx.spawn(async move |weak, cx| {
            while let Ok(update) = updates.recv().await {
                if weak
                    .update(cx, |shell, cx| {
                        shell.catalog.update(cx, |owner, cx| {
                            owner.apply(update);
                            cx.notify();
                        });
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            catalog,
            screen,
            _delivery: delivery,
        }
    }
}
impl Render for AppShell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.screen.clone())
    }
}
