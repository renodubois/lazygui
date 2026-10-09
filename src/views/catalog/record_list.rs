use crate::catalog::Catalog;
use gpui_kit::base::Selectable;
use gpui_kit::{component::button::Button, *};

pub(super) struct RecordList {
    catalog: Entity<Catalog>,
    _observe: Subscription,
}
impl RecordList {
    pub(super) fn new(catalog: Entity<Catalog>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&catalog, |_, _, cx| cx.notify());
        Self {
            catalog,
            _observe: observe,
        }
    }
}
impl Render for RecordList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let records = self.catalog.read(cx).read().records().to_vec();
        let selected = self
            .catalog
            .read(cx)
            .read()
            .selected()
            .map(|r| r.id.clone());
        div()
            .id("record-list")
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scroll()
            .h_full()
            .children(records.into_iter().map(|record| {
                let catalog = self.catalog.clone();
                let id = record.id.clone();
                Button::new(SharedString::from(format!("record-{}", record.id)))
                    .label(record.title)
                    .selected(selected.as_ref() == Some(&id))
                    .on_click(move |_, _, cx| {
                        catalog.update(cx, |owner, cx| {
                            owner.select(&id);
                            cx.notify();
                        });
                    })
            }))
    }
}
