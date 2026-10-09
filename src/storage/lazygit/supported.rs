//! Typed subset consumed by M1 owners/views; unsupported fields stay in raw config with diagnostics.
use super::{Settings, bindings, err, merge};
use crate::input::Key;
use serde::Deserialize;
use serde_yaml::Value;
use std::{collections::BTreeMap, io, path::PathBuf};

pub const DEFAULTS: &str =
    include_str!("../../../llm-docs/research/lazygit-v0.66.0/defaults-linux.yml");
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WarningSettings {
    pub skip_no_staged_files_warning: bool,
    pub skip_amend_warning: bool,
    pub skip_discard_change_warning: bool,
    pub skip_stash_warning: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageSettings {
    pub sign_off: bool,
    pub auto_wrap_commit_message: bool,
    pub auto_wrap_width: usize,
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PanelSettings {
    pub side_panel_width: f64,
    pub expand_focused_side_panel: bool,
    pub expanded_side_panel_weight: usize,
    pub shrink_side_panels_to_content: bool,
    pub side_panels: Vec<Vec<Panel>>,
    pub main_panel_split_mode: SplitMode,
    pub split_diff: SplitDiff,
    pub screen_mode: ScreenMode,
    pub show_file_tree: bool,
    pub show_root_item_in_file_tree: bool,
    pub file_tree_sort_order: FileTreeSort,
    pub file_tree_sort_case_sensitive: bool,
    pub filter_mode: FilterMode,
    pub switch_tabs_with_panel_jump_keys: bool,
    pub show_panel_jumps: bool,
    pub scroll_height: usize,
    pub scroll_past_bottom: bool,
    pub tab_width: usize,
    pub mouse_events: bool,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Panel {
    Status,
    Files,
    Worktrees,
    Submodules,
    Branches,
    Remotes,
    Tags,
    Commits,
    Reflog,
    Stash,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SplitMode {
    Flexible,
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SplitDiff {
    Auto,
    Always,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScreenMode {
    Normal,
    Half,
    Full,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileTreeSort {
    Mixed,
    FilesFirst,
    FoldersFirst,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterMode {
    Substring,
    Fuzzy,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DiffSettings {
    pub context_size: usize,
    pub ignore_whitespace: bool,
    pub wrap_lines: bool,
    pub use_hunk_mode: bool,
    pub rename_similarity_threshold: u8,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RefreshSettings {
    pub auto_refresh: bool,
    pub auto_detect_external_changes: bool,
    /// Desired upstream policy only; network auto-fetch is unavailable until M3.
    pub auto_fetch: bool,
    pub refresh_interval_seconds: u64,
    pub external_change_check_interval_seconds: u64,
    pub fetch_interval_seconds: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct M1Settings {
    pub warnings: WarningSettings,
    pub message: MessageSettings,
    pub skip_hook_prefix: String,
    pub commit_prefixes: super::CommitPrefixes,
    pub show_commit_length: bool,
    pub panels: PanelSettings,
    pub diff: DiffSettings,
    pub refresh: RefreshSettings,
    pub confirm_on_quit: bool,
    pub quit_on_top_level_return: bool,
    /// Context -> canonical action name -> normalized keys, including legacy alternates.
    /// Parsing a binding does not make its workflow available in M1.
    pub keybindings: BTreeMap<String, BTreeMap<String, Vec<Key>>>,
}
fn at<'a>(value: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(value, |v, part| &v[part])
}
fn decode<T: serde::de::DeserializeOwned>(value: &Value, path: &str) -> io::Result<T> {
    serde_yaml::from_value(at(value, path).clone()).map_err(|e| err(format!("{path}: {e}")))
}
impl Default for M1Settings {
    fn default() -> Self {
        Self::from_value(&Value::Mapping(serde_yaml::Mapping::new()))
            .expect("pinned M1 defaults validate")
    }
}
impl M1Settings {
    pub(super) fn from_value(value: &Value) -> io::Result<Self> {
        // Preserve the M0 supplied-default API: missing M1 fields get the pinned Linux defaults.
        let mut effective: Value = serde_yaml::from_str(DEFAULTS).expect("pinned defaults parse");
        merge(
            &mut effective,
            value.clone(),
            "",
            &PathBuf::new(),
            &mut BTreeMap::new(),
        );
        let warnings = decode(&effective, "gui")?;
        let message: MessageSettings = decode(&effective, "git.commit")?;
        let panels: PanelSettings = decode(&effective, "gui")?;
        if message.auto_wrap_width == 0 {
            return Err(err("git.commit.autoWrapWidth: must be positive"));
        }
        if !panels.side_panel_width.is_finite()
            || !(0.0..1.0).contains(&panels.side_panel_width)
            || panels.side_panel_width == 0.0
        {
            return Err(err("gui.sidePanelWidth: must be between 0 and 1"));
        }
        if panels.expanded_side_panel_weight == 0 {
            return Err(err("gui.expandedSidePanelWeight: must be positive"));
        }
        if panels.tab_width == 0 {
            return Err(err("gui.tabWidth: must be positive"));
        }
        let mut seen = std::collections::BTreeSet::new();
        if panels.side_panels.is_empty() || panels.side_panels.iter().any(|p| p.is_empty()) {
            return Err(err("gui.sidePanels: panels must not be empty"));
        }
        for panel in panels.side_panels.iter().flatten() {
            if !seen.insert(*panel) {
                return Err(err("gui.sidePanels: duplicate tab"));
            }
        }
        let threshold: u8 = decode(&effective, "git.renameSimilarityThreshold")?;
        if threshold > 100 {
            return Err(err("git.renameSimilarityThreshold: must be 0..100"));
        }
        let mut keybindings = BTreeMap::new();
        let contexts = effective["keybinding"]
            .as_mapping()
            .ok_or_else(|| err("keybinding: expected mapping"))?;
        for (context, actions) in contexts {
            let context = context
                .as_str()
                .ok_or_else(|| err("keybinding: context must be string"))?;
            let actions = actions
                .as_mapping()
                .ok_or_else(|| err(format!("keybinding.{context}: expected mapping")))?;
            let mut normalized = BTreeMap::<String, Vec<Key>>::new();
            for (name, value) in actions {
                let name = name
                    .as_str()
                    .ok_or_else(|| err("keybinding: action must be string"))?;
                let base = ["-alt1", "-alt2", "-alt"]
                    .into_iter()
                    .find_map(|suffix| name.strip_suffix(suffix))
                    .unwrap_or(name);
                let output = normalized.entry(base.into()).or_default();
                for raw in
                    bindings(value).map_err(|e| err(format!("keybinding.{context}.{name}: {e}")))?
                {
                    if let Some(key) = Key::parse(&raw)?
                        && !output.contains(&key)
                    {
                        output.push(key);
                    }
                }
            }
            keybindings.insert(context.into(), normalized);
        }
        Ok(Self {
            warnings,
            message,
            panels,
            skip_hook_prefix: decode(&effective, "git.skipHookPrefix")?,
            commit_prefixes: super::CommitPrefixes::from_value(&effective)?,
            show_commit_length: decode(&effective, "gui.commitLength.show")?,
            diff: DiffSettings {
                context_size: decode(&effective, "git.diffContextSize")?,
                ignore_whitespace: decode(&effective, "git.ignoreWhitespaceInDiffView")?,
                wrap_lines: decode(&effective, "gui.wrapLinesInDiffView")?,
                use_hunk_mode: decode(&effective, "gui.useHunkModeInDiffView")?,
                rename_similarity_threshold: threshold,
            },
            refresh: RefreshSettings {
                auto_refresh: decode(&effective, "git.autoRefresh")?,
                auto_detect_external_changes: decode(&effective, "git.autoDetectExternalChanges")?,
                auto_fetch: decode(&effective, "git.autoFetch")?,
                refresh_interval_seconds: decode(&effective, "refresher.refreshInterval")?,
                external_change_check_interval_seconds: decode(
                    &effective,
                    "refresher.externalChangeCheckInterval",
                )?,
                fetch_interval_seconds: decode(&effective, "refresher.fetchInterval")?,
            },
            confirm_on_quit: decode(&effective, "confirmOnQuit")?,
            quit_on_top_level_return: decode(&effective, "quitOnTopLevelReturn")?,
            keybindings,
        })
    }
    pub fn binding(&self, context: &str, action: &str) -> &[Key] {
        self.keybindings
            .get(context)
            .and_then(|m| m.get(action))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}
impl Settings {
    pub fn load_m1(sources: &[super::Source]) -> io::Result<Self> {
        Self::load(DEFAULTS, sources)
    }
    pub fn m1(&self) -> &M1Settings {
        &self.supported
    }
}
pub(super) fn supported_path(path: &str) -> bool {
    matches!(
        path,
        "confirmOnQuit"
            | "quitOnTopLevelReturn"
            | "gui.skipNoStagedFilesWarning"
            | "git.commit.signOff"
            | "git.commit.autoWrapCommitMessage"
            | "git.commit.autoWrapWidth"
            | "git.skipHookPrefix"
            | "gui.commitLength.show"
            | "gui.sidePanelWidth"
            | "gui.mainPanelSplitMode"
            | "gui.showFileTree"
            | "gui.showRootItemInFileTree"
            | "gui.fileTreeSortCaseSensitive"
            | "gui.filterMode"
            | "gui.tabWidth"
            | "git.diffContextSize"
            | "git.ignoreWhitespaceInDiffView"
            | "gui.wrapLinesInDiffView"
            | "gui.useHunkModeInDiffView"
            | "git.autoRefresh"
            | "git.autoDetectExternalChanges"
            | "refresher.refreshInterval"
            | "refresher.externalChangeCheckInterval"
    )
}
/// Typed for validation/inspection, but not consumed by an M1 workflow or view.
pub(super) fn parsed_unavailable_path(path: &str) -> bool {
    matches!(
        path,
        "gui.skipAmendWarning"
            | "gui.skipDiscardChangeWarning"
            | "gui.skipStashWarning"
            | "gui.expandFocusedSidePanel"
            | "gui.expandedSidePanelWeight"
            | "gui.shrinkSidePanelsToContent"
            | "gui.sidePanels"
            | "gui.splitDiff"
            | "gui.screenMode"
            | "gui.fileTreeSortOrder"
            | "gui.switchTabsWithPanelJumpKeys"
            | "gui.showPanelJumps"
            | "gui.scrollHeight"
            | "gui.scrollPastBottom"
            | "gui.mouseEvents"
            | "git.renameSimilarityThreshold"
    )
}
/// Availability inventory only, not a second key resolver/executor. Exact names
/// mirror repository actions and commit controls; parsing arbitrary actions never
/// grants them a workflow. Universal action overrides work in active panel contexts.
pub(super) fn supported_binding_path(path: &str) -> bool {
    let Some((context, name)) = path
        .strip_prefix("keybinding.")
        .and_then(|p| p.split_once('.'))
    else {
        return false;
    };
    let name = ["-alt1", "-alt2", "-alt"]
        .into_iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name);
    let panel_action = matches!(
        name,
        "jumpToBlock"
            | "prevItem"
            | "nextItem"
            | "rangeSelectUp"
            | "rangeSelectDown"
            | "toggleRangeSelect"
            | "nextBlock"
            | "prevBlock"
            | "focusMainView"
            | "nextTab"
            | "prevTab"
            | "select"
            | "goInto"
            | "return"
            | "optionMenu"
            | "startSearch"
            | "openRecentRepos"
            | "refresh"
            | "toggleWhitespaceInDiffView"
            | "increaseContextInDiffView"
            | "decreaseContextInDiffView"
            | "copyToClipboard"
            | "nextScreenMode"
            | "quit"
            | "quitWithoutChangingDirectory"
    );
    if panel_action
        && matches!(
            context,
            "universal" | "files" | "branches" | "commits" | "status" | "stash" | "main"
        )
    {
        return true;
    }
    matches!(
        (context, name),
        (
            "universal",
            "confirm"
                | "confirmMenu"
                | "togglePanel"
                | "filteringMenu"
                | "confirmInEditor"
                | "submitEditorText"
        ) | ("main", "toggleSelectHunk" | "prevHunk" | "nextHunk")
            | (
                "files",
                "commitChanges"
                    | "refreshFiles"
                    | "toggleTreeView"
                    | "openStatusFilter"
                    | "copyFileInfoToClipboard"
            )
            | ("commits", "copyCommitAttributeToClipboard")
            | ("status", "recentRepos")
            | ("commitMessage", "commitMenu")
    )
}
