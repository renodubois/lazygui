use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub(crate) query: String,
}

pub(crate) fn config_path() -> Option<PathBuf> {
    let directory = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(
        directory
            .join(env!("CARGO_PKG_NAME"))
            .join("preferences.json"),
    )
}
pub(crate) fn load(path: &Path) -> io::Result<Config> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid preference file.")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
        Err(error) => Err(error),
    }
}
pub(super) fn save(path: &Path, config: &Config) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("missing preference directory"))?;
    std::fs::create_dir_all(directory)?;
    // One worker owns this path within a process. Independent instances need separate profiles.
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec(config)?)?;
    std::fs::rename(temporary, path)
}
#[cfg(test)]
#[path = "tests/preferences.rs"]
mod tests;
