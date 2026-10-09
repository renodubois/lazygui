//! In-process resource coordination, not a replacement for Git's own locks.
//! Share one holder across windows. Owners keep a lease from target verification
//! through dispatch AND authoritative reconciliation, including failed/cancelled writes.
use super::Identity;
use std::{
    collections::BTreeSet,
    io,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutationScope {
    /// Index and worktree operations: stage/unstage/apply.
    Worktree,
    /// Shared refs/history operations. Does not protect a worktree's index.
    Shared,
    /// Commit and operations touching both index/worktree and shared refs.
    All,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Resource {
    Index(PathBuf),
    Worktree(PathBuf),
    Common(PathBuf),
}
#[derive(Default)]
struct Inner {
    held: Mutex<BTreeSet<Resource>>,
    changed: Condvar,
}
/// Startup-created, cloneable holder; do not create one independent holder/window.
#[derive(Clone, Default)]
pub struct MutationGates {
    inner: Arc<Inner>,
}
pub struct MutationLease {
    inner: Arc<Inner>,
    resources: Vec<Resource>,
}
fn resources(identity: &Identity, scope: MutationScope) -> io::Result<Vec<Resource>> {
    let mut result = vec![];
    if matches!(scope, MutationScope::Worktree | MutationScope::All) {
        let path = identity.worktree.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "bare repository has no worktree mutation resource",
            )
        })?;
        // The index may not exist yet (unborn repositories). Canonical git_dir
        // identifies its index even when distinct worktrees bind the same git_dir.
        result.push(Resource::Index(identity.git_dir.canonicalize()?));
        result.push(Resource::Worktree(path.canonicalize()?));
    }
    if matches!(scope, MutationScope::Shared | MutationScope::All) {
        result.push(Resource::Common(identity.common_dir.canonicalize()?));
    }
    Ok(result)
}
impl MutationGates {
    pub fn new() -> Self {
        Self::default()
    }
    /// Nonblocking contention check (canonicalization still performs filesystem I/O).
    /// Prefer calling even this on a worker rather than the GUI thread.
    pub fn try_acquire(
        &self,
        identity: &Identity,
        scope: MutationScope,
    ) -> io::Result<Option<MutationLease>> {
        let resources = resources(identity, scope)?;
        let mut held = self.inner.held.lock().unwrap();
        if resources.iter().any(|key| held.contains(key)) {
            return Ok(None);
        }
        held.extend(resources.iter().cloned());
        Ok(Some(MutationLease {
            inner: self.inner.clone(),
            resources,
        }))
    }
    /// Worker-only cancellable wait. All requested resources are acquired atomically
    /// so linked-worktree/common-dir acquisition cannot introduce lock-order deadlocks.
    pub fn acquire(
        &self,
        identity: &Identity,
        scope: MutationScope,
        cancel: &AtomicBool,
    ) -> io::Result<MutationLease> {
        let resources = resources(identity, scope)?;
        let mut held = self.inner.held.lock().unwrap();
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "mutation gate wait cancelled",
                ));
            }
            if resources.iter().all(|key| !held.contains(key)) {
                held.extend(resources.iter().cloned());
                return Ok(MutationLease {
                    inner: self.inner.clone(),
                    resources,
                });
            }
            held = self
                .inner
                .changed
                .wait_timeout(held, Duration::from_millis(10))
                .unwrap()
                .0;
        }
    }
}
impl MutationLease {
    /// Validate scope/identity before attaching a lease to an owner workflow.
    pub fn covers(&self, identity: &Identity, scope: MutationScope) -> io::Result<bool> {
        Ok(resources(identity, scope)?
            .iter()
            .all(|key| self.resources.contains(key)))
    }
}
impl Drop for MutationLease {
    fn drop(&mut self) {
        let mut held = self.inner.held.lock().unwrap();
        for key in &self.resources {
            held.remove(key);
        }
        self.inner.changed.notify_all();
    }
}
#[cfg(test)]
#[path = "tests/gates.rs"]
mod tests;
