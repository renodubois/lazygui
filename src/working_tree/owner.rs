use crate::{diff, git};
use async_channel::{Receiver, Sender};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::{OsStr, OsString},
    io,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

pub use git::Side;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    #[default]
    Hunk,
    Line,
    Range,
}

/// Bytes and IDs are canonical, not wrapped display-row offsets. A disabled partial
/// selection does not disable the separate whole-file action.
#[derive(Clone, Debug)]
pub struct Pane {
    pub canonical: Vec<u8>,
    pub patch: Option<diff::Patch>,
    pub display: Vec<u8>,
    pub disabled_reason: Option<String>,
    /// Canonical change ID, not a body-line ID (context lines have no change ID).
    pub cursor: usize,
    pub selection: BTreeSet<usize>,
    pub selection_mode: SelectionMode,
    anchor_map: Option<(Vec<u8>, Vec<Option<usize>>)>,
}
impl Default for Pane {
    fn default() -> Self {
        Self {
            canonical: vec![],
            patch: None,
            display: vec![],
            disabled_reason: Some("no changes on this side".into()),
            cursor: 0,
            selection: BTreeSet::new(),
            selection_mode: SelectionMode::default(),
            anchor_map: None,
        }
    }
}
impl Pane {
    /// View-local range anchors must be reconciled from their original canonical
    /// snapshot, or cleared if consumed/ambiguous. Never clamp a stale anchor.
    pub fn remap_anchor(&self, canonical: &[u8], id: usize) -> Option<usize> {
        if canonical == self.canonical {
            return self.patch.as_ref().filter(|p| id < p.changes()).map(|_| id);
        }
        self.anchor_map
            .as_ref()
            .filter(|(old, _)| old == canonical)
            .and_then(|(_, map)| map.get(id).copied().flatten())
    }
    pub fn is_empty(&self) -> bool {
        self.canonical.is_empty()
    }
    pub fn partial_enabled(&self) -> bool {
        self.disabled_reason.is_none()
    }
    /// Effective selection for highlighting/action dispatch, in canonical change IDs.
    pub fn current_selection(&self) -> BTreeSet<usize> {
        if !self.selection.is_empty() {
            return self.selection.clone();
        }
        let Some(patch) = &self.patch else {
            return BTreeSet::new();
        };
        match self.selection_mode {
            SelectionMode::Line | SelectionMode::Range => [self.cursor].into_iter().collect(),
            SelectionMode::Hunk => patch
                .hunks()
                .find(|h| h.changes.contains(&self.cursor))
                .map(|h| h.changes.collect())
                .unwrap_or_default(),
        }
    }
}

#[derive(Clone)]
struct FileSnapshot {
    entry: git::Entry,
    panes: [Pane; 2],
    // Rename operations also touch the original identity. Its canonical bytes
    // must be checked even though the selected pane displays the destination.
    original: [Vec<u8>; 2],
    original_entry: Option<git::Entry>,
}
struct Snapshot {
    entries: Vec<git::Entry>,
    files: BTreeMap<OsString, FileSnapshot>,
}
enum Intent {
    Files(Vec<FileSnapshot>, Side),
    Current {
        path: OsString,
        side: Side,
        canonical: Vec<u8>,
        cursor: usize,
        mode: SelectionMode,
        selection: BTreeSet<usize>,
        repeat: bool,
    },
}
enum Target {
    Files(Vec<FileSnapshot>, Side),
    Partial(Box<FileSnapshot>, Side, BTreeSet<usize>),
}
struct Flight {
    generation: u64,
    cancel: Arc<AtomicBool>,
    mutation: bool,
    partial: Option<(OsString, Side, diff::Patch, BTreeSet<usize>)>,
    whole: Option<(Vec<FileSnapshot>, Side)>,
}

/// Opaque single-consumer delivery. The shell calls `apply`, then notifies its
/// retained entity. Never deliver updates to a replacement owner.
pub struct Update {
    owner: Arc<()>,
    generation: u64,
    options: git::DiffOptions,
    snapshot: io::Result<Snapshot>,
    action_error: Option<String>,
    // Kept until apply has reconciled selection/focus, even on failed writes.
    lease: Option<git::MutationLease>,
    _scope: git::WorkflowScope,
}

