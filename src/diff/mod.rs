//! Canonical single-file byte patches. Presentation never becomes an operation target.
use std::{collections::BTreeSet, io, ops::Range};

/// IDs are ordinals scoped to an exact patch snapshot, not persistent repository identities.
pub type ChangeId = usize;
pub type HunkId = usize;
pub type LineId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Addition,
    Deletion,
}

/// A body line for display mapping. Bytes include the diff prefix and final LF.
/// Line numbers are one-based; IDs exclude file/hunk headers and do not depend on wrapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalLine<'a> {
    pub id: LineId,
    pub hunk_id: HunkId,
    pub change_id: Option<ChangeId>,
    pub kind: LineKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub bytes: &'a [u8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HunkInfo<'a> {
    pub id: HunkId,
    pub changes: Range<ChangeId>,
    pub lines: Range<LineId>,
    /// Zero-based source/destination ranges; empty ranges represent insertion boundaries.
    pub old: Range<usize>,
    pub new: Range<usize>,
    pub header: &'a [u8],
    pub has_context: bool,
    /// Context-free regular-file hunks are disabled; new/deleted-file hunks remain safe.
    pub supports_partial_selection: bool,
}

#[derive(Clone, Debug)]
struct Hunk {
    old: Range<usize>,
    new: Range<usize>,
    header: Vec<u8>,
    lines: Vec<Vec<u8>>,
    changes: Range<ChangeId>,
    line_ids: Range<LineId>,
}

