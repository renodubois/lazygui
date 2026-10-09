//! Screen constructs private children from an existing feature handle.
mod record_detail;
mod record_list;
mod search;
use crate::catalog::Catalog;
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use record_detail::RecordDetail;
use record_list::RecordList;
use search::Search;

pub(super) struct CatalogView {
    list: Entity<RecordList>,
    detail: Entity<RecordDetail>,
    search: Entity<Search>,
}
impl CatalogView {
    pub(super) fn new(
        catalog: Entity<Catalog>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            list: cx.new(|cx| RecordList::new(catalog.clone(), cx)),
            detail: cx.new(|cx| RecordDetail::new(catalog.clone(), cx)),
            search: cx.new(|cx| Search::new(catalog, window, cx)),
        }
    }
}
impl Render for CatalogView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .p_4()
            .gap_4()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(div().text_xl().child(env!("CARGO_PKG_NAME")))
            .child(self.search.clone())
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_4()
                    .child(div().w(px(250.)).h_full().child(self.list.clone()))
                    .child(div().flex_1().child(self.detail.clone())),
            )
    }
}
#[cfg(test)]
#[path = "tests/interaction.rs"]
mod tests;
