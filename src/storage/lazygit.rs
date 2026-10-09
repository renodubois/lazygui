//! Read-only supplied-field merge/trust feasibility. Caller supplies discovered sources in order.
//! Complete path discovery/schema/migration remains M1–M5 work, not silently claimed here.
use crate::input::Key;
use serde_yaml::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io, path::PathBuf};
use std::{
    fs,
    io::{Read, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::Path,
};
#[derive(Clone)]
pub struct Source {
    pub path: PathBuf,
    pub explicitly_chosen_global: bool,
    pub yaml: String,
}
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub source: PathBuf,
    pub path: String,
    pub reason: &'static str,
}
#[derive(Clone)]
pub struct Settings {
    value: Value,
    pub origins: BTreeMap<String, PathBuf>,
    executable: BTreeMap<PathBuf, Value>,
    sources: BTreeMap<PathBuf, bool>,
    pub diagnostics: Vec<Diagnostic>,
}
fn err(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
fn merge(
    base: &mut Value,
    supplied: Value,
    path: &str,
    source: &PathBuf,
    origins: &mut BTreeMap<String, PathBuf>,
) {
    match (base, supplied) {
        (Value::Mapping(base), Value::Mapping(supplied)) => {
            for (key, value) in supplied {
                let name = key.as_str().unwrap_or("?");
                let child = if path.is_empty() {
                    name.into()
                } else {
                    format!("{path}.{name}")
                };
                merge(
                    base.entry(key).or_insert(Value::Null),
                    value,
                    &child,
                    source,
                    origins,
                );
            }
        }
        (Value::Sequence(base), Value::Sequence(mut supplied))
            if path == "customCommands" || path == "gui.branchColorPatterns" =>
        {
            supplied.append(base);
            *base = supplied;
            origins.insert(path.into(), source.clone());
        }
        (base, supplied) => {
            *base = supplied;
            origins.insert(path.into(), source.clone());
        }
    }
}
fn diagnose(value: &Value, path: &str, source: &PathBuf, output: &mut Vec<Diagnostic>) {
    match value {
        Value::Mapping(map) => {
            for (key, value) in map {
                let child = if path.is_empty() {
                    key.as_str().unwrap_or("?").into()
                } else {
                    format!("{path}.{}", key.as_str().unwrap_or("?"))
                };
                diagnose(value, &child, source, output);
            }
        }
        Value::Sequence(values) if path == "customCommands" => {
            for value in values {
                diagnose(value, path, source, output);
            }
        }
        _ => {
            let reason = crate::input::policy_setting(path, value.as_str().unwrap_or(""))
                .unwrap_or(if path.starts_with("customCommands.") {
                    "Custom-command/template execution is unavailable until M5; no interpolation fallback."
                } else if path.starts_with("keybinding.") {
                    "Binding parsed; repository-context wiring begins in M1."
                } else {
                    "Setting retained for feasibility only; not applied to the catalog starter."
                });
            output.push(Diagnostic {
                source: source.clone(),
                path: path.into(),
                reason,
            });
        }
    }
}
fn bindings(value: &Value) -> io::Result<Vec<String>> {
    let values = match value {
        Value::String(s) => vec![s.clone()],
        Value::Sequence(s) => s
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| err("binding must be string"))
            })
            .collect::<io::Result<Vec<_>>>()?,
        Value::Null => vec![],
        _ => return Err(err("binding must be scalar/list")),
    };
    values
        .into_iter()
        .filter(|s| !s.is_empty() && s != "<disabled>")
        .map(|s| {
            Key::parse(&s)?;
            Ok(s)
        })
        .collect()
}
fn validate_keys(value: &Value) -> io::Result<()> {
    if let Value::Mapping(map) = value {
        for context in map.values() {
            let context = context
                .as_mapping()
                .ok_or_else(|| err("keybinding context must be mapping"))?;
            for value in context.values() {
                bindings(value)?;
            }
        }
    }
    Ok(())
}
impl Settings {
    pub fn load(defaults: &str, sources: &[Source]) -> io::Result<Self> {
        let mut settings = Self {
            value: serde_yaml::from_str(defaults).map_err(|e| err(e.to_string()))?,
            origins: BTreeMap::new(),
            executable: BTreeMap::new(),
            sources: BTreeMap::new(),
            diagnostics: Vec::new(),
        };
        for source in sources {
            let value: Value = serde_yaml::from_str(&source.yaml)
                .map_err(|e| err(format!("{}: {e}", source.path.display())))?;
            if !value.is_mapping() {
                return Err(err(format!("{}: expected mapping", source.path.display())));
            }
            settings
                .sources
                .insert(source.path.clone(), source.explicitly_chosen_global);
            if !source.explicitly_chosen_global {
                let mut execution = serde_yaml::Mapping::new();
                // Conservative: custom templates/prompts/suggestions and OS tool definitions.
                for name in ["customCommands", "os"] {
                    if let Some(value) = value.get(name) {
                        execution.insert(Value::String(name.into()), value.clone());
                    }
                }
                if !execution.is_empty() {
                    settings
                        .executable
                        .insert(source.path.clone(), Value::Mapping(execution));
                }
            }
            diagnose(&value, "", &source.path, &mut settings.diagnostics);
            merge(
                &mut settings.value,
                value,
                "",
                &source.path,
                &mut settings.origins,
            );
        }
        validate_keys(&settings.value["keybinding"])?;
        Ok(settings)
    }
    /// Replace only after the entire candidate parses/validates.
    pub fn reload(&mut self, defaults: &str, sources: &[Source]) -> io::Result<()> {
        *self = Self::load(defaults, sources)?;
        Ok(())
    }
    pub fn get(&self, path: &str) -> &Value {
        path.split('.').fold(&self.value, |value, key| &value[key])
    }
    pub fn universal_binding(&self, name: &str) -> io::Result<Vec<String>> {
        let mut result = bindings(&self.value["keybinding"]["universal"][name])?;
        // Source-confirmed legacy alternate pairs, including numbered forms.
        for suffix in ["-alt", "-alt1", "-alt2"] {
            let alt = format!("{name}{suffix}");
            for key in bindings(&self.value["keybinding"]["universal"][alt.as_str()])? {
                if !result.contains(&key) {
                    result.push(key);
                }
            }
        }
        Ok(result)
    }
}
/// Source-specific trust mechanics. Call load/save on the ordered storage worker in M1.
/// Only fingerprints/path bytes persist, never executable configuration text.
#[derive(Default)]
pub struct Trust {
    approved: BTreeMap<PathBuf, [u8; 32]>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct TrustRecord {
    source: Vec<u8>,
    fingerprint: [u8; 32],
}
fn fingerprint(value: &Value) -> [u8; 32] {
    Sha256::digest(
        serde_yaml::to_string(value)
            .expect("parsed YAML serializes")
            .as_bytes(),
    )
    .into()
}
impl Trust {
    pub fn load(path: &Path) -> io::Result<Self> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err(err("trust file too large"));
        }
        let records: Vec<TrustRecord> =
            serde_json::from_slice(&bytes).map_err(|_| err("invalid trust records"))?;
        Ok(Self {
            approved: records
                .into_iter()
                .map(|record| {
                    (
                        PathBuf::from(std::ffi::OsString::from_vec(record.source)),
                        record.fingerprint,
                    )
                })
                .collect(),
        })
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| err("trust path requires parent"))?;
        fs::create_dir_all(parent)?;
        let records: Vec<_> = self
            .approved
            .iter()
            .map(|(source, fingerprint)| TrustRecord {
                source: source.as_os_str().as_bytes().to_vec(),
                fingerprint: *fingerprint,
            })
            .collect();
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary
            .write_all(&serde_json::to_vec(&records).map_err(|_| err("trust serialization"))?)?;
        temporary.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
    pub fn approve(&mut self, settings: &Settings, source: &PathBuf) {
        if let Some(value) = settings.executable.get(source) {
            self.approved.insert(source.clone(), fingerprint(value));
        }
    }
    /// Every execution entry point (templates, suggestions, menus, final commands) calls this.
    pub fn may_execute(&self, settings: &Settings, source: &PathBuf) -> bool {
        settings.sources.contains_key(source)
            && settings
                .executable
                .get(source)
                .is_none_or(|value| self.approved.get(source) == Some(&fingerprint(value)))
    }
}
#[cfg(test)]
#[path = "tests/diagnostics.rs"]
mod diagnostic_tests;
#[cfg(test)]
#[path = "tests/lazygit.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/trust.rs"]
mod trust_tests;
