use crate::catalog::Catalog;
use gpui_kit::*;

pub(super) struct RecordDetail {
    catalog: Entity<Catalog>,
    _observe: Subscription,
}
impl RecordDetail {
    pub(super) fn new(catalog: Entity<Catalog>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&catalog, |_, _, cx| cx.notify());
        Self {
            catalog,
            _observe: observe,
        }
    }
}
impl Render for RecordDetail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = self.catalog.read(cx);
        let state = owner.read();
        let status = if state.loading() {
            "Loading records…".to_owned()
        } else if let Some(error) = state.error() {
            error.to_owned()
        } else if state.records().is_empty() {
            "No records match your query.".to_owned()
        } else {
            format!("{} records", state.records().len())
        };
        let mut detail = div()
            .id("record-detail")
            .aria_label("Record details")
            .test_support()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .id("catalog-status")
                    .aria_label(status.clone())
                    .test_support()
                    .child(status),
            );
        if let Some(record) = state.selected() {
            detail = detail
                .child(
                    div()
                        .id("record-title")
                        .aria_label(record.title.clone())
                        .test_support()
                        .text_lg()
                        .child(record.title.clone()),
                )
                .child(
                    div()
                        .id("record-description")
                        .aria_label(record.description.clone())
                        .test_support()
                        .child(record.description.clone()),
                );
        }
        if let Some(warning) = owner.storage_warning() {
            detail = detail.child(
                div()
                    .id("storage-warning")
                    .aria_label(warning.to_owned())
                    .test_support()
                    .child(warning.to_owned()),
            );
        }
        detail
    }
}
