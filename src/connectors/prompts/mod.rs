//! Fake-only M0 editor/HTTPS-askpass bridge. No credential provider integration.
//! Operation owns socket/tempdir. Secrets never implement Debug or enter logs.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const MAX: usize = 64 * 1024;
#[derive(Serialize, Deserialize)]
pub enum Request {
    Askpass { label: String },
    Editor { path: PathBuf },
    SequenceEditor { path: PathBuf },
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    token: String,
    request_id: u64,
    request: Request,
}
#[derive(Serialize, Deserialize)]
pub enum Reply {
    Value(String),
    Edited,
    Abort,
}
pub struct Bridge {
    directory: tempfile::TempDir,
    listener: UnixListener,
    token: String,
    next: u64,
    file: Option<PathBuf>,
}
fn error() -> io::Error {
    io::Error::other("prompt bridge rejected or cancelled request")
}
fn frame(stream: &mut UnixStream, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > MAX {
        return Err(error());
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)
}
impl Bridge {
    pub fn new(parent: &Path, allowed_editor_file: Option<PathBuf>) -> io::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("lazygui-prompt-")
            .tempdir_in(parent)?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        let listener = UnixListener::bind(directory.path().join("socket"))?;
        fs::set_permissions(
            directory.path().join("socket"),
            fs::Permissions::from_mode(0o600),
        )?;
        listener.set_nonblocking(true)?;
        let mut random = [0; 32];
        fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
        let token = random.iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self {
            directory,
            listener,
            token,
            next: 1,
            file: allowed_editor_file,
        })
    }
    pub fn socket(&self) -> PathBuf {
        self.directory.path().join("socket")
    }
    /// Transfer to an owned helper over its private environment, never command log.
    pub fn helper_token(&self) -> &str {
        &self.token
    }
    pub fn serve_one(
        &mut self,
        cancel: &AtomicBool,
        timeout: Duration,
        respond: impl FnOnce(Request) -> Reply,
    ) -> io::Result<()> {
        let deadline = Instant::now() + timeout;
        let check = || {
            if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                Err(error())
            } else {
                Ok(())
            }
        };
        let mut stream = loop {
            check()?;
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => return Err(e),
            }
        };
        stream.set_read_timeout(Some(Duration::from_millis(20)))?;
        stream.set_write_timeout(Some(Duration::from_millis(100)))?;
        let mut read = |bytes: &mut [u8]| -> io::Result<()> {
            let mut offset = 0;
            while offset < bytes.len() {
                check()?;
                match stream.read(&mut bytes[offset..]) {
                    Ok(0) => return Err(error()),
                    Ok(n) => offset += n,
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::WouldBlock
                                | io::ErrorKind::TimedOut
                                | io::ErrorKind::Interrupted
                        ) => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        };
        let mut size = [0; 4];
        read(&mut size)?;
        let size = u32::from_be_bytes(size) as usize;
        if size > MAX {
            return Err(error());
        }
        let mut bytes = vec![0; size];
        read(&mut bytes)?;
        let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| error())?;
        if envelope.token != self.token || envelope.request_id != self.next {
            return Err(error());
        }
        if let Request::Editor { path } | Request::SequenceEditor { path } = &envelope.request
            && self.file.as_ref() != Some(path)
        {
            return Err(error());
        }
        self.next += 1;
        check()?;
        let reply = respond(envelope.request);
        frame(
            &mut stream,
            &serde_json::to_vec(&reply).map_err(|_| error())?,
        )
    }
}
/// Prototype helper call; a production helper binary/file-return adapter comes in M1/M4.
pub fn request(socket: &Path, token: &str, request_id: u64, request: Request) -> io::Result<Reply> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    frame(
        &mut stream,
        &serde_json::to_vec(&Envelope {
            token: token.into(),
            request_id,
            request,
        })
        .map_err(|_| error())?,
    )?;
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > MAX {
        return Err(error());
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(|_| error())
}
#[cfg(test)]
#[path = "tests/lifetime.rs"]
mod lifetime_tests;
#[cfg(test)]
#[path = "tests/bridge.rs"]
mod tests;
