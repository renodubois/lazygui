//! Linux non-PTY child ownership. Execute/wait on workers, never the GPUI thread.
//! Hosts retain settlement after handles are dropped; startup must await shutdown.
//! Covers helpers remaining in the owned group, not daemonizing/escaping tools.
use std::os::unix::process::CommandExt;
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

const CAP: usize = 1024 * 1024;
pub struct Output {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    /// Bounded diagnostic bytes; never Debug/log this payload automatically.
    pub stderr: Vec<u8>,
    pub cancelled: bool,
    pub truncated: bool,
}
pub trait Executor: Send + Sync {
    /// Admit an entire owner workflow before its worker is spawned.
    fn begin_workflow(&self, _cancel: Arc<AtomicBool>) -> io::Result<Option<Workflow>> {
        Ok(None)
    }
    fn execute_scoped(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
        _workflow: &Workflow,
    ) -> io::Result<Output> {
        self.execute(command, input, cancel)
    }
    fn execute(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output>;
}
pub struct Native;
type DrainWorker = thread::JoinHandle<io::Result<(Vec<u8>, bool)>>;
/// Worker-local cleanup also covers pipe-thread setup failure/unwinding. Never
/// constructed or dropped on the GUI thread; Operation Drop remains nonblocking.
struct ChildSession {
    child: Child,
    pid: i32,
    reaped: bool,
    writer: Option<thread::JoinHandle<io::Result<()>>>,
    out: Option<DrainWorker>,
    err: Option<DrainWorker>,
}
impl ChildSession {
    fn kill_group(&self) {
        unsafe {
            libc::kill(-self.pid, libc::SIGKILL);
        }
    }
}
impl Drop for ChildSession {
    fn drop(&mut self) {
        if !self.reaped {
            self.kill_group();
            let _ = self.child.wait();
        }
        if let Some(worker) = self.writer.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.out.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.err.take() {
            let _ = worker.join();
        }
    }
}
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
        if cancel.load(Ordering::Acquire) {
            return Ok(Output {
                code: None,
                stdout: vec![],
                stderr: vec![],
                cancelled: true,
                truncated: false,
            });
        }
        command
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command.spawn()?;
        let pid = child.id() as i32;
        let mut session = ChildSession {
            child,
            pid,
            reaped: false,
            writer: None,
            out: None,
            err: None,
        };
        let mut stdin = session.child.stdin.take().unwrap();
        session.writer = Some(
            thread::Builder::new()
                .name("git-input".into())
                .spawn(move || stdin.write_all(&input))?,
        );
        let stdout = session.child.stdout.take().unwrap();
        session.out = Some(
            thread::Builder::new()
                .name("git-output".into())
                .spawn(move || drain(stdout))?,
        );
        let stderr = session.child.stderr.take().unwrap();
        session.err = Some(
            thread::Builder::new()
                .name("git-error".into())
                .spawn(move || drain(stderr))?,
        );
        let status = loop {
            if cancel.load(Ordering::Acquire) {
                session.kill_group();
                break session.child.wait();
            }
            // Observe without reaping: reserve our PID until helpers are settled.
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
                session.kill_group();
                session.reaped = session.child.wait().is_ok();
                break Err(error);
            }
            if unsafe { info.si_pid() } != 0 {
                session.kill_group();
                break session.child.wait();
            }
            thread::sleep(Duration::from_millis(5));
        };
        session.reaped |= status.is_ok();
        // Pipe closure can give BrokenPipe on an early rejecting child. Preserve its
        // exit/stderr instead of replacing the useful Git error with a writer error.
        let written = session
            .writer
            .take()
            .unwrap()
            .join()
            .map_err(|_| io::Error::other("input worker"))
            .and_then(|result| result);
        // Join every worker even if one drain failed, before acknowledging settlement.
        let stdout_result = session
            .out
            .take()
            .unwrap()
            .join()
            .map_err(|_| io::Error::other("output worker"));
        let stderr_result = session
            .err
            .take()
            .unwrap()
            .join()
            .map_err(|_| io::Error::other("error worker"));
        let status = status?;
        let (stdout, out_cut) = stdout_result??;
        let (stderr, err_cut) = stderr_result??;
        if status.success() && !cancel.load(Ordering::Acquire) {
            written?;
        }
        Ok(Output {
            code: status.code(),
            stdout,
            stderr,
            cancelled: cancel.load(Ordering::Acquire),
            truncated: out_cut || err_cut,
        })
    }
}

