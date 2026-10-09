//! Canonical single-file text patches, never rendered text. Conservative fallback for metadata.
use std::{collections::BTreeSet, io};
#[derive(Clone, Debug)]
struct Hunk {
    old: i64,
    new: i64,
    lines: Vec<Vec<u8>>,
}
#[derive(Clone, Debug)]
pub struct Patch {
    header: Vec<u8>,
    hunks: Vec<Hunk>,
    raw: Vec<u8>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Selection {
    Patch(Vec<u8>),
    WholeFile,
}
fn invalid() -> io::Error {
    io::Error::other("unsupported canonical patch; use whole-file operation")
}
fn start(token: &str) -> io::Result<i64> {
    token
        .get(1..)
        .ok_or_else(invalid)?
        .split(',')
        .next()
        .unwrap()
        .parse()
        .map_err(|_| invalid())
}
impl Patch {
    pub fn parse(raw: &[u8]) -> io::Result<Self> {
        if raw.is_empty()
            || raw.windows(12).any(|x| x == b"Binary files")
            || raw
                .windows(9)
                .any(|x| x == b"old mode " || x == b"new mode ")
            || raw.windows(6).any(|x| x == b"120000" || x == b"160000")
            || raw.windows(12).any(|x| x == b"rename from ")
            || raw.windows(2).any(|x| x == b"\n\\")
        {
            return Err(invalid());
        }
        let mut header = Vec::new();
        let mut hunks: Vec<Hunk> = Vec::new();
        let mut files = 0;
        for line in raw.split_inclusive(|b| *b == b'\n') {
            if line.starts_with(b"diff --git ") {
                files += 1;
            }
            if line.starts_with(b"@@ ") {
                let text = std::str::from_utf8(line).map_err(|_| invalid())?;
                let mut parts = text.split_whitespace();
                parts.next();
                let old = start(parts.next().ok_or_else(invalid)?)?;
                let new = start(parts.next().ok_or_else(invalid)?)?;
                hunks.push(Hunk {
                    old,
                    new,
                    lines: vec![],
                });
            } else if let Some(hunk) = hunks.last_mut() {
                if !matches!(line.first(), Some(b' ' | b'+' | b'-')) {
                    return Err(invalid());
                }
                hunk.lines.push(line.to_vec());
            } else {
                header.extend_from_slice(line);
            }
        }
        if files != 1 || hunks.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            header,
            hunks,
            raw: raw.to_vec(),
        })
    }
    pub fn changes(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| matches!(l[0], b'+' | b'-'))
            .count()
    }
    /// Ordinals are canonical change IDs scoped to exact raw snapshot, not screen rows.
    /// Reverse by swapping source/destination before selection; don't reverse a forward subset.
    pub fn select(&self, selected: &BTreeSet<usize>, reverse: bool) -> io::Result<Selection> {
        let count = self.changes();
        if selected.is_empty() || selected.iter().any(|x| *x >= count) {
            return Err(invalid());
        }
        let special = self.header.windows(9).any(|x| x == b"/dev/null");
        if selected.len() == count && special {
            return Ok(Selection::WholeFile);
        }
        if special {
            return Err(invalid());
        }
        let header: Vec<u8> = self
            .header
            .split_inclusive(|b| *b == b'\n')
            .filter(|l| !l.starts_with(b"index "))
            .flat_map(|l| {
                if reverse && l.starts_with(b"--- ") {
                    [b"+++ ".as_slice(), &l[4..]].concat()
                } else if reverse && l.starts_with(b"+++ ") {
                    [b"--- ".as_slice(), &l[4..]].concat()
                } else {
                    l.to_vec()
                }
            })
            .collect();
        // Swap header order for reverse application.
        let mut output = if reverse {
            let lines: Vec<_> = header.split_inclusive(|b| *b == b'\n').collect();
            let mut out = Vec::new();
            out.extend_from_slice(lines[0]);
            out.extend_from_slice(lines[2]);
            out.extend_from_slice(lines[1]);
            out
        } else {
            header
        };
        let mut ordinal = 0;
        let mut delta = 0i64;
        for hunk in &self.hunks {
            let mut body = Vec::new();
            let mut old_count = 0i64;
            let mut new_count = 0i64;
            let mut changed = false;
            for line in &hunk.lines {
                let mut kind = line[0];
                let change = matches!(kind, b'+' | b'-');
                let chosen = change && selected.contains(&ordinal);
                if change {
                    ordinal += 1;
                }
                if reverse {
                    kind = match kind {
                        b'+' => b'-',
                        b'-' => b'+',
                        x => x,
                    };
                }
                if change && !chosen {
                    if kind == b'+' {
                        continue;
                    }
                    kind = b' ';
                }
                changed |= chosen;
                old_count += i64::from(kind != b'+');
                new_count += i64::from(kind != b'-');
                body.push(kind);
                body.extend_from_slice(&line[1..]);
            }
            if !changed {
                continue;
            }
            let old_start = if reverse { hunk.new } else { hunk.old };
            if old_start == 0 {
                return Err(invalid());
            }
            output.extend_from_slice(
                format!(
                    "@@ -{old_start},{old_count} +{},{new_count} @@\n",
                    old_start + delta
                )
                .as_bytes(),
            );
            output.extend(body);
            delta += new_count - old_count;
        }
        Ok(Selection::Patch(output))
    }
    pub fn matches_snapshot(&self, current: &[u8]) -> bool {
        self.raw == current
    }
}
#[cfg(test)]
#[path = "tests/patch.rs"]
mod tests;
