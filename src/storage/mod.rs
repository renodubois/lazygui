//! Ordered file operations; workflow policy stays with feature owners.
mod preferences;
pub(crate) use preferences::{Config, config_path, load};
use std::path::PathBuf;

pub(crate) struct Persistence {
    send: std::sync::mpsc::SyncSender<(Config, async_channel::Sender<Result<(), String>>)>,
}
impl Persistence {
    pub(crate) fn new(path: PathBuf) -> Self {
        let (send, receive) = std::sync::mpsc::sync_channel::<(
            Config,
            async_channel::Sender<Result<(), String>>,
        )>(16);
        std::thread::spawn(move || {
            while let Ok((config, reply)) = receive.recv() {
                let result = preferences::save(&path, &config)
                    .map_err(|_| "Could not save preferences.".into());
                let _ = reply.send_blocking(result);
            }
        });
        Self { send }
    }
    pub(crate) fn save(&self, config: Config) -> async_channel::Receiver<Result<(), String>> {
        let (send, receive) = async_channel::bounded(1);
        if let Err(error) = self.send.try_send((config, send)) {
            let (std::sync::mpsc::TrySendError::Full((_, reply))
            | std::sync::mpsc::TrySendError::Disconnected((_, reply))) = error;
            let _ = reply.try_send(Err("Preference worker unavailable or busy.".into()));
        }
        receive
    }
}
