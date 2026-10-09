//! Window-retained commit workflow. Blocking Git work runs on owned worker scopes;
//! the shell delivers opaque updates once and notifies its entity after `apply`.
use crate::{git, lazygit_config};
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
mod message;
mod prefix;
pub use message::Draft;

#[derive(Clone)]
pub struct Settings {
    pub message: lazygit_config::MessageSettings,
    pub skip_no_staged_files_warning: bool,
    pub skip_hook_prefix: String,
    pub commit_prefixes: lazygit_config::CommitPrefixes,
}
impl From<lazygit_config::M1Settings> for Settings {
    fn from(settings: lazygit_config::M1Settings) -> Self {
        Self {
            message: settings.message,
            skip_no_staged_files_warning: settings.warnings.skip_no_staged_files_warning,
            skip_hook_prefix: settings.skip_hook_prefix,
            commit_prefixes: settings.commit_prefixes,
        }
    }
}
impl From<lazygit_config::MessageSettings> for Settings {
    fn from(message: lazygit_config::MessageSettings) -> Self {
        Self {
            message,
            skip_no_staged_files_warning: false,
            skip_hook_prefix: String::new(),
            commit_prefixes: lazygit_config::CommitPrefixes::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Warning {
    NoStagedFiles,
}
/// An exit code alone is not success. Every dispatched mutation is reconciled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Committed {
        head: git::Head,
        status: Vec<git::Entry>,
    },
    NotCommitted {
        head: git::Head,
        status: Vec<git::Entry>,
    },
    /// A changed HEAD after an error, unchanged HEAD after success, or failed read.
    /// Never automatically resubmit: the caller can inspect the observed state.
    Uncertain {
        head: Option<git::Head>,
        status: Option<Vec<git::Entry>>,
    },
}
pub struct Update {
    owner: Arc<()>,
    generation: u64,
    revision: u64,
    result: Completion,
    _scope: git::WorkflowScope,
}
enum Completion {
    Prepared(io::Result<Option<Draft>>),
    Warning,
    Finished {
        outcome: Option<Outcome>,
        error: Option<String>,
    },
}
pub struct Commit {
    client: git::Client,
    identity: git::Identity,
    gates: Arc<git::MutationGates>,
    settings: Settings,
    draft: Draft,
    prepared: bool,
    revision: u64,
    generation: u64,
    token: Arc<()>,
    cancellation: Option<Arc<AtomicBool>>,
    warning: Option<Warning>,
    outcome: Option<Outcome>,
    error: Option<String>,
    send: async_channel::Sender<Update>,
    receive: async_channel::Receiver<Update>,
}
impl Commit {
    pub fn new(
        client: git::Client,
        identity: git::Identity,
        gates: Arc<git::MutationGates>,
        settings: impl Into<Settings>,
    ) -> Self {
        let (send, receive) = async_channel::unbounded();
        Self {
            client,
            identity,
            gates,
            settings: settings.into(),
            draft: Draft::default(),
            prepared: false,
            revision: 0,
            generation: 0,
            token: Arc::new(()),
            cancellation: None,
            warning: None,
            outcome: None,
            error: None,
            send,
            receive,
        }
    }
    /// Single-consumer delivery stream: retain exactly one receiver task per owner.
    pub fn updates(&self) -> async_channel::Receiver<Update> {
        self.receive.clone()
    }
    pub fn draft(&self) -> &Draft {
        &self.draft
    }
    pub fn set_draft(&mut self, subject: impl Into<String>, body: impl Into<String>) {
        let draft = Draft {
            subject: subject.into(),
            body: body.into(),
        };
        if self.draft != draft {
            self.draft = draft;
            self.prepared = true;
            self.revision += 1;
        }
    }
    pub fn busy(&self) -> bool {
        self.cancellation.is_some()
    }
    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn warning(&self) -> Option<Warning> {
        self.warning
    }
    /// Returns false for a duplicate, pending warning or invalid subject. Never queues a write.
    pub fn submit(&mut self) -> bool {
        if self.warning.is_some() {
            return false;
        }
        self.start(false)
    }
    /// Explicitly approves staging all *currently* changed paths, then committing.
    /// Fresh verification and staging/commit/reconciliation share one All lease.
    pub fn confirm_stage_all(&mut self) -> bool {
        if self.warning != Some(Warning::NoStagedFiles) {
            return false;
        }
        self.warning = None;
        self.start(true)
    }
    /// Dismisses the warning and requests cancellation, but retains the draft and
    /// stays busy until settlement/reconciliation. Cancellation cannot undo a commit.
    pub fn cancel(&mut self) {
        self.warning = None;
        if let Some(cancel) = &self.cancellation {
            cancel.store(true, Ordering::Release);
        }
    }
    pub fn apply(&mut self, update: Update) -> bool {
        if !Arc::ptr_eq(&update.owner, &self.token)
            || update.generation != self.generation
            || !self.busy()
        {
            return false;
        }
        let cancelled = self
            .cancellation
            .take()
            .is_some_and(|cancel| cancel.load(Ordering::Acquire));
        match update.result {
            Completion::Prepared(result) => {
                if !cancelled && update.revision == self.revision && !self.prepared {
                    match result {
                        Ok(draft) => {
                            self.prepared = true;
                            if let Some(draft) = draft {
                                self.draft = draft;
                                self.revision += 1;
                            }
                        }
                        Err(error) => self.error = Some(feedback(&error)),
                    }
                }
            }
            Completion::Warning if !cancelled => self.warning = Some(Warning::NoStagedFiles),
            Completion::Warning => {}
            Completion::Finished { outcome, error } => {
                if matches!(outcome, Some(Outcome::Committed { .. }))
                    && update.revision == self.revision
                    && !cancelled
                {
                    self.draft = Draft::default();
                    self.prepared = false;
                    self.revision += 1;
                }
                self.outcome = outcome;
                self.error = error;
            }
        }
        true
    }
    fn start(&mut self, stage_all: bool) -> bool {
        if self.busy() {
            return false;
        }
        if self.draft.subject.trim().is_empty()
            || self.draft.subject.contains(['\n', '\r', '\0'])
            || self.draft.body.contains('\0')
        {
            self.error =
                Some("A nonempty single-line subject and NUL-free body are required.".into());
            return false;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancellation = Some(cancel.clone());
        self.generation += 1;
        self.outcome = None;
        self.error = None;
        let generation = self.generation;
        let token = self.token.clone();
        let revision = self.revision;
        let (client, scope) = match self.client.begin_workflow(cancel.clone()) {
            Ok(admitted) => admitted,
            Err(error) => {
                self.cancellation = None;
                self.error = Some(format!("Commit workflow unavailable: {error}"));
                return false;
            }
        };
        let identity = self.identity.clone();
        let gates = self.gates.clone();
        let settings = self.settings.clone();
        let draft = self.draft.clone();
        let send = self.send.clone();
        let spawn = std::thread::Builder::new()
            .name("commit-workflow".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    execute(
                        client, &identity, &gates, &settings, &draft, stage_all, cancel,
                    )
                }))
                .unwrap_or_else(|_| Completion::Finished {
                    outcome: Some(Outcome::Uncertain {
                        head: None,
                        status: None,
                    }),
                    error: Some(
                        "Commit worker panicked; inspect repository state before retrying.".into(),
                    ),
                });
                let _ = send.send_blocking(Update {
                    owner: token,
                    generation,
                    revision,
                    result,
                    _scope: scope,
                });
            });
        if spawn.is_err() {
            self.cancellation = None;
            self.error = Some("Could not start commit worker; no write dispatched.".into());
            return false;
        }
        true
    }
}
impl Drop for Commit {
    fn drop(&mut self) {
        self.cancel();
        self.receive.close();
        while self.receive.try_recv().is_ok() {}
    }
}
fn execute(
    client: git::Client,
    identity: &git::Identity,
    gates: &git::MutationGates,
    settings: &Settings,
    draft: &Draft,
    stage_all: bool,
    cancel: Arc<AtomicBool>,
) -> Completion {
    let result = (|| -> io::Result<Completion> {
        let _lease = gates.acquire(identity, git::MutationScope::All, &cancel)?;
        let client = client.bind(identity)?;
        let request = client.with_cancellation(cancel.clone());
        if request.discover()? != *identity {
            return Err(io::Error::other(
                "Repository identity changed; reopen before committing.",
            ));
        }
        let before = request.head()?;
        let entries = request.status()?;
        if entries.is_empty() {
            return Err(io::Error::other("No changed files to commit."));
        }
        if entries.iter().any(|e| {
            e.index == b'U'
                || e.worktree == b'U'
                || matches!((e.index, e.worktree), (b'A', b'A') | (b'D', b'D'))
        }) {
            return Err(io::Error::other(
                "Resolve merge conflicts before committing.",
            ));
        }
        let staged = entries
            .iter()
            .any(|e| !matches!(e.index, b' ' | b'?' | b'!'));
        if !staged && !stage_all && !settings.skip_no_staged_files_warning {
            return Ok(Completion::Warning);
        }
        let options = settings.commit_options(&draft.subject);
        // A transport panic is also an uncertain write, not permission to skip
        // reconciliation or release the All lease before authoritative reads.
        let write = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> io::Result<()> {
            if stage_all || !staged {
                let mut paths: Vec<PathBuf> =
                    entries.iter().map(|e| PathBuf::from(&e.path)).collect();
                for e in &entries {
                    if matches!(e.worktree, b'R' | b'C')
                        && let Some(old) = &e.original
                    {
                        paths.push(PathBuf::from(old));
                    }
                }
                request.stage_paths(&paths)?;
                // Stage-all must complete and be verified before attempting the commit.
                if !request
                    .status()?
                    .iter()
                    .any(|e| !matches!(e.index, b' ' | b'?' | b'!'))
                {
                    return Err(io::Error::other("Stage-all produced no staged changes."));
                }
            }
            request.commit_with_options(draft.message(&settings.message).as_bytes(), options)
        }))
        .unwrap_or_else(|_| {
            Err(io::Error::other(
                "Commit transport panicked; write outcome requires reconciliation.",
            ))
        });
        // Deliberately fresh noncancelled read scope after write settlement, still gated.
        let reconcile = client.with_cancellation(Arc::new(AtomicBool::new(false)));
        let head = reconcile.head();
        let status = reconcile.status();
        let mut error = write.as_ref().err().map(feedback);
        let outcome = match (head, status) {
            (Ok(head), Ok(status))
                if write.is_ok() && oid(&head).is_some() && oid(&head) != oid(&before) =>
            {
                Outcome::Committed { head, status }
            }
            (Ok(head), Ok(status)) if write.is_err() && head == before => {
                Outcome::NotCommitted { head, status }
            }
            (head, status) => {
                let read_error = head.as_ref().err().or(status.as_ref().err());
                let detail = read_error
                    .map(feedback)
                    .unwrap_or_else(|| "Command result and observed HEAD disagree.".into());
                error = Some(format!(
                    "{} Outcome uncertain; inspect HEAD before retrying. {detail}",
                    error.unwrap_or_default()
                ));
                Outcome::Uncertain {
                    head: head.ok(),
                    status: status.ok(),
                }
            }
        };
        Ok(Completion::Finished {
            outcome: Some(outcome),
            error,
        })
    })();
    result.unwrap_or_else(|e| Completion::Finished {
        outcome: None,
        error: Some(feedback(&e)),
    })
}
fn oid(head: &git::Head) -> Option<&str> {
    match head {
        git::Head::Unborn { .. } => None,
        git::Head::Branch { oid, .. } | git::Head::Detached { oid } => Some(oid),
    }
}
impl Settings {
    /// LazyGit v0.66.0 CommitCmdObj: nonempty, literal, case-sensitive HasPrefix
    /// on the original summary (no trimming, word boundary or prefix removal).
    /// https://github.com/jesseduffield/lazygit/blob/v0.66.0/pkg/commands/git_commands/commit.go
    pub fn commit_options(&self, subject: &str) -> git::CommitOptions {
        git::CommitOptions {
            sign_off: self.message.sign_off,
            no_verify: !self.skip_hook_prefix.is_empty()
                && subject.starts_with(&self.skip_hook_prefix),
        }
    }
}
fn feedback(error: &io::Error) -> String {
    let mut text = error.to_string();
    if let Some(failure) = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<git::GitFailure>())
        && !failure.stderr.is_empty()
    {
        text.push_str(": ");
        text.push_str(&String::from_utf8_lossy(&failure.stderr));
    }
    sanitize(&text)
}
/// Bound feedback, strip ANSI/OSC/control bytes, and redact URL userinfo.
fn sanitize(text: &str) -> String {
    let mut clean = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for x in chars.by_ref() {
                        if ('@'..='~').contains(&x) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(x) = chars.next() {
                        if x == '\u{7}' || (x == '\u{1b}' && chars.peek() == Some(&'\\')) {
                            if x == '\u{1b}' {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if !c.is_control() || matches!(c, '\n' | '\t') {
            clean.push(c);
        }
        if clean.len() >= 4096 {
            clean.push_str("… [truncated]");
            break;
        }
    }
    clean
        .split_inclusive(char::is_whitespace)
        .map(|token| {
            if let Some((scheme, rest)) = token.split_once("://") {
                let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
                if let Some(at) = rest[..authority_end].rfind('@') {
                    return format!("{scheme}://[redacted]@{}", &rest[at + 1..]);
                }
            }
            token.to_owned()
        })
        .collect()
}
#[cfg(test)]
#[path = "tests/installed.rs"]
mod installed_tests;
#[cfg(test)]
#[path = "tests/workflow.rs"]
mod tests;
