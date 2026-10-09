//! Real Kit commit controls feasibility slice. Emits intentions; no Git or draft workflow.
use crate::input::{self, Action, Context as InputContext, Key};
use gpui_kit::{
    App, AppContext, Context, Entity, EntityInputHandler, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render,
    StatefulInteractiveElement, Styled, TestSupportExt, Window,
    base::input::{Enter, IndentInline, InputState, TextareaState},
    component::input::{Input, Textarea},
    div,
    prelude::FluentBuilder,
};
pub enum Intent {
    Submit { subject: String, body: String },
    Cancel,
    CloseWindow,
    Diagnostic(&'static str),
}
type IntentHandler = Box<dyn Fn(Intent, &mut App)>;
pub struct CommitControls {
    subject: Entity<InputState>,
    body: Entity<TextareaState>,
    focus: FocusHandle,
    menu: bool,
    menu_row: usize,
    on_intent: IntentHandler,
}
impl CommitControls {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        on_intent: impl Fn(Intent, &mut App) + 'static,
    ) -> Self {
        let subject = cx.new(|cx| InputState::new(window, cx));
        let body = cx.new(|cx| TextareaState::new(window, cx).rows(4));
        subject.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            subject,
            body,
            focus: cx.focus_handle(),
            menu: false,
            menu_row: 0,
            on_intent: Box::new(on_intent),
        }
    }
    fn context(&self, window: &Window, cx: &App) -> InputContext {
        if self.menu {
            InputContext::Menu
        } else if self.subject.read(cx).focus_handle(cx).is_focused(window) {
            InputContext::Subject
        } else if self.body.read(cx).focus_handle(cx).is_focused(window) {
            InputContext::Body
        } else {
            InputContext::Files
        }
    }
    fn composing(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let subject = self.subject.update(cx, |state, cx| {
            state.marked_text_range(window, cx).is_some()
        });
        let body = self.body.update(cx, |state, cx| {
            state.marked_text_range(window, cx).is_some()
        });
        subject || body
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = Key {
            control: event.keystroke.modifiers.control,
            alt: event.keystroke.modifiers.alt,
            shift: event.keystroke.modifiers.shift,
            name: event.keystroke.key.to_ascii_lowercase(),
        };
        self.dispatch_key(key, window, cx);
    }
    fn dispatch_key(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        let context = self.context(window, cx);
        if self.menu && matches!(key.name.as_str(), "up" | "down") {
            self.menu_row ^= 1;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let action = input::resolve(&input::defaults(), context, &key);
        if matches!(action, Some(Action::Submit)) && self.composing(window, cx) {
            cx.stop_propagation();
            return;
        }
        match action {
            Some(Action::Submit) => {
                (self.on_intent)(
                    Intent::Submit {
                        subject: self.subject.read(cx).text().to_string(),
                        body: self.body.read(cx).text().to_string(),
                    },
                    cx,
                );
            }
            Some(Action::SwitchField) => {
                if context == InputContext::Subject {
                    self.body.read(cx).focus_handle(cx).focus(window, cx);
                } else {
                    self.subject.read(cx).focus_handle(cx).focus(window, cx);
                }
            }
            Some(Action::Menu) => {
                self.menu = true;
                self.focus.focus(window, cx);
            }
            Some(Action::Escape) if self.menu => {
                self.menu = false;
                self.subject.read(cx).focus_handle(cx).focus(window, cx);
            }
            Some(Action::Escape) => {
                (self.on_intent)(Intent::Cancel, cx);
            }
            Some(Action::CloseWindow) => {
                (self.on_intent)(Intent::CloseWindow, cx);
            }
            Some(Action::Unsupported(reason)) => {
                (self.on_intent)(Intent::Diagnostic(reason), cx);
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}
impl Render for CommitControls {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("commit-controls")
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::key))
            .capture_action(cx.listener(|view, enter: &Enter, window, cx| {
                let key = Key::parse(if enter.secondary {
                    "<ctrl+enter>"
                } else {
                    "<enter>"
                })
                .unwrap()
                .unwrap();
                view.dispatch_key(key, window, cx);
            }))
            .capture_action(cx.listener(|view, _: &IndentInline, window, cx| {
                view.dispatch_key(Key::parse("<tab>").unwrap().unwrap(), window, cx);
            }))
            .child(Input::new(&self.subject).id("commit-subject"))
            .child(div().id("commit-body").child(Textarea::new(&self.body)))
            .when(self.menu, |div| div.child(div_menu(self.menu_row)))
    }
}
fn div_menu(row: usize) -> impl IntoElement {
    div()
        .id("commit-menu")
        .aria_label("Commit menu")
        .test_support()
        .child(if row == 0 {
            "Commit options"
        } else {
            "Previous messages"
        })
}
#[cfg(test)]
#[path = "tests/commit_controls.rs"]
mod tests;
