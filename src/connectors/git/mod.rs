//! Typed, byte-preserving Git capability spike. No shell interpolation.
pub mod process;
use process::{Executor, Native};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::AtomicBool},
};

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
pub struct Client {
    cwd: PathBuf,
    executor: Arc<dyn Executor>,
}
impl Client {
    pub fn new(path: PathBuf) -> Self {
        Self::with_executor(path, Arc::new(Native))
    }
    pub fn with_executor(cwd: PathBuf, executor: Arc<dyn Executor>) -> Self {
        Self { cwd, executor }
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
        let mut command = Command::new("git");
        command.current_dir(&self.cwd).args(args);
        if read {
            command.env("GIT_OPTIONAL_LOCKS", "0");
        }
        let output =
            self.executor
                .execute(command, input.to_vec(), Arc::new(AtomicBool::new(false)))?;
        if output.cancelled
            || output.truncated
            || !output.code.is_some_and(|code| codes.contains(&code))
        {
            return Err(io::Error::other(
                "Git failed; mutation outcome may require reconciliation",
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
        if bytes.pop() != Some(b'\n') {
            return Err(io::Error::other("invalid discovery output"));
        }
        Ok(PathBuf::from(OsString::from_vec(bytes)))
    }
    pub fn discover(&self) -> io::Result<Identity> {
        let bare = self.run(
            &[OsStr::new("rev-parse"), OsStr::new("--is-bare-repository")],
            &[],
            true,
        )? == b"true\n";
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
    pub fn diff(&self, path: &Path, side: Side) -> io::Result<Vec<u8>> {
        let mut args = vec![
            OsStr::new("diff"),
            OsStr::new("--no-ext-diff"),
            OsStr::new("--no-textconv"),
            OsStr::new("--no-color"),
            OsStr::new("--no-renames"),
            OsStr::new("--unified=3"),
        ];
        if side == Side::Index {
            args.push(OsStr::new("--cached"));
        }
        args.extend([OsStr::new("--"), path.as_os_str()]);
        self.run(&args, &[], true)
    }
    /// --no-index returns 1 for differences; that is not a process failure.
    pub fn untracked_diff(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.run_codes(
            &[
                OsStr::new("diff"),
                OsStr::new("--no-index"),
                OsStr::new("--no-ext-diff"),
                OsStr::new("--no-textconv"),
                OsStr::new("--no-color"),
                OsStr::new("--"),
                OsStr::new("/dev/null"),
                path.as_os_str(),
            ],
            &[],
            true,
            &[0, 1],
        )
    }
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
        self.run(
            &[OsStr::new("add"), OsStr::new("--"), path.as_os_str()],
            &[],
            false,
        )
        .map(|_| ())
    }
    pub fn unstage_file(&self, path: &Path) -> io::Result<()> {
        // Unborn HEAD handled without resetting the worktree.
        // Only explicit empty/missing HEAD evidence is unborn. Transport/process failure
        // must not silently choose an index-removal operation.
        let refs = self.run_codes(
            &[OsStr::new("show-ref"), OsStr::new("--head")],
            &[],
            true,
            &[0, 1],
        )?;
        let head = refs
            .split(|b| *b == b'\n')
            .any(|line| line.ends_with(b" HEAD"));
        if head {
            self.run(
                &[
                    OsStr::new("reset"),
                    OsStr::new("-q"),
                    OsStr::new("HEAD"),
                    OsStr::new("--"),
                    path.as_os_str(),
                ],
                &[],
                false,
            )?;
        } else {
            self.run(
                &[
                    OsStr::new("rm"),
                    OsStr::new("--cached"),
                    OsStr::new("--"),
                    path.as_os_str(),
                ],
                &[],
                false,
            )?;
        }
        Ok(())
    }
}
fn decode_status(bytes: &[u8]) -> io::Result<Vec<Entry>> {
    let mut fields = bytes.split(|b| *b == 0);
    let mut result = Vec::new();
    while let Some(field) = fields.next() {
        if field.is_empty() {
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
