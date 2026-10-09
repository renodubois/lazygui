//! Typed, byte-preserving Git operations. No shell interpolation.
//! Calls are blocking worker primitives. Startup shares ProcessHost/MutationGates;
//! owners hold a mutation lease over verification, dispatch and reconciliation.
//! Low-level writes intentionally do not acquire/release a gate internally: doing
//! so would release protection before owner reconciliation (or deadlock a lease).
mod gates;
pub mod process;
mod reads;
mod types;
pub use gates::{MutationGates, MutationLease, MutationScope};
use process::{Executor, Native, ProcessHost};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub use types::*;

#[derive(Clone)]
pub struct Client {
    cwd: PathBuf,
    binding: Option<Identity>,
    executor: Arc<dyn Executor>,
    cancel: Arc<AtomicBool>,
    workflow: Option<process::Workflow>,
}
/// Keep with the opaque result until owner reconciliation (or result drop).
/// Non-host test clients use the same API with an empty retention token.
pub struct WorkflowScope {
    _workflow: Option<process::Workflow>,
}
impl Client {
    pub fn new(path: PathBuf) -> Self {
        Self::with_executor(path, Arc::new(Native))
    }
    pub fn with_executor(cwd: PathBuf, executor: Arc<dyn Executor>) -> Self {
        Self {
            cwd,
            binding: None,
            executor,
            cancel: Arc::new(AtomicBool::new(false)),
            workflow: None,
        }
    }
    /// Application constructor: every command is retained until host shutdown ack.
    pub fn with_process_host(cwd: PathBuf, host: &ProcessHost) -> Self {
        Self::with_executor(cwd, host.retain(Arc::new(Native)))
    }
    /// Startup convenience: validate readiness and bind all subsequent relative paths
    /// to the canonical repository root (including opening from a nested directory).
    pub fn open(path: PathBuf, host: &ProcessHost) -> io::Result<(Self, Readiness)> {
        let client = Self::with_process_host(path, host);
        let readiness = client.readiness()?;
        Ok((client.bind(&readiness.identity)?, readiness))
    }
    /// Bind a discovered identity immutably, with explicit Git-dir/worktree argv.
    /// Also useful with injected executors; supplied paths must still exist.
    pub fn bind(&self, identity: &Identity) -> io::Result<Self> {
        let identity = Identity {
            worktree: identity
                .worktree
                .as_ref()
                .map(|path| path.canonicalize())
                .transpose()?,
            git_dir: identity.git_dir.canonicalize()?,
            common_dir: identity.common_dir.canonicalize()?,
        };
        Ok(Self {
            cwd: identity
                .worktree
                .clone()
                .unwrap_or_else(|| identity.git_dir.clone()),
            binding: Some(identity),
            executor: self.executor.clone(),
            cancel: self.cancel.clone(),
            workflow: self.workflow.clone(),
        })
    }
    /// Bind a request/workflow cancellation scope without mutating other client clones.
    /// A cancelled scope stays cancelled; use a new scope for a deliberate retry.
    pub fn with_cancellation(&self, cancel: Arc<AtomicBool>) -> Self {
        Self {
            cwd: self.cwd.clone(),
            binding: self.binding.clone(),
            executor: self.executor.clone(),
            cancel,
            workflow: self.workflow.clone(),
        }
    }
    /// Synchronous admission BEFORE spawning; retain the token in the delivery.
    pub fn begin_workflow(&self, cancel: Arc<AtomicBool>) -> io::Result<(Self, WorkflowScope)> {
        let workflow = self.executor.begin_workflow(cancel.clone())?;
        let mut client = self.with_cancellation(cancel);
        client.workflow = workflow.clone();
        Ok((
            client,
            WorkflowScope {
                _workflow: workflow,
            },
        ))
    }
    /// Cleanup reads use a fresh cancellation flag while retaining original admission.
    /// Mutations still honor workflow cancellation, including host shutdown.
    pub fn reconciliation(&self) -> Self {
        self.with_cancellation(Arc::new(AtomicBool::new(false)))
    }
    /// A retained session must not retain a completed open workflow indefinitely.
    pub(crate) fn without_workflow(&self) -> Self {
        let mut client = self.reconciliation();
        client.workflow = None;
        client
    }
    fn run(&self, args: &[&OsStr], input: &[u8], read: bool) -> io::Result<Vec<u8>> {
        self.run_codes(args, input, read, &[0])
    }
    fn run_codes(
        &self,
        args: &[&OsStr],
        input: &[u8],
        read: bool,
        codes: &[i32],
    ) -> io::Result<Vec<u8>> {
        if !read
            && self
                .binding
                .as_ref()
                .is_some_and(|identity| identity.worktree.is_none())
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "bare repository mutations are unavailable",
            ));
        }
        if self.cancel.load(Ordering::Acquire)
            || (!read
                && self
                    .workflow
                    .as_ref()
                    .is_some_and(process::Workflow::cancelled))
        {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                GitFailure {
                    code: None,
                    cancelled: true,
                    truncated: false,
                    stderr: vec![],
                },
            ));
        }
        let mut command = Command::new("git");
        // Path bytes are literal, including Git's pathspec-magic prefix characters.
        // Never override hooks, filters, signing, identity or credential configuration.
        command.current_dir(&self.cwd).arg("--literal-pathspecs");
        // Alternate inherited indices are intentionally unsupported: use the bound
        // git_dir's ordinary index, which is also the mutation-gate resource. Strip
        // repository/object selection only; preserve hooks, filters and signing.
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CEILING_DIRECTORIES",
            "GIT_DISCOVERY_ACROSS_FILESYSTEM",
            "GIT_PREFIX",
            "GIT_IMPLICIT_WORK_TREE",
            "GIT_NAMESPACE",
            "GIT_SHALLOW_FILE",
            "GIT_GRAFT_FILE",
            "GIT_REPLACE_REF_BASE",
            "GIT_QUARANTINE_PATH",
        ] {
            command.env_remove(key);
        }
        if let Some(identity) = &self.binding {
            command.arg("--git-dir").arg(&identity.git_dir);
            if let Some(worktree) = &identity.worktree {
                command.arg("--work-tree").arg(worktree);
            }
            command.env("GIT_COMMON_DIR", &identity.common_dir);
        }
        command.args(args).env("GIT_TERMINAL_PROMPT", "0");
        if read {
            command.env("GIT_OPTIONAL_LOCKS", "0");
        }
        let output = if let Some(workflow) = &self.workflow {
            self.executor
                .execute_scoped(command, input.to_vec(), self.cancel.clone(), workflow)?
        } else {
            self.executor
                .execute(command, input.to_vec(), self.cancel.clone())?
        };
        if output.cancelled
            || output.truncated
            || !output.code.is_some_and(|code| codes.contains(&code))
        {
            let kind = if output.cancelled {
                io::ErrorKind::Interrupted
            } else {
                io::ErrorKind::Other
            };
            return Err(io::Error::new(
                kind,
                GitFailure {
                    code: output.code,
                    cancelled: output.cancelled,
                    truncated: output.truncated,
                    stderr: output.stderr,
                },
            ));
        }
        Ok(output.stdout)
    }
    fn path(&self, flag: &str) -> io::Result<PathBuf> {
        let mut bytes = self.run(
            &[
                OsStr::new("rev-parse"),
                OsStr::new("--path-format=absolute"),
                OsStr::new(flag),
            ],
            &[],
            true,
        )?;
        if bytes.pop() != Some(b'\n') || bytes.is_empty() {
            return Err(io::Error::other("invalid discovery output"));
        }
        PathBuf::from(OsString::from_vec(bytes)).canonicalize()
    }
    pub fn discover(&self) -> io::Result<Identity> {
        let bare = match self
            .run(
                &[OsStr::new("rev-parse"), OsStr::new("--is-bare-repository")],
                &[],
                true,
            )?
            .as_slice()
        {
            b"true\n" => true,
            b"false\n" => false,
            _ => return Err(io::Error::other("invalid repository type")),
        };
        Ok(Identity {
            worktree: if bare {
                None
            } else {
                Some(self.path("--show-toplevel")?)
            },
            git_dir: self.path("--absolute-git-dir")?,
            common_dir: self.path("--git-common-dir")?,
        })
    }
    pub fn status(&self) -> io::Result<Vec<Entry>> {
        decode_status(&self.run(
            &[
                OsStr::new("status"),
                OsStr::new("--porcelain=v1"),
                OsStr::new("-z"),
                OsStr::new("--untracked-files=all"),
            ],
            &[],
            true,
        )?)
    }
    /// Default canonical applicable patch. Whitespace display settings never affect it.
    pub fn diff(&self, path: &Path, side: Side) -> io::Result<Vec<u8>> {
        self.canonical_diff(path, side, 3)
    }
    pub fn canonical_diff(&self, path: &Path, side: Side, context: u32) -> io::Result<Vec<u8>> {
        self.diff_options(
            path,
            side,
            DiffOptions {
                context,
                whitespace: Whitespace::Exact,
            },
        )
    }
    /// Presentation only. Never use whitespace-filtered bytes as an apply target.
    pub fn display_diff(
        &self,
        path: &Path,
        side: Side,
        options: DiffOptions,
    ) -> io::Result<Vec<u8>> {
        self.diff_options(path, side, options)
    }
    fn diff_options(&self, path: &Path, side: Side, options: DiffOptions) -> io::Result<Vec<u8>> {
        validate_path(path)?;
        let context = OsString::from(format!("--unified={}", options.context));
        let mut args = vec![
            OsStr::new("diff"),
            OsStr::new("--no-ext-diff"),
            OsStr::new("--no-textconv"),
            OsStr::new("--no-color"),
            OsStr::new("--no-renames"),
            OsStr::new("--default-prefix"),
            OsStr::new("--output-indicator-new=+"),
            OsStr::new("--output-indicator-old=-"),
            OsStr::new("--output-indicator-context= "),
            context.as_os_str(),
        ];
        if side == Side::Index {
            args.push(OsStr::new("--cached"));
        }
        match options.whitespace {
            Whitespace::Exact => {}
            Whitespace::IgnoreSpaceChange => args.push(OsStr::new("--ignore-space-change")),
            Whitespace::IgnoreAllSpace => args.push(OsStr::new("--ignore-all-space")),
            Whitespace::IgnoreAtEol => args.push(OsStr::new("--ignore-space-at-eol")),
        }
        args.extend([OsStr::new("--"), path.as_os_str()]);
        self.run(&args, &[], true)
    }
    pub fn verify_untracked(&self, path: &Path) -> io::Result<()> {
        validate_path(path)?;
        if self.status()?.iter().any(|entry| {
            entry.path == path.as_os_str() && entry.index == b'?' && entry.worktree == b'?'
        }) {
            Ok(())
        } else {
            Err(io::Error::other(
                "target is no longer an untracked file; reconcile before dispatch",
            ))
        }
    }
    /// Verifies exact untracked identity first. Exit 1 is differences, not failure.
    /// External Git/filesystem changes can still race; this is not a transaction.
    pub fn untracked_diff(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.verify_untracked(path)?;
        self.untracked_diff_verified(path)
    }
    /// The owner already verified this entry in its enclosing status snapshot;
    /// avoid a full status traversal per untracked path. Final status is rechecked.
    pub(crate) fn untracked_diff_verified(&self, path: &Path) -> io::Result<Vec<u8>> {
        validate_path(path)?;
        self.run_codes(
            &[
                OsStr::new("diff"),
                OsStr::new("--no-index"),
                OsStr::new("--no-ext-diff"),
                OsStr::new("--no-textconv"),
                OsStr::new("--no-color"),
                OsStr::new("--default-prefix"),
                OsStr::new("--output-indicator-new=+"),
                OsStr::new("--output-indicator-old=-"),
                OsStr::new("--output-indicator-context= "),
                OsStr::new("--"),
                OsStr::new("/dev/null"),
                path.as_os_str(),
            ],
            &[],
            true,
            &[0, 1],
        )
    }
    /// Low-level mutation: caller retains Worktree lease through reconciliation.
    pub fn apply_index(&self, patch: &[u8]) -> io::Result<()> {
        self.run(
            &[
                OsStr::new("apply"),
                OsStr::new("--cached"),
                OsStr::new("--whitespace=nowarn"),
                OsStr::new("-"),
            ],
            patch,
            false,
        )
        .map(|_| ())
    }
    pub fn stage_file(&self, path: &Path) -> io::Result<()> {
        self.stage_paths(&[path.to_owned()])
    }
    /// Both rename paths can be dispatched together, never as separate commands.
    pub fn stage_paths(&self, paths: &[PathBuf]) -> io::Result<()> {
        validate_paths(paths)?;
        let mut args = vec![OsStr::new("add"), OsStr::new("-A"), OsStr::new("--")];
        args.extend(paths.iter().map(|path| path.as_os_str()));
        self.run(&args, &[], false).map(|_| ())
    }
    /// A staged rename's old path can already be absent from the index. Do not send
    /// a nonexistent pathspec to add; include the old path if its deletion is pending.
    pub fn stage_entry(&self, entry: &Entry) -> io::Result<()> {
        let mut paths = vec![PathBuf::from(&entry.path)];
        if let Some(original) = &entry.original {
            let path = Path::new(original);
            validate_path(path)?;
            let indexed = self.run(
                &[
                    OsStr::new("ls-files"),
                    OsStr::new("-z"),
                    OsStr::new("--"),
                    path.as_os_str(),
                ],
                &[],
                true,
            )?;
            if !indexed.is_empty() {
                paths.push(path.to_owned());
            }
        }
        self.stage_paths(&paths)
    }
    pub fn unstage_file(&self, path: &Path) -> io::Result<()> {
        self.unstage_paths(&[path.to_owned()])
    }
    pub fn unstage_entry(&self, entry: &Entry) -> io::Result<()> {
        let mut paths = vec![PathBuf::from(&entry.path)];
        if let Some(original) = &entry.original {
            paths.push(PathBuf::from(original));
        }
        self.unstage_paths(&paths)
    }
    /// Unborn index removal is forced only in the index; worktree bytes are preserved
    /// even when they differ from the previously staged content.
    pub fn unstage_paths(&self, paths: &[PathBuf]) -> io::Result<()> {
        validate_paths(paths)?;
        let mut args = match self.head()? {
            Head::Unborn { .. } => vec![
                OsStr::new("rm"),
                OsStr::new("--cached"),
                OsStr::new("-f"),
                OsStr::new("--ignore-unmatch"),
                OsStr::new("--"),
            ],
            _ => vec![
                OsStr::new("reset"),
                OsStr::new("-q"),
                OsStr::new("HEAD"),
                OsStr::new("--"),
            ],
        };
        args.extend(paths.iter().map(|path| path.as_os_str()));
        self.run(&args, &[], false).map(|_| ())
    }
    /// Ordinary commit, including configured hooks/signing. Message is stdin, never
    /// argv or a shell string. Caller retains All lease; no automatic retry on error.
    pub fn commit(&self, message: &[u8]) -> io::Result<()> {
        self.commit_with_options(message, CommitOptions::default())
    }
    /// Options are direct argv flags, not command/config fragments. Message stays stdin.
    pub fn commit_with_options(&self, message: &[u8], options: CommitOptions) -> io::Result<()> {
        if message.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "commit message contains NUL",
            ));
        }
        let mut args = vec![
            OsStr::new("commit"),
            OsStr::new("--file=-"),
            OsStr::new("--cleanup=verbatim"),
        ];
        if options.sign_off {
            args.push(OsStr::new("--signoff"));
        }
        if options.no_verify {
            args.push(OsStr::new("--no-verify"));
        }
        self.run(&args, message, false).map(|_| ())
    }
}
fn validate_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty()
        || path.as_os_str().as_bytes().contains(&0)
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a nonempty repository-relative literal path",
        ));
    }
    Ok(())
}
fn validate_paths(paths: &[PathBuf]) -> io::Result<()> {
    if paths.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "empty file target list",
        ));
    }
    for path in paths {
        validate_path(path)?;
    }
    Ok(())
}
fn decode_status(bytes: &[u8]) -> io::Result<Vec<Entry>> {
    if !bytes.is_empty() && !bytes.ends_with(&[0]) {
        return Err(io::Error::other("unterminated status"));
    }
    let mut fields = bytes.split(|b| *b == 0);
    let mut result = Vec::new();
    while let Some(field) = fields.next() {
        if field.is_empty() {
            if fields.any(|tail| !tail.is_empty()) {
                return Err(io::Error::other("invalid status terminator"));
            }
            break;
        }
        if field.len() < 4 || field[2] != b' ' {
            return Err(io::Error::other("invalid status"));
        }
        let original = if matches!(field[0], b'R' | b'C') || matches!(field[1], b'R' | b'C') {
            Some(OsString::from_vec(
                fields
                    .next()
                    .filter(|x| !x.is_empty())
                    .ok_or_else(|| io::Error::other("missing rename source"))?
                    .to_vec(),
            ))
        } else {
            None
        };
        result.push(Entry {
            index: field[0],
            worktree: field[1],
            path: OsStr::from_bytes(&field[3..]).to_owned(),
            original,
        });
    }
    Ok(result)
}
#[cfg(test)]
#[path = "tests/git.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/workflows.rs"]
mod workflow_tests;