#[derive(Clone, Debug)]
pub struct Patch {
    diff_header: Vec<u8>,
    old_file: Vec<u8>,
    new_file: Vec<u8>,
    file_mode: Option<Vec<u8>>,
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

fn regular_mode(mode: &[u8]) -> io::Result<()> {
    if matches!(mode, b"100644" | b"100755") {
        Ok(())
    } else {
        Err(invalid())
    }
}

fn range(token: &str, sign: char) -> io::Result<Range<usize>> {
    let text = token.strip_prefix(sign).ok_or_else(invalid)?;
    let (start, count) = text.split_once(',').unwrap_or((text, "1"));
    let start: usize = start.parse().map_err(|_| invalid())?;
    let count: usize = count.parse().map_err(|_| invalid())?;
    let anchor = start
        .checked_sub(usize::from(count != 0))
        .ok_or_else(invalid)?;
    let end = anchor.checked_add(count).ok_or_else(invalid)?;
    // Selection uses a signed delta to relocate later hunks.
    if end >= i64::MAX as usize {
        return Err(invalid());
    }
    Ok(anchor..end)
}

impl Patch {
    pub fn parse(raw: &[u8]) -> io::Result<Self> {
        if !raw.starts_with(b"diff --git ") || raw.contains(&0) || !raw.ends_with(b"\n") {
            return Err(invalid());
        }
        let mut diff_header = None;
        let mut old_file = None;
        let mut new_file = None;
        let mut new_mode = None;
        let mut deleted_mode = None;
        let mut index_seen = false;
        let mut hunks: Vec<Hunk> = Vec::new();
        for line in raw.split_inclusive(|b| *b == b'\n') {
            if line.starts_with(b"\\ No newline at end of file") {
                return Err(io::Error::other(
                    "missing final newline is unsupported for partial patches; use whole-file operation",
                ));
            }
            if line.starts_with(b"@@ ") {
                // Function names are arbitrary bytes; only range tokens are ASCII.
                let end = line
                    .windows(3)
                    .position(|w| w == b" @@")
                    .ok_or_else(invalid)?;
                let text = std::str::from_utf8(&line[3..end]).map_err(|_| invalid())?;
                let mut tokens = text.split_whitespace();
                let old = range(tokens.next().ok_or_else(invalid)?, '-')?;
                let new = range(tokens.next().ok_or_else(invalid)?, '+')?;
                if tokens.next().is_some() || old_file.is_none() || new_file.is_none() {
                    return Err(invalid());
                }
                hunks.push(Hunk {
                    old,
                    new,
                    header: line.to_vec(),
                    lines: Vec::new(),
                    changes: 0..0,
                    line_ids: 0..0,
                });
            } else if let Some(hunk) = hunks.last_mut() {
                if !matches!(line.first(), Some(b' ' | b'+' | b'-')) {
                    return Err(invalid());
                }
                hunk.lines.push(line.to_vec());
            } else if line.starts_with(b"diff --git ") && diff_header.is_none() {
                diff_header = Some(line.to_vec());
            } else if let Some(path) = line.strip_prefix(b"--- ") {
                if old_file.replace(path.to_vec()).is_some() || path == b"\n" {
                    return Err(invalid());
                }
            } else if let Some(path) = line.strip_prefix(b"+++ ") {
                if new_file.replace(path.to_vec()).is_some() || path == b"\n" {
                    return Err(invalid());
                }
            } else if let Some(mode) = line.strip_prefix(b"new file mode ") {
                regular_mode(&mode[..mode.len() - 1])?;
                if new_mode.replace(mode.to_vec()).is_some() {
                    return Err(invalid());
                }
            } else if let Some(mode) = line.strip_prefix(b"deleted file mode ") {
                regular_mode(&mode[..mode.len() - 1])?;
                if deleted_mode.replace(mode.to_vec()).is_some() {
                    return Err(invalid());
                }
            } else if let Some(index) = line.strip_prefix(b"index ") {
                if index_seen {
                    return Err(invalid());
                }
                index_seen = true;
                let mut fields = index[..index.len() - 1].split(|b| *b == b' ');
                let hashes = fields.next().ok_or_else(invalid)?;
                let split = hashes
                    .windows(2)
                    .position(|w| w == b"..")
                    .ok_or_else(invalid)?;
                if split == 0
                    || split + 2 == hashes.len()
                    || !hashes[..split]
                        .iter()
                        .chain(&hashes[split + 2..])
                        .all(u8::is_ascii_hexdigit)
                {
                    return Err(invalid());
                }
                if let Some(mode) = fields.next() {
                    regular_mode(mode)?;
                }
                if fields.next().is_some() {
                    return Err(invalid());
                }
            } else {
                // Binary, mode changes, rename/copy, combined diff, and unknown metadata.
                return Err(invalid());
            }
        }
        let old_file = old_file.ok_or_else(invalid)?;
        let new_file = new_file.ok_or_else(invalid)?;
        let added = old_file == b"/dev/null\n";
        let deleted = new_file == b"/dev/null\n";
        if hunks.is_empty()
            || (added && deleted)
            || added != new_mode.is_some()
            || deleted != deleted_mode.is_some()
        {
            return Err(invalid());
        }
        let mut changes = 0;
        let mut lines = 0;
        let mut old_end = 0;
        let mut new_end = 0;
        for hunk in &mut hunks {
            let old_count = hunk.lines.iter().filter(|l| l[0] != b'+').count();
            let new_count = hunk.lines.iter().filter(|l| l[0] != b'-').count();
            let change_count = hunk.lines.iter().filter(|l| l[0] != b' ').count();
            if old_count != hunk.old.len()
                || new_count != hunk.new.len()
                || change_count == 0
                || hunk.old.start < old_end
                || hunk.new.start < new_end
                || hunk.old.start - old_end != hunk.new.start - new_end
                || (added && old_count != 0)
                || (deleted && new_count != 0)
            {
                return Err(invalid());
            }
            old_end = hunk.old.end;
            new_end = hunk.new.end;
            hunk.changes = changes..changes + change_count;
            hunk.line_ids = lines..lines + hunk.lines.len();
            changes += change_count;
            lines += hunk.lines.len();
        }
        if (added || deleted)
            && (hunks.len() != 1 || hunks[0].old.start != 0 || hunks[0].new.start != 0)
        {
            return Err(invalid());
        }
        Ok(Self {
            diff_header: diff_header.ok_or_else(invalid)?,
            old_file,
            new_file,
            file_mode: new_mode.or(deleted_mode),
            hunks,
            raw: raw.to_vec(),
        })
    }

    pub fn changes(&self) -> usize {
        self.hunks.last().map_or(0, |h| h.changes.end)
    }

    /// Display mapping and hunk targets share these canonical, snapshot-scoped ranges.
    pub fn hunks(&self) -> impl Iterator<Item = HunkInfo<'_>> {
        self.hunks.iter().enumerate().map(|(id, h)| {
            let has_context = h.lines.iter().any(|l| l[0] == b' ');
            HunkInfo {
                id,
                changes: h.changes.clone(),
                lines: h.line_ids.clone(),
                old: h.old.clone(),
                new: h.new.clone(),
                header: &h.header,
                has_context,
                supports_partial_selection: has_context || self.file_mode.is_some(),
            }
        })
    }

