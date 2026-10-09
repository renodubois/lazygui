//! Repository composition and local focus/ranges. Owners decide every workflow.
use gpui_kit::{
    App, AppContext, ClickEvent, ClipboardItem, Context, Entity, EntityInputHandler, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window,
    base::input::{InputEvent, InputState},
    component::input::Input,
    div,
    prelude::FluentBuilder,
    px, relative, rgb, rgba,
};
use lazygui::{
    commit::{Commit, Outcome},
    commit_controls::{CommitControls, Intent as CommitIntent},
    git::{Head, Side, Whitespace},
    history::History,
    input::Key,
    lazygit_config::{FilterMode, M1Settings},
    repository::Repository,
    working_tree::{SelectionMode, WorkingTree},
};
use std::{collections::BTreeSet, ffi::OsString, path::PathBuf};

mod actions;

pub(super) enum HostIntent {
    Switch(PathBuf),
    Close,
    CloseConfirmed,
}
type HostHandler = Box<dyn Fn(HostIntent, &mut Window, &mut App)>;
fn format_key(key: &Key) -> String {
    format!(
        "{}{}{}{}",
        if key.control { "Ctrl+" } else { "" },
        if key.alt { "Alt+" } else { "" },
        if key.shift { "Shift+" } else { "" },
        key.name
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Status,
    Files,
    Branches,
    Commits,
    Stash,
    Diff,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Popup {
    Help,
    Filter,
    Path,
    Commit,
    Quit,
    StatusFilter,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Up,
    Down,
    RangeUp,
    RangeDown,
    Range,
    Next,
    Prev,
    Main,
    NextTab,
    PrevTab,
    ToggleHunk,
    PrevHunk,
    NextHunk,
    Panel(usize),
    Tab(usize),
    Select,
    Enter,
    Escape,
    Confirm,
    Help,
    Filter,
    StatusFilter,
    Path,
    Commit,
    Refresh,
    ReloadConfig,
    Tree,
    Whitespace,
    ContextUp,
    ContextDown,
    Copy,
    Layout,
    Close,
    Unsupported(&'static str),
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum FileRow {
    Directory(PathBuf),
    File(OsString),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StatusFilter {
    #[default]
    All,
    Staged,
    Unstaged,
    Untracked,
    Conflicted,
}
impl StatusFilter {
    const CHOICES: [(Self, &'static str); 5] = [
        (Self::All, "All"),
        (Self::Staged, "Staged"),
        (Self::Unstaged, "Unstaged"),
        (Self::Untracked, "Untracked"),
        (Self::Conflicted, "Conflicted"),
    ];
}
struct DiffAnchor {
    path: OsString,
    side: Side,
    canonical: Vec<u8>,
    id: usize,
}
struct Features {
    tree: Entity<WorkingTree>,
    history: Entity<History>,
    commit: Entity<Commit>,
}
pub(super) struct RepositoryView {
    repository: Entity<Repository>,
    features: Option<Features>,
    settings: M1Settings,
    focus: Focus,
    handle: FocusHandle,
    popup: Option<Popup>,
    prompt: Entity<InputState>,
    controls: Option<Entity<CommitControls>>,
    controls_feedback: Option<(bool, Option<lazygui::commit::Warning>, Option<String>)>,
    controls_draft: Option<lazygui::commit::Draft>,
    commit_completion: Option<String>,
    filter: String,
    status_filter: StatusFilter,
    status_filter_row: usize,
    directory_cursor: Option<PathBuf>,
    files_tab: usize,
    branches_tab: usize,
    commits_tab: usize,
    tree: bool,
    collapsed: BTreeSet<PathBuf>,
    file_anchor: Option<OsString>,
    selected_files: BTreeSet<OsString>,
    diff_anchor: Option<DiffAnchor>,
    sticky: bool,
    commit_row: usize,
    horizontal: bool,
    diagnostic: Option<String>,
    restore_focus: bool,
    close_intent: bool,
    on_host: HostHandler,
    _repository: Subscription,
    _prompt: Subscription,
    _keys: Subscription,
    _observations: Vec<Subscription>,
}
impl RepositoryView {
    pub(super) fn new(
        repository: Entity<Repository>,
        window: &mut Window,
        cx: &mut Context<Self>,
        on_host: impl Fn(HostIntent, &mut Window, &mut App) + 'static,
    ) -> Self {
        let handle = cx.focus_handle();
        handle.focus(window, cx);
        let prompt = cx.new(|cx| InputState::new(window, cx));
        let subscription =
            cx.subscribe_in(
                &prompt,
                window,
                |view, input, event, _window, cx| match event {
                    InputEvent::Change if view.popup == Some(Popup::Filter) => {
                        view.filter = input.read(cx).text().to_string();
                        view.reconcile_files(cx);
                        cx.notify();
                    }
                    _ => {}
                },
            );
        let observation = cx.observe(&repository, |_, _, cx| cx.notify());
        let weak = cx.entity().downgrade();
        let window_handle = window.window_handle();
        // Intercept before Kit actions, which can clear marked text before a
        // capture_key_down handler runs. Never intercept another window/input.
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != window_handle {
                return;
            }
            if let Some(view) = weak.upgrade() {
                view.update(cx, |view, cx| {
                    if view.handle.contains_focused(window, cx) {
                        view.key(&event.keystroke, window, cx);
                    }
                });
            }
        });
        Self {
            repository,
            features: None,
            settings: M1Settings::default(),
            focus: Focus::Files,
            handle,
            popup: None,
            prompt,
            controls: None,
            controls_feedback: None,
            controls_draft: None,
            commit_completion: None,
            filter: String::new(),
            status_filter: StatusFilter::All,
            status_filter_row: 0,
            directory_cursor: None,
            files_tab: 0,
            branches_tab: 0,
            commits_tab: 0,
            tree: true,
            collapsed: BTreeSet::new(),
            file_anchor: None,
            selected_files: BTreeSet::new(),
            diff_anchor: None,
            sticky: false,
            commit_row: 0,
            horizontal: false,
            diagnostic: None,
            restore_focus: false,
            close_intent: false,
            on_host: Box::new(on_host),
            _repository: observation,
            _prompt: subscription,
            _keys: keys,
            _observations: vec![],
        }
    }
    pub(super) fn set_settings(&mut self, settings: M1Settings, cx: &mut Context<Self>) {
        self.tree = settings.panels.show_file_tree;
        self.horizontal = matches!(
            settings.panels.main_panel_split_mode,
            lazygui::lazygit_config::SplitMode::Vertical
        );
        self.settings = settings;
        cx.notify();
    }
    pub(super) fn set_features(
        &mut self,
        tree: Entity<WorkingTree>,
        history: Entity<History>,
        commit: Entity<Commit>,
        settings: M1Settings,
        cx: &mut Context<Self>,
    ) {
        // Controls capture their commit owner and settings. A valid replacement
        // closes the old overlay; reopening uses the retained draft/new policies.
        if self.popup == Some(Popup::Commit) {
            if let Some(controls) = &self.controls {
                let draft = controls.read(cx).draft(cx);
                commit.update(cx, |owner, cx| {
                    owner.set_draft(draft.subject, draft.body);
                    cx.notify();
                });
            }
            self.popup = None;
            self.controls = None;
            self.controls_feedback = None;
            self.restore_focus = true;
        }
        self.commit_completion = None;
        self.diff_anchor = None;
        self._observations = vec![
            cx.observe(&tree, |view, owner, cx| {
                let paths: BTreeSet<_> = owner
                    .read(cx)
                    .entries()
                    .iter()
                    .map(|e| e.path.clone())
                    .collect();
                view.selected_files.retain(|path| paths.contains(path));
                if view
                    .file_anchor
                    .as_ref()
                    .is_some_and(|p| !paths.contains(p))
                {
                    view.file_anchor = None;
                }
                view.reconcile_files(cx);
                view.reconcile_diff_anchor(cx);
                cx.notify();
            }),
            cx.observe(&history, |_, _, cx| cx.notify()),
            cx.observe(&commit, |view, owner, cx| {
                if let Some(Outcome::Committed {
                    head: Head::Branch { oid, .. } | Head::Detached { oid },
                    ..
                }) = owner.read(cx).outcome()
                    && view.commit_completion.as_ref() != Some(oid)
                {
                    view.commit_completion = Some(oid.clone());
                    if owner.read(cx).draft().subject.is_empty() {
                        view.popup = None;
                        view.controls = None;
                        view.restore_focus = true;
                    }
                }
                cx.notify();
            }),
        ];
        self.features = Some(Features {
            tree,
            history,
            commit,
        });
        self.set_settings(settings, cx);
    }
    pub(super) fn clear_features(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(window, cx);
        self.features = None;
        self.commit_completion = None;
        self._observations.clear();
        self.selected_files.clear();
        self.file_anchor = None;
        self.diff_anchor = None;
        self.sticky = false;
        self.filter.clear();
        self.status_filter = StatusFilter::All;
        self.directory_cursor = None;
        self.files_tab = 0;
        self.branches_tab = 0;
        self.commits_tab = 0;
        self.collapsed.clear();
        self.diagnostic = None;
        cx.notify();
    }
    pub(super) fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.settings.confirm_on_quit {
            return false;
        }
        self.dismiss(window, cx);
        self.popup = Some(Popup::Quit);
        self.handle.focus(window, cx);
        cx.notify();
        true
    }
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Change events may still be queued when a captured confirm/dismiss key
        // arrives; read the real input instead of losing its final edit.
        if self.popup == Some(Popup::Filter) {
            self.filter = self.prompt.read(cx).text().to_string();
            self.reconcile_files(cx);
        }
        if self.popup == Some(Popup::Commit)
            && let (Some(features), Some(controls)) = (&self.features, &self.controls)
        {
            let draft = controls.read(cx).draft(cx);
            features.commit.update(cx, |owner, cx| {
                owner.set_draft(draft.subject, draft.body);
                cx.notify();
            });
        }
        self.popup = None;
        self.controls = None;
        self.controls_feedback = None;
        self.restore_focus = false;
        self.handle.focus(window, cx);
        cx.notify();
    }
    fn show_prompt(&mut self, popup: Popup, window: &mut Window, cx: &mut Context<Self>) {
        let value = if popup == Popup::Filter {
            self.filter.clone()
        } else {
            String::new()
        };
        self.prompt
            .update(cx, |input, cx| input.set_value(value, window, cx));
        self.popup = Some(popup);
        self.prompt.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    fn show_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(features) = &self.features else {
            return;
        };
        let commit = features.commit.clone();
        commit.update(cx, |owner, cx| {
            owner.prepare_draft();
            cx.notify();
        });
        let weak = cx.entity().downgrade();
        let draft = commit.read(cx).draft().clone();
        self.controls_draft = Some(draft.clone());
        let controls = cx.new(|cx| {
            CommitControls::new_with_draft(
                window,
                cx,
                draft,
                self.settings.clone(),
                move |intent, cx| match intent {
                    CommitIntent::Submit { subject, body } => commit.update(cx, |owner, cx| {
                        owner.set_draft(subject, body);
                        owner.submit();
                        cx.notify();
                    }),
                    CommitIntent::Changed { subject, body } => commit.update(cx, |owner, cx| {
                        owner.set_draft(subject, body);
                        cx.notify();
                    }),
                    CommitIntent::ConfirmStageAll => commit.update(cx, |owner, cx| {
                        owner.confirm_stage_all();
                        cx.notify();
                    }),
                    CommitIntent::Copy => {
                        let _ = weak.update(cx, |view, cx| view.copy(cx));
                    }
                    CommitIntent::Cancel => {
                        commit.update(cx, |owner, cx| {
                            owner.cancel();
                            cx.notify();
                        });
                        let _ = weak.update(cx, |view, cx| {
                            view.popup = None;
                            view.controls = None;
                            view.restore_focus = true;
                            cx.notify();
                        });
                    }
                    CommitIntent::CloseWindow => {
                        let _ = weak.update(cx, |view, cx| {
                            view.close_intent = true;
                            cx.notify();
                        });
                    }
                    CommitIntent::Diagnostic(reason) => {
                        let _ = weak.update(cx, |view, cx| {
                            view.diagnostic = Some(reason.into());
                            cx.notify();
                        });
                    }
                },
            )
        });
        if let Some(features) = &self.features {
            let history = features
                .history
                .read(cx)
                .commits()
                .iter()
                .filter_map(lazygui::commit::Draft::from_record)
                .collect();
            controls.update(cx, |view, _| view.set_history(history));
        }
        self.controls_feedback = None;
        self.restore_focus = false;
        self.controls = Some(controls);
        self.popup = Some(Popup::Commit);
        cx.notify();
    }
    fn files(&self, cx: &App) -> Vec<OsString> {
        self.matching_files(cx)
            .into_iter()
            .filter(|path| {
                !self.tree
                    || !PathBuf::from(path)
                        .ancestors()
                        .skip(1)
                        .any(|p| self.collapsed.contains(p))
            })
            .collect()
    }
    fn matching_files(&self, cx: &App) -> Vec<OsString> {
        let Some(features) = &self.features else {
            return vec![];
        };
        let needle = self.filter.to_lowercase();
        let mut paths: Vec<_> = features
            .tree
            .read(cx)
            .entries()
            .iter()
            .filter(|entry| {
                let status_matches = match self.status_filter {
                    StatusFilter::All => true,
                    StatusFilter::Staged => !matches!(entry.index, b' ' | b'?'),
                    StatusFilter::Unstaged => !matches!(entry.worktree, b' ' | b'?'),
                    StatusFilter::Untracked => entry.index == b'?' || entry.worktree == b'?',
                    StatusFilter::Conflicted => {
                        entry.index == b'U'
                            || entry.worktree == b'U'
                            || matches!((entry.index, entry.worktree), (b'A', b'A') | (b'D', b'D'))
                    }
                };
                if !status_matches {
                    return false;
                }
                let text = entry.path.to_string_lossy().to_lowercase();
                match self.settings.panels.filter_mode {
                    FilterMode::Substring => text.contains(&needle),
                    FilterMode::Fuzzy => {
                        let mut chars = text.chars();
                        needle.chars().all(|c| chars.by_ref().any(|x| x == c))
                    }
                }
            })
            .map(|e| e.path.clone())
            .collect();
        if self.settings.panels.file_tree_sort_case_sensitive {
            paths.sort();
        } else {
            paths.sort_by(|a, b| {
                a.to_string_lossy()
                    .to_lowercase()
                    .cmp(&b.to_string_lossy().to_lowercase())
                    .then_with(|| a.cmp(b))
            });
        }
        paths
    }
    fn choose_file(&mut self, path: OsString, range: bool, cx: &mut Context<Self>) {
        let Some(features) = &self.features else {
            return;
        };
        if self.files_tab != 0 || !self.files(cx).contains(&path) {
            return;
        }
        let tree = features.tree.clone();
        self.directory_cursor = None;
        if range || self.sticky {
            let paths = self.files(cx);
            let old = tree.read(cx).selected().map(OsString::from);
            let anchor = self
                .file_anchor
                .get_or_insert_with(|| old.unwrap_or_else(|| path.clone()))
                .clone();
            let Some(b) = paths.iter().position(|p| p == &path) else {
                return;
            };
            let a = paths.iter().position(|p| p == &anchor).unwrap_or(b);
            self.file_anchor = Some(paths[a].clone());
            self.selected_files = paths[a.min(b)..=a.max(b)].iter().cloned().collect();
        } else {
            self.file_anchor = None;
            self.selected_files.clear();
        }
        self.diff_anchor = None;
        self.focus = Focus::Files;
        tree.update(cx, |owner, cx| {
            owner.select(path);
            cx.notify();
        });
        cx.notify();
    }
    fn file_rows(&self, cx: &App) -> Vec<FileRow> {
        if self.files_tab != 0 {
            return vec![];
        }
        let mut rows = Vec::new();
        let mut seen = BTreeSet::new();
        for path in self.matching_files(cx) {
            let relative = PathBuf::from(&path);
            let mut hidden = false;
            if self.tree {
                let mut ancestors: Vec<_> = relative
                    .ancestors()
                    .skip(1)
                    .filter(|p| !p.as_os_str().is_empty())
                    .collect();
                ancestors.reverse();
                for directory in ancestors {
                    if seen.insert(directory.to_path_buf()) {
                        rows.push(FileRow::Directory(directory.to_path_buf()));
                    }
                    if self.collapsed.contains(directory) {
                        hidden = true;
                        break;
                    }
                }
            }
            if !hidden {
                rows.push(FileRow::File(path));
            }
        }
        rows
    }
    fn reconcile_files(&mut self, cx: &mut Context<Self>) {
        let visible: BTreeSet<_> = self.files(cx).into_iter().collect();
        self.selected_files.retain(|p| visible.contains(p));
        if self
            .file_anchor
            .as_ref()
            .is_some_and(|p| !visible.contains(p))
        {
            self.file_anchor = None;
            self.selected_files.clear();
        }
        if self
            .directory_cursor
            .as_ref()
            .is_some_and(|p| !self.file_rows(cx).contains(&FileRow::Directory(p.clone())))
        {
            self.directory_cursor = None;
        }
    }
    pub(super) fn refuse_switch(&mut self, cx: &mut Context<Self>) {
        self.diagnostic = Some("Finish the current mutation before switching repository.".into());
        cx.notify();
    }
    fn choose_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.directory_cursor = Some(path);
        self.selected_files.clear();
        self.file_anchor = None;
        self.diff_anchor = None;
        self.focus = Focus::Files;
        cx.notify();
    }
    fn toggle_directory(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = &self.directory_cursor
            && !self.collapsed.remove(path)
        {
            self.collapsed.insert(path.clone());
        }
        self.reconcile_files(cx);
    }
    fn reconcile_diff_anchor(&mut self, cx: &App) {
        let Some(anchor) = &mut self.diff_anchor else {
            return;
        };
        let remapped = self.features.as_ref().and_then(|features| {
            let owner = features.tree.read(cx);
            if owner.selected() != Some(anchor.path.as_os_str()) {
                return None;
            }
            let pane = owner.pane(anchor.side);
            pane.remap_anchor(&anchor.canonical, anchor.id)
                .map(|id| (id, pane.canonical.clone()))
        });
        if let Some((id, canonical)) = remapped {
            anchor.id = id;
            anchor.canonical = canonical;
        } else {
            self.diff_anchor = None;
        }
    }
    fn range_anchor(&mut self, side: Side, cx: &App) -> Option<usize> {
        self.reconcile_diff_anchor(cx);
        let owner = self.features.as_ref()?.tree.read(cx);
        let path = owner.selected()?;
        let pane = owner.pane(side);
        if self.diff_anchor.as_ref().is_none_or(|a| a.side != side) {
            // Only seed from a current, valid canonical cursor, never clamp an ID.
            let id = pane.remap_anchor(&pane.canonical, pane.cursor)?;
            self.diff_anchor = Some(DiffAnchor {
                path: path.into(),
                side,
                canonical: pane.canonical.clone(),
                id,
            });
        }
        self.diff_anchor.as_ref().map(|a| a.id)
    }
    fn navigate(&mut self, down: bool, range: bool, cx: &mut Context<Self>) {
        let Some(features) = &self.features else {
            return;
        };
        if self.focus == Focus::Files {
            let rows = self.file_rows(cx);
            if rows.is_empty() {
                return;
            }
            let old = self
                .directory_cursor
                .clone()
                .map(FileRow::Directory)
                .or_else(|| {
                    features
                        .tree
                        .read(cx)
                        .selected()
                        .map(|p| FileRow::File(p.into()))
                });
            let index = rows.iter().position(|row| Some(row) == old.as_ref());
            let next = match index {
                Some(i) if down => (i + 1).min(rows.len() - 1),
                Some(i) => i.saturating_sub(1),
                None => 0,
            };
            match &rows[next] {
                FileRow::File(path) => self.choose_file(path.clone(), range, cx),
                FileRow::Directory(path) => self.choose_directory(path.clone(), cx),
            }
        } else if self.focus == Focus::Commits {
            let count = features.history.read(cx).commits().len();
            self.commit_row = if down {
                (self.commit_row + 1).min(count.saturating_sub(1))
            } else {
                self.commit_row.saturating_sub(1)
            };
        } else if self.focus == Focus::Diff {
            let tree = features.tree.clone();
            let owner = tree.read(cx);
            let side = owner.focused_side();
            let pane = owner.pane(side);
            let Some(patch) = &pane.patch else {
                return;
            };
            let old = pane.cursor;
            let next = if pane.selection_mode == SelectionMode::Hunk && !range && !self.sticky {
                let hunks: Vec<_> = patch.hunks().collect();
                let index = hunks
                    .iter()
                    .position(|h| h.changes.contains(&old))
                    .unwrap_or(0);
                let next = if down {
                    (index + 1).min(hunks.len().saturating_sub(1))
                } else {
                    index.saturating_sub(1)
                };
                hunks.get(next).map_or(old, |h| h.changes.start)
            } else if down {
                (old + 1).min(patch.changes().saturating_sub(1))
            } else {
                old.saturating_sub(1)
            };
            let selection = if range || self.sticky {
                let Some(anchor) = self.range_anchor(side, cx) else {
                    return;
                };
                (anchor.min(next)..=anchor.max(next)).collect()
            } else {
                self.diff_anchor = None;
                BTreeSet::new()
            };
            tree.update(cx, |owner, cx| {
                owner.set_cursor(side, next);
                owner.set_selection(side, selection);
                cx.notify();
            });
        }
        cx.notify();
    }
    fn definitions(&self) -> impl Iterator<Item = actions::Definition> + '_ {
        let context = actions::context(self.focus);
        // Resolve the entire contextual keymap before any universal action.
        // This includes contextual overrides of universal action names.
        actions::DEFINITIONS
            .iter()
            .filter(move |d| d.scope == context)
            .copied()
            .chain(
                actions::DEFINITIONS
                    .iter()
                    .filter(move |d| {
                        d.scope == "universal"
                            && self
                                .settings
                                .keybindings
                                .get(context)
                                .is_some_and(|map| map.contains_key(d.name))
                    })
                    .map(move |d| actions::Definition {
                        scope: context,
                        context,
                        ..*d
                    }),
            )
            .chain(
                actions::DEFINITIONS
                    .iter()
                    .filter(|d| d.scope == "universal")
                    .copied(),
            )
    }
    fn resolve(&self, key: &Key) -> Option<Action> {
        self.definitions()
            .find(|d| d.keys(&self.settings).contains(key))
            .map(|d| d.action)
    }
    fn key_label(&self, action: Action) -> String {
        let keys: Vec<_> = self
            .definitions()
            .filter(|d| d.action == action)
            .flat_map(|d| d.keys(&self.settings))
            .filter(|key| self.resolve(key) == Some(action))
            .map(format_key)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        keys.join(", ")
    }
    fn popup_definitions(&self) -> impl Iterator<Item = &'static actions::Definition> {
        let menu = !matches!(self.popup, Some(Popup::Filter | Popup::Path));
        actions::DEFINITIONS
            .iter()
            .filter(move |d| d.scope == "popup" || (menu && d.scope == "menu"))
    }
    fn popup_resolve(&self, key: &Key) -> Option<Action> {
        self.popup_definitions()
            .find(|d| d.keys(&self.settings).contains(key))
            .map(|d| d.action)
    }
    fn popup_label(&self, title: &str, action: Action) -> String {
        let keys = self
            .popup_definitions()
            .filter(|d| d.action == action)
            .flat_map(|d| d.keys(&self.settings))
            .filter(|key| self.popup_resolve(key) == Some(action))
            .map(format_key)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        if keys.is_empty() {
            title.into()
        } else {
            format!("{title} ({keys})")
        }
    }
    fn key(
        &mut self,
        keystroke: &gpui_kit::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut key = Key {
            name: keystroke.key.to_ascii_lowercase(),
            control: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
        };
        // Config spells shifted punctuation as the resulting glyph; native
        // backends can report either that glyph or its physical base key.
        if key.shift {
            let symbol = match key.name.as_str() {
                "/" | "?" => Some("?"),
                "[" | "{" => Some("{"),
                "]" | "}" => Some("}"),
                "=" | "+" => Some("+"),
                "-" | "_" => Some("_"),
                "," | "<" => Some("<"),
                "." | ">" => Some(">"),
                "\\" | "|" => Some("|"),
                _ => None,
            };
            if let Some(symbol) = symbol {
                key.name = symbol.into();
                key.shift = false;
            }
        }
        if let Some(popup) = self.popup {
            if popup == Popup::Commit {
                return;
            }
            if matches!(popup, Popup::Filter | Popup::Path)
                && self.prompt.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
            {
                return;
            }
            let action = self.popup_resolve(&key);
            if action == Some(Action::Escape) {
                self.dismiss(window, cx);
                cx.stop_propagation();
                return;
            }
            if popup == Popup::StatusFilter {
                if action == Some(Action::Down) {
                    self.status_filter_row =
                        (self.status_filter_row + 1) % StatusFilter::CHOICES.len();
                } else if action == Some(Action::Up) {
                    self.status_filter_row = (self.status_filter_row + StatusFilter::CHOICES.len()
                        - 1)
                        % StatusFilter::CHOICES.len();
                }
            }
            if action == Some(Action::Confirm) {
                match popup {
                    Popup::Quit => (self.on_host)(HostIntent::CloseConfirmed, window, cx),
                    Popup::Filter => self.dismiss(window, cx),
                    Popup::Path => {
                        let path = PathBuf::from(self.prompt.read(cx).text().to_string());
                        self.dismiss(window, cx);
                        (self.on_host)(HostIntent::Switch(path), window, cx);
                    }
                    Popup::StatusFilter => {
                        self.status_filter = StatusFilter::CHOICES[self.status_filter_row].0;
                        self.reconcile_files(cx);
                        self.dismiss(window, cx);
                    }
                    _ => {}
                }
                cx.stop_propagation();
            } else if matches!(popup, Popup::Help | Popup::Quit | Popup::StatusFilter) {
                cx.stop_propagation();
                cx.notify();
            }
            // Text/popups are exclusive: repository actions never see their keys.
            return;
        }
        if let Some(action) = self.resolve(&key) {
            self.dispatch(action, window, cx);
            cx.stop_propagation();
        }
    }
    fn dispatch(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.is_some() {
            return;
        }
        if matches!(action, Action::Select | Action::Commit)
            && self
                .repository
                .read(cx)
                .session()
                .is_some_and(|session| session.readiness.identity.worktree.is_none())
        {
            self.diagnostic =
                Some("Bare repository: files, staging and commits are unavailable.".into());
            cx.notify();
            return;
        }
        if self.repository.read(cx).loading() && matches!(action, Action::Select | Action::Commit) {
            self.diagnostic =
                Some("Wait for repository/configuration readiness before changing files.".into());
            cx.notify();
            return;
        }
        match action {
            Action::Up | Action::RangeUp => {
                self.navigate(false, matches!(action, Action::RangeUp), cx)
            }
            Action::Down | Action::RangeDown => {
                self.navigate(true, matches!(action, Action::RangeDown), cx)
            }
            Action::Range => {
                self.sticky = !self.sticky;
                self.file_anchor = None;
                self.diff_anchor = None;
                if !self.sticky {
                    self.selected_files.clear();
                }
            }
            Action::Next | Action::Prev => {
                let panels = [
                    Focus::Status,
                    Focus::Files,
                    Focus::Branches,
                    Focus::Commits,
                    Focus::Stash,
                    Focus::Diff,
                ];
                let index = panels.iter().position(|p| *p == self.focus).unwrap_or(0);
                self.focus =
                    panels[(index + if matches!(action, Action::Next) { 1 } else { 5 }) % 6];
            }
            Action::Panel(index) => {
                self.focus = [
                    Focus::Status,
                    Focus::Files,
                    Focus::Branches,
                    Focus::Commits,
                    Focus::Stash,
                ][index]
            }
            Action::Tab(index) => {
                match self.focus {
                    Focus::Files => self.files_tab = index,
                    Focus::Branches => self.branches_tab = index,
                    Focus::Commits => self.commits_tab = index,
                    _ => {}
                }
                self.directory_cursor = None;
                self.selected_files.clear();
                self.file_anchor = None;
            }
            Action::Main => self.focus = Focus::Diff,
            Action::NextTab | Action::PrevTab if self.focus == Focus::Diff => {
                if let Some(features) = &self.features {
                    features.tree.update(cx, |owner, cx| {
                        owner.set_focus(if owner.focused_side() == Side::Worktree {
                            Side::Index
                        } else {
                            Side::Worktree
                        });
                        cx.notify();
                    });
                    self.diff_anchor = None;
                }
            }
            Action::NextTab | Action::PrevTab => {
                let tab = match self.focus {
                    Focus::Files => Some((&mut self.files_tab, 3)),
                    Focus::Branches => Some((&mut self.branches_tab, 3)),
                    Focus::Commits => Some((&mut self.commits_tab, 2)),
                    _ => None,
                };
                if let Some((tab, count)) = tab {
                    *tab = (*tab
                        + if action == Action::NextTab {
                            1
                        } else {
                            count - 1
                        })
                        % count;
                }
                self.directory_cursor = None;
                self.selected_files.clear();
                self.file_anchor = None;
            }
            Action::Select => self.act(cx),
            Action::Enter if self.focus == Focus::Files && self.directory_cursor.is_some() => {
                self.toggle_directory(cx);
            }
            Action::Enter if self.focus == Focus::Files => {
                if self
                    .features
                    .as_ref()
                    .and_then(|f| f.tree.read(cx).selected())
                    .is_some_and(|p| self.files(cx).iter().any(|visible| visible == p))
                {
                    self.focus = Focus::Diff;
                }
            }
            Action::ToggleHunk if self.focus == Focus::Diff => {
                if let Some(features) = &self.features {
                    features.tree.update(cx, |owner, cx| {
                        let side = owner.focused_side();
                        let mode = if owner.selection_mode(side) == SelectionMode::Hunk {
                            SelectionMode::Line
                        } else {
                            SelectionMode::Hunk
                        };
                        owner.set_selection_mode(side, mode);
                        cx.notify();
                    });
                }
            }
            Action::PrevHunk | Action::NextHunk if self.focus == Focus::Diff => {
                if let Some(features) = &self.features {
                    let tree = features.tree.clone();
                    let owner = tree.read(cx);
                    let side = owner.focused_side();
                    let pane = owner.pane(side);
                    if let Some(patch) = &pane.patch {
                        let hunks: Vec<_> = patch.hunks().collect();
                        let index = hunks
                            .iter()
                            .position(|h| h.changes.contains(&pane.cursor))
                            .unwrap_or(0);
                        let next = if action == Action::NextHunk {
                            (index + 1).min(hunks.len().saturating_sub(1))
                        } else {
                            index.saturating_sub(1)
                        };
                        if let Some(hunk) = hunks.get(next) {
                            let cursor = hunk.changes.start;
                            tree.update(cx, |owner, cx| {
                                owner.set_cursor(side, cursor);
                                cx.notify();
                            });
                        }
                    }
                }
                self.diff_anchor = None;
            }
            Action::Enter if self.focus == Focus::Status => {
                self.show_prompt(Popup::Path, window, cx)
            }
            Action::Escape
                if self.focus == Focus::Files && self.settings.quit_on_top_level_return =>
            {
                (self.on_host)(HostIntent::Close, window, cx);
            }
            Action::Escape => {
                self.focus = Focus::Files;
                self.sticky = false;
                self.selected_files.clear();
                self.file_anchor = None;
                self.diff_anchor = None;
            }
            Action::Help => self.popup = Some(Popup::Help),
            Action::Filter => self.show_prompt(Popup::Filter, window, cx),
            Action::StatusFilter => {
                self.status_filter_row = StatusFilter::CHOICES
                    .iter()
                    .position(|(filter, _)| *filter == self.status_filter)
                    .unwrap_or(0);
                self.popup = Some(Popup::StatusFilter);
            }
            Action::Path => self.show_prompt(Popup::Path, window, cx),
            Action::Commit => self.show_commit(window, cx),
            Action::Refresh => {
                if let Some(features) = &self.features {
                    features.tree.update(cx, |owner, cx| {
                        owner.refresh();
                        cx.notify();
                    });
                    features.history.update(cx, |owner, cx| {
                        owner.refresh();
                        cx.notify();
                    });
                }
            }
            Action::ReloadConfig => {
                if self.features.as_ref().is_some_and(|features| {
                    features.tree.read(cx).busy() || features.commit.read(cx).busy()
                }) {
                    self.diagnostic =
                        Some("Finish the current mutation before reloading configuration.".into());
                } else {
                    self.repository.update(cx, |owner, cx| {
                        owner.reload_config();
                        cx.notify();
                    });
                }
            }
            Action::Tree => {
                self.tree = !self.tree;
                self.collapsed.clear();
                self.directory_cursor = None;
                self.reconcile_files(cx);
            }
            Action::Whitespace | Action::ContextUp | Action::ContextDown => {
                if let Some(features) = &self.features {
                    features.tree.update(cx, |owner, cx| {
                        let mut options = owner.options();
                        match action {
                            Action::Whitespace => {
                                options.whitespace = if options.whitespace == Whitespace::Exact {
                                    Whitespace::IgnoreAllSpace
                                } else {
                                    Whitespace::Exact
                                }
                            }
                            Action::ContextUp => {
                                options.context = options.context.saturating_add(1)
                            }
                            Action::ContextDown => {
                                options.context = options.context.saturating_sub(1)
                            }
                            _ => {}
                        }
                        owner.set_options(options);
                        cx.notify();
                    });
                }
            }
            Action::Copy => self.copy(cx),
            Action::Layout => self.horizontal = !self.horizontal,
            Action::Close => (self.on_host)(HostIntent::Close, window, cx),
            Action::Unsupported(reason) => self.diagnostic = Some(reason.into()),
            _ => {}
        }
        if self.popup != Some(Popup::Commit) {
            self.handle.focus(window, cx);
        }
        if matches!(self.popup, Some(Popup::Filter | Popup::Path)) {
            self.prompt.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }
    fn act(&mut self, cx: &mut Context<Self>) {
        let Some(features) = &self.features else {
            return;
        };
        let tree = features.tree.clone();
        let visible = self.files(cx);
        let target_visible = tree
            .read(cx)
            .selected()
            .is_some_and(|p| visible.iter().any(|v| v == p));
        if !target_visible || self.directory_cursor.is_some() || self.files_tab != 0 {
            self.diagnostic = Some("Select a visible file before changing it.".into());
            cx.notify();
            return;
        }
        if self.focus == Focus::Diff {
            tree.update(cx, |owner, cx| {
                owner.act_current(owner.focused_side());
                cx.notify();
            });
        } else if self.focus == Focus::Files {
            if self.selected_files.iter().any(|p| !visible.contains(p)) {
                self.diagnostic =
                    Some("Selection includes hidden files; select a visible range again.".into());
                cx.notify();
                return;
            }
            let paths: Vec<OsString> = if self.selected_files.is_empty() {
                tree.read(cx)
                    .selected()
                    .map(OsString::from)
                    .into_iter()
                    .collect()
            } else {
                self.selected_files.iter().cloned().collect()
            };
            // A range has one intention, independent of its cursor endpoint:
            // unstage only when every target is staged-only; otherwise stage the
            // unstaged targets (the owner verifies already-staged ones as satisfied).
            let side = if paths.iter().all(|path| {
                tree.read(cx)
                    .entries()
                    .iter()
                    .find(|entry| &entry.path == path)
                    .is_some_and(|entry| entry.worktree == b' ' && entry.index != b' ')
            }) {
                Side::Index
            } else {
                Side::Worktree
            };
            tree.update(cx, |owner, cx| {
                owner.act_file(paths, side);
                cx.notify();
            });
        }
    }
    fn copy(&self, cx: &mut Context<Self>) {
        let Some(features) = &self.features else {
            return;
        };
        let text = match self.focus {
            Focus::Files => features
                .tree
                .read(cx)
                .selected()
                .map(|p| p.to_string_lossy().into_owned()),
            Focus::Commits => features
                .history
                .read(cx)
                .commits()
                .get(self.commit_row)
                .map(|c| c.oid.clone()),
            Focus::Diff => {
                let owner = features.tree.read(cx);
                Some(
                    String::from_utf8_lossy(&owner.pane(owner.focused_side()).display).into_owned(),
                )
            }
            _ => self.repository.read(cx).session().map(|s| {
                s.readiness
                    .identity
                    .worktree
                    .as_ref()
                    .unwrap_or(&s.readiness.identity.git_dir)
                    .display()
                    .to_string()
            }),
        };
        if let Some(text) = text {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
    fn navigation_help(&self) -> String {
        self.definitions()
            .filter_map(|d| {
                let keys = d
                    .keys(&self.settings)
                    .iter()
                    .filter(|key| self.resolve(key) == Some(d.action))
                    .map(format_key)
                    .collect::<Vec<_>>()
                    .join(", ");
                (!keys.is_empty()).then(|| format!("{}: {keys}", d.title))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn control(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        action: Action,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label: SharedString = label.into();
        let keys = self.key_label(action);
        let label: SharedString = if keys.is_empty() {
            label
        } else {
            format!("{label} ({keys})").into()
        };
        let focused = matches!(action, Action::Panel(index) if [Focus::Status, Focus::Files, Focus::Branches, Focus::Commits, Focus::Stash][index] == self.focus);
        div()
            .id(id)
            .aria_label(label.clone())
            .test_support()
            .px_2()
            .py_1()
            .border_1()
            .border_color(rgb(0x454545))
            .when(focused, |control| control.bg(rgb(0x344860)))
            .cursor_pointer()
            .child(label)
            .on_click(cx.listener(move |view, _, window, cx| view.dispatch(action, window, cx)))
    }
    fn panel_header(
        &self,
        id: &'static str,
        title: &'static str,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.control(id, title, Action::Panel(index), cx)
    }
    fn tabs(&self, focus: Focus, cx: &mut Context<Self>) -> impl IntoElement {
        let (index, selected, labels): (_, _, &[&str]) = match focus {
            Focus::Files => (
                1,
                self.files_tab,
                &[
                    "Files",
                    "Worktrees (unavailable)",
                    "Submodules (unavailable)",
                ],
            ),
            Focus::Branches => (
                2,
                self.branches_tab,
                &[
                    "Branches (current only)",
                    "Remotes (unavailable)",
                    "Tags (unavailable)",
                ],
            ),
            Focus::Commits => (3, self.commits_tab, &["Commits", "Reflog (unavailable)"]),
            _ => unreachable!(),
        };
        let mut tabs = div().flex().flex_wrap().gap_1();
        for (tab, label) in labels.iter().enumerate() {
            let label = *label;
            tabs = tabs.child(
                div()
                    .id((SharedString::from(format!("tabs-{index}")), tab))
                    .aria_label(label)
                    .test_support()
                    .cursor_pointer()
                    .when(tab == selected, |d| d.bg(rgb(0x344860)))
                    .child(label)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.dispatch(Action::Panel(index), window, cx);
                        view.dispatch(Action::Tab(tab), window, cx);
                    })),
            );
        }
        tabs
    }
    fn render_files(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div()
            .id("files-list")
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .flex_1();
        let Some(features) = &self.features else {
            return list.child("Repository not ready");
        };
        let owner = features.tree.read(cx);
        if owner.identity().worktree.is_none() {
            return list.child("Bare repository — files/staging unavailable");
        }
        if self.tree && self.settings.panels.show_root_item_in_file_tree {
            let root = owner
                .identity()
                .worktree
                .as_ref()
                .unwrap_or(&owner.identity().git_dir);
            list = list.child(
                div()
                    .id("file-tree-root")
                    .test_support()
                    .child(root.display().to_string()),
            );
        }
        if self.files_tab != 0 {
            return list.child(if self.files_tab == 1 {
                "Worktrees — unavailable in M1"
            } else {
                "Submodules — unavailable in M1"
            });
        }
        let mut directory_index = 0usize;
        let mut visible_index = 0usize;
        for row in self.file_rows(cx) {
            let path = match row {
                FileRow::File(path) => path,
                FileRow::Directory(directory) => {
                    directory_index += 1;
                    let depth = directory.components().count().saturating_sub(1);
                    let label = format!(
                        "{} {}",
                        if self.collapsed.contains(&directory) {
                            "▸"
                        } else {
                            "▾"
                        },
                        directory.file_name().unwrap().to_string_lossy()
                    );
                    list = list.child(
                        div()
                            .id(("directory", directory_index))
                            .aria_label(label.clone())
                            .test_support()
                            .pl(px(depth as f32 * 12.))
                            .cursor_pointer()
                            .when(self.directory_cursor.as_ref() == Some(&directory), |d| {
                                d.bg(rgb(0x344860))
                            })
                            .child(label)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if view.popup.is_some() {
                                    return;
                                }
                                view.choose_directory(directory.clone(), cx);
                                view.toggle_directory(cx);
                                view.handle.focus(window, cx);
                                cx.notify();
                            })),
                    );
                    continue;
                }
            };
            let relative = PathBuf::from(&path);
            let entry = owner.entries().iter().find(|e| e.path == path).unwrap();
            let selected = (self.directory_cursor.is_none()
                && owner.selected() == Some(path.as_os_str()))
                || self.selected_files.contains(&path);
            let label = if self.tree {
                relative.file_name().unwrap().to_string_lossy()
            } else {
                path.to_string_lossy()
            };
            let text = format!(
                "{}{} {}",
                entry.index as char, entry.worktree as char, label
            );
            let depth = if self.tree {
                relative.components().count().saturating_sub(1)
            } else {
                0
            };
            list = list.child(
                div()
                    .id(("file", visible_index))
                    .aria_label(text.clone())
                    .test_support()
                    .pl(px(8. + depth as f32 * 12.))
                    .py_1()
                    .when(selected, |d| d.bg(rgb(0x344860)))
                    .cursor_pointer()
                    .child(text)
                    .on_click(cx.listener(move |view, event: &ClickEvent, window, cx| {
                        if view.popup.is_some() {
                            return;
                        }
                        view.choose_file(path.clone(), event.modifiers().shift, cx);
                        view.handle.focus(window, cx);
                    })),
            );
            visible_index += 1;
        }
        if owner.entries().is_empty() {
            list = list.child(if owner.loading() {
                "Loading files…"
            } else {
                "Working tree clean"
            });
        } else if self.matching_files(cx).is_empty() {
            list = list.child("No files match the filter");
        }
        list
    }
    fn render_diff(&self, side: Side, cx: &mut Context<Self>) -> impl IntoElement {
        let id = if side == Side::Worktree {
            "diff-unstaged"
        } else {
            "diff-index"
        };
        let mut pane = div()
            .id(id)
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .border_1()
            .border_color(rgb(0x454545));
        let Some(features) = &self.features else {
            return pane;
        };
        let owner = features.tree.read(cx);
        let snapshot = owner.pane(side);
        if snapshot.is_empty() {
            return pane;
        }
        pane = pane.child(
            div()
                .id(if side == Side::Worktree {
                    "focus-unstaged"
                } else {
                    "focus-index"
                })
                .test_support()
                .cursor_pointer()
                .child(if side == Side::Worktree {
                    "Unstaged — stage selection"
                } else {
                    "Staged — unstage selection"
                })
                .on_click(cx.listener(move |view, _, window, cx| {
                    if view.popup.is_some() {
                        return;
                    }
                    view.focus = Focus::Diff;
                    view.diff_anchor = None;
                    if let Some(features) = &view.features {
                        features.tree.update(cx, |owner, cx| {
                            owner.set_focus(side);
                            cx.notify();
                        });
                    }
                    view.handle.focus(window, cx);
                    cx.notify();
                })),
        );
        if let Some(reason) = &snapshot.disabled_reason {
            pane = pane.child(div().child(format!("Whole-file only: {reason}")));
        }
        let mapping = snapshot
            .patch
            .as_ref()
            .filter(|_| snapshot.partial_enabled());
        let mut body = div()
            .id(if side == Side::Worktree {
                "unstaged-lines"
            } else {
                "index-lines"
            })
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .font_family("monospace");
        let lines: Vec<_> = mapping
            .map(|patch| patch.lines().collect())
            .unwrap_or_default();
        let effective_selection = if owner.focused_side() == side && self.focus == Focus::Diff {
            snapshot.current_selection()
        } else {
            snapshot.selection.clone()
        };
        let mut body_line = 0;
        let mut in_hunk = false;
        for (index, bytes) in snapshot
            .display
            .split_inclusive(|b| *b == b'\n')
            .enumerate()
        {
            if bytes.starts_with(b"@@ ") {
                in_hunk = true;
            }
            let change = if in_hunk && matches!(bytes.first(), Some(b' ' | b'+' | b'-')) {
                let id = lines.get(body_line).and_then(|line| line.change_id);
                body_line += 1;
                id
            } else {
                None
            };
            let selected = change.is_some_and(|id| effective_selection.contains(&id));
            let text = String::from_utf8_lossy(bytes)
                .trim_end_matches('\n')
                .replace('\t', &" ".repeat(self.settings.panels.tab_width));
            let mut row = div()
                .id((id, index))
                .test_support()
                .when(!self.settings.diff.wrap_lines, |row| {
                    row.whitespace_nowrap()
                })
                .aria_label(if selected {
                    format!("Selected: {text}")
                } else {
                    text.clone()
                })
                .child(text)
                .when(selected, |d| d.bg(rgb(0x344860)));
            if bytes.starts_with(b"+") {
                row = row.text_color(rgb(0x95d5a0));
            } else if bytes.starts_with(b"-") {
                row = row.text_color(rgb(0xee9999));
            }
            if let Some(change) = change {
                row = row.cursor_pointer().on_click(cx.listener(
                    move |view, event: &ClickEvent, window, cx| {
                        if view.popup.is_some() {
                            return;
                        }
                        view.focus = Focus::Diff;
                        if let Some(features) = &view.features {
                            let tree = features.tree.clone();
                            let selection = if event.modifiers().shift || view.sticky {
                                let Some(anchor) = view.range_anchor(side, cx) else {
                                    return;
                                };
                                (anchor.min(change)..=anchor.max(change)).collect()
                            } else {
                                view.diff_anchor = None;
                                BTreeSet::new()
                            };
                            tree.update(cx, |owner, cx| {
                                owner.set_focus(side);
                                owner.set_cursor(side, change);
                                owner.set_selection(side, selection);
                                cx.notify();
                            });
                        }
                        view.handle.focus(window, cx);
                        cx.notify();
                    },
                ));
            }
            body = body.child(row);
        }
        pane.child(body)
    }
}
impl Render for RepositoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.restore_focus {
            self.handle.focus(window, cx);
            self.restore_focus = false;
        }
        if self.close_intent {
            self.close_intent = false;
            (self.on_host)(HostIntent::Close, window, cx);
        }
        if let (Some(features), Some(controls)) = (&self.features, &self.controls) {
            let commit = features.commit.read(cx);
            let draft = commit.draft().clone();
            let busy = commit.busy();
            let warning = commit.warning();
            let error = commit.error().map(str::to_owned);
            // Only replace unedited controls after asynchronous owner preparation.
            // A queued native edit must never be overwritten by a delayed prefix read.
            if self.controls_draft.as_ref() != Some(&draft) {
                let local = controls.read(cx).draft(cx);
                if self.controls_draft.as_ref() == Some(&local) {
                    controls.update(cx, |view, cx| view.set_draft(&draft, window, cx));
                }
                self.controls_draft = Some(draft);
            }
            let feedback = (busy, warning, error.clone());
            let changed = self.controls_feedback.as_ref() != Some(&feedback);
            self.controls_feedback = Some(feedback);
            controls.update(cx, |view, cx| {
                if changed {
                    view.set_feedback(busy, warning, error, window, cx);
                }
            });
        }
        let repository = self.repository.read(cx);
        let status = if repository.loading() {
            "Opening repository…".into()
        } else if let Some(error) = repository.error() {
            error.to_owned()
        } else if let Some(session) = repository.session() {
            format!(
                "{}{}",
                session
                    .readiness
                    .identity
                    .worktree
                    .as_ref()
                    .unwrap_or(&session.readiness.identity.git_dir)
                    .display(),
                if session.readiness.identity.worktree.is_none() {
                    " (bare: read-only)"
                } else {
                    ""
                }
            )
        } else {
            "No repository".into()
        };
        let diagnostic_count = repository
            .session()
            .map_or(0, |session| session.settings.diagnostics.len());
        let mut status_panel = div()
            .id("status-panel")
            .flex()
            .flex_col()
            .child(self.panel_header("panel-status", "Status", 0, cx))
            .child(
                div()
                    .id("repository-status")
                    .aria_label(status.clone())
                    .test_support()
                    .child(status),
            );
        status_panel = status_panel.child("M1 configuration/layout limitations: see Help");
        if diagnostic_count > 0 {
            status_panel = status_panel.child(format!(
                "{diagnostic_count} configuration diagnostics; unsupported settings remain inactive"
            ));
        }
        let mut branches = div()
            .id("branches-panel")
            .flex()
            .flex_col()
            .child(self.panel_header("panel-branches", "Branches", 2, cx))
            .child(self.tabs(Focus::Branches, cx));
        let mut commits = div()
            .id("commits-panel")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .child(self.panel_header("panel-commits", "Commits (read-only)", 3, cx))
            .child(self.tabs(Focus::Commits, cx));
        let mut feedback = self.diagnostic.clone().unwrap_or_default();
        if let Some(features) = &self.features {
            let history = features.history.read(cx);
            let head = match history.head() {
                Some(Head::Branch { branch, .. }) => branch.to_string_lossy().into_owned(),
                Some(Head::Unborn { branch }) => format!("{} (unborn)", branch.to_string_lossy()),
                Some(Head::Detached { oid }) => format!("Detached {}", &oid[..8]),
                None => "Loading HEAD…".into(),
            };
            if self.branches_tab == 0 {
                branches = branches.child(
                    div()
                        .id("current-branch")
                        .aria_label(head.clone())
                        .test_support()
                        .child(head),
                );
            } else {
                branches = branches.child(if self.branches_tab == 1 {
                    "Remotes — unavailable in M1"
                } else {
                    "Tags — unavailable in M1"
                });
            }
            let mut rows = div()
                .id("history-list")
                .flex()
                .flex_col()
                .flex_1()
                .overflow_y_scroll();
            for (index, commit) in history
                .commits()
                .iter()
                .enumerate()
                .filter(|_| self.commits_tab == 0)
            {
                let text = format!(
                    "{} {}",
                    &commit.oid[..8],
                    String::from_utf8_lossy(&commit.subject)
                );
                rows = rows.child(
                    div()
                        .id(("commit", index))
                        .aria_label(text.clone())
                        .test_support()
                        .cursor_pointer()
                        .when(
                            self.focus == Focus::Commits && self.commit_row == index,
                            |d| d.bg(rgb(0x344860)),
                        )
                        .child(text)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if view.popup.is_none() {
                                view.focus = Focus::Commits;
                                view.commit_row = index;
                                view.handle.focus(window, cx);
                                cx.notify();
                            }
                        })),
                );
            }
            if self.commits_tab != 0 {
                rows = rows.child("Reflog — unavailable in M1");
            } else if history.commits().is_empty() {
                rows = rows.child(if history.busy() {
                    "Loading history…"
                } else {
                    "No commits"
                });
            }
            if let Some(error) = history.error() {
                rows = rows.child(error.to_owned());
            }
            commits = commits.child(rows);
            if let Some(error) = features
                .tree
                .read(cx)
                .error()
                .or(features.commit.read(cx).error())
            {
                feedback = error.into();
            } else if self.diagnostic.is_none()
                && (features.tree.read(cx).busy() || features.commit.read(cx).busy())
            {
                feedback = "Operation running; repeated commit submission is disabled".into();
            }
        } else {
            branches = branches.child("Unavailable");
            commits = commits.child("Unavailable");
        }
        let side = div()
            .id("side-panels")
            .flex()
            .flex_col()
            .w(relative(self.settings.panels.side_panel_width as f32))
            .gap_1()
            .overflow_hidden()
            .child(status_panel)
            .child(
                div()
                    .id("files-panel")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_hidden()
                    .child(self.panel_header("panel-files", "Files", 1, cx))
                    .child(self.tabs(Focus::Files, cx))
                    .child(self.render_files(cx)),
            )
            .child(branches)
            .child(commits)
            .child(
                div()
                    .id("stash-panel")
                    .child(self.panel_header("panel-stash", "Stash", 4, cx))
                    .child("Stash is unavailable in M1"),
            );
        let mut diffs = div()
            .id("diff-panels")
            .flex()
            .flex_1()
            .gap_1()
            .overflow_hidden()
            .when(!self.horizontal, |d| d.flex_col());
        let has_unstaged = self
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).pane(Side::Worktree).is_empty());
        let has_index = self
            .features
            .as_ref()
            .is_some_and(|f| !f.tree.read(cx).pane(Side::Index).is_empty());
        if has_unstaged {
            diffs = diffs.child(self.render_diff(Side::Worktree, cx));
        }
        if has_index {
            diffs = diffs.child(self.render_diff(Side::Index, cx));
        }
        if !has_unstaged && !has_index {
            diffs = diffs.child(
                div()
                    .id("diff-empty")
                    .aria_label("No diff data")
                    .test_support()
                    .child("No diff data"),
            );
        }
        let toolbar = div()
            .flex()
            .gap_1()
            .flex_wrap()
            .child(self.control("action-stage", "Stage / unstage", Action::Select, cx))
            .child(self.control("action-commit", "Commit", Action::Commit, cx))
            .child(self.control("action-refresh", "Refresh", Action::Refresh, cx))
            .child(self.control("action-filter", "Text filter", Action::Filter, cx))
            .child(self.control(
                "action-status-filter",
                "Status filter",
                Action::StatusFilter,
                cx,
            ))
            .child(self.control(
                "action-tree",
                if self.tree {
                    "View: tree"
                } else {
                    "View: flat"
                },
                Action::Tree,
                cx,
            ))
            .child(self.control("action-whitespace", "Whitespace", Action::Whitespace, cx))
            .child(self.control("action-context-up", "Context +", Action::ContextUp, cx))
            .child(self.control("action-context-down", "Context −", Action::ContextDown, cx))
            .child(self.control("action-copy", "Copy", Action::Copy, cx))
            .child(self.control(
                "action-layout",
                if self.horizontal {
                    "Layout: side by side"
                } else {
                    "Layout: stacked"
                },
                Action::Layout,
                cx,
            ))
            .child(self.control("action-switch", "Repository…", Action::Path, cx))
            .child(self.control(
                "action-config-reload",
                "Reload config",
                Action::ReloadConfig,
                cx,
            ))
            .child(self.control("action-help", "Help", Action::Help, cx));
        let mut root = div()
            .id("repository-view")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .bg(rgb(0x20242b))
            .text_color(rgb(0xe3e3e3))
            .track_focus(&self.handle)
            .child(toolbar)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .gap_2()
                    .overflow_hidden()
                    .child(side)
                    .child(diffs),
            )
            .child(
                div()
                    .id("operation-feedback")
                    .aria_label(feedback.clone())
                    .test_support()
                    .child(feedback),
            );
        if let Some(popup) = self.popup {
            let mut overlay = div()
                .id("repository-popup")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x000000bb));
            let mut dialog = div()
                .id("repository-dialog")
                .w(px(620.))
                .max_h(relative(0.9))
                .overflow_y_scroll()
                .p_4()
                .bg(rgb(0x303640))
                .flex()
                .flex_col()
                .gap_2();
            match popup {
                Popup::Help => {
                    dialog = dialog.child("Navigation help — popup suppresses repository commands")
                        .child(self.navigation_help())
                        .child("Enter expands directories or opens file diff; the configured main.toggleSelectHunk key toggles hunk/line. Tab in diff changes sides. Clipboard and native layout controls are available.")
                        .child("Branch/history/stash mutations, custom commands, terminals and external diff renderers are unavailable.")
                        .child("M1 layout limitations: gui.sidePanels tab grouping, expandFocusedSidePanel, expandedSidePanelWeight, shrinkSidePanelsToContent, fileTreeSortOrder, splitDiff, screenMode, switchTabsWithPanelJumpKeys, showPanelJumps, scrollHeight, scrollPastBottom and mouseEvents are not applied; the native view uses five panels, native scrolling/mouse and its Layout control. git.renameSimilarityThreshold is not applied to canonical no-rename diffs. autoFetch remains unavailable.");
                    if let Some(session) = self.repository.read(cx).session() {
                        for diagnostic in &session.settings.diagnostics {
                            dialog = dialog.child(format!(
                                "{} — {}: {}",
                                diagnostic.source.display(),
                                diagnostic.path,
                                diagnostic.reason
                            ));
                        }
                    }
                }
                Popup::StatusFilter => {
                    dialog = dialog.child("Status filter");
                    for (index, (filter, label)) in StatusFilter::CHOICES.into_iter().enumerate() {
                        dialog = dialog.child(
                            div()
                                .id(("status-filter", index))
                                .aria_label(label)
                                .test_support()
                                .cursor_pointer()
                                .when(index == self.status_filter_row, |d| d.bg(rgb(0x344860)))
                                .child(label)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.status_filter = filter;
                                    view.reconcile_files(cx);
                                    view.dismiss(window, cx);
                                })),
                        );
                    }
                }
                Popup::Filter | Popup::Path => {
                    dialog = dialog
                        .child(if popup == Popup::Filter {
                            self.popup_label("Filter files — accept", Action::Confirm)
                        } else {
                            self.popup_label(
                                "Open repository path — replace repository",
                                Action::Confirm,
                            )
                        })
                        .child(Input::new(&self.prompt).id("repository-prompt"));
                }
                Popup::Quit => {
                    dialog = dialog.child("Quit this repository window? Active operations will be cancelled and settled before closing.")
                        .child(div().id("quit-confirm").aria_label(self.popup_label("Quit", Action::Confirm)).test_support().cursor_pointer().child(self.popup_label("Quit", Action::Confirm))
                            .on_click(cx.listener(|view, _, window, cx| (view.on_host)(HostIntent::CloseConfirmed, window, cx))));
                }
                Popup::Commit => {
                    dialog = dialog.child("Commit — Enter in subject submits; Tab switches fields; Escape retains draft");
                    if let Some(controls) = &self.controls {
                        dialog = dialog.child(controls.clone());
                    }
                }
            }
            dialog = dialog.child(
                div()
                    .id("popup-dismiss")
                    .aria_label(self.popup_label("Dismiss", Action::Escape))
                    .test_support()
                    .cursor_pointer()
                    .child(self.popup_label("Dismiss", Action::Escape))
                    .on_click(cx.listener(|view, _, window, cx| {
                        if view.popup == Some(Popup::Commit)
                            && let Some(features) = &view.features
                        {
                            features.commit.update(cx, |owner, cx| {
                                owner.cancel();
                                cx.notify();
                            });
                        }
                        view.dismiss(window, cx);
                    })),
            );
            overlay = overlay.child(dialog);
            root = root.child(overlay);
        }
        root
    }
}
#[cfg(test)]
#[path = "tests/navigation.rs"]
mod tests;
