use std::{ffi::OsString, fmt, path::PathBuf};

/// Immutable repository resources. Clients use the ordinary index beneath
/// `git_dir`; inherited GIT_INDEX_FILE is removed, not adopted. Alternate-index
/// workflows are intentionally unsupported until explicitly bound and gated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub worktree: Option<PathBuf>,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub index: u8,
    pub worktree: u8,
    pub path: OsString,
    pub original: Option<OsString>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Worktree,
    Index,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}
impl GitVersion {
    /// Conservative tested floor, not a claim that every used flag requires it.
    pub const MINIMUM: Self = Self {
        major: 2,
        minor: 56,
        patch: 0,
    };
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Unborn { branch: OsString },
    Branch { branch: OsString, oid: String },
    Detached { oid: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    pub version: GitVersion,
    pub identity: Identity,
    pub head: Head,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub oid: String,
    pub parents: Vec<String>,
    pub author: Vec<u8>,
    pub timestamp: i64,
    pub subject: Vec<u8>,
    /// Exact commit-object message bytes, including hard breaks and trailing newlines.
    /// Unlike pretty-format output, this is not reencoded or delimiter framed.
    pub message: Vec<u8>,
}
/// Explicit Git flags; defaults preserve ordinary configured hooks and signing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommitOptions {
    /// Git appends the committer's own Signed-off-by trailer; never synthesize identity.
    pub sign_off: bool,
    /// Git --no-verify skips pre-commit/commit-msg, not every possible hook.
    pub no_verify: bool,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Whitespace {
    #[default]
    Exact,
    IgnoreSpaceChange,
    IgnoreAllSpace,
    IgnoreAtEol,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffOptions {
    pub context: u32,
    pub whitespace: Whitespace,
}
impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            context: 3,
            whitespace: Whitespace::Exact,
        }
    }
}
/// Available by downcasting `io::Error::get_ref()`. Output is bounded and deliberately
/// excluded from Display/Debug: owners may present stderr under their redaction policy.
/// Any mutation error can have side effects and requires authoritative reconciliation.
pub struct GitFailure {
    pub code: Option<i32>,
    pub cancelled: bool,
    pub truncated: bool,
    pub stderr: Vec<u8>,
}
impl fmt::Display for GitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.cancelled {
            write!(f, "Git cancelled")?;
        } else {
            write!(f, "Git exited with status {:?}", self.code)?;
        }
        if self.truncated {
            write!(f, " (output truncated)")?;
        }
        write!(f, "; mutation outcome may require reconciliation")
    }
}
impl fmt::Debug for GitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for GitFailure {}
