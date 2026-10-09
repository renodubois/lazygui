//! GUI-only file mechanics. Startup shares one cloneable ordered writer across windows.
//! The profile directory is separate from shared LazyGit sources; no shared config writes.
use super::{Settings, Trust, err, fingerprint};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::mpsc,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GuiPreferences {
    pub version: u32,
    pub window_size: Option<[u32; 2]>,
    /// Byte paths, not message drafts, command definitions or credentials.
    pub recent_repositories: Vec<Vec<u8>>,
}
impl Default for GuiPreferences {
    fn default() -> Self {
        Self {
            version: 1,
            window_size: None,
            recent_repositories: Vec::new(),
        }
    }
}
impl GuiPreferences {
    fn validate(&self) -> io::Result<()> {
        if self.version != 1 {
            return Err(err("unsupported GUI preference version"));
        }
        if self.window_size.is_some_and(|s| s.contains(&0)) {
            return Err(err("invalid window size"));
        }
        if self.recent_repositories.len() > 100
            || self
                .recent_repositories
                .iter()
                .any(|p| p.is_empty() || p.len() > 65536 || p.contains(&0))
        {
            return Err(err("invalid recent repository paths"));
        }
        Ok(())
    }
}
/// Deliberately not CONFIG_DIR or LG_CONFIG_FILE: GUI state has its own namespace.
pub fn profile_directory(
    home: Option<&Path>,
    xdg_config_home: Option<&Path>,
) -> io::Result<PathBuf> {
    let root = xdg_config_home
        .filter(|p| p.is_absolute())
        .map(Path::to_path_buf)
        .or_else(|| home.map(|p| p.join(".config")))
        .ok_or_else(|| err("HOME or absolute XDG_CONFIG_HOME required for GUI profile"))?;
    Ok(root.join(env!("CARGO_PKG_NAME")))
}
fn load_preferences(path: &Path) -> io::Result<GuiPreferences> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(GuiPreferences::default()),
        Err(e) => return Err(e),
    };
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(err("GUI preferences exceed 1 MiB"));
    }
    let preferences: GuiPreferences =
        serde_json::from_slice(&bytes).map_err(|_| err("invalid GUI preferences"))?;
    preferences.validate()?;
    Ok(preferences)
}
pub fn load_profile(profile: &Path) -> io::Result<(GuiPreferences, Trust)> {
    Ok((
        load_preferences(&profile.join("preferences.json"))?,
        Trust::load(&profile.join("trust.json"))?,
    ))
}
fn save_preferences(path: &Path, preferences: &GuiPreferences) -> io::Result<()> {
    preferences.validate()?;
    let parent = path
        .parent()
        .ok_or_else(|| err("GUI profile requires a parent"))?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(
        &serde_json::to_vec(preferences).map_err(|_| err("GUI preference serialization failed"))?,
    )?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
#[derive(Clone)]
pub enum StorageReply {
    Saved,
    Trust(Trust),
    Flushed,
}
pub type Reply = async_channel::Receiver<Result<StorageReply, String>>;
enum Change {
    Preferences(GuiPreferences),
    Approve(PathBuf, [u8; 32]),
    Revoke(PathBuf),
    Flush,
}
struct Request {
    change: Change,
    reply: async_channel::Sender<Result<StorageReply, String>>,
}
/// Construct once per profile and clone this handle, never start one writer per window.
/// Queueing is nonblocking. Dropping a reply does not cancel an accepted write.
/// Atomic replacement is not multi-process locking or crash-durable fsync.
#[derive(Clone)]
pub struct OrderedStorage {
    send: mpsc::SyncSender<Request>,
}
impl OrderedStorage {
    /// Small startup reads do not create directories/files. Corruption is an explicit error.
    pub fn open(profile: PathBuf) -> io::Result<(Self, GuiPreferences, Trust)> {
        if !profile.is_absolute() {
            return Err(err("GUI profile must be absolute"));
        }
        let preferences_path = profile.join("preferences.json");
        let trust_path = profile.join("trust.json");
        let preferences = load_preferences(&preferences_path)?;
        let trust = Trust::load(&trust_path)?;
        let mut worker_trust = trust.clone();
        let (send, receive) = mpsc::sync_channel::<Request>(16);
        std::thread::Builder::new()
            .name("lazygui-storage".into())
            .spawn(move || {
                while let Ok(request) = receive.recv() {
                    let result: io::Result<StorageReply> = match request.change {
                        Change::Preferences(preferences) => {
                            save_preferences(&preferences_path, &preferences)
                                .map(|()| StorageReply::Saved)
                        }
                        Change::Approve(source, hash) => {
                            let mut candidate = worker_trust.clone();
                            candidate.approved.insert(source, hash);
                            candidate.save(&trust_path).map(|()| {
                                worker_trust = candidate;
                                StorageReply::Trust(worker_trust.clone())
                            })
                        }
                        Change::Revoke(source) => {
                            let mut candidate = worker_trust.clone();
                            candidate.approved.remove(&source);
                            candidate.save(&trust_path).map(|()| {
                                worker_trust = candidate;
                                StorageReply::Trust(worker_trust.clone())
                            })
                        }
                        Change::Flush => Ok(StorageReply::Flushed),
                    };
                    // Replies never contain config contents or command strings.
                    let _ = request.reply.send_blocking(
                        result.map_err(|_| "Could not save GUI preferences/trust.".into()),
                    );
                }
            })?;
        Ok((Self { send }, preferences, trust))
    }
    fn submit(&self, change: Change) -> Reply {
        let (reply, receive) = async_channel::bounded(1);
        if let Err(error) = self.send.try_send(Request { change, reply }) {
            let (mpsc::TrySendError::Full(request) | mpsc::TrySendError::Disconnected(request)) =
                error;
            let _ = request
                .reply
                .try_send(Err("GUI storage worker unavailable or busy.".into()));
        }
        receive
    }
    pub fn save_preferences(&self, preferences: GuiPreferences) -> Reply {
        self.submit(Change::Preferences(preferences))
    }
    /// Enqueue the approval for this exact executable projection, not a later config snapshot.
    /// Custom commands remain unavailable in M1 even after source approval.
    pub fn approve(&self, settings: &Settings, source: &Path) -> io::Result<Reply> {
        let value = settings
            .executable
            .get(source)
            .ok_or_else(|| err("source has no approval-requiring executable settings"))?;
        Ok(self.submit(Change::Approve(source.into(), fingerprint(value))))
    }
    pub fn revoke(&self, source: PathBuf) -> Reply {
        self.submit(Change::Revoke(source))
    }
    /// Acknowledges completion of all requests accepted before this barrier.
    pub fn flush(&self) -> Reply {
        self.submit(Change::Flush)
    }
}
