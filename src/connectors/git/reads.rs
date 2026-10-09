use super::*;

impl Client {
    pub fn version(&self) -> io::Result<GitVersion> {
        decode_version(&self.run(&[OsStr::new("--version")], &[], true)?)
    }
    /// Reject unsupported Git before loading repository panels or allowing writes.
    /// Bare identity is valid for inspection, but Worktree/All gates reject it.
    pub fn readiness(&self) -> io::Result<Readiness> {
        let version = self.version()?;
        if version < GitVersion::MINIMUM {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "Git {}.{}.{} is unsupported; Git 2.56.0 or newer is required",
                    version.major, version.minor, version.patch
                ),
            ));
        }
        Ok(Readiness {
            version,
            identity: self.discover()?,
            head: self.head()?,
        })
    }
    pub fn head(&self) -> io::Result<Head> {
        let symbolic = self.run_codes(
            &[
                OsStr::new("symbolic-ref"),
                OsStr::new("--quiet"),
                OsStr::new("HEAD"),
            ],
            &[],
            true,
            &[0, 1],
        )?;
        let oid = self.run_codes(
            &[
                OsStr::new("rev-parse"),
                OsStr::new("--verify"),
                OsStr::new("--quiet"),
                OsStr::new("HEAD^{commit}"),
            ],
            &[],
            true,
            &[0, 1],
        )?;
        let branch = if symbolic.is_empty() {
            None
        } else {
            let name = line(&symbolic)?;
            let name = name.strip_prefix(b"refs/heads/").unwrap_or(name);
            if name.is_empty() {
                return Err(io::Error::other("empty symbolic HEAD"));
            }
            Some(OsString::from_vec(name.to_vec()))
        };
        match (branch, oid.is_empty()) {
            (Some(branch), true) => {
                // A missing commit object behind an existing branch is corruption,
                // not permission to choose the unborn index-removal workflow.
                match self.run(
                    &[
                        OsStr::new("show-ref"),
                        OsStr::new("--exists"),
                        OsStr::from_bytes(line(&symbolic)?),
                    ],
                    &[],
                    true,
                ) {
                    Err(error)
                        if error
                            .get_ref()
                            .and_then(|e| e.downcast_ref::<GitFailure>())
                            .is_some_and(|failure| {
                                failure.code == Some(2) && !failure.cancelled && !failure.truncated
                            }) =>
                    {
                        Ok(Head::Unborn { branch })
                    }
                    Err(error) => Err(error),
                    Ok(_) => Err(io::Error::other(
                        "HEAD branch exists but does not resolve to a commit",
                    )),
                }
            }
            (Some(branch), false) => Ok(Head::Branch {
                branch,
                oid: decode_oid(line(&oid)?)?,
            }),
            (None, false) => Ok(Head::Detached {
                oid: decode_oid(line(&oid)?)?,
            }),
            (None, true) => Err(io::Error::other(
                "HEAD is neither a commit nor an unborn branch",
            )),
        }
    }
    /// Minimal read-only history for commit feedback. No graph/remote/ref mutation.
    /// Limit bounds command work as well as the process output cap.
    pub fn history(&self, limit: usize) -> io::Result<Vec<Commit>> {
        if limit > 1000 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "history limit exceeds 1000",
            ));
        }
        if limit == 0 || matches!(self.head()?, Head::Unborn { .. }) {
            return Ok(vec![]);
        }
        let limit = OsString::from(format!("--max-count={limit}"));
        let mut commits = decode_history(&self.run(
            &[
                OsStr::new("log"),
                OsStr::new("--no-color"),
                OsStr::new("--no-decorate"),
                OsStr::new("--no-show-signature"),
                OsStr::new("-z"),
                OsStr::new("--format=%H%x00%P%x00%an%x00%at%x00%s"),
                limit.as_os_str(),
                OsStr::new("HEAD"),
                OsStr::new("--"),
            ],
            &[],
            true,
        )?)?;
        if !commits.is_empty() {
            // Length-framed raw objects preserve arbitrary message bytes (even NUL),
            // unlike %B separated by sentinel bytes or subject-only pretty output.
            let input: String = commits
                .iter()
                .map(|commit| format!("{}\n", commit.oid))
                .collect();
            let objects = self.run(
                &[OsStr::new("cat-file"), OsStr::new("--batch")],
                input.as_bytes(),
                true,
            )?;
            decode_messages(&objects, &mut commits)?;
        }
        Ok(commits)
    }
}
fn decode_version(bytes: &[u8]) -> io::Result<GitVersion> {
    let version = bytes
        .strip_prefix(b"git version ")
        .ok_or_else(|| io::Error::other("invalid Git version"))?;
    let text = std::str::from_utf8(version).map_err(|_| io::Error::other("invalid Git version"))?;
    let mut parts = text.trim().split('.');
    let major = parts.next().and_then(|part| part.parse().ok());
    let minor = parts.next().and_then(|part| part.parse().ok());
    let patch = parts.next().and_then(|part| {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    });
    match (major, minor, patch) {
        (Some(major), Some(minor), Some(patch)) => Ok(GitVersion {
            major,
            minor,
            patch,
        }),
        _ => Err(io::Error::other("invalid Git version")),
    }
}
fn line(bytes: &[u8]) -> io::Result<&[u8]> {
    let value = bytes
        .strip_suffix(b"\n")
        .ok_or_else(|| io::Error::other("unterminated Git output"))?;
    if value.contains(&b'\n') {
        return Err(io::Error::other("invalid Git output line"));
    }
    Ok(value)
}
fn decode_oid(bytes: &[u8]) -> io::Result<String> {
    if !matches!(bytes.len(), 40 | 64) || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err(io::Error::other("invalid Git object id"));
    }
    Ok(String::from_utf8(bytes.to_vec()).unwrap())
}
fn decode_history(bytes: &[u8]) -> io::Result<Vec<Commit>> {
    if bytes.is_empty() {
        return Ok(vec![]);
    }
    if !bytes.ends_with(&[0]) {
        return Err(io::Error::other("unterminated history"));
    }
    let fields: Vec<_> = bytes[..bytes.len() - 1].split(|b| *b == 0).collect();
    if !fields.len().is_multiple_of(5) {
        return Err(io::Error::other("invalid history fields"));
    }
    fields
        .chunks_exact(5)
        .map(|row| {
            let parents = row[1]
                .split(|b| *b == b' ')
                .filter(|part| !part.is_empty())
                .map(decode_oid)
                .collect::<io::Result<Vec<_>>>()?;
            let timestamp = std::str::from_utf8(row[3])
                .ok()
                .and_then(|text| text.parse().ok())
                .ok_or_else(|| io::Error::other("invalid history timestamp"))?;
            Ok(Commit {
                oid: decode_oid(row[0])?,
                parents,
                author: row[2].to_vec(),
                timestamp,
                subject: row[4].to_vec(),
                message: vec![],
            })
        })
        .collect()
}
/// Git cat-file --batch: <oid> SP commit SP <byte size> LF <object bytes> LF.
fn decode_messages(mut bytes: &[u8], commits: &mut [Commit]) -> io::Result<()> {
    for commit in commits {
        let end = bytes
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(|| io::Error::other("unterminated commit object header"))?;
        let header = std::str::from_utf8(&bytes[..end])
            .map_err(|_| io::Error::other("invalid commit object header"))?;
        let fields: Vec<_> = header.split(' ').collect();
        if fields.len() != 3 || fields[0] != commit.oid || fields[1] != "commit" {
            return Err(io::Error::other("unexpected commit object"));
        }
        let size: usize = fields[2]
            .parse()
            .map_err(|_| io::Error::other("invalid commit object size"))?;
        bytes = &bytes[end + 1..];
        let object = bytes
            .get(..size)
            .ok_or_else(|| io::Error::other("truncated commit object"))?;
        let tail = bytes
            .get(size..)
            .and_then(|tail| tail.strip_prefix(b"\n"))
            .ok_or_else(|| io::Error::other("unterminated commit object"))?;
        let message = object
            .windows(2)
            .position(|part| part == b"\n\n")
            .ok_or_else(|| io::Error::other("commit message separator missing"))?
            + 2;
        commit.message = object[message..].to_vec();
        bytes = tail;
    }
    if !bytes.is_empty() {
        return Err(io::Error::other("unexpected trailing commit objects"));
    }
    Ok(())
}
#[cfg(test)]
#[path = "tests/reads.rs"]
mod tests;
