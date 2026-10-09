//! Conservative Go-regexp-compatible commit prefix rules, compiled transactionally.
//! LazyGit v0.66.0 (c5f7158154602d23b0750d4304c73e2aead8df5b):
//! pkg/gui/controllers/helpers/working_tree_helper.go, HandleCommitPress:
//! repo rules then global rules; skip empty patterns; first match wins, even an
//! empty replacement; ReplaceAllString preserves unmatched branch text.
//! Go syntax/expansion: https://pkg.go.dev/regexp and https://pkg.go.dev/regexp/syntax
use super::err;
use regex::{Captures, Regex};
use serde::Deserialize;
use serde_yaml::Value;
use std::{collections::BTreeMap, io};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommitPrefixes {
    global: Vec<Rule>,
    repositories: BTreeMap<String, Vec<Rule>>,
}
#[derive(Clone, Debug)]
struct Rule {
    pattern: String,
    replace: String,
    compiled: Option<Regex>,
}
impl PartialEq for Rule {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern && self.replace == other.replace
    }
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct SuppliedRule {
    pattern: String,
    replace: String,
}
impl CommitPrefixes {
    pub(super) fn from_value(value: &Value) -> io::Result<Self> {
        fn rules(value: &Value, path: &str) -> io::Result<Vec<Rule>> {
            let supplied: Vec<SuppliedRule> = if value.is_null() {
                vec![]
            } else {
                serde_yaml::from_value(value.clone()).map_err(|e| err(format!("{path}: {e}")))?
            };
            supplied.into_iter().enumerate().map(|(i, rule)| {
                let compiled = if rule.pattern.is_empty() { None } else {
                    let translated = translate(&rule.pattern)
                        .map_err(|e| err(format!("{path}[{i}].pattern: {e}")))?;
                    Some(Regex::new(&translated).map_err(|e| err(format!(
                        "{path}[{i}].pattern: invalid or unsupported Go regular expression: {e}"
                    )))?)
                };
                // ASCII capture names are supported. Do not partially consume Unicode
                // Go variable names using Rust's broader/different Unicode tables.
                for tail in rule.replace.split('$').skip(1) {
                    let name = tail.strip_prefix('{').unwrap_or(tail);
                    if name.chars().take_while(|c| c.is_alphanumeric() || *c == '_')
                        .any(|c| !c.is_ascii()) {
                        return Err(err(format!("{path}[{i}].replace: Unicode replacement variable names are unsupported")));
                    }
                }
                Ok(Rule { pattern: rule.pattern, replace: rule.replace, compiled })
            }).collect()
        }
        let global = rules(&value["git"]["commitPrefix"], "git.commitPrefix")?;
        let mut repositories = BTreeMap::new();
        let value = &value["git"]["commitPrefixes"];
        if !value.is_null() {
            let map = value
                .as_mapping()
                .ok_or_else(|| err("git.commitPrefixes: expected repository map"))?;
            for (name, value) in map {
                let name = name
                    .as_str()
                    .ok_or_else(|| err("git.commitPrefixes: repository key must be a string"))?;
                repositories.insert(
                    name.into(),
                    rules(value, &format!("git.commitPrefixes.{name}"))?,
                );
            }
        }
        Ok(Self {
            global,
            repositories,
        })
    }
    pub fn is_empty(&self) -> bool {
        self.global
            .iter()
            .chain(self.repositories.values().flatten())
            .all(|r| r.compiled.is_none())
    }
    pub fn has_repository_rules(&self) -> bool {
        !self.repositories.is_empty()
    }
    pub fn prefill(&self, repository: &str, branch: &str) -> Option<String> {
        self.repositories
            .get(repository)
            .into_iter()
            .flatten()
            .chain(&self.global)
            .find_map(|rule| {
                let re = rule.compiled.as_ref()?;
                if !re.is_match(branch) {
                    return None;
                }
                let mut output = String::new();
                let mut end = 0;
                for captures in re.captures_iter(branch) {
                    let matched = captures.get(0).expect("whole match");
                    output.push_str(&branch[end..matched.start()]);
                    expand(&mut output, &rule.replace, &captures);
                    end = matched.end();
                }
                output.push_str(&branch[end..]);
                Some(output)
            })
    }
}
/// Explicitly normalize only proven differences. Reject Go-only constructs and
/// Rust-only flags/classes rather than silently changing their meaning. Unicode
/// properties are declined because engine Unicode versions/property sets differ.
fn translate(pattern: &str) -> io::Result<String> {
    validate_repetitions(pattern)?;
    let unsupported = || err("unsupported Go regular-expression syntax; rule not applied");
    let mut output = String::new();
    let mut chars = pattern.chars().peekable();
    let mut class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let escaped = chars.next().ok_or_else(unsupported)?;
                let replacement = match escaped {
                    'd' => Some("[0-9]"),
                    'D' => Some("[^0-9]"),
                    'w' => Some("[0-9A-Za-z_]"),
                    'W' => Some("[^0-9A-Za-z_]"),
                    's' => Some(r"[\t\n\f\r ]"),
                    'S' => Some(r"[^\t\n\f\r ]"),
                    'b' | 'B' if !class => {
                        output.push_str(&format!("(?-u:\\{escaped})"));
                        continue;
                    }
                    'x' if chars.peek() == Some(&'{') => {
                        output.push_str(r"\x");
                        loop {
                            let c = chars.next().ok_or_else(unsupported)?;
                            output.push(c);
                            if c == '}' {
                                break;
                            }
                        }
                        continue;
                    }
                    'A' | 'z' | 'a' | 'f' | 't' | 'n' | 'r' | 'v' | 'x' => None,
                    c if c.is_ascii_punctuation() => None,
                    _ => return Err(unsupported()),
                };
                if let Some(replacement) = replacement {
                    output.push_str(replacement);
                } else {
                    output.push('\\');
                    output.push(escaped);
                }
            }
            '[' if class => {
                if chars.next() != Some(':') {
                    return Err(unsupported());
                }
                output.push_str("[:");
                loop {
                    let c = chars.next().ok_or_else(unsupported)?;
                    output.push(c);
                    if c == ':' && chars.peek() == Some(&']') {
                        output.push(chars.next().unwrap());
                        break;
                    }
                }
            }
            '[' => {
                class = true;
                output.push(c);
            }
            ']' => {
                class = false;
                output.push(c);
            }
            '&' | '-' | '~' if class && chars.peek() == Some(&c) => return Err(unsupported()),
            '(' if !class && chars.peek() == Some(&'?') => {
                output.push_str("(?");
                chars.next();
                if chars.peek() == Some(&'P') {
                    output.push(chars.next().unwrap());
                    if chars.peek() != Some(&'<') {
                        return Err(unsupported());
                    }
                }
                if chars.peek() == Some(&'<') {
                    output.push(chars.next().unwrap());
                    loop {
                        let c = chars.next().ok_or_else(unsupported)?;
                        if c == '>' {
                            output.push(c);
                            break;
                        }
                        if !c.is_ascii_alphanumeric() && c != '_' {
                            return Err(unsupported());
                        }
                        output.push(c);
                    }
                } else {
                    loop {
                        let c = chars.next().ok_or_else(unsupported)?;
                        if c == 'i' {
                            return Err(err(
                                "Go Unicode case-folding flag i is unsupported: engine Unicode tables may differ; rule not applied",
                            ));
                        }
                        if !matches!(c, 'm' | 's' | 'U' | '-' | ':' | ')') {
                            return Err(unsupported());
                        }
                        output.push(c);
                        if matches!(c, ':' | ')') {
                            break;
                        }
                    }
                }
            }
            '{' if !class => {
                let tail: String = chars.clone().collect();
                if let Some((counted, _)) = tail.split_once('}')
                    && !counted.is_empty()
                    && counted.bytes().all(|c| c.is_ascii_digit() || c == b',')
                {
                    for number in counted.split(',').filter(|n| !n.is_empty()) {
                        if number.starts_with('0') && number.len() > 1 {
                            return Err(unsupported());
                        }
                        if number.parse::<usize>().map_or(true, |n| n > 1000) {
                            return Err(err("Go counted repetitions above 1000 are invalid"));
                        }
                    }
                }
                output.push(c);
            }
            _ => output.push(c),
        }
    }
    Ok(output)
}
/// Rust accepts stacked repetitions that Go rejects. Go also limits the product
/// of nested counted repetitions; conservatively decline nested counts instead
/// of accepting a Go-invalid expression through the Rust parser.
fn validate_repetitions(pattern: &str) -> io::Result<()> {
    let unsupported = || err("invalid or unsupported Go repetition syntax; rule not applied");
    let mut chars = pattern.chars().peekable();
    let mut groups = Vec::<bool>::new();
    let mut counted_atom = false;
    let mut repeat = 0;
    let mut class = false;
    while let Some(c) = chars.next() {
        if c == '\\' {
            if chars.next() == Some('x') && chars.peek() == Some(&'{') {
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                }
            }
            repeat = 0;
            counted_atom = false;
            continue;
        }
        if class {
            if c == '[' && chars.peek() == Some(&':') {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == ':' && chars.peek() == Some(&']') {
                        chars.next();
                        break;
                    }
                }
            } else if c == ']' {
                class = false;
            }
            continue;
        }
        match c {
            '[' => {
                class = true;
                repeat = 0;
                counted_atom = false;
            }
            '(' => {
                repeat = 0;
                counted_atom = false;
                if chars.peek() == Some(&'?') {
                    chars.next();
                    let mut delimiter = None;
                    for c in chars.by_ref() {
                        if matches!(c, ':' | '>' | ')') {
                            delimiter = Some(c);
                            break;
                        }
                    }
                    if delimiter == Some(')') {
                        continue;
                    }
                }
                groups.push(false);
            }
            ')' => {
                counted_atom = groups.pop().unwrap_or(false);
                if counted_atom && let Some(parent) = groups.last_mut() {
                    *parent = true;
                }
                repeat = 0;
            }
            '*' | '+' | '?' => {
                if repeat > 0 {
                    if c != '?' || repeat != 1 {
                        return Err(unsupported());
                    }
                    repeat = 2;
                } else {
                    repeat = 1;
                }
            }
            '{' => {
                let tail: String = chars.clone().collect();
                let Some((count, _)) = tail.split_once('}') else {
                    return Err(unsupported());
                };
                // Decline malformed/leading-zero forms: Go treats some as literal
                // text while Rust may parse them as a quantifier.
                if count.is_empty()
                    || !count.bytes().all(|c| c.is_ascii_digit() || c == b',')
                    || count.starts_with(',')
                    || count.matches(',').count() > 1
                {
                    return Err(unsupported());
                }
                if repeat > 0 || counted_atom {
                    return Err(unsupported());
                }
                if let Some(group) = groups.last_mut() {
                    *group = true;
                }
                counted_atom = true;
                repeat = 1;
                for _ in 0..=count.len() {
                    chars.next();
                }
            }
            _ => {
                repeat = 0;
                counted_atom = false;
            }
        }
    }
    Ok(())
}
/// Go Expand: maximal names, $$, ${1}, missing groups -> empty; leading-zero
/// numbers are names, not group indices. Malformed $ forms remain literal.
fn expand(output: &mut String, template: &str, captures: &Captures<'_>) {
    let mut tail = template;
    while let Some((before, after)) = tail.split_once('$') {
        output.push_str(before);
        tail = after;
        if let Some(rest) = tail.strip_prefix('$') {
            output.push('$');
            tail = rest;
            continue;
        }
        let braced = tail.starts_with('{');
        let name_tail = if braced { &tail[1..] } else { tail };
        let length = name_tail
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_')
            .count();
        if length == 0 || (braced && name_tail.as_bytes().get(length) != Some(&b'}')) {
            output.push('$');
            continue;
        }
        let name = &name_tail[..length];
        tail = &name_tail[length + usize::from(braced)..];
        let numeric =
            name.bytes().all(|c| c.is_ascii_digit()) && !(name.starts_with('0') && name.len() > 1);
        let matched = if numeric {
            name.parse::<usize>()
                .ok()
                .filter(|n| *n < 1_000_000_000)
                .and_then(|n| captures.get(n))
        } else {
            captures.name(name)
        };
        if let Some(matched) = matched {
            output.push_str(matched.as_str());
        }
    }
    output.push_str(tail);
}
