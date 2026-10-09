//! Native editable controls. Draft edits and workflow actions go through callbacks;
//! this view never constructs a Git client or performs transport work.
use crate::{
    commit::{Draft, Warning},
    input::{Context as InputContext, Key},
    lazygit_config::M1Settings,
};
use gpui_kit::{
    App, AppContext, ClipboardItem, Context, Entity, EntityInputHandler, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled,
    Subscription, TestSupportExt, Window,
    base::Disableable,
    base::input::{Enter, IndentInline, InputEvent, InputState, TextareaState},
    component::{
        button::Button,
        input::{Input, Textarea},
    },
    div,
    prelude::FluentBuilder,
};
#[derive(Debug)]
pub enum Intent {
    Changed {
        subject: String,
        body: String,
    },
    Submit {
        subject: String,
        body: String,
    },
    ConfirmStageAll,
    Cancel,
    CloseWindow,
    /// Outside editable controls Ctrl+O is copy, never the commit options menu.
    Copy,
    Diagnostic(&'static str),
}
type IntentHandler = Box<dyn Fn(Intent, &mut App)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlAction {
    Submit,
    SwitchField,
    OpenMenu,
    Cancel,
    Accept,
    Previous,
    Next,
    RecallPrevious,
    RecallNext,
    Copy,
    Close,
}
/// One source of truth for dispatch, shortcut help and clickable labels.
#[derive(Clone, Copy)]
struct Shortcut {
    namespace: &'static str,
    name: &'static str,
    action: ControlAction,
    title: &'static str,
}
impl Shortcut {
    fn universal(name: &'static str, action: ControlAction, title: &'static str) -> Self {
        Self {
            namespace: "universal",
            name,
            action,
            title,
        }
    }
    fn permits(self, key: &Key) -> bool {
        // Universal navigation also contains j/k. In an editor these are text,
        // and modified arrows remain selection/cursor shortcuts owned by Kit.
        !matches!(
            self.action,
            ControlAction::RecallPrevious | ControlAction::RecallNext
        ) || (!key.control && !key.alt && !key.shift && matches!(key.name.as_str(), "up" | "down"))
    }
}
pub struct CommitControls {
    subject: Entity<InputState>,
    body: Entity<TextareaState>,
    focus: FocusHandle,
    menu: bool,
    menu_row: usize,
    return_focus: InputContext,
    pending_paste: Option<Draft>,
    history: Vec<Draft>,
    history_index: Option<usize>,
    saved_draft: Draft,
    settings: M1Settings,
    busy: bool,
    warning: Option<Warning>,
    error: Option<String>,
    _changes: Vec<Subscription>,
    on_intent: IntentHandler,
}
impl CommitControls {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        on_intent: impl Fn(Intent, &mut App) + 'static,
    ) -> Self {
        Self::new_with_draft(
            window,
            cx,
            Draft::default(),
            M1Settings::default(),
            on_intent,
        )
    }
    /// Recreate controls using the retained owner's draft; creation emits no edit.
    pub fn new_with_draft(
        window: &mut Window,
        cx: &mut Context<Self>,
        draft: Draft,
        settings: M1Settings,
        on_intent: impl Fn(Intent, &mut App) + 'static,
    ) -> Self {
        let subject = cx.new(|cx| InputState::new(window, cx).default_value(draft.subject.clone()));
        let body = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(4)
                .default_value(draft.body.clone())
        });
        let subject_changes = cx.subscribe(&subject, |view: &mut Self, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                view.changed(cx);
            }
        });
        let body_changes = cx.subscribe(&body, |view: &mut Self, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                view.changed(cx);
            }
        });
        subject.read(cx).focus_handle(cx).focus(window, cx);
        let focus = cx.focus_handle();
        let weak = cx.entity().downgrade();
        let window_handle = window.window_handle();
        // Raw capture_key_down runs *after* Kit actions. Escape can already have
        // cleared marked text by then. Resolve workflows before action dispatch,
        // scoped to this window/focus subtree, and leave unhandled editing to Kit.
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != window_handle {
                return;
            }
            if let Some(view) = weak.upgrade() {
                view.update(cx, |view, cx| {
                    if view.focus.contains_focused(window, cx) {
                        view.dispatch_key(
                            Key {
                                control: event.keystroke.modifiers.control,
                                alt: event.keystroke.modifiers.alt,
                                shift: event.keystroke.modifiers.shift,
                                name: event.keystroke.key.to_ascii_lowercase(),
                            },
                            window,
                            cx,
                        );
                    }
                });
            }
        });
        Self {
            subject,
            body,
            focus,
            menu: false,
            menu_row: 0,
            return_focus: InputContext::Subject,
            pending_paste: None,
            history: vec![],
            history_index: None,
            saved_draft: draft,
            settings,
            busy: false,
            warning: None,
            error: None,
            _changes: vec![subject_changes, body_changes, keys],
            on_intent: Box::new(on_intent),
        }
    }
    pub fn draft(&self, cx: &App) -> Draft {
        Draft {
            subject: self.subject.read(cx).text().to_string(),
            body: self.body.read(cx).text().to_string(),
        }
    }
    /// Owner-to-view synchronization; set_value emits no Change, avoiding callback loops.
    pub fn set_draft(&mut self, draft: &Draft, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft(cx) == *draft {
            return;
        }
        self.subject.update(cx, |state, cx| {
            state.set_value(draft.subject.clone(), window, cx)
        });
        self.body.update(cx, |state, cx| {
            state.set_value(draft.body.clone(), window, cx)
        });
        cx.notify();
    }
    /// Newest first, supplied by the owning feature/host, never fabricated menu data.
    pub fn set_history(&mut self, history: Vec<Draft>) {
        if self.history != history {
            self.history = history;
            self.history_index = None;
        }
    }
    /// Use the actual newest-first read-only history list, including full bodies.
    /// Unsupported non-UTF-8 messages are not silently converted into editable text.
    pub fn set_history_records(&mut self, records: &[crate::git::Commit]) {
        self.set_history(records.iter().filter_map(Draft::from_record).collect());
    }
    pub fn set_feedback(
        &mut self,
        busy: bool,
        warning: Option<Warning>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entering_warning = warning.is_some() && self.warning.is_none();
        let leaving_warning = warning.is_none() && self.warning.is_some();
        if entering_warning {
            self.return_focus = self.context(window, cx);
            self.menu = false;
            self.pending_paste = None;
            self.focus.focus(window, cx);
        } else if leaving_warning {
            self.restore_focus(window, cx);
        }
        self.busy = busy;
        self.warning = warning;
        self.error = error;
        cx.notify();
    }
    pub fn set_settings(&mut self, settings: M1Settings, cx: &mut Context<Self>) {
        self.settings = settings;
        cx.notify();
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        let Draft { subject, body } = self.draft(cx);
        (self.on_intent)(Intent::Changed { subject, body }, cx);
        cx.notify();
    }
    fn context(&self, window: &Window, cx: &App) -> InputContext {
        if self.menu || self.pending_paste.is_some() || self.warning.is_some() {
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
        let subject = self
            .subject
            .update(cx, |s, cx| s.marked_text_range(window, cx).is_some());
        let body = self
            .body
            .update(cx, |s, cx| s.marked_text_range(window, cx).is_some());
        subject || body
    }
    fn shortcuts(&self, context: InputContext) -> Vec<Shortcut> {
        use ControlAction::*;
        let universal = Shortcut::universal;
        match context {
            InputContext::Subject | InputContext::Body => {
                // Resolve contextual bindings before *any* universal binding.
                let mut shortcuts = vec![
                    Shortcut {
                        namespace: "commitMessage",
                        name: "commitMenu",
                        action: OpenMenu,
                        title: "Commit options",
                    },
                    universal("confirmInEditor", Submit, "Commit"),
                ];
                if context == InputContext::Subject {
                    shortcuts.push(universal("submitEditorText", Submit, "Commit"));
                }
                shortcuts.extend([
                    universal("togglePanel", SwitchField, "Other field"),
                    universal("return", Cancel, "Cancel (keep draft)"),
                ]);
                if context == InputContext::Subject {
                    shortcuts.extend([
                        universal("prevItem", RecallPrevious, "Previous message"),
                        universal("nextItem", RecallNext, "Next message"),
                    ]);
                }
                shortcuts
            }
            InputContext::Menu => {
                let mut shortcuts = vec![
                    universal("return", Cancel, "Keep draft / dismiss"),
                    // A stage-all warning is a confirmation, not an options menu:
                    // confirmMenu must not bypass a rebound/disabled confirm key.
                    universal(
                        if self.menu { "confirmMenu" } else { "confirm" },
                        Accept,
                        if self.warning.is_some() {
                            "Stage all and commit"
                        } else {
                            "Accept"
                        },
                    ),
                ];
                if self.menu {
                    shortcuts.extend([
                        universal("prevItem", Previous, "Previous option"),
                        universal("nextItem", Next, "Next option"),
                    ]);
                }
                shortcuts
            }
            _ => vec![
                universal("copyToClipboard", Copy, "Copy"),
                universal("quit", Close, "Close window"),
            ],
        }
    }
    fn resolve(&self, context: InputContext, key: &Key) -> Option<Shortcut> {
        self.shortcuts(context).into_iter().find(|shortcut| {
            shortcut.permits(key)
                && self
                    .settings
                    .binding(shortcut.namespace, shortcut.name)
                    .contains(key)
        })
    }
    fn shortcut_label(&self, context: InputContext, action: ControlAction, title: &str) -> String {
        let mut keys = Vec::new();
        for shortcut in self
            .shortcuts(context)
            .into_iter()
            .filter(|s| s.action == action)
        {
            for key in self.settings.binding(shortcut.namespace, shortcut.name) {
                if self
                    .resolve(context, key)
                    .is_some_and(|s| s.action == action)
                {
                    let label = format!(
                        "{}{}{}{}",
                        if key.control { "Ctrl+" } else { "" },
                        if key.alt { "Alt+" } else { "" },
                        if key.shift { "Shift+" } else { "" },
                        key.name
                    );
                    if !keys.contains(&label) {
                        keys.push(label);
                    }
                }
            }
        }
        if keys.is_empty() {
            format!("{title} (shortcut disabled)")
        } else {
            format!("{title} ({})", keys.join(", "))
        }
    }
    fn shortcut_help(&self, context: InputContext) -> String {
        let mut actions = Vec::new();
        self.shortcuts(context)
            .into_iter()
            .filter_map(|shortcut| {
                if actions.contains(&shortcut.action) {
                    return None;
                }
                actions.push(shortcut.action);
                Some(self.shortcut_label(context, shortcut.action, shortcut.title))
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
    fn restore_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.return_focus == InputContext::Body {
            self.body.read(cx).focus_handle(cx).focus(window, cx);
        } else {
            self.subject.read(cx).focus_handle(cx).focus(window, cx);
        }
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.composing(window, cx) {
            return;
        }
        self.changed(cx);
        let Draft { subject, body } = self.draft(cx);
        (self.on_intent)(Intent::Submit { subject, body }, cx);
    }
    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.changed(cx);
        (self.on_intent)(Intent::Cancel, cx);
    }
    fn recall(&mut self, previous: bool, window: &mut Window, cx: &mut Context<Self>) {
        let index = if previous {
            let next = self.history_index.map(|i| i + 1).unwrap_or(0);
            if next >= self.history.len() {
                return;
            }
            if self.history_index.is_none() {
                self.saved_draft = self.draft(cx);
            }
            Some(next)
        } else {
            match self.history_index {
                None => return,
                Some(0) => None,
                Some(i) => Some(i - 1),
            }
        };
        let draft = index
            .map(|i| self.history[i].clone())
            .unwrap_or_else(|| self.saved_draft.clone());
        self.history_index = index;
        self.set_draft(&draft, window, cx);
        self.changed(cx);
    }
    fn menu_action(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = false;
        match row {
            0 => cx.write_to_clipboard(ClipboardItem::new_string(
                self.draft(cx).message(&self.settings.message),
            )),
            1 => {
                if let Some(message) = cx.read_from_clipboard().and_then(|item| item.text())
                    && !message.is_empty()
                {
                    let draft = Draft::from_message(&message.replace("\r\n", "\n"));
                    if self.draft(cx) == Draft::default() {
                        self.set_draft(&draft, window, cx);
                        self.changed(cx);
                    } else {
                        self.pending_paste = Some(draft);
                        self.focus.focus(window, cx);
                    }
                }
            }
            2 => (self.on_intent)(
                Intent::Diagnostic("External editor commit workflow is unavailable in M1."),
                cx,
            ),
            _ => (self.on_intent)(
                Intent::Diagnostic("Co-author selection is unavailable in M1."),
                cx,
            ),
        }
        if self.pending_paste.is_none() {
            self.restore_focus(window, cx);
        }
        cx.notify();
    }
    fn dispatch_key(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        let context = self.context(window, cx);
        if matches!(context, InputContext::Subject | InputContext::Body)
            && self.composing(window, cx)
        {
            // Let Kit/the IME handle candidate navigation, Escape, Enter and Tab.
            // Do not consume these as commit workflows, even when rebound.
            return;
        }
        let Some(shortcut) = self.resolve(context, &key) else {
            if context == InputContext::Menu {
                // Unknown popup keys cannot reach the editor or global commands.
                cx.stop_propagation();
            }
            return;
        };
        use ControlAction::*;
        match shortcut.action {
            Submit => self.submit(window, cx),
            SwitchField => {
                if context == InputContext::Subject {
                    self.body.read(cx).focus_handle(cx).focus(window, cx);
                } else {
                    self.subject.read(cx).focus_handle(cx).focus(window, cx);
                }
            }
            OpenMenu => self.open_menu(context, window, cx),
            Cancel => {
                if self.menu {
                    self.menu = false;
                } else if self.pending_paste.is_some() {
                    self.pending_paste = None;
                } else {
                    self.cancel(cx);
                }
                if context == InputContext::Menu {
                    self.restore_focus(window, cx);
                }
            }
            Accept => {
                if let Some(draft) = self.pending_paste.take() {
                    self.set_draft(&draft, window, cx);
                    self.changed(cx);
                    self.restore_focus(window, cx);
                } else if self.menu {
                    self.menu_action(self.menu_row, window, cx);
                } else if self.warning.is_some() && !self.busy {
                    (self.on_intent)(Intent::ConfirmStageAll, cx);
                }
            }
            Previous => self.menu_row = (self.menu_row + 3) % 4,
            Next => self.menu_row = (self.menu_row + 1) % 4,
            RecallPrevious => self.recall(true, window, cx),
            RecallNext => self.recall(false, window, cx),
            Copy => (self.on_intent)(Intent::Copy, cx),
            Close => (self.on_intent)(Intent::CloseWindow, cx),
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn open_menu(&mut self, context: InputContext, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(window, cx) {
            return;
        }
        self.menu = true;
        self.menu_row = 0;
        self.return_focus = context;
        self.focus.focus(window, cx);
        cx.notify();
    }
}
impl Render for CommitControls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = self.draft(cx);
        let context = self.context(window, cx);
        let editor_context = if context == InputContext::Body {
            context
        } else {
            InputContext::Subject
        };
        let popup = self.menu || self.pending_paste.is_some() || self.warning.is_some();
        div().id("commit-controls").flex().flex_col().track_focus(&self.focus)
            .capture_action(cx.listener(|view, enter: &Enter, window, cx| {
                let key = Key { control: enter.secondary, alt: false, shift: enter.shift, name: "enter".into() };
                view.dispatch_key(key, window, cx);
            }))
            .capture_action(cx.listener(|view, _: &IndentInline, window, cx| {
                view.dispatch_key(Key::parse("<tab>").unwrap().unwrap(), window, cx);
            }))
            .child(Input::new(&self.subject).id("commit-subject").disabled(popup))
            .child(div().id("commit-body").child(Textarea::new(&self.body).disabled(popup)))
            .when(self.settings.show_commit_length, |container| container.child(div().id("commit-length").child(format!("Subject: {} characters", draft.subject.chars().count()))))
            .when(self.settings.message.auto_wrap_commit_message && !draft.body.is_empty(), |container| container.child(div().id("commit-message-preview").child(draft.message(&self.settings.message))))
            .when(!self.settings.skip_hook_prefix.is_empty() && draft.subject.starts_with(&self.settings.skip_hook_prefix), |container| container.child("Configured skipHookPrefix skips pre-commit and commit-msg hooks (--no-verify)"))
            .child(div().id("commit-shortcut-help").aria_label(self.shortcut_help(context)).test_support().child(self.shortcut_help(context)))
            .child(Button::new("commit-submit").label(if self.busy { "Committing…".into() } else { self.shortcut_label(editor_context, ControlAction::Submit, "Commit") }).disabled(self.busy || popup).on_click(cx.listener(|view, _, window, cx| view.submit(window, cx))))
            .child(Button::new("commit-options").label(self.shortcut_label(editor_context, ControlAction::OpenMenu, "Commit options")).disabled(popup).on_click(cx.listener(move |view, _, window, cx| view.open_menu(editor_context, window, cx))))
            .child(Button::new("commit-cancel").label(self.shortcut_label(if self.warning.is_some() { InputContext::Menu } else { editor_context }, ControlAction::Cancel, "Cancel (keep draft)")).disabled(self.menu || self.pending_paste.is_some()).on_click(cx.listener(|view, _, _, cx| view.cancel(cx))))
            .when_some(self.error.clone(), |container, error| container.child(div().id("commit-error").child(error)))
            .when(self.warning.is_some(), |container| container.child(div().id("commit-stage-all-warning").test_support().child("No staged files. Stage all changes and commit?").child(Button::new("commit-confirm-stage-all").label(self.shortcut_label(InputContext::Menu, ControlAction::Accept, "Stage all and commit")).disabled(self.busy).on_click(cx.listener(|view, _, _, cx| (view.on_intent)(Intent::ConfirmStageAll, cx))))))
            .when(self.menu, |container| container.child(div().id("commit-menu").aria_label("Commit options").test_support().children(["Copy message", "Paste message", "External editor (unavailable)", "Add co-author (unavailable)"].into_iter().enumerate().map(|(row, label)| Button::new(("commit-menu-row", row)).label(if row == self.menu_row { format!("› {label}") } else { label.into() }).on_click(cx.listener(move |view, _, window, cx| view.menu_action(row, window, cx)))))))
            .when(self.pending_paste.is_some(), |container| container.child(div().id("commit-paste-warning").test_support().child(format!("Replace current draft with clipboard message? {}; {}.", self.shortcut_label(InputContext::Menu, ControlAction::Accept, "Replace"), self.shortcut_label(InputContext::Menu, ControlAction::Cancel, "Keep draft")))))
    }
}
#[cfg(test)]
#[path = "tests/commit_controls.rs"]
mod tests;