/// Cloneable completion acknowledgment. Async wait is safe on the GUI executor;
/// blocking wait is for tests/workers only. Completion means child/pipe settlement.
#[derive(Clone)]
pub struct Settlement {
    done: async_channel::Receiver<()>,
}
impl Settlement {
    pub fn is_complete(&self) -> bool {
        self.done.is_closed()
    }
    pub async fn wait(&self) {
        let _ = self.done.recv().await;
    }
    pub fn wait_blocking(&self) {
        let _ = self.done.recv_blocking();
    }
}
struct HostState {
    next: u64,
    active: BTreeMap<u64, Arc<AtomicBool>>,
    shutting_down: bool,
    done: Option<async_channel::Sender<()>>,
}
struct HostInner {
    state: Mutex<HostState>,
    settled: Settlement,
}
/// A narrow process-lifetime holder shared by windows and retained by startup.
/// `shutdown` is nonblocking and rejects new work. Await its acknowledgment before
/// last-window application exit; dropping a host is not a shutdown acknowledgment.
#[derive(Clone)]
pub struct ProcessHost {
    inner: Arc<HostInner>,
}
impl Default for ProcessHost {
    fn default() -> Self {
        Self::new()
    }
}
impl ProcessHost {
    pub fn new() -> Self {
        let (tx, rx) = async_channel::bounded(1);
        Self {
            inner: Arc::new(HostInner {
                state: Mutex::new(HostState {
                    next: 0,
                    active: BTreeMap::new(),
                    shutting_down: false,
                    done: Some(tx),
                }),
                settled: Settlement { done: rx },
            }),
        }
    }
    pub fn spawn(&self, command: Command, input: Vec<u8>) -> io::Result<Operation> {
        self.spawn_with_executor(
            command,
            input,
            Arc::new(Native),
            Arc::new(AtomicBool::new(false)),
        )
    }
    pub fn spawn_with_executor(
        &self,
        command: Command,
        input: Vec<u8>,
        executor: Arc<dyn Executor>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Operation> {
        let id = {
            let mut state = self.inner.state.lock().unwrap();
            if state.shutting_down {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "process host shutting down",
                ));
            }
            let id = state.next;
            state.next += 1;
            state.active.insert(id, cancel.clone());
            id
        };
        let (result_tx, result_rx) = async_channel::bounded(1);
        let (done_tx, done_rx) = async_channel::bounded(1);
        let inner = self.inner.clone();
        let flag = cancel.clone();
        let worker = thread::Builder::new()
            .name("git-settlement".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    executor.execute(command, input, flag)
                }))
                .unwrap_or_else(|_| Err(io::Error::other("process worker panicked")));
                let _ = result_tx.try_send(result);
                // Native has reaped and joined its pipe workers before acknowledging.
                drop(done_tx);
                let mut state = inner.state.lock().unwrap();
                state.active.remove(&id);
                if state.shutting_down && state.active.is_empty() {
                    state.done.take();
                }
            });
        if let Err(error) = worker {
            let mut state = self.inner.state.lock().unwrap();
            state.active.remove(&id);
            if state.shutting_down && state.active.is_empty() {
                state.done.take();
            }
            return Err(error);
        }
        // JoinHandle is deliberately detached. The retained state/acknowledgment,
        // not a GUI-thread join, owns the completion protocol.
        Ok(Operation {
            cancel,
            result: result_rx,
            settled: Settlement { done: done_rx },
            cancel_on_drop: true,
        })
    }
    pub fn shutdown(&self) -> Settlement {
        let mut state = self.inner.state.lock().unwrap();
        state.shutting_down = true;
        for cancel in state.active.values() {
            cancel.store(true, Ordering::Release);
        }
        if state.active.is_empty() {
            state.done.take();
        }
        self.inner.settled.clone()
    }
    /// Wrap an injected executor so every Client command participates in shutdown.
    pub fn retain(&self, executor: Arc<dyn Executor>) -> Arc<dyn Executor> {
        Arc::new(RetainedExecutor {
            host: self.clone(),
            executor,
        })
    }
}
/// Retained admission, shared by the worker client and its opaque delivery.
/// Last drop acknowledges only after worker cleanup and update reconciliation/drop.
#[derive(Clone)]
pub struct Workflow {
    inner: Arc<WorkflowInner>,
}
struct WorkflowInner {
    host: Arc<HostInner>,
    id: u64,
    cancel: Arc<AtomicBool>,
}
impl Workflow {
    pub fn cancelled(&self) -> bool {
        self.inner.cancel.load(Ordering::Acquire)
    }
}
impl Drop for WorkflowInner {
    fn drop(&mut self) {
        let mut state = self.host.state.lock().unwrap();
        state.active.remove(&self.id);
        if state.shutting_down && state.active.is_empty() {
            state.done.take();
        }
    }
}
impl ProcessHost {
    fn admit_workflow(&self, cancel: Arc<AtomicBool>) -> io::Result<Workflow> {
        let mut state = self.inner.state.lock().unwrap();
        if state.shutting_down {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "process host shutting down",
            ));
        }
        let id = state.next;
        state.next += 1;
        state.active.insert(id, cancel.clone());
        Ok(Workflow {
            inner: Arc::new(WorkflowInner {
                host: self.inner.clone(),
                id,
                cancel,
            }),
        })
    }
}
struct RetainedExecutor {
    host: ProcessHost,
    executor: Arc<dyn Executor>,
}
impl Executor for RetainedExecutor {
    fn begin_workflow(&self, cancel: Arc<AtomicBool>) -> io::Result<Option<Workflow>> {
        self.host.admit_workflow(cancel).map(Some)
    }
    fn execute_scoped(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
        _workflow: &Workflow,
    ) -> io::Result<Output> {
        // Admission belongs to the workflow, not individual commands. In particular
        // cleanup reads must remain possible after shutdown closes new admission.
        self.executor.execute(command, input, cancel)
    }
    fn execute(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output> {
        self.host
            .spawn_with_executor(command, input, self.executor.clone(), cancel)?
            .finish()
    }
}
/// Feature-owned handle. Drop requests cancellation without waiting or joining.
/// View replacement must not drop the feature's operation. Hosts retain settlement.
pub struct Operation {
    cancel: Arc<AtomicBool>,
    result: async_channel::Receiver<io::Result<Output>>,
    settled: Settlement,
    cancel_on_drop: bool,
}
impl Operation {
    /// Convenience for isolated operations. Application code should share a host.
    pub fn spawn(command: Command, input: Vec<u8>) -> Self {
        ProcessHost::new()
            .spawn(command, input)
            .expect("spawn process worker")
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn cancellation(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    pub fn settlement(&self) -> Settlement {
        self.settled.clone()
    }
    /// Worker-only blocking result retrieval. No UI-thread join.
    pub fn finish(mut self) -> io::Result<Output> {
        let result = self.result.recv_blocking();
        self.cancel_on_drop = false;
        result.map_err(|_| io::Error::other("process result unavailable"))?
    }
    pub async fn finish_async(mut self) -> io::Result<Output> {
        let result = self.result.recv().await;
        self.cancel_on_drop = false;
        result.map_err(|_| io::Error::other("process result unavailable"))?
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            self.cancel();
        }
    }
}
#[cfg(test)]
#[path = "tests/process.rs"]
mod tests;