/// Window/repository-scoped non-rendering owner. Construct once and retain above
/// replaceable views. Startup supplies a ProcessHost-retained Client and shares
/// MutationGates across windows. No connector call or wait runs on the GUI thread.
pub struct WorkingTree {
    client: git::Client,
    identity: git::Identity,
    gates: Arc<git::MutationGates>,
    options: git::DiffOptions,
    token: Arc<()>,
    tx: Sender<Update>,
    rx: Receiver<Update>,
    entries: Vec<git::Entry>,
    files: BTreeMap<OsString, FileSnapshot>,
    selected: Option<OsString>,
    panes: [Pane; 2],
    focus: Side,
    generation: u64,
    flight: Option<Flight>,
    queue: VecDeque<Intent>,
    refresh_pending: bool,
    auto_refresh_pending: bool,
    error: Option<String>,
    interaction: u64,
    last_current: Option<(OsString, Side, u64)>,
}
impl WorkingTree {
    /// Begins the initial refresh. Binding/canonicalization happens on the worker.
    pub fn new(
        client: git::Client,
        identity: git::Identity,
        gates: Arc<git::MutationGates>,
        options: git::DiffOptions,
    ) -> Self {
        let (tx, rx) = async_channel::unbounded();
        let mut owner = Self {
            client,
            identity,
            gates,
            options,
            token: Arc::new(()),
            tx,
            rx,
            entries: vec![],
            files: BTreeMap::new(),
            selected: None,
            panes: [Pane::default(), Pane::default()],
            focus: Side::Worktree,
            generation: 0,
            flight: None,
            queue: VecDeque::new(),
            refresh_pending: false,
            auto_refresh_pending: false,
            error: None,
            interaction: 0,
            last_current: None,
        };
        owner.refresh();
        owner
    }
    /// Cloning this receiver does not broadcast. Retain exactly one shell consumer.
    pub fn updates(&self) -> Receiver<Update> {
        self.rx.clone()
    }
    pub fn identity(&self) -> &git::Identity {
        &self.identity
    }
    pub fn entries(&self) -> &[git::Entry] {
        &self.entries
    }
    pub fn selected(&self) -> Option<&OsStr> {
        self.selected.as_deref()
    }
    pub fn pane(&self, side: Side) -> &Pane {
        &self.panes[index(side)]
    }
    pub fn options(&self) -> git::DiffOptions {
        self.options
    }
    pub fn focused_side(&self) -> Side {
        self.focus
    }
    pub fn set_focus(&mut self, side: Side) {
        self.interaction += 1;
        self.focus = side;
        self.reconcile_focus();
    }
    pub fn loading(&self) -> bool {
        self.flight.as_ref().is_some_and(|f| !f.mutation)
    }
    pub fn busy(&self) -> bool {
        self.flight.as_ref().is_some_and(|f| f.mutation) || !self.queue.is_empty()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn refresh(&mut self) {
        // Bare repositories have history but no index/worktree status capability.
        if self.identity.worktree.is_none() {
            return;
        }
        if self.flight.as_ref().is_some_and(|f| f.mutation) {
            self.refresh_pending = true;
            return;
        }
        self.start(None);
    }
    /// Timer ticks coalesce; unlike explicit refresh they never cancel a slow read
    /// or jump ahead of queued actions.
    pub fn auto_refresh(&mut self) {
        if self.identity.worktree.is_none() {
            return;
        }
        if self.flight.is_some() || !self.queue.is_empty() {
            self.auto_refresh_pending = true;
        } else {
            self.start(None);
        }
    }
    fn reset_panes(&mut self) {
        for pane in &mut self.panes {
            let mode = pane.selection_mode;
            *pane = Pane::default();
            pane.selection_mode = mode;
        }
    }
    pub fn select(&mut self, path: OsString) {
        if self.selected.as_ref() == Some(&path) {
            return;
        }
        self.interaction += 1;
        self.selected = Some(path);
        self.reset_panes();
        self.reconcile_panes(false);
    }
    pub fn set_options(&mut self, options: git::DiffOptions) {
        if options != self.options {
            self.options = options;
            self.refresh();
        }
    }
    /// Capture the displayed confirmation now, not a fresh file at dispatch time.
    /// Rename originals travel with their destination under the same lease.
    pub fn act_file(&mut self, paths: Vec<OsString>, side: Side) {
        self.interaction += 1;
        let files = paths
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|path| {
                self.files.get(&path).cloned().ok_or_else(|| {
                    io::Error::other("file target disappeared; refresh and reconcile")
                })
            })
            .collect::<io::Result<Vec<_>>>();
        match files {
            Ok(files) if !files.is_empty() => {
                self.queue.push_back(Intent::Files(files, side));
                self.drive();
            }
            Ok(_) => self.error = Some("empty file selection".into()),
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    /// IDs refer to the currently shown canonical patch. Explicit targets retain
    /// their surviving identities through our own writes; consumed IDs are removed.
    pub fn act_selection(&mut self, side: Side, selected: BTreeSet<usize>) {
        if selected.is_empty() {
            self.error = Some("select at least one change".into());
            return;
        }
        let pane = &mut self.panes[index(side)];
        pane.cursor = *selected.first().unwrap();
        pane.selection = selected;
        self.act_current(side);
    }
    pub fn set_cursor(&mut self, side: Side, change_id: usize) {
        self.interaction += 1;
        let pane = &mut self.panes[index(side)];
        pane.cursor = change_id;
        pane.selection.clear();
    }
    pub fn selection_mode(&self, side: Side) -> SelectionMode {
        self.pane(side).selection_mode
    }
    pub fn set_selection_mode(&mut self, side: Side, mode: SelectionMode) {
        self.interaction += 1;
        let pane = &mut self.panes[index(side)];
        pane.selection_mode = mode;
        pane.selection.clear();
    }
    /// Range anchors are local interaction state; targets remain canonical IDs.
    pub fn set_selection(&mut self, side: Side, selected: BTreeSet<usize>) {
        self.interaction += 1;
        self.panes[index(side)].selection = selected;
    }
    pub fn act_current(&mut self, side: Side) {
        let Some(path) = self.selected.clone() else {
            self.error = Some("no selected file".into());
            return;
        };
        let signature = (path.clone(), side, self.interaction);
        let pane = self.pane(side);
        let repeat = self.busy()
            && self.last_current.as_ref() == Some(&signature)
            && pane.selection.is_empty();
        self.queue.push_back(Intent::Current {
            path,
            side,
            canonical: pane.canonical.clone(),
            cursor: pane.cursor,
            mode: pane.selection_mode,
            selection: pane.current_selection(),
            repeat,
        });
        self.last_current = Some(signature);
        self.drive();
    }

    /// Returns false for superseded/cross-owner updates. The lease is released only
    /// after authoritative state, cursor, selection and focus reconciliation.
    pub fn apply(&mut self, mut update: Update) -> bool {
        if !Arc::ptr_eq(&self.token, &update.owner)
            || !self
                .flight
                .as_ref()
                .is_some_and(|f| f.generation == update.generation)
        {
            return false;
        }
        let flight = self.flight.take().unwrap();
        let mutation = flight.mutation;
        let mut failed = update.action_error.is_some();
        self.error = update.action_error.take();
        match update.snapshot {
            Ok(snapshot) => {
                let old_position = self
                    .entries
                    .iter()
                    .position(|e| Some(&e.path) == self.selected.as_ref())
                    .unwrap_or(0);
                if !failed {
                    for intent in &mut self.queue {
                        if let Err(error) = reconcile_intent(
                            intent,
                            &snapshot.files,
                            flight.partial.as_ref(),
                            flight.whole.as_ref(),
                        ) {
                            self.error = Some(error.to_string());
                            failed = true;
                            break;
                        }
                    }
                }
                let maps: [Option<Vec<Option<usize>>>; 2] = std::array::from_fn(|i| {
                    let path = self.selected.as_ref()?;
                    let old = &self.panes[i];
                    let fresh = snapshot.files.get(path).map(|f| &f.panes[i]);
                    remap_pane(
                        path,
                        side_at(i),
                        old,
                        fresh,
                        (!failed).then_some(flight.partial.as_ref()).flatten(),
                    )
                    .ok()
                });
                self.entries = snapshot.entries;
                self.files = snapshot.files;
                if !self
                    .selected
                    .as_ref()
                    .is_some_and(|p| self.files.contains_key(p))
                {
                    self.selected = self
                        .entries
                        .get(old_position.min(self.entries.len().saturating_sub(1)))
                        .map(|e| e.path.clone());
                }
                let previous = self.panes.clone();
                self.reconcile_panes(mutation);
                for (i, map) in maps.into_iter().enumerate() {
                    if let Some(map) = map {
                        let old = &previous[i];
                        let pane = &mut self.panes[i];
                        pane.cursor = successor(&map, old.cursor).unwrap_or(0);
                        pane.selection = old
                            .selection
                            .iter()
                            .filter_map(|id| map.get(*id).copied().flatten())
                            .collect();
                        pane.anchor_map = Some((old.canonical.clone(), map));
                    }
                }
            }
            Err(error) => {
                failed = true;
                let message = format!("refresh failed: {error}");
                self.error = Some(match self.error.take() {
                    Some(action) => {
                        format!("{action}; {message}; mutation outcome may be uncertain")
                    }
                    None if mutation => format!("{message}; mutation outcome may be uncertain"),
                    None => message,
                });
            }
        }
        // No replay of failed or uncertain operations, including queued repeat keys.
        if failed {
            self.queue.clear();
        }
        drop(update.lease.take());
        let reread = std::mem::take(&mut self.refresh_pending) || update.options != self.options;
        if reread {
            self.start(None);
        } else {
            self.drive();
            if self.flight.is_none() && std::mem::take(&mut self.auto_refresh_pending) {
                self.start(None);
            }
        }
        true
    }
    fn reconcile_panes(&mut self, mutation: bool) {
        if let Some(file) = self.selected.as_ref().and_then(|p| self.files.get(p)) {
            for i in 0..2 {
                let old = &self.panes[i];
                let mut pane = file.panes[i].clone();
                pane.selection_mode = old.selection_mode;
                if old.canonical == pane.canonical {
                    pane.cursor = old.cursor;
                    if !mutation {
                        pane.selection = old.selection.clone();
                    }
                }
                self.panes[i] = pane;
            }
        } else {
            self.reset_panes();
        }
        self.reconcile_focus();
    }
    fn reconcile_focus(&mut self) {
        if self.pane(self.focus).is_empty() {
            let other = opposite(self.focus);
            if !self.pane(other).is_empty() {
                self.focus = other;
            }
        }
    }
    fn drive(&mut self) {
        if self.flight.is_some() {
            return;
        }
        let intent = loop {
            let Some(intent) = self.queue.pop_front() else {
                return;
            };
            if matches!(&intent, Intent::Files(files, _) if files.is_empty()) {
                // Successful known writes consumed these confirmations.
                continue;
            }
            break intent;
        };
        let target = match intent {
            Intent::Files(files, side) => Ok(Target::Files(files, side)),
            Intent::Current {
                path,
                side,
                canonical,
                selection,
                ..
            } => {
                if let Some(file) = self.files.get(&path) {
                    let pane = &file.panes[index(side)];
                    if canonical != pane.canonical {
                        self.error = Some("stale queued selection; refresh and reconcile".into());
                        self.queue.clear();
                        return;
                    }
                    if let Some(reason) = &pane.disabled_reason {
                        self.error = Some(reason.clone());
                        self.queue.clear();
                        return;
                    }
                    let selected = selection;
                    if selected.is_empty() {
                        Err(io::Error::other("no current change"))
                    } else {
                        Ok(Target::Partial(Box::new(file.clone()), side, selected))
                    }
                } else {
                    Err(io::Error::other("no selected file"))
                }
            }
        };
        match target {
            Ok(target) => {
                if let Target::Partial(file, side, selected) = &target
                    && self.selected.as_ref() == Some(&file.entry.path)
                {
                    self.panes[index(*side)].cursor = *selected.first().unwrap();
                }
                self.start(Some(target));
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.queue.clear();
            }
        }
    }
    fn start(&mut self, target: Option<Target>) {
        if let Some(previous) = self.flight.take() {
            previous.cancel.store(true, Ordering::Release);
        }
        self.generation += 1;
        let generation = self.generation;
        let mutation = target.is_some();
        let cancel = Arc::new(AtomicBool::new(false));
        let (client, scope) = match self.client.begin_workflow(cancel.clone()) {
            Ok(admitted) => admitted,
            Err(error) => {
                self.queue.clear();
                self.error = Some(format!("working-tree workflow unavailable: {error}"));
                return;
            }
        };
        let partial = match &target {
            Some(Target::Partial(file, side, selected)) => {
                file.panes[index(*side)].patch.as_ref().map(|patch| {
                    (
                        file.entry.path.clone(),
                        *side,
                        patch.clone(),
                        selected.clone(),
                    )
                })
            }
            _ => None,
        };
        let whole = match &target {
            Some(Target::Files(files, side)) => Some((files.clone(), *side)),
            _ => None,
        };
        self.flight = Some(Flight {
            generation,
            cancel: cancel.clone(),
            mutation,
            partial,
            whole,
        });
        let identity = self.identity.clone();
        let gates = self.gates.clone();
        let options = self.options;
        let token = self.token.clone();
        let tx = self.tx.clone();
        let spawned = thread::Builder::new()
            .name("working-tree".into())
            .spawn(move || {
                let mut lease = None;
                let mut action_error = None;
                let snapshot = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let client = client.bind(&identity)?;
                    if let Some(target) = target {
                        match gates.acquire(&identity, git::MutationScope::Worktree, &cancel) {
                            Ok(acquired) => {
                                lease = Some(acquired);
                                let result =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        dispatch(&client, target, options)
                                    }))
                                    .unwrap_or_else(|_| {
                                        Err(io::Error::other(
                                            "mutation worker panicked; outcome may be uncertain",
                                        ))
                                    });
                                if let Err(error) = result {
                                    action_error =
                                        Some(format!("action failed: {error}; no automatic retry"));
                                }
                            }
                            Err(error) => {
                                action_error = Some(format!("action unavailable: {error}"))
                            }
                        }
                    }
                    // This runs even if verification or a write failed. The lease stays
                    // attached to the update until shell delivery/reconciliation.
                    if mutation {
                        read_snapshot(&client.reconciliation(), options)
                    } else {
                        read_snapshot(&client, options)
                    }
                }))
                .unwrap_or_else(|_| {
                    Err(io::Error::other(
                        "working-tree worker panicked; outcome may be uncertain",
                    ))
                });
                let _ = tx.try_send(Update {
                    owner: token,
                    generation,
                    options,
                    snapshot,
                    action_error,
                    lease,
                    _scope: scope,
                });
            });
        if let Err(error) = spawned {
            self.flight = None;
            self.queue.clear();
            self.error = Some(format!("cannot start working-tree worker: {error}"));
        }
    }
}
impl Drop for WorkingTree {
    fn drop(&mut self) {
        if let Some(flight) = &self.flight {
            flight.cancel.store(true, Ordering::Release);
        }
        self.rx.close();
        while self.rx.try_recv().is_ok() {}
    }
}
fn index(side: Side) -> usize {
    if side == Side::Worktree { 0 } else { 1 }
}
fn side_at(i: usize) -> Side {
    if i == 0 { Side::Worktree } else { Side::Index }
}
fn successor(map: &[Option<usize>], cursor: usize) -> Option<usize> {
    map.get(cursor..)?.iter().find_map(|id| *id)
}
fn remap_pane(
    path: &OsStr,
    side: Side,
    old: &Pane,
    fresh: Option<&Pane>,
    partial: Option<&(OsString, Side, diff::Patch, BTreeSet<usize>)>,
) -> io::Result<Vec<Option<usize>>> {
    let patch = old
        .patch
        .as_ref()
        .ok_or_else(|| io::Error::other("partial patch unavailable"))?;
    let mutation = partial
        .filter(|(p, s, snapshot, _)| {
            p == path && *s == side && snapshot.matches_snapshot(&old.canonical)
        })
        .map(|(_, _, _, selected)| (selected, side == Side::Index));
    patch.remap_changes(fresh.and_then(|p| p.patch.as_ref()), mutation)
}
fn reconcile_intent(
    intent: &mut Intent,
    files: &BTreeMap<OsString, FileSnapshot>,
    partial: Option<&(OsString, Side, diff::Patch, BTreeSet<usize>)>,
    whole: Option<&(Vec<FileSnapshot>, Side)>,
) -> io::Result<()> {
    if let Intent::Files(targets, side) = intent {
        let mut surviving = Vec::new();
        for old in targets.iter() {
            let fresh = files.get(&old.entry.path);
            let known_whole = whole.is_some_and(|(written, written_side)| {
                written_side == side && written.iter().any(|file| same_file(file, old))
            });
            if known_whole {
                // A repeated explicit-side action is satisfied, not a toggle/retry.
                // Replacement worktree/index bytes leave a nonempty diff and reject.
                if fresh.is_some_and(|file| {
                    !file.panes[index(*side)].is_empty()
                        || (untracked(&file.entry) && *side == Side::Worktree)
                }) || old.entry.original.as_ref().is_some_and(|path| {
                    files
                        .get(path)
                        .is_some_and(|file| !file.panes[index(*side)].is_empty())
                        || fresh.is_some_and(|file| !file.original[index(*side)].is_empty())
                }) {
                    return Err(io::Error::other("stale queued file; refresh and reconcile"));
                }
                continue;
            }
            let own_partial = partial.filter(|(path, written_side, patch, _)| {
                path == &old.entry.path
                    && written_side == side
                    && patch.matches_snapshot(&old.panes[index(*side)].canonical)
            });
            if own_partial.is_some() {
                remap_pane(
                    &old.entry.path,
                    *side,
                    &old.panes[index(*side)],
                    fresh.map(|file| &file.panes[index(*side)]),
                    partial,
                )?;
                let Some(fresh) = fresh else {
                    // The exact known selection consumed the last diff in a now-clean file.
                    continue;
                };
                if fresh.entry.path != old.entry.path
                    || fresh.entry.original != old.entry.original
                    || fresh.original != old.original
                    || fresh.original_entry != old.original_entry
                {
                    return Err(io::Error::other(
                        "stale queued file identity; refresh and reconcile",
                    ));
                }
                if fresh.panes[index(*side)].is_empty() {
                    continue;
                }
                surviving.push(fresh.clone());
            } else {
                let fresh = fresh.ok_or_else(|| {
                    io::Error::other("file target disappeared; refresh and reconcile")
                })?;
                if !same_file(old, fresh) {
                    return Err(io::Error::other("stale queued file; refresh and reconcile"));
                }
                surviving.push(old.clone());
            }
        }
        *targets = surviving;
        return Ok(());
    }
    let Intent::Current {
        path,
        side,
        canonical,
        cursor,
        mode,
        selection,
        repeat,
    } = intent
    else {
        return Ok(());
    };
    let fresh = files.get(path).map(|f| &f.panes[index(*side)]);
    let own_mutation = partial.is_some_and(|(p, s, patch, _)| {
        p == path && s == side && patch.matches_snapshot(canonical)
    });
    if !own_mutation && fresh.is_some_and(|p| p.canonical == *canonical) {
        return Ok(());
    }
    let patch = diff::Patch::parse(canonical)?;
    let old = Pane {
        canonical: canonical.clone(),
        patch: Some(patch),
        ..Pane::default()
    };
    let map = remap_pane(path, *side, &old, fresh, partial)?;
    let mut surviving: BTreeSet<_> = selection
        .iter()
        .filter_map(|id| map.get(*id).copied().flatten())
        .collect();
    if *repeat {
        // Only an unmoved, non-explicit repeat may advance past a consumed target.
        // Hunk boundaries are fresh; explicitly navigated/ranged sets are not expanded.
        let next = successor(&map, *cursor)
            .ok_or_else(|| io::Error::other("no surviving queued change"))?;
        surviving = match mode {
            SelectionMode::Hunk => fresh
                .and_then(|p| p.patch.as_ref())
                .and_then(|p| p.hunks().find(|h| h.changes.contains(&next)))
                .map(|h| h.changes.collect())
                .unwrap_or_default(),
            _ => [next].into_iter().collect(),
        };
    }
    if surviving.is_empty() {
        return Err(io::Error::other(
            "queued selection was consumed; select a surviving change",
        ));
    }
    *cursor = *surviving.first().unwrap();
    *selection = surviving;
    *canonical = fresh.unwrap().canonical.clone();
    Ok(())
}
fn same_file(old: &FileSnapshot, fresh: &FileSnapshot) -> bool {
    old.entry == fresh.entry
        && old.original == fresh.original
        && old.original_entry == fresh.original_entry
        && old
            .panes
            .iter()
            .zip(&fresh.panes)
            .all(|(a, b)| a.canonical == b.canonical)
}
fn opposite(side: Side) -> Side {
    if side == Side::Worktree {
        Side::Index
    } else {
        Side::Worktree
    }
}
fn untracked(entry: &git::Entry) -> bool {
    entry.index == b'?' && entry.worktree == b'?'
}
fn canonical(
    client: &git::Client,
    entry: &git::Entry,
    side: Side,
    options: git::DiffOptions,
) -> io::Result<Vec<u8>> {
    if untracked(entry) {
        if side == Side::Index {
            Ok(vec![])
        } else {
            client.untracked_diff_verified(Path::new(&entry.path))
        }
    } else {
        client.canonical_diff(Path::new(&entry.path), side, options.context)
    }
}
fn read_snapshot(client: &git::Client, options: git::DiffOptions) -> io::Result<Snapshot> {
    let entries = client.status()?;
    let mut files = BTreeMap::new();
    for entry in &entries {
        let mut panes = [Pane::default(), Pane::default()];
        for side in [Side::Worktree, Side::Index] {
            let canonical = canonical(client, entry, side, options)?;
            let display = if options.whitespace == git::Whitespace::Exact || untracked(entry) {
                canonical.clone()
            } else {
                client.display_diff(Path::new(&entry.path), side, options)?
            };
            let parsed = diff::Patch::parse(&canonical);
            let disabled_reason = if canonical.is_empty() {
                Some("no changes on this side".into())
            } else if options.whitespace != git::Whitespace::Exact {
                Some("partial selection is disabled while ignoring whitespace; use exact display or whole-file action".into())
            } else if entry.original.is_some()
                || matches!(entry.index, b'R' | b'C')
                || matches!(entry.worktree, b'R' | b'C')
            {
                Some("rename/copy requires a whole-file operation".into())
            } else {
                match &parsed {
                    Err(error) => Some(error.to_string()),
                    Ok(patch) if patch.hunks().any(|h| !h.supports_partial_selection) => Some("zero-context partial selection is unsafe; increase context or use whole-file action".into()),
                    _ => None,
                }
            };
            panes[index(side)] = Pane {
                canonical,
                display,
                patch: parsed.ok(),
                disabled_reason,
                ..Pane::default()
            };
        }
        let original_entry = entry
            .original
            .as_ref()
            .and_then(|path| entries.iter().find(|candidate| &candidate.path == path))
            .cloned();
        let mut original = [vec![], vec![]];
        if let Some(path) = &entry.original {
            for side in [Side::Worktree, Side::Index] {
                original[index(side)] =
                    original_canonical(client, path, original_entry.as_ref(), side, options)?;
            }
        }
        files.insert(
            entry.path.clone(),
            FileSnapshot {
                entry: entry.clone(),
                panes,
                original,
                original_entry,
            },
        );
    }
    if entries != client.status()? {
        return Err(io::Error::other(
            "repository changed during refresh; refresh again",
        ));
    }
    Ok(Snapshot { entries, files })
}
fn verify(
    client: &git::Client,
    file: &FileSnapshot,
    side: Side,
    options: git::DiffOptions,
    entries: &[git::Entry],
) -> io::Result<()> {
    if !entries.iter().any(|entry| entry == &file.entry)
        || canonical(client, &file.entry, side, options)? != file.panes[index(side)].canonical
    {
        return Err(io::Error::other("stale target; refresh and reconcile"));
    }
    if let Some(path) = &file.entry.original
        && client.canonical_diff(Path::new(path), side, options.context)?
            != file.original[index(side)]
    {
        return Err(io::Error::other(
            "stale rename original; refresh and reconcile",
        ));
    }
    if file.panes[index(side)].is_empty() && !(untracked(&file.entry) && side == Side::Worktree) {
        return Err(io::Error::other("no changes on requested side"));
    }
    Ok(())
}
fn original_canonical(
    client: &git::Client,
    path: &OsStr,
    entry: Option<&git::Entry>,
    side: Side,
    options: git::DiffOptions,
) -> io::Result<Vec<u8>> {
    if let Some(entry) = entry {
        canonical(client, entry, side, options)
    } else {
        client.canonical_diff(Path::new(path), side, options.context)
    }
}
fn verify_file(
    client: &git::Client,
    file: &FileSnapshot,
    options: git::DiffOptions,
    entries: &[git::Entry],
) -> io::Result<()> {
    if !entries.iter().any(|entry| entry == &file.entry) {
        return Err(io::Error::other("stale target; refresh and reconcile"));
    }
    for side in [Side::Worktree, Side::Index] {
        if canonical(client, &file.entry, side, options)? != file.panes[index(side)].canonical {
            return Err(io::Error::other("stale target; refresh and reconcile"));
        }
        if let Some(path) = &file.entry.original {
            let entry = entries.iter().find(|entry| &entry.path == path);
            if entry != file.original_entry.as_ref()
                || original_canonical(client, path, entry, side, options)?
                    != file.original[index(side)]
            {
                return Err(io::Error::other(
                    "stale rename original; refresh and reconcile",
                ));
            }
        }
    }
    Ok(())
}
fn whole(client: &git::Client, file: &FileSnapshot, side: Side) -> io::Result<()> {
    if side == Side::Index {
        client.unstage_entry(&file.entry)
    } else {
        client.stage_entry(&file.entry)
    }
}
fn dispatch(client: &git::Client, target: Target, options: git::DiffOptions) -> io::Result<()> {
    let entries = client.status()?;
    match target {
        Target::Files(files, side) => {
            // Verify every path before the first write; errors may still follow
            // partial side effects, so the caller always reconciles under its lease.
            for file in &files {
                verify_file(client, file, options, &entries)?;
            }
            for file in &files {
                if !file.panes[index(side)].is_empty()
                    || (untracked(&file.entry) && side == Side::Worktree)
                {
                    whole(client, file, side)?;
                }
            }
            Ok(())
        }
        Target::Partial(file, side, selected) => {
            verify(client, &file, side, options, &entries)?;
            let pane = &file.panes[index(side)];
            if let Some(reason) = &pane.disabled_reason {
                return Err(io::Error::other(reason.clone()));
            }
            let patch = pane
                .patch
                .as_ref()
                .ok_or_else(|| io::Error::other("partial patch unavailable"))?;
            match patch.select(&selected, side == Side::Index)? {
                diff::Selection::Patch(bytes) => client.apply_index(&bytes),
                diff::Selection::WholeFile => whole(client, &file, side),
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/lifecycle.rs"]
mod lifecycle_tests;
#[cfg(test)]
#[path = "tests/owner.rs"]
mod tests;
