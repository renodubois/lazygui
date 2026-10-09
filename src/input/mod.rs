//! Pure context resolution shared by controls/help. M0 action subset, not a full keymap.
use std::io;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub name: String,
}
impl Key {
    pub fn parse(raw: &str) -> io::Result<Option<Self>> {
        if raw.is_empty() || raw == "<disabled>" {
            return Ok(None);
        }
        let text = raw
            .strip_prefix('<')
            .and_then(|x| x.strip_suffix('>'))
            .unwrap_or(raw);
        let mut key = Self {
            control: false,
            alt: false,
            shift: false,
            name: String::new(),
        };
        let mut rest = text;
        while let Some((prefix, tail)) = rest.split_once(['-', '+']) {
            match prefix.to_ascii_lowercase().as_str() {
                "c" | "ctrl" => key.control = true,
                "a" | "alt" => key.alt = true,
                "s" | "shift" => key.shift = true,
                _ => break,
            }
            rest = tail;
        }
        key.name = match rest.to_ascii_lowercase().as_str() {
            "esc" => "escape".into(),
            "return" => "enter".into(),
            "space" => "space".into(),
            "pgup" => "pageup".into(),
            "pgdown" => "pagedown".into(),
            "backtab" => {
                key.shift = true;
                "tab".into()
            }
            "enter" | "escape" | "tab" | "up" | "down" | "left" | "right" | "home" | "end"
            | "pageup" | "pagedown" | "backspace" | "delete" => rest.to_ascii_lowercase(),
            _ if rest.chars().count() == 1 => {
                if rest.chars().next().unwrap().is_ascii_uppercase() {
                    key.shift = true;
                    rest.to_ascii_lowercase()
                } else {
                    rest.into()
                }
            }
            _ => return Err(io::Error::other("unsupported LazyGit key spelling")),
        };
        Ok(Some(key))
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Context {
    Global,
    Files,
    Subject,
    Body,
    Menu,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Stage,
    Submit,
    SwitchField,
    Menu,
    Escape,
    CloseWindow,
    Unsupported(&'static str),
    Custom,
}
#[derive(Clone)]
pub struct Binding {
    pub context: Context,
    pub key: Key,
    pub action: Action,
    pub custom: bool,
}
pub fn resolve(bindings: &[Binding], context: Context, key: &Key) -> Option<Action> {
    let specific = bindings
        .iter()
        .filter(|b| b.context == context && &b.key == key);
    if let Some(binding) = specific
        .clone()
        .find(|b| b.custom)
        .or_else(|| specific.into_iter().next())
    {
        return Some(binding.action);
    }
    // Text/menu contexts are exclusive. Editing shortcuts are not global commands.
    if matches!(context, Context::Subject | Context::Body | Context::Menu) {
        return None;
    }
    let global = bindings
        .iter()
        .filter(|b| b.context == Context::Global && &b.key == key);
    global
        .clone()
        .find(|b| b.custom)
        .or_else(|| global.into_iter().next())
        .map(|b| b.action)
}
pub fn defaults() -> Vec<Binding> {
    use {Action::*, Context::*};
    [
        (Files, "<space>", Stage),
        (Subject, "<enter>", Submit),
        (Subject, "<tab>", SwitchField),
        (Body, "<tab>", SwitchField),
        (Subject, "<c-s>", Submit),
        (Body, "<c-s>", Submit),
        (Subject, "<c-enter>", Submit),
        (Body, "<c-enter>", Submit),
        (Subject, "<c-o>", Action::Menu),
        (Body, "<c-o>", Action::Menu),
        (Subject, "<esc>", Escape),
        (Body, "<esc>", Escape),
        (Context::Menu, "<esc>", Escape),
        (Context::Menu, "<enter>", Escape),
        (Global, "q", CloseWindow),
        (Global, "Q", CloseWindow),
        (Global, "<c-c>", CloseWindow),
        (
            Global,
            "<c-z>",
            Unsupported("Suspend is unsupported; no terminal fallback."),
        ),
    ]
    .into_iter()
    .map(|(context, key, action)| Binding {
        context,
        key: Key::parse(key).unwrap().unwrap(),
        action,
        custom: false,
    })
    .collect()
}
pub fn policy_setting(path: &str, value: &str) -> Option<&'static str> {
    match path {
        "gui.language" if !matches!(value, "en" | "auto") => Some("Only English UI is supported."),
        "os.editPreset" if matches!(value, "vim" | "nvim" | "nano" | "emacs") => {
            Some("Terminal editors are unsupported.")
        }
        "customCommands.output" if matches!(value, "terminal" | "logWithPty") => {
            Some("Terminal/PTY output is unsupported.")
        }
        "git.paging" | "git.pagers" | "git.diffRenderers" => {
            Some("Custom renderers are unsupported; native unified diff only.")
        }
        "update" | "disableStartupPopups" => {
            Some("Update checks and decorative startup content are not implemented.")
        }
        _ => None,
    }
}
pub fn policy_action(name: &str) -> Option<&'static str> {
    match name {
        "editConfig" => Some("Shared LazyGit configuration is read-only."),
        "suspend" => Some("Suspend is unsupported."),
        "cycleDiffRenderer" => Some("Custom diff renderers are unsupported."),
        "update" => Some("Update checks/downloads are unsupported."),
        "snake" | "explode" => Some("Decorative extras are unsupported."),
        _ => None,
    }
}
#[cfg(test)]
#[path = "tests/resolution.rs"]
mod tests;
