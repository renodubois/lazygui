use crate::lazygit_config::MessageSettings;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub subject: String,
    /// Unwrapped text, including explicit hard line breaks; never persist display wraps.
    pub body: String,
}
impl Draft {
    pub fn from_message(message: &str) -> Self {
        let (subject, body) = message.split_once('\n').unwrap_or((message, ""));
        Self {
            subject: subject.into(),
            body: body.strip_prefix('\n').unwrap_or(body).into(),
        }
    }
    /// Editable inputs are UTF-8: decline non-UTF-8 records rather than silently
    /// replacing repository bytes. NUL-containing messages cannot be submitted,
    /// so decline those too. The read-only history retains all exact bytes.
    pub fn from_record(record: &crate::git::Commit) -> Option<Self> {
        let message = std::str::from_utf8(&record.message).ok()?;
        if message.contains('\0') {
            return None;
        }
        Some(Self::from_message(message))
    }
    pub fn message(&self, settings: &MessageSettings) -> String {
        let body = if settings.auto_wrap_commit_message {
            wrap(&self.body, settings.auto_wrap_width)
        } else {
            self.body.clone()
        };
        if body.is_empty() {
            self.subject.clone()
        } else {
            format!("{}\n\n{body}", self.subject)
        }
    }
}
/// LazyGit v0.66.0 contentToCells: grapheme display-cell widths, only ASCII space
/// breakpoints, retained whitespace/hard breaks, and no splitting of long words.
/// Matchers reset at both hard and soft breaks. Footnotes protect spaces after
/// `[digits]:` only (not the entire line); trailers protect the final paragraph.
/// https://github.com/jesseduffield/lazygit/blob/v0.66.0/pkg/gocui/text_area.go
fn wrap(body: &str, width: usize) -> String {
    if width == 0 {
        return body.into();
    }
    let end = body.trim_end_matches('\n').len();
    let trailer_start = body[..end].rfind("\n\n").map(|i| i + 2).unwrap_or(0);
    let mut output = String::new();
    let mut start = 0;
    let mut columns = 0;
    let mut space = None;
    let mut footnote = Footnote::default();
    let mut trailer = Trailer::default();
    for (offset, grapheme) in body.grapheme_indices(true) {
        let next = offset + grapheme.len();
        if grapheme == "\n" || grapheme == "\r\n" {
            output.push_str(&body[start..next]);
            start = next;
            space = None;
            columns = 0;
            footnote = Footnote::default();
            trailer = Trailer::default();
            continue;
        }
        columns += cell_width(grapheme);
        let in_trailers = offset >= trailer_start;
        if grapheme == " "
            && !footnote.matches()
            && !(in_trailers && trailer.matches(&body[next..]))
        {
            space = Some(next);
        } else if columns > width
            && let Some(at) = space
        {
            output.push_str(&body[start..at]);
            output.push('\n');
            start = at;
            space = None;
            columns = body[at..next].graphemes(true).map(cell_width).sum();
            footnote = Footnote::default();
            trailer = Trailer::default();
        }
        footnote.add(grapheme);
        if in_trailers {
            trailer.add(grapheme);
        }
    }
    output.push_str(&body[start..]);
    output
}
// uniseg treats control graphemes (notably tabs) as zero display cells.
// unicode-width's string method assigns a fallback width to controls instead.
fn cell_width(grapheme: &str) -> usize {
    if grapheme.chars().all(char::is_control) {
        0
    } else {
        grapheme.width()
    }
}
#[derive(Default)]
struct Footnote {
    prefix: String,
    failed: bool,
}
impl Footnote {
    fn add(&mut self, grapheme: &str) {
        if self.failed {
            return;
        }
        if self.prefix.is_empty() && grapheme != "[" {
            self.failed = true;
        } else {
            self.prefix.push_str(grapheme);
        }
    }
    fn matches(&mut self) -> bool {
        if self.failed {
            return false;
        }
        // Go regexp ^\[\d+\]:\s*$ uses ASCII digits/whitespace.
        let prefix = self
            .prefix
            .trim_end_matches([' ', '\t', '\r', '\n', '\u{c}']);
        let matched = prefix
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix("]:"))
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
        self.failed = !matched;
        matched
    }
}
#[derive(Default)]
struct Trailer {
    failed: bool,
    matched: bool,
    dash: bool,
    colon: bool,
}
impl Trailer {
    fn add(&mut self, grapheme: &str) {
        if self.failed || self.matched {
            return;
        }
        if grapheme.len() != 1 {
            self.failed = true;
            return;
        }
        self.dash |= grapheme == "-";
        self.colon = grapheme == ":";
    }
    fn matches(&mut self, remaining: &str) -> bool {
        if self.failed {
            return false;
        }
        if self.matched {
            return true;
        }
        let remaining = remaining.trim_start_matches([' ', '\t']);
        self.matched = self.colon
            && (self.dash || remaining.starts_with("http://") || remaining.starts_with("https://"));
        self.failed = !self.matched;
        self.matched
    }
}
#[cfg(test)]
#[path = "tests/message.rs"]
mod tests;
