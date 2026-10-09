//! Editable text/focus are local. Only submitted queries become feature state.
use crate::catalog::Catalog;
use gpui_kit::{
    base::input::{InputBaseState, InputEvent, InputMode, InputState},
    component::{button::Button, input::Input},
    *,
};

pub(super) struct Search {
    catalog: Entity<Catalog>,
    input: Entity<InputBaseState<InputMode>>,
    _submit: Subscription,
}
impl Search {
    pub(super) fn new(
        catalog: Entity<Catalog>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search records; Enter to submit")
                .default_value(catalog.read(cx).read().query())
        });
        let submit = cx.subscribe(&input, |view: &mut Self, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                view.submit(cx);
            }
        });
        Self {
            catalog,
            input,
            _submit: submit,
        }
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        self.catalog.update(cx, |owner, cx| {
            owner.search(query);
            cx.notify();
        });
    }
}
impl Render for Search {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let catalog = self.catalog.clone();
        div()
            .flex()
            .gap_2()
            .child(Input::new(&self.input).id("catalog-search").flex_1())
            .child(
                Button::new("search-submit")
                    .label("Search")
                    .on_click(cx.listener(|view, _, _, cx| view.submit(cx))),
            )
            .child(
                Button::new("catalog-reload")
                    .label("Reload")
                    .on_click(move |_, _, cx| {
                        catalog.update(cx, |owner, cx| {
                            let query = owner.read().query().to_owned();
                            owner.load(query);
                            cx.notify();
                        });
                    }),
            )
    }
}
