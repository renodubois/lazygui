//! Retained non-rendering files/index owner and checked patch primitives.
//! UI keys enqueue intentions, not display-row targets. Shell delivery is single-consumer.
mod owner;
pub use owner::{Pane, SelectionMode, Side, Update, WorkingTree};
#[derive(Default)]
pub struct Barrier {
    pending: usize,
    in_flight: bool,
    generation: u64,
}
impl Barrier {
    pub fn enqueue(&mut self) {
        self.pending += 1;
    }
    pub fn begin(&mut self) -> Option<u64> {
        if self.in_flight || self.pending == 0 {
            return None;
        }
        self.pending -= 1;
        self.in_flight = true;
        self.generation += 1;
        Some(self.generation)
    }
    /// Called only after authoritative read + target/focus reconciliation, not command exit.
    pub fn reconciled(&mut self, generation: u64) -> bool {
        if !self.in_flight || generation != self.generation {
            return false;
        }
        self.in_flight = false;
        true
    }
}
/// Confirm against fresh canonical bytes immediately before dispatch.
/// External Git can still race; Git apply/index locking is authoritative, no write retry.
/// Worker-only low-level compatibility API: the caller MUST retain a shared
/// Worktree mutation lease over this call AND authoritative reconciliation,
/// including failure. WorkingTree does this automatically. This primitive uses
/// default canonical context; use the owner for other options and rename entries.
pub fn apply_selection(
    client: &crate::git::Client,
    path: &std::path::Path,
    side: crate::git::Side,
    snapshot: &crate::diff::Patch,
    selected: &std::collections::BTreeSet<usize>,
) -> std::io::Result<()> {
    let current = if side == crate::git::Side::Worktree
        && client.status()?.iter().any(|entry| {
            entry.path == path.as_os_str() && entry.index == b'?' && entry.worktree == b'?'
        }) {
        client.untracked_diff(path)?
    } else {
        client.diff(path, side)?
    };
    if !snapshot.matches_snapshot(&current) {
        return Err(std::io::Error::other(
            "stale patch target; refresh and reconcile",
        ));
    }
    match snapshot.select(selected, side == crate::git::Side::Index)? {
        crate::diff::Selection::Patch(patch) => client.apply_index(&patch),
        crate::diff::Selection::WholeFile if side == crate::git::Side::Index => {
            client.unstage_file(path)
        }
        crate::diff::Selection::WholeFile => client.stage_file(path),
    }
}
#[cfg(test)]
#[path = "tests/stale.rs"]
mod stale_tests;
#[cfg(test)]
#[path = "tests/barrier.rs"]
mod tests;
