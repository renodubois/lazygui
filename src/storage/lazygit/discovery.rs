//! Linux v0.66.0 lookup order, with no shared-file creation or migration.
use super::{Settings, Source, err};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

/// Explicit environment snapshot: tests and alternate profiles never read user paths implicitly.
#[derive(Clone, Debug, Default)]
pub struct DiscoveryOptions {
    pub cwd: PathBuf,
    pub home: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_config_dirs: Vec<PathBuf>,
    pub lg_config_file: Option<OsString>,
    /// `--use-config-file`; comma-separated like LG_CONFIG_FILE, takes precedence over it.
    pub cli_config_file: Option<OsString>,
    /// Original path-resolution base for global sources, frozen by the repository owner.
    pub global_source_cwd: Option<PathBuf>,
    pub repository_root: Option<PathBuf>,
    /// Resolved per-worktree Git dir, not necessarily `<root>/.git` or the common dir.
    pub git_dir: Option<PathBuf>,
}
impl DiscoveryOptions {
    pub fn from_environment(cwd: PathBuf) -> Self {
        let nonempty = |name| std::env::var_os(name).filter(|s| !s.is_empty());
        Self {
            cwd,
            home: nonempty("HOME").map(PathBuf::from),
            config_dir: nonempty("CONFIG_DIR").map(PathBuf::from),
            xdg_config_home: nonempty("XDG_CONFIG_HOME").map(PathBuf::from),
            xdg_config_dirs: nonempty("XDG_CONFIG_DIRS")
                .map(|s| {
                    std::env::split_paths(&s)
                        .filter(|p| p.is_absolute())
                        .collect()
                })
                .unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")]),
            lg_config_file: nonempty("LG_CONFIG_FILE"),
            ..Self::default()
        }
    }
    /// Freeze global path resolution against the original startup cwd. Repository
    /// ancestor/Git-directory sources remain dynamic and are rediscovered on switch.
    /// Do not canonicalize here: optional files may be created before a later reload.
    pub fn anchor_global_sources(&mut self) {
        let cwd = self
            .global_source_cwd
            .get_or_insert_with(|| self.cwd.clone());
        // Keep CLI/env lists byte-for-byte: joining and re-serializing their paths
        // would accidentally turn commas in the startup cwd into list delimiters.
        for path in [&mut self.config_dir, &mut self.home].into_iter().flatten() {
            if !path.as_os_str().is_empty() && !path.is_absolute() {
                *path = cwd.join(&*path);
            }
        }
        // Relative XDG paths are deliberately ignored by the upstream lookup rules.
    }
    fn global_absolute(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.into()
        } else {
            self.global_source_cwd
                .as_ref()
                .unwrap_or(&self.cwd)
                .join(path)
        }
    }
    fn absolute(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.into()
        } else {
            self.cwd.join(path)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Global,
    Ancestor,
    GitDirectory,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStatus {
    Loaded,
    MissingOptional,
    MissingRequired,
    Duplicate,
    Failed,
}
#[derive(Clone, Debug)]
pub struct SourceReport {
    pub requested_path: PathBuf,
    /// Canonical identity exists only for an existing readable source.
    pub identity: Option<PathBuf>,
    pub kind: SourceKind,
    pub status: SourceStatus,
    pub message: String,
}
#[derive(Clone, Default)]
pub struct Discovery {
    pub sources: Vec<Source>,
    pub reports: Vec<SourceReport>,
}
#[derive(Debug)]
pub struct DiscoveryError {
    pub reports: Vec<SourceReport>,
    pub message: String,
}
impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)?;
        for report in &self.reports {
            write!(
                f,
                "\nConfig source {}: {:?} ({:?})",
                report.requested_path.display(),
                report.status,
                report.kind
            )?;
            if !report.message.is_empty() {
                write!(f, "; {}", report.message)?;
            }
        }
        Ok(())
    }
}
impl std::error::Error for DiscoveryError {}