    pub fn lines(&self) -> impl Iterator<Item = CanonicalLine<'_>> {
        self.hunks.iter().enumerate().flat_map(|(hunk_id, h)| {
            let mut old_line = h.old.start + 1;
            let mut new_line = h.new.start + 1;
            let mut change_id = h.changes.start;
            h.lines.iter().enumerate().map(move |(offset, bytes)| {
                let kind = match bytes[0] {
                    b'+' => LineKind::Addition,
                    b'-' => LineKind::Deletion,
                    _ => LineKind::Context,
                };
                let change = (kind != LineKind::Context).then_some(change_id);
                change_id += usize::from(change.is_some());
                let old = (kind != LineKind::Addition).then_some(old_line);
                let new = (kind != LineKind::Deletion).then_some(new_line);
                old_line += usize::from(old.is_some());
                new_line += usize::from(new.is_some());
                CanonicalLine {
                    id: h.line_ids.start + offset,
                    hunk_id,
                    change_id: change,
                    kind,
                    old_line: old,
                    new_line: new,
                    bytes,
                }
            })
        })
    }

    /// Ordinals are canonical change IDs scoped to exact raw snapshot, not screen rows.
    /// Reverse by swapping source/destination before selection; don't reverse a forward subset.
    /// Ordinary zero-context hunks are blocked; callers must request whole-file operations.
    pub fn select(&self, selected: &BTreeSet<ChangeId>, reverse: bool) -> io::Result<Selection> {
        if selected.is_empty() || selected.iter().any(|x| *x >= self.changes()) {
            return Err(invalid());
        }
        let special = self.file_mode.is_some();
        if special && selected.len() == self.changes() {
            return Ok(Selection::WholeFile);
        }
        if self.hunks().any(|h| {
            !h.supports_partial_selection && h.changes.clone().any(|id| selected.contains(&id))
        }) {
            return Err(io::Error::other(
                "zero-context line selection is unsafe; use whole-file operation or refresh with context",
            ));
        }
        let (source, destination) = if reverse {
            (&self.new_file, &self.old_file)
        } else {
            (&self.old_file, &self.new_file)
        };
        let creates = source == b"/dev/null\n";
        // A partial deletion keeps the file; it must not carry deletion metadata or /dev/null.
        let destination = if destination == b"/dev/null\n" {
            source
        } else {
            destination
        };
        let mut output = self.diff_header.clone();
        if creates {
            output.extend_from_slice(b"new file mode ");
            output.extend_from_slice(self.file_mode.as_ref().ok_or_else(invalid)?);
        }
        // Blob IDs describe the full patch, never a selected subset. Rebuild marker order.
        output.extend_from_slice(b"--- ");
        output.extend_from_slice(source);
        output.extend_from_slice(b"+++ ");
        output.extend_from_slice(destination);
        let mut delta = 0i64;
        for hunk in &self.hunks {
            let mut body = Vec::new();
            let mut old_count = 0i64;
            let mut new_count = 0i64;
            let mut changed = false;
            let mut ordinal = hunk.changes.start;
            for line in &hunk.lines {
                let mut kind = line[0];
                let change = matches!(kind, b'+' | b'-');
                let chosen = change && selected.contains(&ordinal);
                ordinal += usize::from(change);
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
            let anchor = if reverse {
                hunk.new.start
            } else {
                hunk.old.start
            } as i64;
            let new_anchor = anchor.checked_add(delta).ok_or_else(invalid)?;
            if new_anchor < 0 {
                return Err(invalid());
            }
            output.extend_from_slice(
                format!(
                    "@@ -{},{old_count} +{},{new_count} @@\n",
                    anchor + i64::from(old_count != 0),
                    new_anchor + i64::from(new_count != 0),
                )
                .as_bytes(),
            );
            output.extend(body);
            delta += new_count - old_count;
        }
        Ok(Selection::Patch(output))
    }

    /// Reconcile snapshot-scoped IDs after a *known* index selection. The unchanged
    /// side supplies source coordinates; the edited side is transformed exactly as
    /// `select` transforms bytes. Compare all surviving changes, never ordinals or
    /// byte-only searches (identical lines at different locations are distinct).
    /// `None` means no mutation, e.g. a context-only refresh.
    pub fn remap_changes(
        &self,
        fresh: Option<&Patch>,
        mutation: Option<(&BTreeSet<ChangeId>, bool)>,
    ) -> io::Result<Vec<Option<ChangeId>>> {
        if let Some((selected, reverse)) = mutation {
            // Keep unsupported/zero-context selections on the same safe path.
            self.select(selected, reverse)?;
        }
        if let Some(fresh) = fresh {
            let becomes_regular = mutation.is_some_and(|(_, reverse)| {
                (!reverse && self.old_file == b"/dev/null\n")
                    || (reverse && self.new_file == b"/dev/null\n")
            });
            let metadata_matches = self.diff_header == fresh.diff_header
                && if becomes_regular {
                    fresh.file_mode.is_none()
                        && fresh.old_file != b"/dev/null\n"
                        && fresh.new_file != b"/dev/null\n"
                        && if self.old_file == b"/dev/null\n" {
                            self.new_file == fresh.new_file
                        } else {
                            self.old_file == fresh.old_file
                        }
                } else {
                    self.old_file == fresh.old_file
                        && self.new_file == fresh.new_file
                        && self.file_mode == fresh.file_mode
                };
            if !metadata_matches {
                return Err(io::Error::other(
                    "externally changed patch identity/mode; refresh and reconcile",
                ));
            }
        }
        let mut expected = Vec::new();
        let mut context = Vec::new();
        let mut delta = 0i64;
        for hunk in &self.hunks {
            let mut old = hunk.old.start as i64 + 1;
            let mut new = hunk.new.start as i64 + 1;
            if let Some((_, reverse)) = mutation {
                if reverse {
                    new += delta;
                } else {
                    old += delta;
                }
            }
            let mut id = hunk.changes.start;
            for bytes in &hunk.lines {
                let kind = bytes[0];
                let change = kind != b' ';
                let chosen = mutation.is_some_and(|(selected, _)| change && selected.contains(&id));
                let transformed = if chosen {
                    let reverse = mutation.unwrap().1;
                    match (kind, reverse) {
                        (b'+', false) | (b'-', true) => Some(b' '),
                        _ => None,
                    }
                } else {
                    Some(kind)
                };
                if let Some(kind) = transformed {
                    let key = (
                        kind,
                        (kind != b'+').then_some(old as usize),
                        (kind != b'-').then_some(new as usize),
                        bytes[1..].to_vec(),
                    );
                    if kind == b' ' {
                        context.push(key);
                    } else {
                        expected.push((id, key));
                    }
                    old += i64::from(kind != b'+');
                    new += i64::from(kind != b'-');
                }
                if chosen {
                    delta += if (kind == b'+') != mutation.unwrap().1 {
                        1
                    } else {
                        -1
                    };
                }
                id += usize::from(change);
            }
        }
        let lines: Vec<_> = fresh.into_iter().flat_map(Patch::lines).collect();
        let changes: Vec<_> = lines.iter().filter(|l| l.change_id.is_some()).collect();
        if changes.len() != expected.len() {
            return Err(io::Error::other(
                "ambiguous or externally changed patch; refresh and reconcile",
            ));
        }
        let mut map = vec![None; self.changes()];
        for ((id, key), line) in expected.iter().zip(changes) {
            if *key
                != (
                    line.bytes[0],
                    line.old_line,
                    line.new_line,
                    line.bytes[1..].to_vec(),
                )
            {
                return Err(io::Error::other(
                    "ambiguous or externally changed patch; refresh and reconcile",
                ));
            }
            map[*id] = line.change_id;
        }
        // Git may split/merge hunks and extend context. Known context must still
        // agree wherever fresh context overlaps its transformed source location.
        for line in lines.iter().filter(|l| l.kind == LineKind::Context) {
            if context.iter().any(|(_, old, new, bytes)| {
                (*old == line.old_line || *new == line.new_line)
                    && (*old != line.old_line || *new != line.new_line || bytes != &line.bytes[1..])
            }) {
                return Err(io::Error::other(
                    "externally changed patch context; refresh and reconcile",
                ));
            }
        }
        Ok(map)
    }

    pub fn matches_snapshot(&self, current: &[u8]) -> bool {
        self.raw == current
    }
}

#[cfg(test)]
#[path = "tests/selection.rs"]
mod selection_tests;
#[cfg(test)]
#[path = "tests/patch.rs"]
mod tests;
