use super::*;

impl Commit {
    /// Call on opening the new-commit form, not on submit/retry. No Git I/O runs
    /// on the caller thread. Deliver `updates()` through the existing consumer;
    /// observe the owner's draft after `apply`, never prepend in the controls.
    /// Retained/edited/recalled drafts, duplicate opens and completed empty
    /// preparations are untouched. Cancellation/stale edits cannot overwrite text.
    pub fn prepare_draft(&mut self) -> bool {
        if self.busy() || self.warning.is_some() || self.prepared || self.draft != Draft::default()
        {
            return false;
        }
        if self.settings.commit_prefixes.is_empty() {
            self.prepared = true;
            return false;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (client, scope) = match self.client.begin_workflow(cancel.clone()) {
            Ok(admitted) => admitted,
            Err(error) => {
                self.error = Some(format!("Commit draft preparation unavailable: {error}"));
                return false;
            }
        };
        self.cancellation = Some(cancel.clone());
        self.generation += 1;
        self.outcome = None;
        self.error = None;
        let generation = self.generation;
        let owner = self.token.clone();
        let revision = self.revision;
        let identity = self.identity.clone();
        let gates = self.gates.clone();
        let rules = self.settings.commit_prefixes.clone();
        let send = self.send.clone();
        let spawn = std::thread::Builder::new().name("commit-draft".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _lease = gates.acquire(&identity, git::MutationScope::All, &cancel)?;
                let client = client.bind(&identity)?;
                if client.discover()? != identity {
                    return Err(io::Error::other("Repository identity changed; reopen before preparing a draft."));
                }
                let head = client.head()?;
                let branch = match &head {
                    git::Head::Unborn { branch } | git::Head::Branch { branch, .. } => branch.to_str()
                        .ok_or_else(|| io::Error::other("Commit prefix cannot match a non-UTF-8 branch name without loss."))?,
                    // LazyGit BranchLoader uses CurrentBranchInfo.RefName: the
                    // full detached object ID, not its display name or short hash.
                    git::Head::Detached { oid } => oid,
                };
                let repository = repository_name(&identity, &rules)?;
                Ok(rules.prefill(repository, branch).map(|message| Draft::from_message(&message)))
            })).unwrap_or_else(|_| Err(io::Error::other("Commit draft worker panicked; no write dispatched.")));
            let _ = send.send_blocking(Update {
                owner, generation, revision, result: Completion::Prepared(result), _scope: scope,
            });
        });
        if spawn.is_err() {
            self.cancellation = None;
            self.error = Some("Could not start commit draft worker; no write dispatched.".into());
            return false;
        }
        true
    }
}
/// RepoPaths.repoPathsForDir at pinned c5f7158: main worktree (including
/// submodules) uses its root basename; ordinary linked worktrees use the parent
/// of the common Git dir. A bare repository uses that same metadata fallback.
/// The current identity does not expose --show-superproject-working-tree, so
/// nonstandard linked metadata cannot faithfully distinguish linked submodules
/// from bare/separate-dir worktrees. Diagnose that case when repo overrides exist.
fn repository_name<'a>(
    identity: &'a git::Identity,
    rules: &lazygit_config::CommitPrefixes,
) -> io::Result<&'a str> {
    let path = if identity.git_dir == identity.common_dir {
        identity
            .worktree
            .as_deref()
            .or_else(|| identity.common_dir.parent())
    } else {
        if identity
            .common_dir
            .file_name()
            .is_none_or(|name| name != ".git")
            && rules.has_repository_rules()
        {
            return Err(io::Error::other(
                "Repository-specific commit prefixes are unsupported for linked worktrees with nonstandard common metadata; the repository name is ambiguous.",
            ));
        }
        identity.common_dir.parent()
    };
    path.and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            io::Error::other("Commit prefix repository name is not an unambiguous UTF-8 basename.")
        })
}

#[cfg(test)]
#[path = "tests/prefix.rs"]
mod tests;