pub fn discover(options: &DiscoveryOptions) -> Result<Discovery, DiscoveryError> {
    let run = || -> io::Result<Vec<(PathBuf, SourceKind, bool)>> {
        if !options.cwd.is_absolute() {
            return Err(err("config discovery cwd must be absolute"));
        }
        let custom = options
            .cli_config_file
            .as_ref()
            .filter(|s| !s.is_empty())
            .or(options.lg_config_file.as_ref().filter(|s| !s.is_empty()));
        let mut paths = Vec::new();
        if let Some(custom) = custom {
            // Preserve Linux path bytes, including non-UTF8; commas delimit paths upstream.
            use std::os::unix::ffi::{OsStrExt, OsStringExt};
            for bytes in custom.as_os_str().as_bytes().split(|b| *b == b',') {
                if bytes.is_empty() {
                    return Err(err("empty custom config path"));
                }
                paths.push((
                    options.global_absolute(Path::new(&OsString::from_vec(bytes.to_vec()))),
                    SourceKind::Global,
                    true,
                ));
            }
        } else if let Some(dir) = options
            .config_dir
            .as_ref()
            .filter(|p| !p.as_os_str().is_empty())
        {
            paths.push((
                options.global_absolute(dir).join("config.yml"),
                SourceKind::Global,
                false,
            ));
        } else {
            let home = options
                .xdg_config_home
                .as_ref()
                .filter(|p| p.is_absolute())
                .cloned()
                .or_else(|| options.home.as_ref().map(|p| p.join(".config")))
                .ok_or_else(|| {
                    err("HOME or absolute XDG_CONFIG_HOME required for default config discovery")
                })?;
            let mut roots = vec![home.clone()];
            roots.extend(
                options
                    .xdg_config_dirs
                    .iter()
                    .filter(|p| p.is_absolute())
                    .cloned(),
            );
            let mut chosen = None;
            // Search every legacy root before every modern root, not per-root interleaving.
            'search: for namespace in ["jesseduffield/lazygit", "lazygit"] {
                for root in &roots {
                    let path = root.join(namespace).join("config.yml");
                    match fs::metadata(&path) {
                        Ok(_) => {
                            chosen = Some(path);
                            break 'search;
                        }
                        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                        // Select unreadable candidates too, so the reader emits source diagnostics.
                        Err(_) => {
                            chosen = Some(path);
                            break 'search;
                        }
                    }
                }
            }
            paths.push((
                chosen.unwrap_or_else(|| home.join("lazygit/config.yml")),
                SourceKind::Global,
                false,
            ));
        }
        if let Some(root) = &options.repository_root {
            let root = fs::canonicalize(options.absolute(root))
                .map_err(|e| err(format!("repository config root {}: {e}", root.display())))?;
            paths.extend(ancestor_paths(&root));
        }
        if let Some(dir) = &options.git_dir {
            let dir = fs::canonicalize(options.absolute(dir))
                .map_err(|e| err(format!("repository config Git dir {}: {e}", dir.display())))?;
            paths.push((dir.join("lazygit.yml"), SourceKind::GitDirectory, false));
        }
        Ok(paths)
    };
    let paths = run().map_err(|e| DiscoveryError {
        reports: vec![],
        message: e.to_string(),
    })?;
    read_sources(paths)
}
fn ancestor_paths(root: &Path) -> Vec<(PathBuf, SourceKind, bool)> {
    let mut ancestors: Vec<_> = root.ancestors().skip(1).collect();
    ancestors.reverse();
    ancestors
        .into_iter()
        .map(|dir| (dir.join(".lazygit.yml"), SourceKind::Ancestor, false))
        .collect()
}
fn read_sources(paths: Vec<(PathBuf, SourceKind, bool)>) -> Result<Discovery, DiscoveryError> {
    let mut result = Discovery::default();
    let mut seen = BTreeSet::new();
    let mut failed = false;
    for (path, kind, required) in paths {
        let mut report = SourceReport {
            requested_path: path.clone(),
            identity: None,
            kind,
            status: SourceStatus::Failed,
            message: String::new(),
        };
        let read = || -> io::Result<(PathBuf, String)> {
            let identity = fs::canonicalize(&path)?;
            if !fs::metadata(&identity)?.is_file() {
                return Err(err("config source is not a regular file"));
            }
            let file = fs::File::open(&identity)?;
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                return Err(err("config source exceeds 1 MiB"));
            }
            let yaml =
                String::from_utf8(bytes).map_err(|_| err("config source is not UTF-8 YAML"))?;
            Ok((identity, yaml))
        };
        match read() {
            Ok((identity, yaml)) => {
                report.identity = Some(identity.clone());
                if seen.insert(identity.clone()) {
                    report.status = SourceStatus::Loaded;
                    report.message =
                        "Shared config is read-only; migration unavailable in M1, not applied."
                            .into();
                    result.sources.push(Source {
                        path: identity,
                        explicitly_chosen_global: kind == SourceKind::Global,
                        yaml,
                    });
                } else {
                    report.status = SourceStatus::Duplicate;
                    report.message =
                        "Canonical source already loaded; alias not applied twice.".into();
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                report.status = if required {
                    SourceStatus::MissingRequired
                } else {
                    SourceStatus::MissingOptional
                };
                report.message = if required {
                    "Explicit config source is missing."
                } else {
                    "Optional config source is absent; no file created."
                }
                .into();
                failed |= required;
            }
            Err(e) => {
                report.message = e.to_string();
                failed = true;
            }
        }
        result.reports.push(report);
    }
    if failed {
        let message = result
            .reports
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    SourceStatus::Failed | SourceStatus::MissingRequired
                )
            })
            .map(|r| format!("{}: {}", r.requested_path.display(), r.message))
            .collect::<Vec<_>>()
            .join("\n");
        Err(DiscoveryError {
            reports: result.reports,
            message,
        })
    } else {
        Ok(result)
    }
}
#[cfg(test)]
#[path = "../tests/config_discovery.rs"]
mod tests;

impl Settings {
    pub fn load_discovered(
        options: &DiscoveryOptions,
    ) -> Result<(Self, Discovery), DiscoveryError> {
        let discovery = discover(options)?;
        let settings = Self::load_m1(&discovery.sources).map_err(|e| {
            let message = e.to_string();
            let mut reports = discovery.reports.clone();
            for report in &mut reports {
                if report
                    .identity
                    .as_ref()
                    .is_some_and(|path| message.starts_with(&format!("{}:", path.display())))
                {
                    report.status = SourceStatus::Failed;
                    report.message = message.clone();
                }
            }
            DiscoveryError { reports, message }
        })?;
        Ok((settings, discovery))
    }
    /// Read and validate all sources before replacing the current settings/keymap.
    pub fn reload_discovered(
        &mut self,
        options: &DiscoveryOptions,
    ) -> Result<Discovery, DiscoveryError> {
        let (candidate, discovery) = Self::load_discovered(options)?;
        *self = candidate;
        Ok(discovery)
    }
}
