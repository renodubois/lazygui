//! Linux non-PTY child ownership. Run on a worker, never on the GPUI thread.
use std::os::unix::process::CommandExt;
use std::{
    io::{self, Read, Write},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

const CAP: usize = 1024 * 1024;
pub struct Output {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    // Deliberately no stderr/payload Debug or command logging.
    pub cancelled: bool,
    pub truncated: bool,
}
pub trait Executor: Send + Sync {
    fn execute(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output>;
}
pub struct Native;
fn drain(mut stream: impl Read) -> io::Result<(Vec<u8>, bool)> {
    let mut retained = Vec::new();
    let mut chunk = [0; 8192];
    let mut truncated = false;
    loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let keep = n.min(CAP - retained.len());
        retained.extend_from_slice(&chunk[..keep]);
        truncated |= keep < n;
    }
    Ok((retained, truncated))
}
impl Executor for Native {
    fn execute(
        &self,
        mut command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output> {
        command
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let pid = child.id() as i32;
        let mut stdin = child.stdin.take().unwrap();
        let writer = thread::spawn(move || stdin.write_all(&input));
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out = thread::spawn(move || drain(stdout));
        let err = thread::spawn(move || drain(stderr));
        let status = loop {
            if cancel.load(Ordering::Acquire) {
                // Only the group created above; never a desktop-wide process name.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
                break child.wait();
            }
            // Observe without reaping: keep our PID reserved until the group is settled.
            // Killing a group after try_wait reaped the leader could target a reused PID.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid as u32,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
                let _ = child.wait();
                break Err(error);
            }
            if unsafe { info.si_pid() } != 0 {
                // Settle pipe-holding helpers before reaping the direct child.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
                break child.wait();
            }
            thread::sleep(Duration::from_millis(5));
        };
        let _ = writer.join();
        let (stdout, out_cut) = out
            .join()
            .map_err(|_| io::Error::other("output worker"))??;
        let (_, err_cut) = err.join().map_err(|_| io::Error::other("error worker"))??;
        Ok(Output {
            code: status?.code(),
            stdout,
            cancelled: cancel.load(Ordering::Acquire),
            truncated: out_cut || err_cut,
        })
    }
}
/// Operation lifetime handle. Window/feature hosts retain it; closing settles before release.
/// M1 must move this blocking shutdown onto its retained worker, not Drop on the UI thread.
pub struct Operation {
    cancel: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<io::Result<Output>>>,
}
impl Operation {
    pub fn spawn(command: Command, input: Vec<u8>) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        Self {
            cancel,
            worker: Some(thread::spawn(move || Native.execute(command, input, flag))),
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    /// Prompt waiters belong to this operation's cancellation scope.
    pub fn cancellation(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    pub fn finish(mut self) -> io::Result<Output> {
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| io::Error::other("process worker"))?
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
#[path = "tests/process.rs"]
mod tests;
