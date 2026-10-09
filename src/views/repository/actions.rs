//! One action vocabulary for key resolution, contextual help and clickable labels.
use super::{Action, Focus};
use lazygui::{input::Key, lazygit_config::M1Settings};

#[derive(Clone, Copy)]
pub(super) struct Definition {
    pub scope: &'static str,
    pub context: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub action: Action,
}
impl Definition {
    pub fn keys<'a>(&self, settings: &'a M1Settings) -> &'a [Key] {
        let keys = settings.binding(self.context, self.name);
        if let Action::Panel(index) = self.action {
            keys.get(index..index + 1).unwrap_or(&[])
        } else {
            keys
        }
    }
}
macro_rules! definitions {
    ($(($scope:literal, $context:literal, $name:literal, $title:literal, $action:expr)),* $(,)?) => {
        pub(super) const DEFINITIONS: &[Definition] = &[$(Definition {
            scope: $scope, context: $context, name: $name, title: $title, action: $action,
        }),*];
    };
}
definitions![
    (
        "universal",
        "universal",
        "jumpToBlock",
        "Status panel",
        Action::Panel(0)
    ),
    (
        "universal",
        "universal",
        "jumpToBlock",
        "Files panel",
        Action::Panel(1)
    ),
    (
        "universal",
        "universal",
        "jumpToBlock",
        "Branches panel",
        Action::Panel(2)
    ),
    (
        "universal",
        "universal",
        "jumpToBlock",
        "Commits panel",
        Action::Panel(3)
    ),
    (
        "universal",
        "universal",
        "jumpToBlock",
        "Stash panel",
        Action::Panel(4)
    ),
    ("popup", "universal", "return", "Dismiss", Action::Escape),
    ("popup", "universal", "confirm", "Confirm", Action::Confirm),
    (
        "menu",
        "universal",
        "confirmMenu",
        "Confirm option",
        Action::Confirm
    ),
    (
        "menu",
        "universal",
        "prevItem",
        "Previous option",
        Action::Up
    ),
    ("menu", "universal", "nextItem", "Next option", Action::Down),
    (
        "main",
        "universal",
        "togglePanel",
        "Other diff side",
        Action::NextTab
    ),
    (
        "main",
        "main",
        "toggleSelectHunk",
        "Toggle hunk / line",
        Action::ToggleHunk
    ),
    (
        "main",
        "main",
        "prevHunk",
        "Previous hunk",
        Action::PrevHunk
    ),
    ("main", "main", "nextHunk", "Next hunk", Action::NextHunk),
    ("files", "files", "commitChanges", "Commit", Action::Commit),
    ("files", "files", "refreshFiles", "Refresh", Action::Refresh),
    (
        "files",
        "files",
        "toggleTreeView",
        "Tree / flat",
        Action::Tree
    ),
    (
        "files",
        "files",
        "openStatusFilter",
        "Status filter",
        Action::StatusFilter
    ),
    (
        "files",
        "files",
        "copyFileInfoToClipboard",
        "Copy",
        Action::Copy
    ),
    (
        "commits",
        "commits",
        "copyCommitAttributeToClipboard",
        "Copy",
        Action::Copy
    ),
    (
        "status",
        "status",
        "recentRepos",
        "Switch repository",
        Action::Path
    ),
    (
        "universal",
        "universal",
        "prevItem",
        "Previous item",
        Action::Up
    ),
    (
        "universal",
        "universal",
        "nextItem",
        "Next item",
        Action::Down
    ),
    (
        "universal",
        "universal",
        "rangeSelectUp",
        "Extend up",
        Action::RangeUp
    ),
    (
        "universal",
        "universal",
        "rangeSelectDown",
        "Extend down",
        Action::RangeDown
    ),
    (
        "universal",
        "universal",
        "toggleRangeSelect",
        "Sticky range",
        Action::Range
    ),
    (
        "universal",
        "universal",
        "nextBlock",
        "Next panel",
        Action::Next
    ),
    (
        "universal",
        "universal",
        "prevBlock",
        "Previous panel",
        Action::Prev
    ),
    (
        "universal",
        "universal",
        "focusMainView",
        "Focus diff",
        Action::Main
    ),
    (
        "universal",
        "universal",
        "nextTab",
        "Next tab / diff side",
        Action::NextTab
    ),
    (
        "universal",
        "universal",
        "prevTab",
        "Previous tab / diff side",
        Action::PrevTab
    ),
    (
        "universal",
        "universal",
        "select",
        "Stage / unstage",
        Action::Select
    ),
    (
        "universal",
        "universal",
        "goInto",
        "Expand directory / open diff",
        Action::Enter
    ),
    (
        "universal",
        "universal",
        "return",
        "Return / dismiss",
        Action::Escape
    ),
    ("universal", "universal", "optionMenu", "Help", Action::Help),
    (
        "universal",
        "universal",
        "startSearch",
        "File text filter",
        Action::Filter
    ),
    (
        "files",
        "universal",
        "filteringMenu",
        "Status filter",
        Action::StatusFilter
    ),
    (
        "universal",
        "universal",
        "openRecentRepos",
        "Switch repository",
        Action::Path
    ),
    (
        "universal",
        "universal",
        "refresh",
        "Refresh",
        Action::Refresh
    ),
    (
        "universal",
        "universal",
        "toggleWhitespaceInDiffView",
        "Whitespace",
        Action::Whitespace
    ),
    (
        "universal",
        "universal",
        "increaseContextInDiffView",
        "Context +",
        Action::ContextUp
    ),
    (
        "universal",
        "universal",
        "decreaseContextInDiffView",
        "Context −",
        Action::ContextDown
    ),
    (
        "universal",
        "universal",
        "copyToClipboard",
        "Copy",
        Action::Copy
    ),
    (
        "universal",
        "universal",
        "nextScreenMode",
        "Layout",
        Action::Layout
    ),
    ("universal", "universal", "quit", "Quit", Action::Close),
    (
        "universal",
        "universal",
        "quitWithoutChangingDirectory",
        "Quit without changing directory",
        Action::Close
    ),
    (
        "universal",
        "universal",
        "suspendApp",
        "Suspend",
        Action::Unsupported("Suspend is unavailable.")
    ),
    (
        "universal",
        "universal",
        "editConfig",
        "Edit config",
        Action::Unsupported("Shared configuration is read-only.")
    ),
    (
        "universal",
        "universal",
        "cycleDiffRenderers",
        "Diff renderer",
        Action::Unsupported("Only native unified diff is available.")
    ),
];
pub(super) fn context(focus: Focus) -> &'static str {
    match focus {
        Focus::Files => "files",
        Focus::Branches => "branches",
        Focus::Commits => "commits",
        Focus::Status => "status",
        Focus::Stash => "stash",
        Focus::Diff => "main",
    }
}
